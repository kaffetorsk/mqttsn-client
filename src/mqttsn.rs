use heapless::{String, HistoryBuffer};
use crate::socket::{SendBytes, RecieveBytes, SocketError};
use crate::ackmap::{AckMap, AckMapError};
use mqtt_sn::defs::*;
use bimap::BiMap;
use byte::{TryRead, TryWrite};
use log::*;

type Error = MqttSnClientError;

#[derive(Hash, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
#[repr(u8)]
pub enum TopicIdType {
    Id,
    PreDef,
    Short
}

impl TryFrom<u8> for TopicIdType {
    type Error = MqttSnClientError;
    fn try_from(i: u8) -> Result<Self, Error> {
        match i {
            0 => Ok(TopicIdType::Id),
            1 => Ok(TopicIdType::PreDef),
            2 => Ok(TopicIdType::Short),
            _ => Err(Error::ParseError)
        }
    }
}

pub struct MqttSnClient<S> {
    client_id: ClientId,
    msg_id: MsgId,
    socket: S,
    topics: BiMap<(TopicIdType, u16), String<256>>,
    acks: AckMap<16>,
    rx_queue: HistoryBuffer<MqttMessage, 16>,
}

impl<S> MqttSnClient<S>
where
    S: SendBytes + RecieveBytes
{
    pub fn new(
        client_id: &str,
        socket: S
    ) -> Result<MqttSnClient<S>, Error> {
        Ok(MqttSnClient {
            client_id: client_id.into(),
            msg_id: MsgId {last_id: 0},
            socket,
            topics: BiMap::new(),
            acks: AckMap::<16>::new(),
            rx_queue: HistoryBuffer::new(),
        })
    }

    pub async fn recieve(&mut self) -> Result<Option<Message>, Error> {
        // get acks and publish, push to ackmap and message queue
        // return other types
        let mut buffer = [0u8; 1024];
        match Message::try_read(self.socket.recv(&mut buffer).await?, ()) {
            Ok((Message::RegAck(msg), _)) => self.acks.insert(msg.msg_id, Message::RegAck(msg)).await?,
            Ok((Message::SubAck(msg), _)) => self.acks.insert(msg.msg_id, Message::SubAck(msg)).await?,
            Ok((Message::PubAck(msg), _)) => self.acks.insert(msg.msg_id, Message::PubAck(msg)).await?,
            Ok((Message::UnsubAck(msg), _)) => self.acks.insert(msg.msg_id, Message::UnsubAck(msg)).await?,
            Ok((Message::Publish(msg), _)) => self.rx_queue.write(MqttMessage::from_publish(msg, &self.topics)?),
            Ok((msg, _)) => return Ok(Some(msg)),
            _ => return Err(MqttSnClientError::ParseError),
        };
        Ok(None)
    }

    pub async fn send(&mut self, msg: Message) -> Result<(), Error> {
        let mut buffer = [0u8; 1024];
        let len = msg.try_write(&mut buffer, ())?;
        self.socket.send(&buffer[..len]).await?;
        Ok(())
    }

    pub async fn ping(&mut self) -> Result<(), Error>{
        self.send(PingReq {client_id: self.client_id.clone()}.into()).await?;
        match self.recieve().await {
            Ok(Some(Message::PingResp(_))) => Ok(()),
            _ => Err(Error::NoPingResponse)
        }
    }

    pub async fn publish(&mut self, msg: MqttMessage) -> Result<(), Error> {
        let mut flags = Flags::default();
        let topic_id;
        if let Some((topic_type, id)) = self.topics.get_by_right(&msg.topic) {
            topic_id = *id;
            flags.set_topic_id_type(*topic_type as u8);
        } else {
            topic_id = self.register(&msg.topic).await?;
            self.topics.insert((TopicIdType::Id, topic_id), msg.topic);
        }

        let mut data = PublishData::new();
        data.push_str(&msg.payload)?;

        let packet = Publish {
            flags,
            topic_id: topic_id,
            msg_id: self.msg_id.next().unwrap(),
            data,

        };

        self.send(packet.into()).await?;
        Ok(())
    }

    async fn register(&mut self, topic: &String<256>) -> Result<u16, Error> {
        let msg_id = self.msg_id.next().unwrap();
        let packet = Register {
            topic_id: 0,
            msg_id,
            topic_name: TopicName::from(&topic)
        };
        self.send(packet.into()).await?;

        match self.acks.wait(msg_id).await? {
            Message::RegAck(RegAck {
                topic_id, code: ReturnCode::Accepted, ..
            }) => {
                return Ok(topic_id);
            },
            _ => Err(MqttSnClientError::AckError)
        }
    }

    pub async fn connect(&mut self) -> Result<(), Error> {
        info!("Connecting MQTT-SN");
        let packet = Connect {
            flags: Flags::default(),
            duration: 120,
            client_id: self.client_id.clone()
        };
        self.send(packet.into()).await?;
        match self.recieve().await {
            Ok(Some(Message::ConnAck(ConnAck{code: ReturnCode::Accepted}))) => Ok(()),
            _ => Err(Error::AckError)
        }
    }

    pub async fn subscribe(&mut self, topic: String<256>) -> Result<(), Error> {
        let mut flags = Flags::default();
        let topic_id;
        if let Some((topic_type, id)) = self.topics.get_by_right(&topic) {
            topic_id = *id;
            flags.set_topic_id_type(*topic_type as u8);
        } else {
            debug!("1");
            topic_id = self.register(&topic).await?;
            debug!("2");
            self.topics.insert((TopicIdType::Id, topic_id), topic.clone());
        }
        let msg_id = self.msg_id.next().unwrap();
        let packet = Subscribe {
            flags: Flags::default(),
            msg_id,
            topic: TopicNameOrId::Id(topic_id),
        };

        self.send(packet.into()).await?;

        match self.acks.wait(msg_id).await? {
            Message::SubAck(SubAck {
                code: ReturnCode::Accepted, ..
            }) => {
                return Ok(());
            },
            _ => Err(MqttSnClientError::AckError)
        }
    }

    /// If duration is set, then client will go to sleep, with keep-alive < duration
    pub async fn disconnect(&mut self, duration: Option<u16>) -> Result<(), Error> {
        let packet = Disconnect {
            duration
        };

        self.send(packet.into()).await?;

        match self.recieve().await {
            Ok(Some(Message::Disconnect(_))) => Ok(()),
            _ => Err(Error::AckError)
        }
    }
}

pub struct MqttMessage {
    topic_id: Option<u16>,
    msg_id: Option<u16>,
    qos: Option<u8>,
    pub topic: String<256>,
    pub payload: String<256>,
}

impl MqttMessage {
    pub fn new(topic: String<256>, payload: String<256>) -> Self {
        Self { topic_id: None, msg_id: None, qos: None, topic, payload }
    }
    fn from_publish(
        msg: Publish,
        topics: &BiMap<(TopicIdType, u16), String<256>>
    ) -> Result<Self, Error> {
        Ok(Self {
            topic_id: Some(msg.topic_id),
            msg_id: Some(msg.msg_id),
            qos: Some(msg.flags.qos()),
            topic: topics.get_by_left(
                &(msg.flags.topic_id_type().try_into()?, msg.topic_id)
            ).ok_or(Error::TopicNotRegistered)?.clone(),
            payload: msg.data.as_str().into(),
        })
    }
    pub fn get_ack(&self) -> Option<PubAck> {
        if let (Some(topic_id), Some(msg_id), Some(_)) = (self.topic_id, self.msg_id, self.qos) {
            return Some(PubAck {
                topic_id, msg_id,
                code: ReturnCode::Accepted
            })
        }
        None
    }
}

pub struct MsgId {
    last_id: u16
}

impl Iterator for MsgId {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        self.last_id += 1;
        Some(self.last_id)
    }
}

#[derive(Debug)]
pub enum MqttSnClientError {
    ModemError,
    SocketError,
    CodecError,
    AckError,
    UnknownError,
    ParseError,
    TopicNotRegistered,
    NoPingResponse,
}

impl From<nrf_modem::Error> for MqttSnClientError {
    fn from(_e: nrf_modem::Error) -> Self {
        MqttSnClientError::ModemError
    }
}

impl From<SocketError> for MqttSnClientError {
    fn from(_e: SocketError) -> Self {
        MqttSnClientError::SocketError
    }
}

impl From<byte::Error> for MqttSnClientError {
    fn from(_e: byte::Error) -> Self {
        MqttSnClientError::CodecError
    }
}

impl From<AckMapError> for MqttSnClientError {
    fn from(_e: AckMapError) -> Self {
        MqttSnClientError::AckError
    }
}

impl From<()> for MqttSnClientError {
    fn from(_e: ()) -> Self {
        MqttSnClientError::UnknownError
    }
}

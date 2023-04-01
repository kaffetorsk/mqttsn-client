use heapless::{String, HistoryBuffer};
use crate::socket::{SendBytes, RecieveBytes, SocketError};
use crate::ackmap::{AckMap, AckMapError};
use mqtt_sn::defs::*;
use bimap::BiMap;
use byte::{TryRead, TryWrite};

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
    // rx: fn(&mut [u8]) -> Result<(&mut [u8], SocketAddr), Error>,
    // tx: fn(&[u8]) -> Result<(), Error>,
    client_id: String<32>,
    username: Option<String<32>>,
    password: Option<String<32>>,
    msg_id: MsgId,
    send_buffer: [u8; 2048],
    recv_buffer: [u8; 2048],
    socket: S,
    topics: BiMap<(TopicIdType, u16), String<256>>,
    acks: AckMap<10>,
    rx_queue: HistoryBuffer<MqttMessage, 10>,
}

impl<S> MqttSnClient<S>
where
    S: SendBytes + RecieveBytes
{
    pub fn new(
        device_id: &String<32>,
        username: Option<String<32>>,
        password: Option<String<32>>,
        socket: S
    ) -> Result<MqttSnClient<S>, Error> {
        Ok(MqttSnClient {
            client_id: device_id.clone(),
            username,
            password,
            msg_id: MsgId {last_id: 0},
            send_buffer: [0; 2048],
            recv_buffer: [0; 2048],
            socket,
            topics: BiMap::new(),
            acks: AckMap::<10>::new(),
            rx_queue: HistoryBuffer::new(),
        })
    }

    pub async fn recieve(&mut self) -> Result<Option<Message>, Error> {
        // get acks and publish, push to ackmap and message queue
        // return other types
        // add timeout here with select?
        match Message::try_read(self.socket.recv(&mut self.recv_buffer).await?, ()) {
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
        //bruke tokio serde?
        let len = packet.try_write(&mut self.send_buffer, ())?;
        self.socket.send(&self.send_buffer[..len]).await?;
        Ok(())
    }

    async fn register(&mut self, topic: &String<256>) -> Result<u16, Error> {
        let msg_id = self.msg_id.next().unwrap();
        let packet = Register {
            topic_id: 0,
            msg_id,
            topic_name: TopicName::from(&topic)
        };
        let len = packet.try_write(&mut self.send_buffer, ())?;
        self.socket.send(&self.send_buffer[..len]).await?;
        match self.acks.wait(msg_id).await? {
            Message::RegAck(RegAck {
                topic_id, code: ReturnCode::Accepted, ..
            }) => {
                return Ok(topic_id);
            },
            _ => Err(MqttSnClientError::AckError)
        }

        // lag en no_std versjon av waitmap, lag eventloop som legger
        // inn ack der etterhvert som de blir tilgjengelige.
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

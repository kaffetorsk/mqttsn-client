use heapless::String;
use crate::socket::{SendBytes, ReceiveBytes, SocketError};
// use crate::ackmap::{AckMap, AckMapError};
use mqtt_sn::defs::*;
use byte::{TryRead, TryWrite};
use embassy_futures::select::{select, Either};
use embassy_sync::pubsub::subscriber::DynSubscriber;
use embassy_sync::pubsub::publisher::DynPublisher;
use embassy_time::{with_timeout, Duration, TimeoutError};
use crate::topics::Topics;

#[cfg(feature = "std")]
use log::*;

#[cfg(feature = "no_std")]
use defmt::*;

const T_RETRY: u8 = 10;
const N_RETRY: u8 = 10;

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
    topics: Topics,
    rx: DynSubscriber<'static, MqttMessage>,
    tx: DynPublisher<'static, MqttMessage>,
}

impl<S> MqttSnClient<S>
where
    S: SendBytes + ReceiveBytes
{
    pub fn new(
        client_id: &str,
        rx: DynSubscriber<'static, MqttMessage>,
        tx: DynPublisher<'static, MqttMessage>,
        socket: S
    ) -> Result<MqttSnClient<S>, Error> {
        Ok(MqttSnClient {
            client_id: client_id.into(),
            msg_id: MsgId {last_id: 0},
            topics: Topics::new(),
            socket, rx, tx
        })
    }

    // pub async fn run_active(
    //     &mut self,
    // ) {
    //     loop {
    //         match select(self.receive(), self.rx.next_message_pure()).await {
    //             Either::First(msg) => {
    //                 // Handle message received from the client's receive method
    //                 match msg {
    //                     Ok(Some(Message::Publish(msg))) => self.tx.publish_immediate(MqttMessage::from_publish(msg, &self.topics).unwrap()),
    //                     _ => (),
    //                 }
    //             },
    //             Either::Second(msg) => {
    //                 // Handle message received from the user (via DynSubscriber)
    //                 self.publish(msg).await.unwrap();
    //             }
    //         }
    //     }
    // }

    pub async fn run(
        &mut self,
        sleep: u16,
    ) {
        loop {
            match with_timeout(
                Duration::from_secs(sleep.into()),
                self.rx.next_message_pure()
            ).await {
                Ok(msg) => {
                    // Handle message received from the user (via DynSubscriber)
                    self.connect().await.unwrap();
                    self.publish(msg).await.unwrap();
                    self.disconnect(Some(sleep)).await.unwrap();
                },
                _ => {
                    self.ping().await.unwrap();
                }
            }
        }
    }

    pub async fn receive(&mut self) -> Result<Option<Message>, Error> {
        let mut buffer = [0u8; 1024];
        loop {
            match Message::try_read(with_timeout(Duration::from_secs(T_RETRY.into()), self.socket.recv(&mut buffer)).await??, ()) {
                Ok((Message::Publish(msg), _)) => self.recieve_publish(msg).await?,
                Ok((msg, _)) => return Ok(Some(msg)),
                _ => return Err(MqttSnClientError::AckError)
            }
        }
    }

    async fn recieve_publish(&mut self, msg: Publish) -> Result<(), Error> {
        let msg = MqttMessage::from_publish(msg, &self.topics)?;
        if let Some(ack) = msg.get_ack() {
            self.send(Message::PubAck(ack)).await?;
        }
        self.tx.publish_immediate(msg);
        Ok(())
    }

    pub async fn send(&mut self, msg: Message) -> Result<(), Error> {
        let mut buffer = [0u8; 1024];
        let len = msg.try_write(&mut buffer, ())?;
        dbg!(&buffer[..len]);
        self.socket.send(&buffer[..len]).await?;
        Ok(())
    }

    // pub async fn get_ack()

    pub async fn ping(&mut self) -> Result<(), Error>{
        self.send(PingReq {client_id: self.client_id.clone()}.into()).await?;
        match self.receive().await {
            Ok(Some(Message::PingResp(_))) => Ok(()),
            _ => Err(Error::NoPingResponse)
        }
    }

    pub async fn publish(&mut self, msg: MqttMessage) -> Result<(), Error> {
        let mut flags = Flags::default();
        let topic_id;
        if let Some((topic_type, id)) = self.topics.get_by_topic(&msg.topic) {
            topic_id = *id;
            flags.set_topic_id_type(*topic_type as u8);
        } else {
            topic_id = self.register(&msg.topic).await?;
            self.topics.insert(msg.topic, TopicIdType::Id, topic_id)?
        }
        let next_msg_id = self.msg_id.next();

        let mut data = PublishData::new();
        data.push_str(&msg.payload)?;

        let packet = Publish { flags, topic_id, msg_id: next_msg_id, data };

        self.send(packet.into()).await?;

        // Get ACK for QoS 1 & 2, retry according to protocol
        match msg.qos {
            Some(qos) if qos > 0 => {
                for _ in 1..N_RETRY {
                    match with_timeout(
                        Duration::from_secs(T_RETRY.into()),
                        self.receive()).await
                    {
                        Ok(Ok(Some(Message::PubAck(PubAck {
                            msg_id, code: ReturnCode::Accepted, ..
                        })))) if msg_id == next_msg_id => return Ok(()),
                        _ => ()
                    }
                }
                return Err(MqttSnClientError::AckError);
            },
            _ => ()
        }
        Ok(())
    }

    async fn register(&mut self, topic: &String<256>) -> Result<u16, Error> {
        let msg_id = self.msg_id.next();
        let packet = Register {
            topic_id: 0,
            msg_id,
            topic_name: TopicName::from(&topic)
        };
        self.send(packet.into()).await?;

        match self.receive().await {
            Ok(Some(Message::RegAck(RegAck {
                topic_id, code: ReturnCode::Accepted, ..
            }))) => Ok(topic_id),
            _ => Err(Error::AckError)
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
        match self.receive().await {
            Ok(Some(Message::ConnAck(ConnAck{code: ReturnCode::Accepted}))) => Ok(()),
            _ => Err(Error::AckError)
        }
    }

    pub async fn subscribe(&mut self, topic: &str) -> Result<(), Error> {
        let mut flags = Flags::default();
        let topic_id;
        let topic = String::<256>::try_from(topic)?;
        if let Some((topic_type, id)) = self.topics.get_by_topic(&topic) {
            topic_id = *id;
            flags.set_topic_id_type(*topic_type as u8);
        } else {
            debug!("1");
            topic_id = self.register(&topic).await?;
            debug!("2");
            self.topics.insert(topic, TopicIdType::Id, topic_id)?;
        }
        let msg_id = self.msg_id.next();
        let mut flags = Flags::default();
        flags.set_topic_id_type(1);
        let packet = Subscribe {
            flags,
            msg_id,
            topic: TopicNameOrId::Id(topic_id),
        };
        dbg!(&packet);

        self.send(packet.into()).await?;

        match self.receive().await? {
            Some(Message::SubAck(SubAck {
                code: ReturnCode::Accepted, ..
            })) => {
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

        match self.receive().await {
            Ok(Some(Message::Disconnect(_))) => Ok(()),
            _ => Err(Error::AckError)
        }
    }
}

#[derive(Debug, Clone)]
pub struct MqttMessage {
    topic_id: Option<u16>,
    msg_id: Option<u16>,
    qos: Option<u8>,
    pub topic: String<256>,
    pub payload: String<256>,
}

impl MqttMessage {
    pub fn new(
        topic: &str,
        payload: &str,
        qos: Option<u8>
    ) -> Result<Self, Error> {
        Ok(Self {
            topic_id: None,
            msg_id: None,
            topic: String::try_from(topic)?,
            payload: String::try_from(payload)?,
            qos
        })
    }
    fn from_publish(
        msg: Publish,
        topics: &Topics,
    ) -> Result<Self, Error> {
        Ok(Self {
            topic_id: Some(msg.topic_id),
            msg_id: Some(msg.msg_id),
            qos: Some(msg.flags.qos()),
            topic: String::try_from(topics.get_by_id(msg.topic_id)?)?,
            payload: String::try_from(msg.data.as_str())?,
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

impl MsgId {
    fn next(&mut self) -> u16 {
        self.last_id = self.last_id.wrapping_add(1);
        self.last_id
    }
}

// impl Iterator for MsgId {
//     type Item = u16;

//     fn next(&mut self) -> Option<Self::Item> {
//         self.last_id += 1;
//         Some(self.last_id)
//     }
// }

#[derive(Debug, Clone, Format)]
pub enum MqttSnClientError {
    ModemError,
    SocketError,
    CodecError,
    AckError,
    UnknownError,
    ParseError,
    TopicNotRegistered,
    TopicFailedInsert,
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

impl From<TimeoutError> for MqttSnClientError {
    fn from(_e: TimeoutError) -> Self {
        MqttSnClientError::AckError
    }
}

// impl From<AckMapError> for MqttSnClientError {
//     fn from(_e: AckMapError) -> Self {
//         MqttSnClientError::AckError
//     }
// }

impl From<()> for MqttSnClientError {
    fn from(_e: ()) -> Self {
        MqttSnClientError::UnknownError
    }
}

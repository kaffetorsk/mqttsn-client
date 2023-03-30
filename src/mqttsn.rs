use heapless::String;
use crate::socket::{SendBytes, RecieveBytes, SocketError};
use mqtt_sn::defs::*;
use bimap::BiMap;
use byte::{TryWrite};

type Error = MqttSnClientError;

#[derive(Hash, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
#[repr(u8)]
pub enum TopicIdType {
    Id,
    PreDef,
    Short
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
    topics: BiMap<(TopicIdType, u16), String<256>>
    //acks: FnvIndexMap<u16, , 10>
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
            topics: BiMap::new()
        })
    }

    pub async fn publish(&mut self, topic: String<256>, payload: String<256>) -> Result<(), Error> {
        let mut flags = Flags::default();
        let topic_id;
        if let Some((topic_type, id)) = self.topics.get_by_right(&topic) {
            topic_id = id;
            flags.set_topic_id_type(*topic_type as u8);
        } else {
            self.register()
            // Register new topic
        }



        let mut data = PublishData::new();
        data.push_str(&payload);

        let packet = Publish {
            flags,
            topic_id: *topic_id,
            msg_id: self.msg_id.next().unwrap(),
            data,

        };
        //bruke tokio serde?
        let len = packet.try_write(&mut self.send_buffer, ())?;
        self.socket.send(&self.send_buffer[..len]).await?;
        Ok(())
    }

    async fn register(&self, topic: String<256>) -> Result<u16, Error> {
        let packet = Register {
            topic_id: 0,
            msg_id: self.msg_id.next().unwrap(),
            topic_name: TopicName::from(&topic)
        };
        let len = packet.try_write(&mut self.send_buffer, ())?;
        self.socket.send(&self.send_buffer[..len]).await?;

        // lag en no_std versjon av waitmap, lag eventloop som legger
        // inn ack der etterhvert som de blir tilgjengelige.
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
    CodecError
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

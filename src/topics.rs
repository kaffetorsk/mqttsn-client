use heapless::{String, FnvIndexMap};
use crate::mqttsn::{MqttSnClientError, TopicIdType};

type Error = MqttSnClientError;

pub struct Topics {
    store: FnvIndexMap<String<256>, (TopicIdType, u16), 32>
}

impl Topics {
    pub fn new() -> Self {
        Self {
            store: FnvIndexMap::<String<256>, (TopicIdType, u16), 32>::new()
        }
    }
    pub fn insert(
        &mut self,
        topic: String<256>,
        topic_type: TopicIdType,
        id: u16
    ) -> Result<(), Error> {
        match self.get_by_id(id) {
            Ok(topic) => {self.store.remove(&String::try_from(topic)?);},
            _ => ()
        }
        self.store.insert(topic, (topic_type, id)).map_err(|_|Error::TopicFailedInsert)?;
        Ok(())
    }
    pub fn get_by_topic(&self, topic: &str) -> Option<&(TopicIdType, u16)> {
        match String::try_from(topic) {
            Ok(topic) => self.store.get(&topic),
            _ => None
        }
    }
    pub fn get_by_id(&self, id: u16) -> Result<&str, Error> {
        if let Some((topic, _)) = self.store.iter().filter(|(_, (_, i))| i.clone() == id).next() {
            return Ok(topic);
        } else {
            return Err(Error::TopicNotRegistered);
        }
    }
}
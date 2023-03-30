use mqtt_sn::defs::Message;
use heapless::FnvIndexMap;
use core::task::Waker;

pub struct AckMap<const C: usize> {
    acks: FnvIndexMap<u16, AckEntry, C>
}

type Error = AckMapError;

impl<const C: usize> AckMap<C> {
    pub fn insert(&mut self, key: u16, value: Message) -> Result<(), Error> {
        if self.acks.contains_key(&key) {
            // TODO
        } else {
            self.acks.insert(key, AckEntry::new(value))?;
        }
        Ok(())
    }

    pub fn wait(&self, key: u16) -> impl Future<_> {
        // TODO
    }
}

struct AckEntry {
    value: Option<Message>,
    waker: Option<Waker>
}

impl AckEntry {
    pub fn new(msg: Message) -> Self {
        Self {value: Some(msg), waker: None}
    }
}
pub enum AckMapError {
    Full,
    Generic,
}

impl<K, V> From<(K, V)> for AckMapError {
    fn from(_e: (K, V)) -> Self {
        AckMapError::Generic
    }
}

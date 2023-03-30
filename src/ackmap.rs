use mqtt_sn::defs::Message;
use heapless::FnvIndexMap;
use core::task::Waker;

pub struct AckMap<const C: usize> {
    acks: FnvIndexMap<u16, AckEntry, C>
}

type Error = AckMapError;

impl<const C: usize> AckMap<C> {
    pub fn new() -> Self {
        Self { acks: FnvIndexMap::<u16, AckEntry, C>::new() }
    }

    pub fn insert(&mut self, key: u16, value: Message) -> Result<(), Error> {
        match self.acks.get(&key) {
            Some(AckEntry::Value(msg)) => (),
            Some(AckEntry::Waker(waker)) => (),
            None => { self.acks.insert(key, AckEntry::Value(value))?; }
        }
        Ok(())
    }

    // pub fn wait(&self, key: u16) -> impl Future<_> {
    //     // TODO
    // }
}

enum AckEntry {
    Value(Message),
    Waker(Waker),
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

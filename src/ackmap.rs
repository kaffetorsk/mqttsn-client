use mqtt_sn::defs::Message;
use heapless::FnvIndexMap;
use core::task::Waker;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

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
            Some(AckEntry::Waker(waker)) => (), // wake task
            None => { self.acks.insert(key, AckEntry::Value(value))?; }
        }
        Ok(())
    }

    pub fn wait(&self, key: u16) -> impl Future<Output = Result<Message, Error>>  + '_ {
        Ack { key, acks: &self.acks }
    }
}

enum AckEntry {
    Value(Message),
    Waker(Waker),
}

pub struct Ack<'a, const C: usize> {
    key: u16,
    acks: &'a FnvIndexMap<u16, AckEntry, C>
}

impl<const C: usize> Future for Ack<'_, C> {
    type Output = Result<Message, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.acks.get(&self.key) {
            Some(AckEntry::Value(msg)) => Poll::Ready(Ok(*msg)),
            _ => {
                self.acks.insert(self.key, AckEntry::Waker(cx.waker().clone()));
                Poll::Pending
            }
        }
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

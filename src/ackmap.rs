use mqtt_sn::defs::Message;
use heapless::FnvIndexMap;
use core::task::Waker;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use embassy_sync::blocking_mutex::{Mutex, raw::NoopRawMutex};

pub struct AckMap<const C: usize> {
    acks: Mutex<NoopRawMutex, FnvIndexMap<u16, AckEntry, C>>
}

type Error = AckMapError;

impl<const C: usize> AckMap<C> {
    pub fn new() -> Self {
        Self { acks: Mutex::new(FnvIndexMap::<u16, AckEntry, C>::new()) }
    }

    pub async fn insert(&mut self, key: u16, value: Message) -> Result<(), Error> {
        let mut acks = self.acks.get_mut();
        match acks.get(&key) {
            Some(AckEntry::Value(msg)) => return Err(AckMapError::IdUsed),
            Some(AckEntry::Waker(waker)) => {
                acks.insert(key, AckEntry::Value(value))?;
                // Må muligens clone denne før jeg inserter?
                waker.clone().wake();
            },
            None => { acks.insert(key, AckEntry::Value(value))?; }
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
    acks: &'a Mutex<NoopRawMutex, FnvIndexMap<u16, AckEntry, C>>
}

impl<const C: usize> Future for Ack<'_, C> {
    type Output = Result<Message, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut acks = self.acks.get_mut();
        match acks.get(&self.key) {
            Some(AckEntry::Value(msg)) => {
                acks.remove(&self.key);
                Poll::Ready(Ok(*msg))
            },
            _ => {
                acks.insert(self.key, AckEntry::Waker(cx.waker().clone()));
                Poll::Pending
            }
        }
    }
}

pub enum AckMapError {
    Full,
    Generic,
    IdUsed
}

impl<K, V> From<(K, V)> for AckMapError {
    fn from(_e: (K, V)) -> Self {
        AckMapError::Generic
    }
}

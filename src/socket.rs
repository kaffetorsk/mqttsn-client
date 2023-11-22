#[cfg(feature = "std")]
use tokio::net::UdpSocket;

#[derive(Debug)]
pub enum SocketError {
    Generic,
}

#[cfg(feature = "std")]
impl From<std::io::Error> for SocketError {
    fn from(_e: std::io::Error) -> SocketError {
        SocketError::Generic
    }
}

pub trait SendBytes {
    async fn send(&mut self, buf: &[u8]) -> Result<(), SocketError>;
}

pub trait ReceiveBytes {
    async fn recv<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a mut [u8], SocketError>;
}


#[cfg(feature = "std")]
pub struct TokioUdp(pub UdpSocket);

#[cfg(feature = "std")]
impl SendBytes for TokioUdp {
    async fn send(&mut self, buf: &[u8]) -> Result<(), SocketError> {
        self.0.send(buf).await?;
        Ok(())
    }
}

#[cfg(feature = "std")]
impl ReceiveBytes for TokioUdp {
    async fn recv<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a mut [u8], SocketError> {
        Ok(self.0.recv(buf).await.map(|len| &mut buf[..len])?)
    }
}

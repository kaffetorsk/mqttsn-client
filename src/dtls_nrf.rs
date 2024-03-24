use nrf_modem::DtlsSocket;
use crate::socket::{SocketError, SendBytes, ReceiveBytes};

impl From<nrf_modem::Error> for SocketError {
    fn from(_e: nrf_modem::Error) -> SocketError {
        SocketError::Generic
    }
}
pub struct DtlsSession(DtlsSocket);

impl DtlsSession {
    pub fn new(socket: DtlsSocket) -> Self {
        DtlsSession(socket)
    }
}

impl SendBytes for DtlsSession {
    async fn send(&mut self, buf: &[u8]) -> Result<(), SocketError> {
        self.0.send(buf).await?;
        Ok(())
    }
}

impl ReceiveBytes for DtlsSession {
    async fn recv<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a mut [u8], SocketError> {
        Ok(self.0.receive_from(buf).await?.0)
    }
}
use tokio::net::UdpSocket;
use tokio_dtls_stream_sink::{Client, Session};
use openssl::ssl::{SslContext, SslMethod, SslRef};
use openssl::error::ErrorStack;
use openssl_errors::{openssl_errors, put_error};
use log::*;
use std::error;
use std::net::ToSocketAddrs;
use serde_yaml::Value;
use crate::socket::{SocketError, SendBytes, ReceiveBytes};
use std::ffi::CString;

openssl_errors! {
    pub library DtlsErr("DTLS errors") {
        functions {
            FIND_PRIVATE_KEY("find_private_key");
        }

        reasons {
            FILE_NOT_READ("Could not read file");
            BAD_PASSWORD("invalid private key password");
            NOT_FOUND("client not found");
            ID_NOT_VALID("Not valid client id");
        }
    }
}

fn get_server_psk(
    ssl: &mut SslRef,
    id_hint: Option<&[u8]>,
    client_id: &mut [u8],
    psk: &mut [u8]
) -> Result<usize, ErrorStack> {
    trace!("SSL PSK for: {:#?} {:#?} ", &id_hint, &ssl);

    let f = std::fs::File::open("key.yml").map_err(|_| {
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::FILE_NOT_READ);
        trace!("SSL PSK file not found ");
        ErrorStack::get()
        })?;

    let key: Value = serde_yaml::from_reader(f).map_err(|_| {
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::FILE_NOT_READ);
        trace!("SSL PSK file not valid yaml");
        ErrorStack::get()
        })?;

    let key = key.as_mapping().ok_or_else(|| {
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::FILE_NOT_READ);
        trace!("SSL PSK file not valid ");
        ErrorStack::get()
        })?;

    let (id, key) = key.iter().next().ok_or_else(|| {
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::NOT_FOUND);
        trace!("SSL PSK file not valid");
        ErrorStack::get()
        })?;

    let id = id.as_str().ok_or_else(|| {
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::BAD_PASSWORD);
        trace!("SSL PSK invalid id: {:#?} ", &id);
        ErrorStack::get()
        })?;

    let key = key.as_str().ok_or_else(|| {
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::BAD_PASSWORD);
        trace!("SSL PSK invalid key: {:#?} ", &key);
        ErrorStack::get()
        })?;

    let id = CString::new(id)
        .map_err(|_| {
            put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::BAD_PASSWORD);
            trace!("SSL PSK invalid id: {:#?} ", &id);
            ErrorStack::get()
        })?.into_bytes_with_nul();

    let key = hex::decode(key)
        .map_err(|_| {
            put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::BAD_PASSWORD);
            trace!("SSL PSK invalid key");
            ErrorStack::get()
        })?;

    for (i, b) in id.iter().enumerate() {
        client_id[i] = *b;
    }

    let len = key.len();
    for (i, b) in key.iter().enumerate() {
        psk[i] = *b;
    }

    Ok(len)
}

pub struct DtlsSocket {
    client: Client,
    context: SslContext,
}

impl DtlsSocket {
    pub async fn new() -> Result<Self, Box<dyn error::Error>> {

        let sock = UdpSocket::bind("0.0.0.0:0").await?;
        let client = Client::new(sock);
        let mut context = SslContext::builder(SslMethod::dtls())?;
        context.set_psk_client_callback(get_server_psk);
        let context = context.build();

        Ok(Self {
            client,
            context
        })
    }

    pub async fn connect(&self,
            addr: impl ToSocketAddrs,
        ) -> Result<DtlsSession, Box<dyn error::Error>> {
        info!("Connecting DTLS");
        Ok(DtlsSession(self.client.connect(addr, Some(self.context.clone())).await?))
    }
}

pub struct DtlsSession(Session);

impl SendBytes for DtlsSession {
    async fn send(&mut self, buf: &[u8]) -> Result<(), SocketError> {
        self.0.write(buf).await?;
        Ok(())
    }
}

impl ReceiveBytes for DtlsSession {
    async fn recv<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a mut [u8], SocketError> {
        Ok(self.0.read(buf).await.map(|len| &mut buf[..len])?)
    }
}

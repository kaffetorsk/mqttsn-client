use tokio::net::UdpSocket;
use tokio_dtls_stream_sink::{Server, Session};
use openssl::ssl::{SslContext, SslMethod, SslRef};
use openssl::error::ErrorStack;
use openssl_errors::{openssl_errors, put_error};
use std::io::prelude::*;
use log::*;
use std::error;
use tokio::net::ToSocketAddrs;
use futures::stream::{Stream, unfold};
use serde_yaml::Value;

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

fn get_client_psk(ssl: &mut SslRef, id: Option<&[u8]>, mut psk: &mut [u8]) -> Result<usize, ErrorStack> {
    trace!("SSL PSK from: {:#?} {:#?} ", &id, &ssl);
    let id = {
        match id {
            Some(i) => std::str::from_utf8(&i).ok(),
            None => None
        }
    }.ok_or({
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::ID_NOT_VALID);
        trace!("SSL PSK invalid id: {:#?} ", &id);
        ErrorStack::get()
    })?;

    let f = std::fs::File::open("clients.yml").map_err(|_| {
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::FILE_NOT_READ);
        trace!("SSL PSK file not found ");
        ErrorStack::get()
        })?;

    let clients: Value = serde_yaml::from_reader(f).map_err(|_| {
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::FILE_NOT_READ);
        trace!("SSL PSK file not valid yaml");
        ErrorStack::get()
        })?;

    let key = {
            clients.get(id)
        }.ok_or({
            put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::NOT_FOUND);
            trace!("SSL PSK id not in file: {:#?} ", &id);
            ErrorStack::get()
        })?.as_str().ok_or({
            put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::BAD_PASSWORD);
            trace!("SSL PSK invalid key for id: {:#?} ", &id);
            ErrorStack::get()
        })?;

    let key = hex::decode(key)
        .map_err(|_| {
            put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::BAD_PASSWORD);
            trace!("SSL PSK invalid key for id: {:#?} ", &id);
            ErrorStack::get()
        })?;

    let len = psk.write(&key).map_err(|_| {
        put_error!(DtlsErr::FIND_PRIVATE_KEY, DtlsErr::BAD_PASSWORD);
        trace!("SSL PSK invalid key for id: {:#?} ", &id);
        ErrorStack::get()
        })?;

    info!("New connection from: {:#?}", id);
    Ok(len)
}

pub struct DtlsSocket {
    server: Server,
    ssl_cxt: SslContext
}

impl DtlsSocket {
    pub async fn new(
            addr: impl ToSocketAddrs,
        ) -> Result<Self, Box<dyn error::Error>> {

        let sock = UdpSocket::bind(addr).await?;
        let mut context = SslContext::builder(SslMethod::dtls())?;
        context.set_psk_server_callback(get_client_psk);

        Ok(DtlsSocket {
            server: Server::new(sock),
            ssl_cxt: context.build(),
        })
    }

    pub fn as_stream<'a>(&'a mut self) -> impl Stream<Item = Session> + 'a {
        unfold(&mut self.server, |serv| async {
            let session;
            loop {
                if let Ok(s) = serv.accept(Some(&self.ssl_cxt)).await {
                    session = s;
                    break;
                }
            }
            Some((session, serv))
        })
    }
}

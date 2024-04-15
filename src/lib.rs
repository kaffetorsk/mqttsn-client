#![cfg_attr(feature = "no_std", no_std)]
// #![feature(async_fn_in_trait)]
#![allow(incomplete_features)]
#![allow(async_fn_in_trait)]

pub mod mqttsn;
pub mod socket;
pub mod topics;
// pub(crate) mod ackmap;

#[cfg(not(feature = "no_std"))]
pub mod dtls_std;

#[cfg(feature = "no_std")]
pub mod dtls_nrf;


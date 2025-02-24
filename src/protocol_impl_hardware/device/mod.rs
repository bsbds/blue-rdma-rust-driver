#![allow(missing_docs, clippy::missing_docs_in_private_items)]
#![allow(clippy::todo)] // FIXME: implement
#![allow(clippy::missing_errors_doc)] // FIXME: add error docs

/// Hardware device adaptor
pub(crate) mod hardware;

/// Emulated device adaptor
pub(crate) mod emulated;

/// Dummy device adaptor for testing
pub(crate) mod dummy;

/// CSR proxy types
pub(crate) mod proxy;

/// Adaptors
pub(crate) mod adaptor;

/// Device mode reader
pub(crate) mod mode;

pub(crate) mod ops_impl;

pub(crate) mod ffi_impl;

mod config;

pub(crate) use adaptor::*;

/// Memory-mapped I/O addresses of device registers
mod constants;

const CARD_MAC_ADDRESS: u64 = 0xAABB_CCDD_EE0A;
const CARD_IP_ADDRESS: u32 = 0x1122_330A;

#![allow(missing_docs, clippy::missing_docs_in_private_items)]

/// Emulated device adaptor
pub(crate) mod emulated;

mod constants;

use std::io;

/// An adaptor to read the tail pointer and write the head pointer, using by writer.
pub(crate) trait CsrWriterAdaptor {
    fn write_head(&self, data: u32) -> io::Result<()>;
    fn read_tail(&self) -> io::Result<u32>;
}

/// An adaptor to read the head pointer and write the tail pointer, using by reader.
pub(crate) trait CsrReaderAdaptor {
    fn write_tail(&self, data: u32) -> io::Result<()>;
    fn read_head(&self) -> io::Result<u32>;
}

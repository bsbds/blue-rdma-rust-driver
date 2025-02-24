use std::{collections::HashMap, io, sync::Arc};

use parking_lot::Mutex;

use super::DeviceAdaptor;

#[derive(Default, Clone, Debug)]
pub(crate) struct DummyDevice(Arc<Mutex<HashMap<usize, u32>>>);

impl DeviceAdaptor for DummyDevice {
    fn read_csr(&self, addr: usize) -> io::Result<u32> {
        Ok(self.0.lock().get(&addr).copied().unwrap_or(0))
    }

    fn write_csr(&self, addr: usize, data: u32) -> io::Result<()> {
        let _ignore = self.0.lock().insert(addr, data);
        Ok(())
    }
}

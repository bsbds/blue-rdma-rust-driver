/// Second stage mtt allocator
mod pgt_alloc;

/// First stage mtt allocator
mod mr_alloc;

use std::{
    io, iter,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};

use mr_alloc::MrTableAlloc;
use parking_lot::Mutex;
use pgt_alloc::{simple::SimplePgtAlloc, PgtAlloc};

use crate::{
    desc::{
        cmd::{CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT},
        RingBufDescUntyped,
    },
    device::DeviceAdaptor,
    mem::{
        page::ContiguousPages,
        virt_to_phy::{virt_to_phy, virt_to_phy_range},
        PAGE_SIZE,
    },
    queue::cmd_queue::{
        worker::{CmdId, Registration},
        CmdQueue, CmdQueueDesc,
    },
};

/// Memory region key
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MrKey(u32);

/// RDMA memory region representation
pub(crate) struct IbvMr {
    /// Virtual address of the memory region
    addr: u64,
    /// Length of the memory region in bytes
    length: u32,
    /// Access permissions for the memory region
    access: u32,
    /// Memory region key
    mr_key: MrKey,
    /// Index in the page table
    index: usize,
}

impl IbvMr {
    /// Creates a new `IbvMr`
    pub(crate) fn new(addr: u64, length: u32, access: u32, mr_key: MrKey, index: usize) -> Self {
        Self {
            addr,
            length,
            access,
            mr_key,
            index,
        }
    }
}

/// Memory Translation Table implementation
struct Mtt<PAlloc, Dev> {
    /// Table memory allocator
    alloc: Arc<Mutex<Alloc<PAlloc>>>,
    /// Command queue for submitting commands to device
    cmd_queue: Arc<Mutex<CmdQueue<Dev>>>,
    /// Registration for getting notifies from the device
    reg: Arc<Mutex<Registration>>,
    /// Command ID generator
    cmd_id: AtomicU16,
}

impl<PAlloc, Dev> Mtt<PAlloc, Dev>
where
    PAlloc: PgtAlloc,
    Dev: DeviceAdaptor,
{
    /// Creates a new `Mtt`
    fn new(
        alloc: Arc<Mutex<Alloc<PAlloc>>>,
        cmd_queue: Arc<Mutex<CmdQueue<Dev>>>,
        reg: Arc<Mutex<Registration>>,
    ) -> Self {
        Self {
            alloc,
            cmd_queue,
            reg,
            cmd_id: AtomicU16::new(0),
        }
    }

    /// Registers a memory region
    #[allow(clippy::as_conversions)]
    pub(crate) fn reg_mr(&self, addr: u64, length: usize) -> io::Result<IbvMr> {
        Self::ensure_valid(addr, length)?;
        Self::try_pin_pages(addr, length)?;
        let num_pages = Self::get_num_page(addr, length);
        let virt_addrs = Self::get_page_start_virt_addrs(addr, length)
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        let phy_addrs = virt_to_phy_range(addr, num_pages)?;
        if phy_addrs.iter().any(Option::is_none) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "physical address not found",
            ));
        }
        let (mut page, page_start_phy_addr) = Self::alloc_new_page()?;
        Self::copy_phy_addrs_to_page(phy_addrs.into_iter().flatten(), &mut page)?;
        let (mr_key, index) = self
            .alloc
            .lock()
            .alloc(num_pages)
            .ok_or(io::Error::from(io::ErrorKind::OutOfMemory))?;

        let index_u32 = u32::try_from(index)
            .unwrap_or_else(|_| unreachable!("allocator should not alloc index larger than u32"));
        let length_u32 =
            u32::try_from(length).map_err(|_err| io::Error::from(io::ErrorKind::InvalidInput))?;
        let update_mr_table_id = self.new_cmd_id();
        let update_pgt_id = self.new_cmd_id();
        let entry_count = u32::try_from(num_pages.saturating_sub(1))
            .map_err(|_err| io::Error::from(io::ErrorKind::InvalidInput))?;

        // TODO: pd_handler and acc_flags
        let update_mr_table = CmdQueueReqDescUpdateMrTable::new(
            update_mr_table_id,
            addr,
            length_u32,
            mr_key.0,
            0,
            0,
            index_u32,
        );
        let update_pgt = CmdQueueReqDescUpdatePGT::new(
            update_pgt_id,
            page_start_phy_addr,
            index_u32,
            entry_count,
        );

        let (notify_update_mr_table, notify_update_pgt) = {
            let mut reg_l = self.reg.lock();
            let a = reg_l
                .register(CmdId(update_mr_table_id))
                .unwrap_or_else(|| unreachable!("id should not be registered"));
            let b = reg_l
                .register(CmdId(update_pgt_id))
                .unwrap_or_else(|| unreachable!("id should not be registered"));
            (a, b)
        };

        {
            let mut cmd_queue_l = self.cmd_queue.lock();
            cmd_queue_l.push(CmdQueueDesc::UpdateMrTable(update_mr_table));
            cmd_queue_l.push(CmdQueueDesc::UpdatePGT(update_pgt));
        }

        loop {
            if notify_update_mr_table.notified() && notify_update_pgt.notified() {
                break;
            }
        }

        Ok(IbvMr::new(addr, length_u32, 0, mr_key, index))
    }

    /// Deregisters a memory region
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the memory region was successfully deregistered
    /// - `Ok(false)` if the memory region was not found
    /// - `Err` if unpinning pages failed
    #[allow(clippy::as_conversions)] // convert u32 to usize is safe
    pub(crate) fn dereg_mr(&self, mr: &IbvMr) -> io::Result<bool> {
        if !self.alloc.lock().dealloc(mr) {
            return Ok(false);
        }
        let update_mr_table_id = self.new_cmd_id();
        let update_mr_table =
            CmdQueueReqDescUpdateMrTable::new(update_mr_table_id, 0, 0, mr.mr_key.0, 0, 0, 0);
        let notify_update_mr_table = self
            .reg
            .lock()
            .register(CmdId(update_mr_table_id))
            .unwrap_or_else(|| unreachable!("id should not be registered"));
        self.cmd_queue
            .lock()
            .push(CmdQueueDesc::UpdateMrTable(update_mr_table));

        loop {
            if notify_update_mr_table.notified() {
                break;
            }
        }

        Self::try_unpin_pages(mr.addr, mr.length as usize)?;

        Ok(true)
    }

    /// Generates a new command ID
    fn new_cmd_id(&self) -> u16 {
        self.cmd_id.fetch_add(1, Ordering::Relaxed)
    }

    // TODO: reuse a page for multiple registration
    /// Allocates a new page and returns a tuple containing the page and its physical address
    fn alloc_new_page() -> io::Result<(ContiguousPages<1>, u64)> {
        let mut page = ContiguousPages::new()?;
        let start_virt_addr = page.as_ptr();
        let start_phy_addr = virt_to_phy(Some(start_virt_addr))?
            .into_iter()
            .flatten()
            .next()
            .ok_or(io::Error::new(
                io::ErrorKind::NotFound,
                "physical address not found",
            ))?;
        Ok((page, start_phy_addr))
    }

    /// Pins pages in memory to prevent swapping
    ///
    /// # Errors
    ///
    /// Returns an error if the pages could not be locked in memory
    #[allow(unsafe_code, clippy::as_conversions)]
    fn try_pin_pages(addr: u64, length: usize) -> io::Result<()> {
        let result = unsafe { libc::mlock(addr as *const std::ffi::c_void, length) };
        if result != 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "failed to lock pages"));
        }
        Ok(())
    }

    /// Unpins pages
    ///
    /// # Errors
    ///
    /// Returns an error if the pages could not be locked in memory
    #[allow(unsafe_code, clippy::as_conversions)]
    fn try_unpin_pages(addr: u64, length: usize) -> io::Result<()> {
        let result = unsafe { libc::munlock(addr as *const std::ffi::c_void, length) };
        if result != 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to unlock pages",
            ));
        }
        Ok(())
    }

    /// Validates memory region parameters
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` error if:
    /// - The address + length would overflow u64
    /// - The length is larger than `u32::MAX`
    /// - The length is 0
    #[allow(clippy::arithmetic_side_effects, clippy::as_conversions)]
    fn ensure_valid(addr: u64, length: usize) -> io::Result<()> {
        if u64::MAX - addr < length as u64 || length > u32::MAX as usize || length == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }

    /// Calculates number of pages needed for memory region
    #[allow(clippy::arithmetic_side_effects)]
    fn get_num_page(addr: u64, length: usize) -> usize {
        let num = length / PAGE_SIZE;
        if length % PAGE_SIZE != 0 {
            num + 1
        } else {
            num
        }
    }

    /// Gets starting virtual addresses for each page in memory region
    ///
    /// # Returns
    ///
    /// * `Some(Vec<u64>)` - Vector of page-aligned virtual addresses
    /// * `None` - If addr + length would overflow
    #[allow(clippy::as_conversions)]
    fn get_page_start_virt_addrs(addr: u64, length: usize) -> Option<Vec<u64>> {
        addr.checked_add(length as u64)
            .map(|end| (addr..end).step_by(PAGE_SIZE).collect())
    }

    /// Copies physical addresses into a page.
    ///
    /// # Errors
    ///
    /// Returns an error if the page is too small to hold all addresses.
    fn copy_phy_addrs_to_page<Addrs: IntoIterator<Item = u64>>(
        phy_addrs: Addrs,
        page: &mut ContiguousPages<1>,
    ) -> io::Result<()> {
        let bytes: Vec<u8> = phy_addrs.into_iter().flat_map(u64::to_ne_bytes).collect();
        page.get_mut(..bytes.len())
            .ok_or(io::Error::from(io::ErrorKind::OutOfMemory))?
            .copy_from_slice(&bytes);

        Ok(())
    }
}

/// Table memory allocator for MTT
struct Alloc<PAlloc> {
    /// First stage table allocator
    mr: MrTableAlloc,
    /// Second stage table allocator
    pgt: PAlloc,
}

impl<PAlloc> Alloc<PAlloc> {
    /// Creates a new allocator instance
    fn new(pgt: PAlloc) -> Self {
        Self {
            mr: MrTableAlloc::new(),
            pgt,
        }
    }
}

impl Alloc<SimplePgtAlloc> {
    /// Creates a new allocator with simple page table allocator
    fn new_simple() -> Self {
        Self {
            mr: MrTableAlloc::new(),
            pgt: SimplePgtAlloc::new(),
        }
    }
}

impl<PAlloc> Alloc<PAlloc>
where
    PAlloc: PgtAlloc,
{
    /// Allocates memory region and page table entries
    ///
    /// # Returns
    ///
    /// * `Some((mr_key, page_index))`
    /// * `None` - If allocation fails
    fn alloc(&mut self, num_pages: usize) -> Option<(MrKey, usize)> {
        let mr_key = self.mr.alloc_mr_key()?;
        let index = self.pgt.alloc(num_pages)?;
        Some((mr_key, index))
    }

    /// Deallocates memory region and page table entries
    ///
    /// # Returns
    ///
    /// `true` if deallocation is successful, `false` otherwise
    #[allow(clippy::as_conversions)]
    fn dealloc(&mut self, mr: &IbvMr) -> bool {
        self.mr.dealloc_mr_key(mr.mr_key);
        self.pgt.dealloc(mr.index, mr.length as usize)
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::AtomicBool;

    use crate::{device::dummy::DummyDevice, ringbuffer::new_test_ring};

    use super::*;

    #[test]
    fn mtt_mr_reg_dereg_ok() {
        let alloc = Arc::new(Mutex::new(Alloc::new_simple()));
        let ring = new_test_ring::<RingBufDescUntyped>();
        let mut queue = Arc::new(Mutex::new(CmdQueue::new(DummyDevice::default()).unwrap()));
        let mut reg = Arc::new(Mutex::new(Registration::new()));
        let reg_c = Arc::clone(&reg);
        let mtt = Mtt::new(alloc, queue, reg);

        let page = ContiguousPages::<1>::new().unwrap();
        let vec0 = vec![0; 128];
        let vec1 = vec![0; 0x10000];

        let abort = Arc::new(AtomicBool::new(true));
        let abort_c = Arc::clone(&abort);
        let handle = std::thread::spawn(move || {
            while abort.load(Ordering::Relaxed) {
                reg_c.lock().notify_all();
            }
        });

        let mr0 = mtt.reg_mr(page.as_ptr() as u64, page.len()).unwrap();
        let mr1 = mtt.reg_mr(vec0.as_ptr() as u64, vec0.len()).unwrap();
        let mr2 = mtt.reg_mr(vec1.as_ptr() as u64, vec1.len()).unwrap();

        assert!(mtt.dereg_mr(&mr0).unwrap());
        assert!(mtt.dereg_mr(&mr1).unwrap());
        assert!(mtt.dereg_mr(&mr2).unwrap());

        abort_c.store(false, Ordering::Relaxed);
        handle.join();
    }
}

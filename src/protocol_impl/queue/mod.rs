/// Command queue implementation
pub(crate) mod cmd_queue;

/// Simple NIC tx queue implementation
pub(crate) mod simple_nic;

/// Send queue implementation
pub(crate) mod send_queue;

/// Meta report queue implementation
pub(crate) mod meta_report_queue;

/// Queue allocator implementation
pub(crate) mod alloc;

pub(crate) use alloc::DescRingBuffer;

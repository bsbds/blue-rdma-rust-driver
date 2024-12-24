use crate::ring::{Descriptor, Ring, SyncDevice};

/// Strategy for controlling ring buffer operations.
/// Determines actions based on current context.
trait Strategy {
    /// Updates strategy state and returns next action to take
    fn update(&mut self, ctx: Context) -> Action;
}

/// Worker that manages a ring buffer and descriptor injection
struct RingWorker<Buf, Dev, Desc> {
    /// The underlying ring buffer
    queue: Ring<Buf, Dev, Desc>,
}

/// Context information passed to strategy updates
struct Context {
    /// Number of descriptors processed in current iteration
    num_desc: usize,
}

impl Context {
    /// Creates a new `Context`
    fn new(num_desc: usize) -> Self {
        Self { num_desc }
    }
}

/// Actions that can be taken after a strategy update
enum Action {
    /// Flush the ring buffer
    Flush,
    /// Take no action
    Nothing,
}

/// Run the consumer worker
// TODO: Breaks the loop when shutdown.
fn run_consume<Buf, Dev, Desc, Strat>(mut worker: RingWorker<Buf, Dev, Desc>, mut strategy: Strat)
where
    Buf: AsMut<[Desc]>,
    Dev: SyncDevice,
    Desc: Descriptor,
    Strat: Strategy,
{
    loop {
        let mut num_desc = 0;
        if let Some(desc) = worker.queue.try_consume() {
            num_desc = 1;
            // process desc
        }
        let ctx = Context::new(num_desc);
        match strategy.update(ctx) {
            Action::Flush => worker.queue.flush_consume(),
            Action::Nothing => {}
        }
    }
}

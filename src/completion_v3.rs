use std::{collections::VecDeque, iter, ops::ControlFlow, sync::Arc};

use bitvec::vec::BitVec;
use parking_lot::Mutex;

use crate::{
    ack_responder::AckResponse, constants::MAX_CQ_CNT, qp_table::QpTable,
    queue_pair::QueuePairAttrTable, send_queue::Psn, tracker::Msn,
};

struct EventRegister {
    message_id: MessageIdentifier,
    event: Event,
}

struct Message {
    id: MessageIdentifier,
    meta: MessageMeta,
}

struct MessageIdentifier {
    qpn: u32,
    msn: u16,
    is_send: bool,
}

struct CompletionQueueRegistry {
    inner: VecDeque<EventRegister>,
}

impl CompletionQueueRegistry {
    fn push(&mut self, register: EventRegister) {
        self.inner.push_back(register);
    }

    fn pop() -> Option<EventRegister> {
        None
    }
}

#[derive(Debug)]
#[allow(variant_size_differences)]
pub(crate) enum CompletionTask {
    Register {
        qpn: u32,
        event: Event,
    },
    Ack {
        qpn: u32,
        base_psn: u32,
        is_send: bool,
    },
}

pub(crate) struct CompletionWorker {
    completion_rx: flume::Receiver<CompletionTask>,
    tracker_table: QpTable<QueuePairMessageTracker>,
    cq_table: CompletionQueueTable,
    qp_table: QueuePairAttrTable,
    ack_resp_tx: flume::Sender<AckResponse>,
}

impl CompletionWorker {
    pub(crate) fn new(
        completion_rx: flume::Receiver<CompletionTask>,
        cq_table: CompletionQueueTable,
        qp_table: QueuePairAttrTable,
        ack_resp_tx: flume::Sender<AckResponse>,
    ) -> Self {
        Self {
            completion_rx,
            tracker_table: QpTable::new(),
            cq_table,
            qp_table,
            ack_resp_tx,
        }
    }

    pub(crate) fn spawn(self) {
        let _handle = std::thread::Builder::new()
            .name("completion-worker".into())
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    fn run(mut self) {
        while let Ok(x) = self.completion_rx.recv() {
            let qpn = match x {
                CompletionTask::Register { qpn, .. } | CompletionTask::Ack { qpn, .. } => qpn,
            };
            let Some(tracker) = self.tracker_table.get_qp_mut(qpn) else {
                continue;
            };
            let Some(qp_attr) = self.qp_table.get(qpn) else {
                continue;
            };
            match x {
                CompletionTask::Register { event, .. } => {
                    tracker.append(event);
                }
                CompletionTask::Ack {
                    base_psn,
                    is_send: true,
                    ..
                } => {
                    if let Some(send_cq) = qp_attr.send_cq.and_then(|h| self.cq_table.get_cq(h)) {
                        tracker.ack_send(Some(base_psn), send_cq);
                    }
                }
                CompletionTask::Ack {
                    base_psn,
                    is_send: false,
                    ..
                } => {
                    let send_cq = qp_attr.send_cq.and_then(|h| self.cq_table.get_cq(h));
                    if let Some(recv_cq) = qp_attr.recv_cq.and_then(|h| self.cq_table.get_cq(h)) {
                        tracker.ack_recv(base_psn, recv_cq, send_cq, qpn, &self.ack_resp_tx);
                    }
                }
            }
        }
    }
}

pub(crate) struct EventWithQpn {
    qpn: u32,
    event: Event,
}

impl EventWithQpn {
    pub(crate) fn new(qpn: u32, event: Event) -> Self {
        Self { qpn, event }
    }
}

#[derive(Default)]
struct QueuePairMessageTracker {
    send: MessageTracker<SendEvent>,
    recv: MessageTracker<RecvEvent>,
    read_resp_queue: VecDeque<RecvEvent>,
    post_recv_queue: VecDeque<PostRecvEvent>,
}

impl QueuePairMessageTracker {
    fn new(
        send: MessageTracker<SendEvent>,
        recv: MessageTracker<RecvEvent>,
        read_resp_queue: VecDeque<RecvEvent>,
        post_recv_queue: VecDeque<PostRecvEvent>,
    ) -> Self {
        Self {
            send,
            recv,
            read_resp_queue,
            post_recv_queue,
        }
    }

    fn append(&mut self, event: Event) {
        match event {
            Event::Send(x) => self.send.append(x),
            Event::Recv(x) => self.recv.append(x),
            Event::PostRecv(x) => {
                self.post_recv_queue.push_back(x);
            }
        }
    }

    fn ack_send(&mut self, psn: Option<u32>, send_cq: &CompletionQueue) {
        if let Some(psn) = psn {
            self.send.ack(psn);
        }
        while let Some(event) = self.send.peek() {
            match event.op {
                SendEventOp::WriteSignaled | SendEventOp::SendSignaled => {
                    let x = self.send.pop().unwrap_or_else(|| unreachable!());
                    let completion = match x.op {
                        SendEventOp::WriteSignaled => Completion::RdmaWrite { wr_id: x.wr_id },
                        SendEventOp::SendSignaled => Completion::Send { wr_id: x.wr_id },
                        SendEventOp::ReadSignaled => unreachable!(),
                    };
                    send_cq.push_back(completion);
                }
                SendEventOp::ReadSignaled => {
                    if let Some(recv_event) = self.read_resp_queue.pop_front() {
                        let x = self.send.pop().unwrap_or_else(|| unreachable!());
                        let completion = Completion::RdmaRead { wr_id: x.wr_id };
                        send_cq.push_back(completion);
                    } else {
                        break;
                    }
                }
            }
        }
    }

    fn ack_recv(
        &mut self,
        psn: u32,
        recv_cq: &CompletionQueue,
        send_cq: Option<&CompletionQueue>,
        qpn: u32,
        ack_resp_tx: &flume::Sender<AckResponse>,
    ) {
        self.recv.ack(psn);
        while let Some(event) = self.recv.pop() {
            match event.op {
                RecvEventOp::WriteWithImm { imm } => {
                    let completion = Completion::RecvRdmaWithImm { imm };
                    recv_cq.push_back(completion);
                }
                RecvEventOp::Recv => {
                    let x = self
                        .post_recv_queue
                        .pop_back()
                        .unwrap_or_else(|| unreachable!("no posted recv wr"));
                    let completion = Completion::Recv { wr_id: x.wr_id };
                    recv_cq.push_back(completion);
                }
                RecvEventOp::ReadResp => {
                    self.read_resp_queue.push_back(event);
                    // check if the read  completion could be updated
                    if let Some(cq) = send_cq {
                        self.ack_send(None, cq);
                    }
                }
                RecvEventOp::WriteAckReq => {
                    let _ignore = ack_resp_tx.send(AckResponse::Ack {
                        qpn,
                        msn: event.meta().msn,
                        last_psn: event.meta().end_psn,
                    });
                }
            }
        }
    }
}

#[derive(Debug)]
struct MessageTracker<E> {
    inner: VecDeque<E>,
    base_psn: u32,
}

impl<E> Default for MessageTracker<E> {
    fn default() -> Self {
        Self {
            inner: VecDeque::default(),
            base_psn: 0,
        }
    }
}

impl<E: EventMeta> MessageTracker<E> {
    fn append(&mut self, event: E) {
        if self
            .inner
            .back()
            .is_some_and(|last| Msn(last.meta().msn) > Msn(event.meta().msn))
        {
            let insert_pos = self
                .inner
                .iter()
                .position(|e| Msn(e.meta().msn) > Msn(event.meta().msn))
                .unwrap_or(self.inner.len());
            self.inner.insert(insert_pos, event);
        } else {
            self.inner.push_back(event);
        }
    }

    fn ack(&mut self, base_psn: u32) {
        self.base_psn = base_psn;
    }

    fn peek(&self) -> Option<&E> {
        let front = self.inner.front()?;
        (Psn(front.meta().end_psn) <= Psn(self.base_psn)).then_some(front)
    }

    fn pop(&mut self) -> Option<E> {
        let front = self.inner.front()?;
        if Psn(front.meta().end_psn) <= Psn(self.base_psn) {
            self.inner.pop_front()
        } else {
            None
        }
    }
}

trait EventMeta {
    fn meta(&self) -> MessageMeta;
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Event {
    Send(SendEvent),
    Recv(RecvEvent),
    PostRecv(PostRecvEvent),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SendEvent {
    op: SendEventOp,
    meta: MessageMeta,
    wr_id: u64,
}

impl SendEvent {
    pub(crate) fn new(op: SendEventOp, meta: MessageMeta, wr_id: u64) -> Self {
        Self { op, meta, wr_id }
    }
}

impl EventMeta for SendEvent {
    fn meta(&self) -> MessageMeta {
        self.meta
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum SendEventOp {
    WriteSignaled,
    SendSignaled,
    ReadSignaled,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RecvEvent {
    op: RecvEventOp,
    meta: MessageMeta,
}

impl RecvEvent {
    pub(crate) fn new(op: RecvEventOp, meta: MessageMeta) -> Self {
        Self { op, meta }
    }
}

impl EventMeta for RecvEvent {
    fn meta(&self) -> MessageMeta {
        self.meta
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RecvEventOp {
    WriteWithImm { imm: u32 },
    WriteAckReq,
    Recv,
    ReadResp,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PostRecvEvent {
    wr_id: u64,
}

impl PostRecvEvent {
    pub(crate) fn new(wr_id: u64) -> Self {
        Self { wr_id }
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct MessageMeta {
    pub(crate) msn: u16,
    pub(crate) end_psn: u32,
}

impl MessageMeta {
    pub(crate) fn new(msn: u16, end_psn: u32) -> Self {
        Self { msn, end_psn }
    }
}

pub(crate) struct CompletionQueueTable {
    inner: Arc<[CompletionQueue]>,
}

impl CompletionQueueTable {
    pub(crate) fn new() -> Self {
        Self {
            inner: iter::repeat_with(CompletionQueue::default)
                .take(MAX_CQ_CNT)
                .collect(),
        }
    }

    pub(crate) fn clone_arc(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }

    pub(crate) fn get_cq(&self, handle: u32) -> Option<&CompletionQueue> {
        self.inner.get(handle as usize)
    }
}

#[derive(Default)]
pub(crate) struct CompletionQueue {
    inner: Mutex<VecDeque<Completion>>,
}

impl CompletionQueue {
    pub(crate) fn push_back(&self, event: Completion) {
        let mut queue = self.inner.lock();
        queue.push_back(event);
    }

    pub(crate) fn pop_front(&self) -> Option<Completion> {
        let mut queue = self.inner.lock();
        queue.pop_front()
    }

    pub(crate) fn front(&self) -> Option<Completion> {
        let queue = self.inner.lock();
        queue.front().copied()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Completion {
    Send { wr_id: u64 },
    RdmaWrite { wr_id: u64 },
    RdmaRead { wr_id: u64 },
    Recv { wr_id: u64 },
    RecvRdmaWithImm { imm: u32 },
}

impl Completion {
    pub(crate) fn opcode(&self) -> u32 {
        match *self {
            Completion::Send { .. } => ibverbs_sys::ibv_wc_opcode::IBV_WC_SEND,
            Completion::RdmaWrite { .. } => ibverbs_sys::ibv_wc_opcode::IBV_WC_RDMA_WRITE,
            Completion::RdmaRead { .. } => ibverbs_sys::ibv_wc_opcode::IBV_WC_RDMA_READ,
            Completion::Recv { .. } => ibverbs_sys::ibv_wc_opcode::IBV_WC_RECV,
            Completion::RecvRdmaWithImm { .. } => {
                ibverbs_sys::ibv_wc_opcode::IBV_WC_RECV_RDMA_WITH_IMM
            }
        }
    }
}

/// Manages CQs
pub(crate) struct CqManager {
    /// Bitmap tracking allocated CQ handles
    bitmap: BitVec,
}

#[allow(clippy::as_conversions, clippy::indexing_slicing)]
impl CqManager {
    /// Creates a new `CqManager`
    pub(crate) fn new() -> Self {
        let mut bitmap = BitVec::with_capacity(MAX_CQ_CNT);
        bitmap.resize(MAX_CQ_CNT, false);
        Self { bitmap }
    }

    /// Allocates a new cq and returns its cqN
    #[allow(clippy::cast_possible_truncation)] // no larger than u32
    pub(crate) fn create_cq(&mut self) -> Option<u32> {
        let handle = self.bitmap.first_zero()? as u32;
        self.bitmap.set(handle as usize, true);
        Some(handle)
    }

    /// Removes and returns the cq associated with the given cqN
    pub(crate) fn destroy_cq(&mut self, handle: u32) {
        if handle as usize >= MAX_CQ_CNT {
            return;
        }
        self.bitmap.set(handle as usize, false);
    }
}

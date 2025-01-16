#![allow(clippy::missing_docs_in_private_items, clippy::struct_excessive_bools)]
// TODO: add field validations
use std::marker::PhantomData;

pub(crate) struct Initial;
pub(crate) struct WithQpParams;
pub(crate) struct WithIbvParams;
pub(crate) struct WithChunkInfo;

/// Work Request Builder
#[derive(Debug, Default)]
pub(crate) struct WrBuilder<S> {
    inner: WrChunk,
    _state: PhantomData<S>,
}

impl WrBuilder<Initial> {
    pub(crate) fn new() -> Self {
        Self {
            inner: WrChunk::default(),
            _state: PhantomData,
        }
    }

    pub(crate) fn set_qp_params(
        qp_type: u8,
        sqpn: u32,
        mac_addr: u64,
        dqpn: u32,
        dqp_ip: u32,
        pmtu: u8,
    ) -> WrBuilder<WithQpParams> {
        WrBuilder {
            inner: WrChunk {
                qp_type,
                sqpn,
                mac_addr,
                dqpn,
                dqp_ip,
                pmtu,
                ..Default::default()
            },
            _state: PhantomData,
        }
    }
}

impl WrBuilder<WithQpParams> {
    pub(crate) fn set_ibv_params(
        mut self,
        flags: u8,
        raddr: u64,
        rkey: u32,
        total_len: u32,
        lkey: u32,
        imm: u32,
    ) -> WrBuilder<WithIbvParams> {
        self.inner.flags = flags;
        self.inner.raddr = raddr;
        self.inner.rkey = rkey;
        self.inner.total_len = total_len;
        self.inner.lkey = lkey;
        self.inner.imm = imm;

        WrBuilder {
            inner: self.inner,
            _state: PhantomData,
        }
    }
}

impl WrBuilder<WithIbvParams> {
    pub(crate) fn set_chunk_info(
        mut self,
        msn: u16,
        psn: u32,
        addr: u64,
        len: u32,
        pos: ChunkPos,
    ) -> WrBuilder<WithChunkInfo> {
        self.inner.msn = msn;
        self.inner.psn = psn;
        self.inner.laddr = addr;
        self.inner.len = len;
        match pos {
            ChunkPos::First => self.inner.is_first = true,
            ChunkPos::Last => self.inner.is_last = true,
            ChunkPos::Middle => {}
        }

        WrBuilder {
            inner: self.inner,
            _state: PhantomData,
        }
    }
}

impl WrBuilder<WithChunkInfo> {
    pub(crate) fn set_is_retry(mut self) -> Self {
        self.inner.is_retry = true;
        self
    }

    pub(crate) fn set_enable_ecn(mut self) -> Self {
        self.inner.enable_ecn = true;
        self
    }

    pub(crate) fn build(self) -> WrChunk {
        self.inner
    }
}

#[derive(Debug, Default)]
pub(crate) struct WrChunk {
    qp_type: u8,
    sqpn: u32,
    mac_addr: u64,
    dqpn: u32,
    dqp_ip: u32,
    pmtu: u8,
    flags: u8,
    raddr: u64,
    rkey: u32,
    total_len: u32,
    lkey: u32,
    imm: u32,
    laddr: u64,
    len: u32,
    is_first: bool,
    is_last: bool,
    msn: u16,
    psn: u32,
    is_retry: bool,
    enable_ecn: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ChunkPos {
    First,
    Middle,
    Last,
}

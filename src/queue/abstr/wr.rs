#![allow(clippy::missing_docs_in_private_items, clippy::struct_excessive_bools)]
// TODO: add field validations
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Initial;
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WithQpParams;
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WithIbvParams;
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WithChunkInfo;

/// Work Request Builder
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WrChunkBuilder<S> {
    inner: WrChunk,
    _state: PhantomData<S>,
}

impl WrChunkBuilder<Initial> {
    pub(crate) fn new() -> Self {
        Self {
            inner: WrChunk::default(),
            _state: PhantomData,
        }
    }

    #[allow(clippy::unused_self, clippy::too_many_arguments)]
    pub(crate) fn set_qp_params(
        self,
        msn: u16,
        qp_type: u8,
        sqpn: u32,
        mac_addr: u64,
        dqpn: u32,
        dqp_ip: u32,
        pmtu: u8,
    ) -> WrChunkBuilder<WithQpParams> {
        WrChunkBuilder {
            inner: WrChunk {
                qp_type,
                sqpn,
                mac_addr,
                dqpn,
                dqp_ip,
                pmtu,
                msn,
                ..Default::default()
            },
            _state: PhantomData,
        }
    }
}

impl WrChunkBuilder<WithQpParams> {
    pub(crate) fn set_ibv_params(
        mut self,
        flags: u8,
        rkey: u32,
        total_len: u32,
        lkey: u32,
        imm: u32,
    ) -> WrChunkBuilder<WithIbvParams> {
        self.inner.flags = flags;
        self.inner.rkey = rkey;
        self.inner.total_len = total_len;
        self.inner.lkey = lkey;
        self.inner.imm = imm;

        WrChunkBuilder {
            inner: self.inner,
            _state: PhantomData,
        }
    }
}

impl WrChunkBuilder<WithIbvParams> {
    pub(crate) fn set_chunk_meta(
        mut self,
        psn: u32,
        laddr: u64,
        raddr: u64,
        len: u32,
        pos: ChunkPos,
    ) -> WrChunkBuilder<WithChunkInfo> {
        self.inner.psn = psn;
        self.inner.laddr = laddr;
        self.inner.raddr = raddr;
        self.inner.len = len;
        match pos {
            ChunkPos::First => self.inner.is_first = true,
            ChunkPos::Last => self.inner.is_last = true,
            ChunkPos::Middle => {}
        }

        WrChunkBuilder {
            inner: self.inner,
            _state: PhantomData,
        }
    }

    pub(crate) fn pmtu(&self) -> u8 {
        self.inner.pmtu
    }
}

impl WrChunkBuilder<WithChunkInfo> {
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

#[derive(Clone, Copy, Debug, Default)]
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

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChunkPos {
    #[default]
    First,
    Middle,
    Last,
}

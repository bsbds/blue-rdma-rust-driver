use crate::{
    constants::PSN_MASK,
    device_protocol::{QpParams, WithIbvParams, WorkReqOpCode, WrChunk, WrChunkBuilder},
    queue_pair::convert_ibv_mtu_to_u16,
    send::SendWrRdma,
};

use super::Fragmenter;

/// (Max) size of a single WR chunk
const WR_CHUNK_SIZE: u32 = 0x10000;

pub(crate) struct WrChunkFragmenter {
    inner: ChunkFragmenter,
}

impl WrChunkFragmenter {
    pub(crate) fn new(wr: SendWrRdma, qp_param: QpParams, base_psn: u32) -> Self {
        Self {
            inner: ChunkFragmenter::new(wr, qp_param, base_psn, WR_CHUNK_SIZE.into(), false),
        }
    }
}

impl IntoIterator for WrChunkFragmenter {
    type Item = WrChunk;

    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

pub(crate) struct WrPacketFragmenter {
    inner: ChunkFragmenter,
}

impl WrPacketFragmenter {
    pub(crate) fn new(wr: SendWrRdma, qp_param: QpParams, base_psn: u32) -> Self {
        let pmtu = convert_ibv_mtu_to_u16(qp_param.pmtu)
            .unwrap_or_else(|| unreachable!("invalid ibv_mtu"))
            .into();
        Self {
            inner: ChunkFragmenter::new(wr, qp_param, base_psn, pmtu, true),
        }
    }
}

impl IntoIterator for WrPacketFragmenter {
    type Item = WrChunk;

    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

/// Work Request Fragmenter, used to split a single work request into multiple chunks
#[derive(Debug)]
struct ChunkFragmenter {
    wr: SendWrRdma,
    qp_param: QpParams,
    base_psn: u32,
    chunk_size: u64,
    is_retry: bool,
}

impl ChunkFragmenter {
    fn new(
        wr: SendWrRdma,
        qp_param: QpParams,
        base_psn: u32,
        chunk_size: u64,
        is_retry: bool,
    ) -> Self {
        Self {
            wr,
            qp_param,
            base_psn,
            chunk_size,
            is_retry,
        }
    }
}

impl IntoIterator for ChunkFragmenter {
    type Item = WrChunk;

    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        let builder = WrChunkBuilder::new_with_opcode(self.wr.opcode())
            .set_qp_params(self.qp_param)
            .set_ibv_params(
                self.wr.send_flags() as u8,
                self.wr.rkey(),
                self.wr.length(),
                self.wr.lkey(),
                self.wr.imm(),
            );

        let pmtu = convert_ibv_mtu_to_u16(self.qp_param.pmtu)
            .unwrap_or_else(|| unreachable!("invalid ibv_mtu"))
            .into();

        let f = Fragmenter::new(
            self.chunk_size,
            pmtu,
            self.wr.raddr(),
            self.wr.length().into(),
        );
        IntoIter {
            inner: f.into_iter(),
            psn: self.base_psn,
            wr: self.wr,
            builder,
            laddr: self.wr.laddr(),
            pmtu,
            is_retry: self.is_retry,
        }
    }
}

pub(crate) struct IntoIter {
    inner: super::IntoIter,
    psn: u32,
    wr: SendWrRdma,
    builder: WrChunkBuilder<WithIbvParams>,
    laddr: u64,
    pmtu: u64,
    is_retry: bool,
}

impl Iterator for IntoIter {
    type Item = WrChunk;

    fn next(&mut self) -> Option<Self::Item> {
        let f = self.inner.next()?;
        let builder =
            self.builder
                .set_chunk_meta(self.psn, self.laddr, f.addr, f.len as u32, f.pos);
        let chunk = if self.is_retry {
            builder.set_is_retry().build()
        } else {
            builder.build()
        };
        let num_packets = f.len.div_ceil(self.pmtu) as u32;
        self.psn = (self.psn + num_packets) % PSN_MASK;
        self.laddr += f.len;

        Some(chunk)
    }
}

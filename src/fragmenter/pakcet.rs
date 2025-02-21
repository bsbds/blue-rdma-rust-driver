use crate::{
    constants::PSN_MASK,
    device_protocol::{QpParams, WithIbvParams, WorkReqOpCode, WrChunk, WrChunkBuilder},
    qp::convert_ibv_mtu_to_u16,
    send::SendWrRdma,
};

use super::Fragmenter;

pub(crate) struct PacketFragmenter {
    wr: SendWrRdma,
    opcode: WorkReqOpCode,
    qp_param: QpParams,
    base_psn: u32,
}

impl PacketFragmenter {
    pub(crate) fn new(
        wr: SendWrRdma,
        opcode: WorkReqOpCode,
        qp_param: QpParams,
        base_psn: u32,
    ) -> Self {
        Self {
            wr,
            opcode,
            qp_param,
            base_psn,
        }
    }
}

impl IntoIterator for PacketFragmenter {
    type Item = WrChunk;
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        let pmtu = convert_ibv_mtu_to_u16(self.qp_param.pmtu)
            .unwrap_or_else(|| unreachable!("invalid ibv_mtu"))
            .into();
        let f = Fragmenter::new(pmtu, pmtu, self.wr.raddr(), self.wr.length().into());
        let builder = WrChunkBuilder::new_with_opcode(self.opcode)
            .set_qp_params(self.qp_param)
            .set_ibv_params(
                self.wr.send_flags() as u8,
                self.wr.rkey(),
                self.wr.length(),
                self.wr.lkey(),
                self.wr.imm(),
            );

        Self::IntoIter {
            inner: f.into_iter(),
            wr: self.wr,
            psn: self.base_psn,
            builder,
            laddr: self.wr.laddr(),
        }
    }
}

pub(crate) struct IntoIter {
    inner: super::IntoIter,
    psn: u32,
    wr: SendWrRdma,
    builder: WrChunkBuilder<WithIbvParams>,
    laddr: u64,
}

impl Iterator for IntoIter {
    type Item = WrChunk;

    fn next(&mut self) -> Option<Self::Item> {
        let f = self.inner.next()?;
        let chunk = self
            .builder
            .set_chunk_meta(self.psn, self.laddr, f.addr, f.len as u32, f.pos)
            .set_is_retry()
            .build();
        self.psn = (self.psn + 1) % PSN_MASK;
        self.laddr += f.len;

        Some(chunk)
    }
}

use crate::queue::abstr::{ChunkPos, WithIbvParams, WithQpParams, WrChunk, WrChunkBuilder};

use ibverbs_sys::{
    ibv_send_wr,
    ibv_wr_opcode::{IBV_WR_RDMA_WRITE, IBV_WR_RDMA_WRITE_WITH_IMM},
};
use thiserror::Error;

/// (Max) size of a single WR chunk
const WR_CHUNK_SIZE: u32 = 0x10000;

/// A Result type for validation operations.
type Result<T> = std::result::Result<T, ValidationError>;

/// Work Request Fragmenter, used to split a single work request into multiple chunks
#[derive(Default)]
struct WrFragmenter {
    /// Current PSN
    psn: u32,
    /// Current laddr
    laddr: u64,
    /// Current raddr
    raddr: u64,
    /// Remaining length
    rem_len: u32,
    /// Current chunk position
    chunk_pos: ChunkPos,
    /// Chunk builder
    builder: WrChunkBuilder<WithIbvParams>,
}

impl Iterator for WrFragmenter {
    type Item = WrChunk;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rem_len == 0 {
            return None;
        }

        let pmtu = self.builder.pmtu();
        let pmtu_mask = pmtu.checked_sub(1)?;
        // Chunk boundary must align with PMTU
        let chunk_end = self.laddr.saturating_add(WR_CHUNK_SIZE.into()) & !u64::from(pmtu_mask);
        let chunk_size: u32 = chunk_end
            .saturating_sub(self.laddr)
            .try_into()
            .unwrap_or_else(|_| unreachable!("chunk size should smaller than u32::MAX"));

        if self.rem_len <= chunk_size {
            self.chunk_pos = ChunkPos::Last;
        }

        let chunk = self
            .builder
            .set_chunk_meta(self.psn, self.laddr, self.raddr, chunk_size, self.chunk_pos)
            .build();

        let num_packets = chunk_size
            .checked_add(u32::from(pmtu_mask))?
            .checked_div(u32::from(pmtu))?;
        self.psn = self.psn.wrapping_add(num_packets); // FIXME: is wrapping add correct?
        self.laddr = self.laddr.checked_add(u64::from(chunk_size))?;
        self.raddr = self.raddr.checked_add(u64::from(chunk_size))?;
        self.rem_len = self.rem_len.saturating_sub(chunk_size);
        if matches!(self.chunk_pos, ChunkPos::First) {
            self.chunk_pos = ChunkPos::Middle;
        }

        Some(chunk)
    }
}

impl WrFragmenter {
    /// Creates a new `SgeSplitter`
    #[allow(unsafe_code)]
    fn new(wr: ibv_send_wr, builder: WrChunkBuilder<WithQpParams>, base_psn: u32) -> Result<Self> {
        let num_sge: usize = usize::try_from(wr.num_sge).map_err(ValidationError::invalid_input)?;
        match num_sge {
            0 => return Ok(Self::default()),
            1 => return Err(ValidationError::unimplemented("multiple sges unsupported")),
            _ => {}
        };
        // SAFETY: The sg_list pointer is guaranteed to be valid if num_sge > 0
        let sge = unsafe { *wr.sg_list };
        let opcode = wr.opcode;
        match opcode {
            IBV_WR_RDMA_WRITE | IBV_WR_RDMA_WRITE_WITH_IMM => {}
            _ => return Err(ValidationError::unimplemented("opcode not supported")),
        }
        // SAFETY: union member is rdma for RDMA_WRITE
        let (raddr, rkey, imm_data) = unsafe {
            (
                wr.wr.rdma.remote_addr,
                wr.wr.rdma.rkey,
                wr.__bindgen_anon_1.imm_data,
            )
        };
        #[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
        // truncation is exptected
        // behavior
        let builder =
            builder.set_ibv_params(wr.send_flags as u8, rkey, sge.length, sge.lkey, imm_data);

        Ok(Self {
            psn: base_psn,
            laddr: sge.addr,
            raddr,
            rem_len: sge.length,
            chunk_pos: ChunkPos::First,
            builder,
        })
    }
}

/// Error type for invalid input validation
#[derive(Error, Debug)]
pub(crate) enum ValidationError {
    /// The user input is invalid
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// The operation is unimplemented
    #[error("unimplemented: {0}")]
    Unimplemented(String),
}

impl ValidationError {
    /// `ValidationError::InvalidInput` error
    #[allow(clippy::needless_pass_by_value)] // consume the error
    pub(crate) fn invalid_input<T: ToString>(value: T) -> Self {
        Self::InvalidInput(value.to_string())
    }

    /// `ValidationError::Unimplemented` error
    #[allow(clippy::needless_pass_by_value)] // consume the error
    pub(crate) fn unimplemented<T: ToString>(value: T) -> Self {
        Self::Unimplemented(value.to_string())
    }
}

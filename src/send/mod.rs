use crate::queue::abstr::{ChunkPos, WithIbvParams, WithQpParams, WrChunk, WrChunkBuilder};

use ibverbs_sys::{
    ibv_send_wr,
    ibv_wr_opcode::{IBV_WR_RDMA_WRITE, IBV_WR_RDMA_WRITE_WITH_IMM},
};
use thiserror::Error;

/// (Max) size of a single WR chunk
const WR_CHUNK_SIZE: u32 = 0x1000;

/// A Result type for validation operations.
type Result<T> = std::result::Result<T, ValidationError>;

/// Work Request Fragmenter, used to split a single work request into multiple chunks
#[derive(Default)]
pub(crate) struct WrFragmenter {
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
        if self.is_complete() {
            return None;
        }

        let pmtu = self.builder.pmtu();
        let pmtu_mask = pmtu
            .checked_sub(1)
            .unwrap_or_else(|| unreachable!("pmtu should be greater than 1"));

        // Chunk boundary must align with PMTU
        let chunk_end = self.laddr.saturating_add(WR_CHUNK_SIZE.into()) & !u64::from(pmtu_mask);
        println!("chunk_end: {:x}", chunk_end);
        let mut chunk_size: u32 = chunk_end
            .saturating_sub(self.laddr)
            .try_into()
            .unwrap_or_else(|_| unreachable!("chunk size should smaller than u32::MAX"));
        println!("laddr: {:x}", self.laddr);
        println!("raddr: {:x}", self.raddr);
        println!("rem size: {}", self.rem_len);
        println!("chunk size: {chunk_size}");

        if self.rem_len <= chunk_size {
            chunk_size = self.rem_len;
            if !matches!(self.chunk_pos, ChunkPos::Only) {
                self.chunk_pos = ChunkPos::Last;
            }
        }

        let chunk = self
            .builder
            .set_chunk_meta(self.psn, self.laddr, self.raddr, chunk_size, self.chunk_pos)
            .build();

        let num_packets = chunk_size.div_ceil(u32::from(pmtu));
        self.psn = self.psn.wrapping_add(num_packets); // FIXME: is wrapping add correct?
        self.laddr = self.laddr.checked_add(u64::from(chunk_size))?;
        self.raddr = self.raddr.checked_add(u64::from(chunk_size))?;
        self.rem_len = self.rem_len.saturating_sub(chunk_size);
        self.chunk_pos = self.chunk_pos.next();

        Some(chunk)
    }
}

impl WrFragmenter {
    /// Creates a new `WrFragmenter`
    #[allow(unsafe_code)]
    pub(crate) fn new(
        wr: SendWrResolver,
        builder: WrChunkBuilder<WithQpParams>,
        base_psn: u32,
    ) -> Self {
        #[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
        // truncation is exptected
        // behavior
        let builder = builder.set_ibv_params(
            wr.send_flags() as u8,
            wr.rkey(),
            wr.length(),
            wr.lkey(),
            wr.imm(),
        );

        Self {
            psn: base_psn,
            laddr: wr.laddr(),
            raddr: wr.raddr(),
            rem_len: wr.length(),
            chunk_pos: ChunkPos::First,
            builder,
        }
    }

    /// Checks if the fragmentation is complete, the iteration will yeild `None`
    pub(crate) fn is_complete(&self) -> bool {
        self.rem_len == 0
    }
}

/// A resolver and validator for `ibv_send_wr`
#[derive(Clone, Copy)]
pub(crate) struct SendWrResolver(ibv_send_wr);

#[allow(unsafe_code)] // SAFETY: The sg_list pointer is guaranteed to be valid if num_sge > 0
impl SendWrResolver {
    /// Creates a new resolver from the given work request.
    /// Returns None if the input is invalid
    pub(crate) fn new(wr: ibv_send_wr) -> Result<Self> {
        match wr.opcode {
            IBV_WR_RDMA_WRITE | IBV_WR_RDMA_WRITE_WITH_IMM => {}
            _ => return Err(ValidationError::unimplemented("opcode not supported")),
        }
        let num_sge: usize = usize::try_from(wr.num_sge).map_err(ValidationError::invalid_input)?;
        if num_sge != 1 {
            return Err(ValidationError::unimplemented("only support single sge"));
        }
        Ok(Self(wr))
    }

    /// Returns the local address of the SGE buffer.
    pub(crate) fn laddr(&self) -> u64 {
        unsafe { *self.0.sg_list }.addr
    }

    /// Returns the length of the SGE buffer in bytes.
    pub(crate) fn length(&self) -> u32 {
        unsafe { *self.0.sg_list }.length
    }

    /// Returns the local key associated with the SGE buffer.
    pub(crate) fn lkey(&self) -> u32 {
        unsafe { *self.0.sg_list }.lkey
    }

    /// Returns the remote memory address for RDMA operations
    pub(crate) fn raddr(&self) -> u64 {
        unsafe { self.0.wr.rdma.remote_addr }
    }

    /// Returns the remote key for RDMA operations
    pub(crate) fn rkey(&self) -> u32 {
        unsafe { self.0.wr.rdma.rkey }
    }

    /// Returns the immediate data value
    pub(crate) fn imm(&self) -> u32 {
        unsafe { self.0.__bindgen_anon_1.imm_data }
    }

    /// Returns the send flags
    pub(crate) fn send_flags(&self) -> u32 {
        self.0.send_flags
    }

    /// Returns the ID associated with this WR
    pub(crate) fn wr_id(&self) -> u64 {
        self.0.wr_id
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

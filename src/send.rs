use crate::{
    constants::PSN_MASK,
    device_protocol::{ChunkPos, WithIbvParams, WithQpParams, WrChunk, WrChunkBuilder},
};

use ibverbs_sys::{
    ibv_send_wr,
    ibv_wr_opcode::{
        IBV_WR_RDMA_READ, IBV_WR_RDMA_WRITE, IBV_WR_RDMA_WRITE_WITH_IMM, IBV_WR_SEND,
        IBV_WR_SEND_WITH_IMM,
    },
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
    /// Length of the iterator
    len: usize,
    /// Chunk size
    chunk_size: u32,
    /// whether the current message is a retry
    is_retry: bool,
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
        let chunk_end = self.laddr.saturating_add(self.chunk_size.into()) & !u64::from(pmtu_mask);
        let mut chunk_size: u32 = chunk_end
            .saturating_sub(self.laddr)
            .try_into()
            .unwrap_or_else(|_| unreachable!("chunk size should smaller than u32::MAX"));

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
        wr: SendWrRdma,
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

        let num_chunks = Self::num_chunks(wr.raddr(), wr.length().into(), builder.pmtu());
        let chunk_pos = if num_chunks == 1 {
            ChunkPos::Only
        } else {
            ChunkPos::First
        };

        Self {
            psn: base_psn,
            laddr: wr.laddr(),
            raddr: wr.raddr(),
            rem_len: wr.length(),
            chunk_pos,
            builder,
            len: num_chunks,
            chunk_size: WR_CHUNK_SIZE,
            is_retry: false,
        }
    }

    /// Creates a new `WrFragmenter`
    #[allow(unsafe_code)]
    fn new_custom(
        wr: SendWrRdma,
        builder: WrChunkBuilder<WithQpParams>,
        base_psn: u32,
        chunk_size: u32,
        is_retry: bool,
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

        let num_chunks = Self::num_chunks(wr.raddr(), wr.length().into(), builder.pmtu());
        let chunk_pos = if num_chunks == 1 {
            ChunkPos::Only
        } else {
            ChunkPos::First
        };

        Self {
            psn: base_psn,
            laddr: wr.laddr(),
            raddr: wr.raddr(),
            rem_len: wr.length(),
            chunk_pos,
            builder,
            len: num_chunks,
            chunk_size,
            is_retry,
        }
    }

    /// Checks if the fragmentation is complete, the iteration will yeild `None`
    pub(crate) fn is_complete(&self) -> bool {
        self.rem_len == 0
    }

    fn num_chunks(addr: u64, length: u64, pmtu: u16) -> usize {
        let pmtu_mask = pmtu
            .checked_sub(1)
            .unwrap_or_else(|| unreachable!("pmtu should be greater than 1"));
        let first_chunk_end = addr.saturating_add(WR_CHUNK_SIZE.into()) & !u64::from(pmtu_mask);
        let first_chunk_len = first_chunk_end.wrapping_sub(addr);
        if first_chunk_len >= length {
            return 1;
        }
        let rem = length.wrapping_sub(first_chunk_len);
        usize::try_from(rem.div_ceil(WR_CHUNK_SIZE.into())).unwrap_or_else(|_| unreachable!())
    }
}

pub(crate) struct WrPacketFragmenter {
    wr: SendWrRdma,
    builder: WrChunkBuilder<WithQpParams>,
    base_psn: u32,
}

impl WrPacketFragmenter {
    pub(crate) fn new(
        wr: SendWrRdma,
        builder: WrChunkBuilder<WithQpParams>,
        base_psn: u32,
    ) -> Self {
        Self {
            wr,
            builder,
            base_psn,
        }
    }

    pub(crate) fn packets(self) -> Vec<WrChunk> {
        WrFragmenter::new_custom(
            self.wr,
            self.builder,
            self.base_psn,
            self.builder.pmtu().into(),
            // used for retransmission
            true,
        )
        .collect()
    }

    pub(crate) fn last(self) -> WrChunk {
        self.packets()
            .into_iter()
            .last()
            .unwrap_or_else(|| unreachable!("empty message"))
    }

    pub(crate) fn packets_alt(self) -> Vec<WrChunk> {
        let builder = self.builder.set_ibv_params(
            self.wr.send_flags() as u8,
            self.wr.rkey(),
            self.wr.length(),
            self.wr.lkey(),
            self.wr.imm(),
        );
        let pmtu = u64::from(builder.pmtu());
        let length = self.wr.length();
        let start_addr = self.wr.raddr();
        let end_addr = start_addr + u64::from(length);
        let mut addr = start_addr;
        let mut laddr = self.wr.laddr();
        let mut psn = self.base_psn;
        let mut chunks = Vec::new();
        while addr < end_addr {
            let end = ((addr + pmtu) & !(pmtu - 1)).min(end_addr);
            let len = end - addr;
            let chunk = builder
                .set_chunk_meta(psn, laddr, addr, len as u32, ChunkPos::Middle)
                .build();
            chunks.push(chunk);
            psn = (psn + 1) & PSN_MASK;
            addr += len;
            laddr += len;
        }
        chunks
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SendWr {
    RdmaWrite(SendWrRdma),
    RdmaRead(SendWrRdma),
    Send(SendWrBase),
}

impl SendWr {
    #[allow(unsafe_code)]
    /// Creates a new `SendWr`
    pub(crate) fn new(wr: ibv_send_wr) -> Result<Self> {
        let num_sge = usize::try_from(wr.num_sge).map_err(ValidationError::invalid_input)?;
        if num_sge != 1 {
            return Err(ValidationError::unimplemented("only support single sge"));
        }
        // SAFETY: sg_list is valid when num_sge > 0, which we've verified above
        let sge = unsafe { *wr.sg_list };

        let base = SendWrBase {
            wr_id: wr.wr_id,
            send_flags: wr.send_flags,
            laddr: sge.addr,
            length: sge.length,
            lkey: sge.lkey,
            // SAFETY: imm_data is valid for operations with immediate data
            imm_data: unsafe { wr.__bindgen_anon_1.imm_data },
        };

        match wr.opcode {
            IBV_WR_RDMA_WRITE | IBV_WR_RDMA_WRITE_WITH_IMM => {
                let wr = SendWrRdma {
                    base,
                    // SAFETY: rdma field is valid for RDMA operations
                    raddr: unsafe { wr.wr.rdma.remote_addr },
                    rkey: unsafe { wr.wr.rdma.rkey },
                };
                Ok(Self::RdmaWrite(wr))
            }
            IBV_WR_RDMA_READ => {
                let wr = SendWrRdma {
                    base,
                    // SAFETY: rdma field is valid for RDMA operations
                    raddr: unsafe { wr.wr.rdma.remote_addr },
                    rkey: unsafe { wr.wr.rdma.rkey },
                };
                Ok(Self::RdmaRead(wr))
            }
            IBV_WR_SEND | IBV_WR_SEND_WITH_IMM => Ok(Self::Send(base)),
            _ => Err(ValidationError::unimplemented("opcode not supported")),
        }
    }

    pub(crate) fn wr_id(&self) -> u64 {
        match *self {
            SendWr::RdmaWrite(wr) | SendWr::RdmaRead(wr) => wr.base.wr_id,
            SendWr::Send(wr) => wr.wr_id,
        }
    }
    pub(crate) fn send_flags(&self) -> u32 {
        match *self {
            SendWr::RdmaWrite(wr) | SendWr::RdmaRead(wr) => wr.base.send_flags,
            SendWr::Send(wr) => wr.send_flags,
        }
    }

    pub(crate) fn laddr(&self) -> u64 {
        match *self {
            SendWr::RdmaWrite(wr) | SendWr::RdmaRead(wr) => wr.base.laddr,
            SendWr::Send(wr) => wr.laddr,
        }
    }

    pub(crate) fn length(&self) -> u32 {
        match *self {
            SendWr::RdmaWrite(wr) | SendWr::RdmaRead(wr) => wr.base.length,
            SendWr::Send(wr) => wr.length,
        }
    }

    pub(crate) fn lkey(&self) -> u32 {
        match *self {
            SendWr::RdmaWrite(wr) | SendWr::RdmaRead(wr) => wr.base.lkey,
            SendWr::Send(wr) => wr.lkey,
        }
    }

    pub(crate) fn imm_data(&self) -> u32 {
        match *self {
            SendWr::RdmaWrite(wr) | SendWr::RdmaRead(wr) => wr.base.imm_data,
            SendWr::Send(wr) => wr.imm_data,
        }
    }
}

/// A resolver and validator for send work requests
#[derive(Debug, Clone, Copy)]
pub(crate) struct SendWrRdma {
    base: SendWrBase,
    pub(crate) raddr: u64,
    pub(crate) rkey: u32,
}

impl SendWrRdma {
    #[allow(unsafe_code)]
    /// Creates a new resolver from the given work request.
    /// Returns None if the input is invalid
    pub(crate) fn new(wr: ibv_send_wr) -> Result<Self> {
        match wr.opcode {
            IBV_WR_RDMA_WRITE | IBV_WR_RDMA_WRITE_WITH_IMM => {}
            _ => return Err(ValidationError::unimplemented("opcode not supported")),
        }

        let num_sge = usize::try_from(wr.num_sge).map_err(ValidationError::invalid_input)?;

        if num_sge != 1 {
            return Err(ValidationError::unimplemented("only support single sge"));
        }

        // SAFETY: sg_list is valid when num_sge > 0, which we've verified above
        let sge = unsafe { *wr.sg_list };

        Ok(Self {
            base: SendWrBase {
                wr_id: wr.wr_id,
                send_flags: wr.send_flags,
                laddr: sge.addr,
                length: sge.length,
                lkey: sge.lkey,
                // SAFETY: imm_data is valid for operations with immediate data
                imm_data: unsafe { wr.__bindgen_anon_1.imm_data },
            },
            // SAFETY: rdma field is valid for RDMA operations
            raddr: unsafe { wr.wr.rdma.remote_addr },
            rkey: unsafe { wr.wr.rdma.rkey },
        })
    }

    pub(crate) fn new_from_base(base: SendWrBase, raddr: u64, rkey: u32) -> SendWrRdma {
        Self { base, raddr, rkey }
    }

    /// Returns the local address of the SGE buffer
    #[inline]
    pub(crate) fn laddr(&self) -> u64 {
        self.base.laddr
    }

    /// Returns the length of the SGE buffer in bytes
    #[inline]
    pub(crate) fn length(&self) -> u32 {
        self.base.length
    }

    /// Returns the local key associated with the SGE buffer
    #[inline]
    pub(crate) fn lkey(&self) -> u32 {
        self.base.lkey
    }

    /// Returns the remote memory address for RDMA operations
    #[inline]
    pub(crate) fn raddr(&self) -> u64 {
        self.raddr
    }

    /// Returns the remote key for RDMA operations
    #[inline]
    pub(crate) fn rkey(&self) -> u32 {
        self.rkey
    }

    /// Returns the immediate data value
    #[inline]
    pub(crate) fn imm(&self) -> u32 {
        self.base.imm_data
    }

    /// Returns the send flags
    #[inline]
    pub(crate) fn send_flags(&self) -> u32 {
        self.base.send_flags
    }

    /// Returns the ID associated with this WR
    #[inline]
    pub(crate) fn wr_id(&self) -> u64 {
        self.base.wr_id
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SendWrBase {
    pub(crate) wr_id: u64,
    pub(crate) send_flags: u32,
    pub(crate) laddr: u64,
    pub(crate) length: u32,
    pub(crate) lkey: u32,
    pub(crate) imm_data: u32,
}

impl SendWrBase {
    pub(crate) fn new(
        wr_id: u64,
        send_flags: u32,
        laddr: u64,
        length: u32,
        lkey: u32,
        imm_data: u32,
    ) -> Self {
        Self {
            wr_id,
            send_flags,
            laddr,
            length,
            lkey,
            imm_data,
        }
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

use crate::device_protocol::WorkReqOpCode;

use ibverbs_sys::{
    ibv_send_wr,
    ibv_wr_opcode::{
        IBV_WR_RDMA_READ, IBV_WR_RDMA_WRITE, IBV_WR_RDMA_WRITE_WITH_IMM, IBV_WR_SEND,
        IBV_WR_SEND_WITH_IMM,
    },
};
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub(crate) enum SendWr {
    Rdma(SendWrRdma),
    Send(SendWrBase),
}

impl SendWr {
    #[allow(unsafe_code)]
    /// Creates a new `SendWr`
    pub(crate) fn new(wr: ibv_send_wr) -> Result<Self, ValidationError> {
        let num_sge = usize::try_from(wr.num_sge).map_err(ValidationError::invalid_input)?;
        if num_sge != 1 {
            return Err(ValidationError::unimplemented("only support single sge"));
        }
        // SAFETY: sg_list is valid when num_sge > 0, which we've verified above
        let sge = unsafe { *wr.sg_list };
        let opcode = match wr.opcode {
            IBV_WR_RDMA_WRITE => WorkReqOpCode::RdmaWrite,
            IBV_WR_RDMA_WRITE_WITH_IMM => WorkReqOpCode::RdmaWriteWithImm,
            IBV_WR_RDMA_READ => WorkReqOpCode::RdmaRead,
            IBV_WR_SEND => WorkReqOpCode::Send,
            IBV_WR_SEND_WITH_IMM => WorkReqOpCode::SendWithImm,
            _ => return Err(ValidationError::unimplemented("opcode not supported")),
        };

        let base = SendWrBase {
            wr_id: wr.wr_id,
            send_flags: wr.send_flags,
            laddr: sge.addr,
            length: sge.length,
            lkey: sge.lkey,
            // SAFETY: imm_data is valid for operations with immediate data
            imm_data: unsafe { wr.__bindgen_anon_1.imm_data },
            opcode,
        };

        match wr.opcode {
            IBV_WR_RDMA_WRITE | IBV_WR_RDMA_WRITE_WITH_IMM | IBV_WR_RDMA_READ => {
                let wr = SendWrRdma {
                    base,
                    // SAFETY: rdma field is valid for RDMA operations
                    raddr: unsafe { wr.wr.rdma.remote_addr },
                    rkey: unsafe { wr.wr.rdma.rkey },
                };
                Ok(Self::Rdma(wr))
            }
            IBV_WR_SEND | IBV_WR_SEND_WITH_IMM => Ok(Self::Send(base)),
            _ => Err(ValidationError::unimplemented("opcode not supported")),
        }
    }

    pub(crate) fn wr_id(&self) -> u64 {
        match *self {
            SendWr::Rdma(wr) => wr.base.wr_id,
            SendWr::Send(wr) => wr.wr_id,
        }
    }
    pub(crate) fn send_flags(&self) -> u32 {
        match *self {
            SendWr::Rdma(wr) => wr.base.send_flags,
            SendWr::Send(wr) => wr.send_flags,
        }
    }

    pub(crate) fn laddr(&self) -> u64 {
        match *self {
            SendWr::Rdma(wr) => wr.base.laddr,
            SendWr::Send(wr) => wr.laddr,
        }
    }

    pub(crate) fn length(&self) -> u32 {
        match *self {
            SendWr::Rdma(wr) => wr.base.length,
            SendWr::Send(wr) => wr.length,
        }
    }

    pub(crate) fn lkey(&self) -> u32 {
        match *self {
            SendWr::Rdma(wr) => wr.base.lkey,
            SendWr::Send(wr) => wr.lkey,
        }
    }

    pub(crate) fn imm_data(&self) -> u32 {
        match *self {
            SendWr::Rdma(wr) => wr.base.imm_data,
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
    pub(crate) fn new(wr: ibv_send_wr) -> Result<Self, ValidationError> {
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

        let opcode = match wr.opcode {
            IBV_WR_RDMA_WRITE => WorkReqOpCode::RdmaWrite,
            IBV_WR_RDMA_WRITE_WITH_IMM => WorkReqOpCode::RdmaWriteWithImm,
            IBV_WR_RDMA_READ => WorkReqOpCode::RdmaRead,
            IBV_WR_SEND => WorkReqOpCode::Send,
            IBV_WR_SEND_WITH_IMM => WorkReqOpCode::SendWithImm,
            _ => return Err(ValidationError::unimplemented("opcode not supported")),
        };

        Ok(Self {
            base: SendWrBase {
                wr_id: wr.wr_id,
                send_flags: wr.send_flags,
                laddr: sge.addr,
                length: sge.length,
                lkey: sge.lkey,
                // SAFETY: imm_data is valid for operations with immediate data
                imm_data: unsafe { wr.__bindgen_anon_1.imm_data },
                opcode,
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

    pub(crate) fn opcode(&self) -> WorkReqOpCode {
        self.base.opcode
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
    pub(crate) opcode: WorkReqOpCode,
}

impl SendWrBase {
    pub(crate) fn new(
        wr_id: u64,
        send_flags: u32,
        laddr: u64,
        length: u32,
        lkey: u32,
        imm_data: u32,
        opcode: WorkReqOpCode,
    ) -> Self {
        Self {
            wr_id,
            send_flags,
            laddr,
            length,
            lkey,
            imm_data,
            opcode,
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

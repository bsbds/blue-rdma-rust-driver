enum CsrIndex {
    BaseAddrLow = 0x0,
    BaseAddrHigh = 0x1,
    Head = 0x2,
    Tail = 0x3,
}

#[allow(clippy::as_conversions, clippy::arithmetic_side_effects)]
const fn generate_csr_addr(
    is_cmdq: bool,
    queue_index: usize,
    is_2h: bool,
    reg_index: CsrIndex,
) -> usize {
    let t = if is_cmdq {
        0x40 + (if is_2h { 4 } else { 0 }) + (reg_index as usize)
    } else {
        (queue_index * 0x10) + (if is_2h { 4 } else { 0 }) + (reg_index as usize)
    };
    t << 2
}

macro_rules! generate_qp_array {
    ($is_recv:expr, $reg_index:expr) => {
        [
            generate_csr_addr(false, 0, $is_recv, $reg_index),
            generate_csr_addr(false, 1, $is_recv, $reg_index),
            generate_csr_addr(false, 2, $is_recv, $reg_index),
            generate_csr_addr(false, 3, $is_recv, $reg_index),
        ]
    };
}

pub(super) const NUM_QPS: usize = 4;

pub(super) const QP_WQE_ADDR_LOW: [usize; NUM_QPS] =
    generate_qp_array!(false, CsrIndex::BaseAddrLow);
pub(super) const QP_WQE_ADDR_HIGH: [usize; NUM_QPS] =
    generate_qp_array!(false, CsrIndex::BaseAddrHigh);
pub(super) const QP_WQE_HEAD: [usize; NUM_QPS] = generate_qp_array!(false, CsrIndex::Head);
pub(super) const QP_WQE_TAIL: [usize; NUM_QPS] = generate_qp_array!(false, CsrIndex::Tail);
pub(super) const QP_RECV_ADDR_LOW: [usize; NUM_QPS] =
    generate_qp_array!(true, CsrIndex::BaseAddrLow);
pub(super) const QP_RECV_ADDR_HIGH: [usize; NUM_QPS] =
    generate_qp_array!(true, CsrIndex::BaseAddrHigh);
pub(super) const QP_RECV_HEAD: [usize; NUM_QPS] = generate_qp_array!(true, CsrIndex::Head);
pub(super) const QP_RECV_TAIL: [usize; NUM_QPS] = generate_qp_array!(true, CsrIndex::Tail);

pub(super) const CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW: usize =
    generate_csr_addr(true, 0, false, CsrIndex::BaseAddrLow);
pub(super) const CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH: usize =
    generate_csr_addr(true, 0, false, CsrIndex::BaseAddrHigh);
pub(super) const CSR_ADDR_CMD_REQ_QUEUE_HEAD: usize =
    generate_csr_addr(true, 0, false, CsrIndex::Head);
pub(super) const CSR_ADDR_CMD_REQ_QUEUE_TAIL: usize =
    generate_csr_addr(true, 0, false, CsrIndex::Tail);
pub(super) const CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW: usize =
    generate_csr_addr(true, 0, true, CsrIndex::BaseAddrLow);
pub(super) const CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH: usize =
    generate_csr_addr(true, 0, true, CsrIndex::BaseAddrHigh);
pub(super) const CSR_ADDR_CMD_RESP_QUEUE_HEAD: usize =
    generate_csr_addr(true, 0, true, CsrIndex::Head);
pub(super) const CSR_ADDR_CMD_RESP_QUEUE_TAIL: usize =
    generate_csr_addr(true, 0, true, CsrIndex::Tail);

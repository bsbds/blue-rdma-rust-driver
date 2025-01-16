use crate::queue::abstr::WrChunk;

#[derive(Debug, Clone, Copy)]
struct IbvSge {
    addr: u64,
    length: u32,
    lkey: u32,
}

struct SgeSplitter;

impl Iterator for SgeSplitter {
    type Item = WrChunk;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}
impl SgeSplitter {
    fn new(sge: IbvSge) {}
}

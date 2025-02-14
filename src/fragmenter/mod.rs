use crate::device_protocol::ChunkPos;

// TODO: rewrite `WrFragmenter`
mod chunk;
mod pakcet;

pub(crate) use pakcet::PacketFragmenter;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Fragmenter {
    segment_size: u64,
    align: u64,
    base_addr: u64,
    end_addr: u64,
}

impl Fragmenter {
    pub(crate) fn new(segment_size: u64, align: u64, base_addr: u64, length: u64) -> Self {
        Self {
            segment_size,
            align,
            base_addr,
            end_addr: base_addr + length,
        }
    }

    fn num_segments(&self) -> usize {
        if self.base_addr >= self.end_addr {
            return 0;
        }
        let first_aligned = ((self.base_addr + self.segment_size) & !(self.align - 1));
        let remaining_after_first = self.end_addr.saturating_sub(first_aligned);
        remaining_after_first.div_ceil(self.segment_size) as usize + 1
    }
}

pub(crate) struct IntoIter {
    segment_size: u64,
    align: u64,
    current_addr: u64,
    current_pos: ChunkPos,
    end_addr: u64,
    total_segments: usize,
    count: usize,
}

impl IntoIterator for Fragmenter {
    type Item = Fragment;
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        let num_segments = self.num_segments();
        let current_pos = if num_segments == 1 {
            ChunkPos::Only
        } else {
            ChunkPos::First
        };
        IntoIter {
            segment_size: self.segment_size,
            align: self.align,
            current_addr: self.base_addr,
            end_addr: self.end_addr,
            total_segments: self.num_segments(),
            current_pos,
            count: num_segments,
        }
    }
}

impl Iterator for IntoIter {
    type Item = Fragment;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            return None;
        }

        let end = ((self.current_addr + self.segment_size) & !(self.align - 1)).min(self.end_addr);
        let len = end - self.current_addr;
        let fragment = Fragment {
            addr: self.current_addr,
            len,
            pos: self.current_pos,
        };
        self.current_addr += len;
        self.count -= 1;
        let next_pos = match (self.count == 1, self.current_pos) {
            (_, ChunkPos::Only) => ChunkPos::Only,
            (true, _) | (false, ChunkPos::Last) => ChunkPos::Last,
            (false, ChunkPos::First | ChunkPos::Middle) => ChunkPos::Middle,
        };
        self.current_pos = next_pos;

        Some(fragment)
    }
}

impl ExactSizeIterator for IntoIter {
    fn len(&self) -> usize {
        self.total_segments
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fragment {
    addr: u64,
    len: u64,
    pos: ChunkPos,
}

impl Fragment {
    pub(crate) fn addr(&self) -> u64 {
        self.addr
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fragmentation() {
        let f = Fragmenter::new(1024, 256, 0x1, 2048);
        let expect = [
            Fragment {
                addr: 0x1,
                len: 1023,
                pos: ChunkPos::First,
            },
            Fragment {
                addr: 0x400,
                len: 1024,
                pos: ChunkPos::Middle,
            },
            Fragment {
                addr: 0x800,
                len: 1,
                pos: ChunkPos::Last,
            },
        ];
        assert!(f.into_iter().zip(expect).all(|(x, y)| x == y));
    }

    #[test]
    fn fragmentation_len() {
        let f = Fragmenter::new(256, 256, 0x0, 4096);
        assert_eq!(f.into_iter().len(), 16);
        let f = Fragmenter::new(256, 256, 0x1, 4096);
        assert_eq!(f.into_iter().len(), 17);
        let f = Fragmenter::new(256, 256, 0x01, 4097);
        assert_eq!(f.into_iter().len(), 17);
        let f = Fragmenter::new(256, 256, 0xff, 4096);
        assert_eq!(f.into_iter().len(), 17);
        let f = Fragmenter::new(1024, 256, 0x0, 4096);
        assert_eq!(f.into_iter().len(), 4);
        let f = Fragmenter::new(1024, 256, 0x1, 4096);
        assert_eq!(f.into_iter().len(), 5);
        let f = Fragmenter::new(1024, 256, 0x3ff, 4096);
        assert_eq!(f.into_iter().len(), 5);
    }
}

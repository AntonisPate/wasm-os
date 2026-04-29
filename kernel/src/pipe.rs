use alloc::collections::VecDeque;
use alloc::sync::Arc;
use spin::Mutex;

#[derive(Debug)]
pub struct PipeBuffer {
    pub data: VecDeque<u8>,
    pub capacity: usize,
    pub writer_count: usize,
    pub reader_count: usize,
}

impl PipeBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: VecDeque::new(),
            capacity,
            writer_count: 0,
            reader_count: 0,
        }
    }
}

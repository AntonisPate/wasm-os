use spin::Mutex;
use crate::shared_memory;

const QUEUE_SIZE: usize = 256;
const LINE_SIZE: usize = 256;

pub struct Tty {
    raw_input_queue: [u8; QUEUE_SIZE],
    head: usize,
    tail: usize,
    
    line_buffer: [u8; LINE_SIZE],
    line_len: usize,
    line_ready: bool,
}

impl Tty {
    pub const fn new() -> Self {
        Self {
            raw_input_queue: [0; QUEUE_SIZE],
            head: 0,
            tail: 0,
            line_buffer: [0; LINE_SIZE],
            line_len: 0,
            line_ready: false,
        }
    }

    pub fn enqueue_raw_input(&mut self, data: &[u8]) {
        for &b in data {
            if b != 0 {
                let next_tail = (self.tail + 1) % QUEUE_SIZE;
                if next_tail != self.head {
                    self.raw_input_queue[self.tail] = b;
                    self.tail = next_tail;
                }
            }
        }
    }

    fn pop_raw_input(&mut self) -> Option<u8> {
        if self.head == self.tail {
            None
        } else {
            let val = self.raw_input_queue[self.head];
            self.head = (self.head + 1) % QUEUE_SIZE;
            Some(val)
        }
    }

    pub fn process_input(&mut self) -> bool {
        while let Some(c) = self.pop_raw_input() {
            if c == b'\r' || c == b'\n' {
                shared_memory::write_to_shared_memory(b"\r\n");
                self.line_ready = true;
                return true;
            } else if c == 8 || c == 127 {
                if self.line_len > 0 {
                    self.line_len -= 1;
                    shared_memory::write_to_shared_memory(b"\x08 \x08");
                }
            } else if c >= 32 && c <= 126 {
                if self.line_len < LINE_SIZE {
                    self.line_buffer[self.line_len] = c;
                    self.line_len += 1;
                    let buf = [c];
                    shared_memory::write_to_shared_memory(&buf);
                }
            }
        }
        false
    }

    pub fn is_line_ready(&self) -> bool {
        self.line_ready
    }

    pub fn get_line(&self) -> &[u8] {
        &self.line_buffer[..self.line_len]
    }

    pub fn clear_line(&mut self) {
        self.line_len = 0;
        self.line_ready = false;
    }
}

pub static TTY: Mutex<Tty> = Mutex::new(Tty::new());

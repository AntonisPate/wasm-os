use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Tty,
}

pub struct Process {
    pub id: u32,
    pub memory_start: usize,
    pub size: usize,
    pub permissions: u32,
    pub state: ProcessState,
    pub entry_point: Option<fn()>,
    pub file_descriptors: [Option<FileType>; 8],
}

pub static PROCESS_TABLE: Mutex<Vec<Process>> = Mutex::new(Vec::new());
pub static mut NEXT_PID: u32 = 1;
pub static mut CURRENT_PROCESS: Option<u32> = None;

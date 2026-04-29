use alloc::vec::Vec;
use alloc::string::String;
use spin::Mutex;
use alloc::sync::Arc;
use crate::pipe::PipeBuffer;

#[derive(Clone)]
pub enum BlockedReason {
    Tty,
    Wait(u32),
    PipeRead(Arc<Mutex<PipeBuffer>>),
    PipeWrite(Arc<Mutex<PipeBuffer>>),
}

impl core::fmt::Debug for BlockedReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BlockedReason::Tty => write!(f, "Tty"),
            BlockedReason::Wait(p) => write!(f, "Wait({})", p),
            BlockedReason::PipeRead(_) => write!(f, "PipeRead"),
            BlockedReason::PipeWrite(_) => write!(f, "PipeWrite"),
        }
    }
}

impl PartialEq for BlockedReason {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BlockedReason::Tty, BlockedReason::Tty) => true,
            (BlockedReason::Wait(p1), BlockedReason::Wait(p2)) => p1 == p2,
            (BlockedReason::PipeRead(a1), BlockedReason::PipeRead(a2)) => Arc::ptr_eq(a1, a2),
            (BlockedReason::PipeWrite(a1), BlockedReason::PipeWrite(a2)) => Arc::ptr_eq(a1, a2),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked(BlockedReason),
    Terminated,
}

#[derive(Debug, Clone)]
pub enum FileType {
    Tty,
    File { path: String, offset: usize },
    PipeRead(Arc<Mutex<PipeBuffer>>),
    PipeWrite(Arc<Mutex<PipeBuffer>>),
}

impl PartialEq for FileType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FileType::Tty, FileType::Tty) => true,
            (FileType::File { path: p1, offset: o1 }, FileType::File { path: p2, offset: o2 }) => p1 == p2 && o1 == o2,
            (FileType::PipeRead(a1), FileType::PipeRead(a2)) => Arc::ptr_eq(a1, a2),
            (FileType::PipeWrite(a1), FileType::PipeWrite(a2)) => Arc::ptr_eq(a1, a2),
            _ => false,
        }
    }
}

impl Eq for FileType {}

pub struct Process {
    pub id: u32,
    pub memory_start: usize,
    pub size: usize,
    pub permissions: u32,
    pub state: ProcessState,
    pub entry_point: Option<fn(usize, *const *const u8)>,
    pub file_descriptors: [Option<FileType>; 8],
    pub argc: usize,
    pub argv: *const *const u8,
    pub arg_storage: Option<(Vec<Vec<u8>>, Vec<*const u8>)>,
    pub cwd: String,
}

unsafe impl Send for Process {}
unsafe impl Sync for Process {}

pub static PROCESS_TABLE: Mutex<Vec<Process>> = Mutex::new(Vec::new());
pub static mut NEXT_PID: u32 = 1;
pub static mut CURRENT_PROCESS: Option<u32> = None;

use alloc::vec::Vec;
use alloc::string::String;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    Tty,
    File { path: String, offset: usize },
}

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

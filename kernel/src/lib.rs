#![no_std]
extern crate alloc;

mod commands;
mod dynamic_memory;
mod fs;
mod process;
mod shared_memory;
mod shell;
mod syscalls;
mod tty;
mod vfs;

use core::fmt::{self, Write};
use process::{CURRENT_PROCESS, NEXT_PID, PROCESS_TABLE, ProcessState};

static mut INITIALIZED: bool = false;

#[unsafe(no_mangle)]
pub extern "C" fn kernel_init() {
    unsafe {
        if INITIALIZED {
            return;
        }
        INITIALIZED = true;
    }

    // Spawn the Shell as PID 1
    kernel_spawn(0x100000, 0x10000, 3, None, alloc::string::String::from("/")); // Symbolic memory
    let mut table = PROCESS_TABLE.lock();
    if let Some(shell) = table.iter_mut().find(|p| p.id == 1) {
        shell.entry_point = Some(shell::shell_main);
        shell.state = ProcessState::Ready;
    }
}

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    // Δηλώνουμε τη JS συνάρτηση
    pub unsafe fn host_log(ptr: *const u8, len: usize);
}

struct HostLogger;

impl Write for HostLogger {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            host_log(s.as_ptr(), s.len());
        }
        Ok(())
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Χρησιμοποιούμε το writeln! macro που γράφει κατευθείαν
    // στο HostLogger ΧΩΡΙΣ να κάνει allocate String.
    let mut logger = HostLogger;
    let _ = writeln!(logger, "{}", info);

    loop {} // Σταματάμε την εκτέλεση
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_spawn(ptr: usize, size: usize, perms: u32, stdout_type: Option<process::FileType>, cwd: alloc::string::String) -> u32 {
    let mut table = PROCESS_TABLE.lock();
    let pid = unsafe {
        let current = NEXT_PID;
        NEXT_PID += 1;
        current
    };

    let mut fds = [const { None }; 8];
    fds[vfs::STDIN as usize] = Some(process::FileType::Tty); // stdin
    fds[vfs::STDOUT as usize] = Some(stdout_type.unwrap_or(process::FileType::Tty)); // stdout (inherited or TTY)

    table.push(process::Process {
        id: pid,
        memory_start: ptr,
        size,
        permissions: perms,
        state: process::ProcessState::Running,
        entry_point: None,
        file_descriptors: fds,
        argc: 0,
        argv: core::ptr::null(),
        arg_storage: None,
        cwd,
    });

    pid
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_loop() {
    // 0. Ensure init
    unsafe {
        if !INITIALIZED {
            kernel_init();
        }
    }

    // 1. Hardware Interrupt Routine (ISR)
    if shared_memory::is_input_ready() {
        let mut my_local_buffer = [0u8; 128];
        shared_memory::read_from_shared_memory(&mut my_local_buffer);

        let actual_len = my_local_buffer.iter().position(|&b| b == 0).unwrap_or(128);

        if actual_len > 0 {
            tty::TTY
                .lock()
                .enqueue_raw_input(&my_local_buffer[..actual_len]);
        }

        shared_memory::set_input_empty();
    }

    // 2. TTY Driver Processing
    let line_ready = tty::TTY.lock().process_input();

    // 3. Scheduler (Minimal)
    let mut i = 0;
    loop {
        let mut table = PROCESS_TABLE.lock();
        if i >= table.len() {
            break;
        }
        
        let pid = table[i].id;

        // Unblock processes waiting for TTY
        if table[i].state == ProcessState::Blocked {
            if line_ready || tty::TTY.lock().is_line_ready() {
                table[i].state = ProcessState::Ready;
            }
        }

        if table[i].state == ProcessState::Ready || table[i].state == ProcessState::Running {
            if let Some(entry) = table[i].entry_point {
                unsafe {
                    CURRENT_PROCESS = Some(pid);
                }
                let argc = table[i].argc;
                let argv = table[i].argv;

                // Release the lock before calling the process to avoid deadlocks
                // if the process makes a syscall that needs the table lock.
                drop(table);

                entry(argc, argv);

                // Re-acquire lock and check if process exited or blocked itself
                table = PROCESS_TABLE.lock();
                if table[i].state == ProcessState::Running {
                    table[i].state = ProcessState::Ready;
                }
                unsafe {
                    CURRENT_PROCESS = None;
                }
            }
        }
        i += 1;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_alloc(size: usize) -> *mut u8 {
    dynamic_memory::fast_alloc(size)
}

#[unsafe(no_mangle)]
pub extern "C" fn check_access(pid: u32, ptr: usize, len: usize, mode: u32) -> bool {
    let table = PROCESS_TABLE.lock();
    if let Some(proc) = table.iter().find(|p| p.id == pid) {
        let in_bounds = ptr >= proc.memory_start && (ptr + len) <= (proc.memory_start + proc.size);
        let has_perm = (proc.permissions & mode) == mode;
        return in_bounds && has_perm;
    }
    false
}

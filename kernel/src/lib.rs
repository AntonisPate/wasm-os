#![no_std]
extern crate alloc;

mod commands;
mod dynamic_memory;
mod fs;
mod pipe;
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
        if INITIALIZED { return; }
        INITIALIZED = true;
    }

    // Initialize the root filesystem
    {
        let mut fs_root = fs::RAM_FS.lock();
        if let fs::FsNode::Directory(entries) = &mut *fs_root {
            entries.insert(alloc::string::String::from("dev"), fs::FsNode::Directory(alloc::collections::BTreeMap::new()));
        }
    }

    // Spawn the Shell as PID 1
    kernel_spawn(0x100000, 0x10000, 3, None, None, alloc::string::String::from("/")); // Symbolic memory
    let mut table = PROCESS_TABLE.lock();
    if let Some(shell) = table.iter_mut().find(|p| p.id == 1) {
        shell.entry_point = Some(shell::shell_main);
        shell.state = ProcessState::Ready;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_spawn(
    ptr: usize, 
    size: usize, 
    perms: u32, 
    stdin_type: Option<process::FileType>,
    stdout_type: Option<process::FileType>, 
    cwd: alloc::string::String
) -> u32 {
    let mut table = PROCESS_TABLE.lock();
    let pid = unsafe {
        let current = NEXT_PID;
        NEXT_PID += 1;
        current
    };

    let mut fds = [const { None }; 8];
    fds[vfs::STDIN as usize] = stdin_type.or(Some(process::FileType::Tty));
    fds[vfs::STDOUT as usize] = stdout_type.or(Some(process::FileType::Tty));

    // When a child inherits a pipe end, increment the reference count so that
    // the shell closing its own copy of the FD doesn't prematurely signal EOF/EPIPE.
    for fd in &fds {
        match fd {
            Some(process::FileType::PipeWrite(buf)) => { buf.lock().writer_count += 1; }
            Some(process::FileType::PipeRead(buf))  => { buf.lock().reader_count += 1; }
            _ => {}
        }
    }

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
    unsafe {
        if !INITIALIZED { kernel_init(); }
    }

    if shared_memory::is_input_ready() {
        let mut my_local_buffer = [0u8; 128];
        shared_memory::read_from_shared_memory(&mut my_local_buffer);
        let actual_len = my_local_buffer.iter().position(|&b| b == 0).unwrap_or(128);
        if actual_len > 0 {
            tty::TTY.lock().enqueue_raw_input(&my_local_buffer[..actual_len]);
        }
        shared_memory::set_input_empty();
    }

    let line_ready = tty::TTY.lock().process_input();

    let mut i = 0;
    loop {
        let mut table = PROCESS_TABLE.lock();
        if i >= table.len() { break; }

        if table[i].state == ProcessState::Terminated {
            i += 1;
            continue;
        }

        let pid = table[i].id;
        let mut should_ready = false;

        if let ProcessState::Blocked(ref reason) = table[i].state {
            match reason {
                process::BlockedReason::Tty => {
                    if line_ready || tty::TTY.lock().is_line_ready() { should_ready = true; }
                }
                process::BlockedReason::Wait(target_pid) => {
                    let target_pid_val = *target_pid;
                    if table.iter().any(|p| p.id == target_pid_val && p.state == ProcessState::Terminated) {
                        should_ready = true;
                    }
                }
                process::BlockedReason::PipeRead(buf) => {
                    let pipe = buf.lock();
                    if !pipe.data.is_empty() || pipe.writer_count == 0 { should_ready = true; }
                }
                process::BlockedReason::PipeWrite(buf) => {
                    let pipe = buf.lock();
                    if pipe.data.len() < pipe.capacity { should_ready = true; }
                }
            }
        }

        if should_ready {
            table[i].state = ProcessState::Ready;
        }

        if table[i].state == ProcessState::Ready || table[i].state == ProcessState::Running {
            let argc = table[i].argc;
            let argv = table[i].argv;
            let entry_opt = table[i].entry_point;
            
            if let Some(entry) = entry_opt {
                table[i].state = ProcessState::Running;
                drop(table);
                unsafe { CURRENT_PROCESS = Some(pid); }
                entry(argc, argv);
                unsafe { CURRENT_PROCESS = None; }
            } else { drop(table); }
        } else { drop(table); }

        i += 1;
    }
}

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    pub unsafe fn host_log(ptr: *const u8, len: usize);
}

struct HostLogger;

impl Write for HostLogger {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe { host_log(s.as_ptr(), s.len()); }
        Ok(())
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let mut logger = HostLogger;
    let _ = writeln!(logger, "{}", info);
    loop {}
}

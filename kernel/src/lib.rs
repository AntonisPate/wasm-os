#![no_std]
extern crate alloc;

mod dynamic_memory;
mod process;
mod shared_memory;
mod shell;
mod syscalls;
mod tty;

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
    kernel_spawn(0x100000, 0x10000, 3); // Symbolic memory
    let mut table = PROCESS_TABLE.lock();
    if let Some(shell) = table.iter_mut().find(|p| p.id == 1) {
        shell.entry_point = Some(shell::shell_main);
        shell.state = ProcessState::Ready;
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    shared_memory::write_to_shared_memory(b"PANIC!");
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_spawn(ptr: usize, size: usize, perms: u32) -> u32 {
    let mut table = PROCESS_TABLE.lock();
    let pid = unsafe {
        let current = NEXT_PID;
        NEXT_PID += 1;
        current
    };

    table.push(process::Process {
        id: pid,
        memory_start: ptr,
        size,
        permissions: perms,
        state: process::ProcessState::Running,
        entry_point: None,
    });

    let msg = "Process spawned successfully!";
    shared_memory::write_to_shared_memory(msg.as_bytes());
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
    let mut table = PROCESS_TABLE.lock();
    for i in 0..table.len() {
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
                table[i].state = ProcessState::Running;

                // Release the lock before calling the process to avoid deadlocks
                // if the process makes a syscall that needs the table lock.
                drop(table);

                entry();

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

use core::ptr::{addr_of, addr_of_mut};

const BUFFER_SIZE: usize = 1024;

const BUFFER_EMPTY: u8 = 0;
const BUFFER_READ: u8 = 1;
const BUFFER_EDIT: u8 = 2;
const BUFFER_READY: u8 = 3;

static mut INPUT_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]; //INPUT_BUFFER[0] is status

static mut OUTPUT_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];

pub fn write_to_shared_memory(data: &[u8]) {
    unsafe {
        let ptr = addr_of_mut!(OUTPUT_BUFFER) as *mut u8;
        let mut offset = 4;

        // If the buffer is already READY, we append to it instead of overwriting.
        // We find the end of the existing data by looking for the first null byte.
        if core::ptr::read_volatile(ptr) == BUFFER_READY {
            while offset < BUFFER_SIZE && core::ptr::read_volatile(ptr.add(offset)) != 0 {
                offset += 1;
            }
        }

        let remaining = BUFFER_SIZE.saturating_sub(offset);
        let len = data.len().min(remaining);
        
        if len > 0 {
            core::ptr::copy_nonoverlapping(data.as_ptr(), ptr.add(offset), len);
            // Ensure we set the status to READY
            core::ptr::write_volatile(ptr, BUFFER_READY);
        }
    }
}

pub fn read_from_shared_memory(buffer: &mut [u8]) {
    unsafe {
        let len = buffer.len().min(BUFFER_SIZE - 4);
        core::ptr::copy_nonoverlapping(
            (addr_of!(INPUT_BUFFER) as *const u8).add(4),
            buffer.as_mut_ptr(),
            len,
        );
    }
}

pub fn set_input_empty() {
    unsafe {
        let ptr = addr_of_mut!(INPUT_BUFFER) as *mut u8;
        core::ptr::write_volatile(ptr, BUFFER_EMPTY);
    }
}

pub fn set_input_ready() {
    unsafe {
        INPUT_BUFFER[0] = BUFFER_READY;
    }
}

pub fn set_input_edit() {
    unsafe {
        INPUT_BUFFER[0] = BUFFER_EDIT;
    }
}

pub fn is_input_ready() -> bool {
    unsafe {
        let ptr = addr_of!(INPUT_BUFFER) as *const u8;
        core::ptr::read_volatile(ptr) == BUFFER_READY
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn get_input_buffer_ptr() -> *mut u8 {
    addr_of_mut!(INPUT_BUFFER) as *mut u8
}

#[unsafe(no_mangle)]
pub extern "C" fn get_output_buffer_ptr() -> *const u8 {
    addr_of!(OUTPUT_BUFFER) as *const u8
}

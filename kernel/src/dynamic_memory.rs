use core::ptr::{addr_of_mut, null_mut};

const HEAP_SIZE: usize = 1024 * 1024;

static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

static mut NEXT_FREE: usize = 0;

#[global_allocator]
static ALLOCATOR: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

pub fn fast_alloc(size: usize) -> *mut u8 {
    unsafe {
        if NEXT_FREE + size > HEAP_SIZE {
            return null_mut();
        }
        let ptr = (addr_of_mut!(HEAP) as *mut u8).add(NEXT_FREE);
        NEXT_FREE += size;
        ptr
    }
}

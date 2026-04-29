use core::alloc::{GlobalAlloc, Layout};
use dlmalloc::Dlmalloc;
use spin::Mutex;

// 1. Δημιουργούμε έναν System Allocator που εξηγεί στο dlmalloc πώς να ζητάει σελίδες μνήμης (pages)
struct WasmSystem;

unsafe impl dlmalloc::Allocator for WasmSystem {
    fn alloc(&self, size: usize) -> (*mut u8, usize, u32) {
        let pages = (size / 65536) + 1;
        let prev = core::arch::wasm32::memory_grow(0, pages);
        if prev == usize::MAX {
            (core::ptr::null_mut(), 0, 0)
        } else {
            // Επιστρέφει: (δείκτης μνήμης, μέγεθος σε bytes, flags)
            ((prev * 65536) as *mut u8, pages * 65536, 0)
        }
    }

    fn remap(&self, _ptr: *mut u8, _oldsize: usize, _newsize: usize, _can_move: bool) -> *mut u8 {
        core::ptr::null_mut()
    }
    fn free_part(&self, _ptr: *mut u8, _oldsize: usize, _newsize: usize) -> bool {
        false
    }
    fn free(&self, _ptr: *mut u8, _size: usize) -> bool {
        false
    }
    fn can_release_part(&self, _flags: u32) -> bool {
        false
    }
    fn allocates_zeros(&self) -> bool {
        true
    }
    fn page_size(&self) -> usize {
        65536
    }
}

// 2. Τυλίγουμε τον πυρήνα της dlmalloc με το ΔΙΚΟ ΜΑΣ thread-safe lock (spin::Mutex)
static INNER_ALLOCATOR: Mutex<Dlmalloc<WasmSystem>> =
    Mutex::new(Dlmalloc::new_with_allocator(WasmSystem));

struct SafeGlobalAlloc;

unsafe impl GlobalAlloc for SafeGlobalAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        INNER_ALLOCATOR.lock().malloc(layout.size(), layout.align())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        INNER_ALLOCATOR
            .lock()
            .free(ptr, layout.size(), layout.align())
    }
}

// 3. Εγκαθιστούμε τον δικό μας ασφαλή Allocator
#[global_allocator]
static ALLOCATOR: SafeGlobalAlloc = SafeGlobalAlloc;

// Wrapper για JS/C calls (αν χρειαστεί στο μέλλον)
pub fn fast_alloc(size: usize) -> *mut u8 {
    unsafe { ALLOCATOR.alloc(Layout::from_size_align_unchecked(size, 8)) }
}

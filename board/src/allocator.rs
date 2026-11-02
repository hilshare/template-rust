//! A bump allocator for crates that need `alloc`. It never frees, so each block stays until the
//! run ends. With no call of `alloc`, the linker removes the arena.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

/// The arena, the stack and the static data must fit in `LIBTOCK_LINKER_RAM_LENGTH`.
const ARENA_SIZE: usize = 8 * 1024;

struct Arena(UnsafeCell<[u8; ARENA_SIZE]>);

// SAFETY: a Tock process has one thread.
unsafe impl Sync for Arena {}

static ARENA: Arena = Arena(UnsafeCell::new([0; ARENA_SIZE]));
static USED_BYTES: AtomicUsize = AtomicUsize::new(0);

struct BumpAllocator;

// SAFETY: each block is inside `ARENA`, has the alignment of its layout, and overlaps no other
// block, because `USED_BYTES` only grows.
unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let arena_start = ARENA.0.get() as usize;
        let free_start = arena_start + USED_BYTES.load(Ordering::Relaxed);
        let start = (free_start + layout.align() - 1) & !(layout.align() - 1);
        let end = start + layout.size();
        if end > arena_start + ARENA_SIZE {
            return core::ptr::null_mut();
        }
        USED_BYTES.store(end - arena_start, Ordering::Relaxed);
        start as *mut u8
    }

    unsafe fn dealloc(&self, _pointer: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

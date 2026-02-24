use alloc::alloc::handle_alloc_error;
use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::NonNull,
};
use customizable_buddy::{BuddyAllocator, LinkedListBuddy, UsizeBuddy};

/// 初始化全局分配器和内核堆分配器。
pub fn init() {
    // 托管空间 64 KiB（exec 后 format/println 等需更大缓冲区，16KiB 易导致 4KB 分配失败）
    const MEMORY_SIZE: usize = 64 << 10;
    static mut MEMORY: [u8; MEMORY_SIZE] = [0u8; MEMORY_SIZE];
    unsafe {
        HEAP.init(
            core::mem::size_of::<usize>().trailing_zeros() as _,
            NonNull::new(MEMORY.as_mut_ptr()).unwrap(),
        );
        HEAP.transfer(NonNull::new_unchecked(MEMORY.as_mut_ptr()), MEMORY.len());
    }
}

type MutAllocator<const N: usize> = BuddyAllocator<N, UsizeBuddy, LinkedListBuddy>;
static mut HEAP: MutAllocator<32> = MutAllocator::new();

struct Global;

#[global_allocator]
static GLOBAL: Global = Global;

unsafe impl GlobalAlloc for Global {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if let Ok((ptr, _)) = HEAP.allocate_layout::<u8>(layout) {
            ptr.as_ptr()
        } else {
            handle_alloc_error(layout)
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        HEAP.deallocate_layout(NonNull::new(ptr).unwrap(), layout)
    }
}

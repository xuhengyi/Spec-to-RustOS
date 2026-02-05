//! 内核堆分配器：通过 `#[global_allocator]` 提供基于 buddy allocator 的全局分配器，
//! 暴露 `init` / `transfer` 供内核初始化与托管内存。

#![cfg_attr(not(test), no_std)]

#[cfg(not(test))]
extern crate alloc;

use core::cell::UnsafeCell;
use core::ptr::NonNull;
use customizable_buddy::{BuddyAllocator, LinkedListBuddy, UsizeBuddy};

#[cfg(not(test))]
use alloc::alloc::{handle_alloc_error, Layout};
#[cfg(not(test))]
use core::alloc::GlobalAlloc;

/// 伙伴分配器类型：阶数 21，最大可管理约 2^30 字节（约 1 GiB）。
type Buddy = BuddyAllocator<21, UsizeBuddy, LinkedListBuddy>;

/// 无锁包装：调用方必须保证不存在并发的 alloc/dealloc/transfer（见 spec）。
struct BuddyCell(UnsafeCell<Buddy>);

unsafe impl Sync for BuddyCell {}

static BUDDY: BuddyCell = BuddyCell(UnsafeCell::new(BuddyAllocator::new()));

/// 初始化全局堆分配器。
///
/// 调用方必须保证 `base_address` 非零且在内核地址空间中可安全解引用/写入。
/// 在首次堆分配或 `transfer` 前必须调用一次（可多次调用，行为由底层实现决定）。
pub fn init(base_address: usize) {
    let base = NonNull::new(base_address as *mut u8).unwrap();
    // min_order = 6，与 design 中的容量估算一致；调用方须保证 base 与 transfer 区域按 2^6 对齐。
    const MIN_ORDER: usize = 6;
    unsafe {
        (*BUDDY.0.get()).init(MIN_ORDER, base);
    }
}

/// 将一段内存托管给全局堆分配器。
///
/// # Safety
///
/// 调用方必须保证：已调用过 `init`；`region` 与已托管区域不重叠；
/// `region` 未被其他对象引用；`region` 在内核中可安全访问。
pub unsafe fn transfer(region: &'static mut [u8]) {
    let ptr = NonNull::new(region.as_mut_ptr()).unwrap();
    (*BUDDY.0.get()).transfer(ptr, region.len());
}

#[allow(dead_code)]
struct KernelAlloc;

/// 单元测试二进制无 ctor，使用系统分配器避免在 init/transfer 前分配失败。
#[cfg(test)]
#[global_allocator]
static ALLOC: std::alloc::System = std::alloc::System;

#[cfg(not(test))]
#[global_allocator]
static ALLOC: KernelAlloc = KernelAlloc;

#[cfg(not(test))]
unsafe impl GlobalAlloc for KernelAlloc {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match (*BUDDY.0.get()).allocate_layout::<u8>(layout) {
            Ok((ptr, _)) => ptr.as_ptr(),
            Err(_) => handle_alloc_error(layout),
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Some(non_null) = NonNull::new(ptr) {
            (*BUDDY.0.get()).deallocate_layout(non_null, layout);
        }
    }
}

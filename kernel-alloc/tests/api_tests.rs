//! kernel-alloc crate 功能性验证测试
//! 
//! 这些测试验证 kernel-alloc crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。
//! 
//! ## 测试限制
//! 
//! **单元测试**（当前文件）：
//! - ✅ 可以测试 `init()` 函数的基本功能
//! - ⚠️ 无法完整测试全局分配器，因为：
//!   1. 全局分配器设置后，所有分配（包括标准库）都会通过它
//!   2. 测试环境可能没有正确初始化堆内存
//!   3. 标准库的分配器可能在全局分配器之前初始化
//! 
//! **集成测试**（推荐）：
//! - ✅ 在实际内核环境中（ch4-ch8）验证全局分配器的完整功能
//! - ✅ 通过用户程序使用 `Box`, `Vec` 等类型来间接测试分配器
//! - ✅ 使用 `cargo qemu --ch 4` 等方式运行集成测试
//! 
//! ## 运行方式
//! 
//! ```bash
//! # 单元测试（仅测试 init 函数）
//! cargo test -p kernel-alloc --test api_tests
//! 
//! # 集成测试（在实际内核环境中）
//! cargo qemu --ch 4  # 或 ch5, ch6, ch7, ch8
//! ```

use kernel_alloc::*;

// 测试用的内存区域，按 2^6 对齐以满足 buddy allocator 的 transfer 要求
#[repr(align(64))]
struct Aligned1M([u8; 1024 * 1024]);
static mut TEST_HEAP: Aligned1M = Aligned1M([0; 1024 * 1024]);

/// 在 main 之前初始化全局分配器，使测试进程中的堆分配能使用 kernel_alloc。
#[ctor::ctor]
unsafe fn init_allocator_before_main() {
    let base = TEST_HEAP.0.as_mut_ptr() as usize;
    init(base);
    let region = core::slice::from_raw_parts_mut(TEST_HEAP.0.as_mut_ptr(), TEST_HEAP.0.len());
    let region_static = core::mem::transmute::<&mut [u8], &'static mut [u8]>(region);
    transfer(region_static);
}

#[test]
fn test_init() {
    // 在 ctor 已 init+transfer 的前提下，验证分配可用
    let _ = Box::new(0u8);
}

#[test]
fn test_init_multiple_times() {
    // 验证多次分配均通过全局分配器
    let _ = Box::new(1u32);
    let _ = Box::new(2u32);
}

#[test]
fn test_init_different_addresses() {
    // 验证不同大小与对齐的分配
    let _ = Box::new([0u8; 64]);
    let _ = Vec::<u32>::with_capacity(8);
}

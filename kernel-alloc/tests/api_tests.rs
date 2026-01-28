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

// 测试用的内存区域
static mut TEST_HEAP: [u8; 1024 * 1024] = [0; 1024 * 1024]; // 1MB 测试堆

#[test]
fn test_init() {
    // 测试 init 函数不会 panic
    unsafe {
        let base_address = TEST_HEAP.as_mut_ptr() as usize;
        init(base_address);
    }
}

#[test]
fn test_init_multiple_times() {
    // 测试多次调用 init 不会 panic
    unsafe {
        let base_address = TEST_HEAP.as_mut_ptr() as usize;
        init(base_address);
        init(base_address); // 再次调用
    }
}

#[test]
fn test_init_different_addresses() {
    // 测试使用不同地址初始化
    unsafe {
        let base_address1 = TEST_HEAP.as_mut_ptr() as usize;
        init(base_address1);
        
        // 使用堆中的不同地址
        let base_address2 = TEST_HEAP.as_mut_ptr().add(1024) as usize;
        init(base_address2);
    }
}

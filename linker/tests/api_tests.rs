//! linker crate 功能性验证测试
//! 
//! 这些测试验证 linker crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。

use linker::*;

#[test]
fn test_script_not_empty() {
    // 验证链接脚本不为空
    assert!(!SCRIPT.is_empty());
    assert!(SCRIPT.len() > 100);
}

#[test]
fn test_script_contains_sections() {
    // 验证链接脚本包含必要的段
    let script_str = core::str::from_utf8(SCRIPT).unwrap();
    assert!(script_str.contains(".text"));
    assert!(script_str.contains(".rodata"));
    assert!(script_str.contains(".data"));
    assert!(script_str.contains(".bss"));
    assert!(script_str.contains(".boot"));
    assert!(script_str.contains("__start"));
    assert!(script_str.contains("__end"));
}

#[test]
fn test_script_contains_riscv_arch() {
    // 验证链接脚本包含 RISC-V 架构定义
    let script_str = core::str::from_utf8(SCRIPT).unwrap();
    assert!(script_str.contains("riscv") || script_str.contains("RISCV"));
}

#[test]
fn test_kernel_layout_init() {
    // 测试 KernelLayout::INIT
    let layout = KernelLayout::INIT;
    
    // 验证 INIT 的 start 和 end 都是 usize::MAX
    assert_eq!(layout.start(), usize::MAX);
    assert_eq!(layout.end(), usize::MAX);
    
    // len() 应该返回 end - text
    // 注意：对于 INIT，len() 会是 0（因为 usize::MAX - usize::MAX = 0）
    let len = layout.len();
    assert_eq!(len, 0);
}

#[test]
fn test_kernel_layout_methods() {
    // 测试 KernelLayout 的方法（使用 INIT 值）
    let layout = KernelLayout::INIT;
    
    // start() 应该返回 text 地址
    assert_eq!(layout.start(), usize::MAX);
    
    // end() 应该返回 end 地址
    assert_eq!(layout.end(), usize::MAX);
    
    // len() 应该返回 end - text
    // 注意：对于 INIT，len() 会是 0（因为 usize::MAX - usize::MAX = 0）
    let len = layout.len();
    assert_eq!(len, 0);
}

#[test]
fn test_kernel_layout_iter() {
    // 测试 KernelLayout 的迭代器
    let layout = KernelLayout::INIT;
    
    // 应该能迭代出 4 个区域
    let regions: Vec<_> = layout.iter().collect();
    assert_eq!(regions.len(), 4);
    
    // 验证区域顺序
    let mut iter2 = layout.iter();
    let region1 = iter2.next().unwrap();
    assert!(matches!(region1.title, KernelRegionTitle::Text));
    
    let region2 = iter2.next().unwrap();
    assert!(matches!(region2.title, KernelRegionTitle::Rodata));
    
    let region3 = iter2.next().unwrap();
    assert!(matches!(region3.title, KernelRegionTitle::Data));
    
    let region4 = iter2.next().unwrap();
    assert!(matches!(region4.title, KernelRegionTitle::Boot));
    
    assert!(iter2.next().is_none());
}

#[test]
fn test_kernel_region_display() {
    // 测试 KernelRegion 的 Display trait
    let layout = KernelLayout::INIT;
    let mut iter = layout.iter();
    
    let text_region = iter.next().unwrap();
    let display_str = format!("{}", text_region);
    assert!(display_str.contains(".text"));
    assert!(display_str.contains("0x"));
    
    let rodata_region = iter.next().unwrap();
    let display_str = format!("{}", rodata_region);
    assert!(display_str.contains(".rodata"));
    
    let data_region = iter.next().unwrap();
    let display_str = format!("{}", data_region);
    assert!(display_str.contains(".data"));
    
    let boot_region = iter.next().unwrap();
    let display_str = format!("{}", boot_region);
    assert!(display_str.contains(".boot"));
}

#[test]
fn test_kernel_region_range() {
    // 测试 KernelRegion 的地址范围
    let layout = KernelLayout::INIT;
    let mut iter = layout.iter();
    
    // 对于 INIT，所有地址都是 usize::MAX，所以范围会是 MAX..MAX
    let text_region = iter.next().unwrap();
    // 验证 range 是有效的 Range<usize>
    assert!(text_region.range.start <= text_region.range.end);
    
    // 验证所有区域的 range 都是有效的
    for region in layout.iter() {
        assert!(region.range.start <= region.range.end);
    }
}

#[test]
fn test_kernel_region_title_clone_copy() {
    // 测试 KernelRegionTitle 的 Clone 和 Copy
    let title1 = KernelRegionTitle::Text;
    let title2 = title1; // Copy
    let title3 = title1.clone(); // Clone
    
    // 验证它们相等
    assert!(matches!(title1, KernelRegionTitle::Text));
    assert!(matches!(title2, KernelRegionTitle::Text));
    assert!(matches!(title3, KernelRegionTitle::Text));
}

#[test]
fn test_app_meta_structure() {
    // 测试 AppMeta 结构体存在
    // 注意：locate() 需要链接时存在 apps 符号，在测试环境中可能不存在
    // 这里只验证结构体和方法签名正确
    let _meta_size = core::mem::size_of::<AppMeta>();
    assert_eq!(core::mem::size_of::<AppMeta>(), 32); // 4 * u64 = 32 bytes
}

#[test]
fn test_app_iterator_structure() {
    // 测试 AppIterator 结构体存在
    let _iter_size = core::mem::size_of::<AppIterator>();
    // AppIterator 包含一个指针和一个 u64，大小取决于平台
    assert!(core::mem::size_of::<AppIterator>() > 0);
}

#![no_std]

//! linker crate 提供内核链接脚本、启动入口和布局查询功能

/// 链接脚本文本（字节序列）
/// 
/// 该链接脚本用于 RISC-V 内核，定义了段布局和导出符号。
pub const SCRIPT: &[u8] = include_bytes!("linker.ld");

/// 内核布局信息与操作
pub struct KernelLayout {
    text: usize,
    rodata: usize,
    data: usize,
    sbss: usize,
    ebss: usize,
    boot: usize,
    end: usize,
}

impl KernelLayout {
    /// 初始化值，所有字段为 usize::MAX
    pub const INIT: KernelLayout = KernelLayout {
        text: usize::MAX,
        rodata: usize::MAX,
        data: usize::MAX,
        sbss: usize::MAX,
        ebss: usize::MAX,
        boot: usize::MAX,
        end: usize::MAX,
    };

    /// 通过读取链接符号地址定位布局
    pub fn locate() -> Self {
        extern "C" {
            static __start: u8;
            static __rodata: u8;
            static __data: u8;
            static __sbss: u8;
            static __ebss: u8;
            static __boot: u8;
            static __end: u8;
        }

        unsafe {
            Self {
                text: &__start as *const u8 as usize,
                rodata: &__rodata as *const u8 as usize,
                data: &__data as *const u8 as usize,
                sbss: &__sbss as *const u8 as usize,
                ebss: &__ebss as *const u8 as usize,
                boot: &__boot as *const u8 as usize,
                end: &__end as *const u8 as usize,
            }
        }
    }

    /// 返回内核起始地址（__start）
    pub fn start(&self) -> usize {
        self.text
    }

    /// 返回内核结束地址（__end）
    pub fn end(&self) -> usize {
        self.end
    }

    /// 返回内核长度（end - start）
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.text)
    }

    /// 将地址区间 [__sbss, __ebss) 清零
    /// 
    /// 使用 volatile 写入以确保对其他处理器核可见。
    pub unsafe fn zero_bss(&self) {
        let start = self.sbss;
        let end = self.ebss;
        let mut ptr = start as *mut u8;
        let end_ptr = end as *mut u8;
        
        while ptr < end_ptr {
            core::ptr::write_volatile(ptr, 0);
            ptr = ptr.add(1);
        }
    }

    /// 返回按固定顺序遍历的内核分区迭代器（Text → Rodata → Data → Boot）
    pub fn iter(&self) -> KernelRegionIterator<'_> {
        KernelRegionIterator {
            layout: self,
            index: 0,
        }
    }
}

/// 内核分区迭代器
pub struct KernelRegionIterator<'a> {
    layout: &'a KernelLayout,
    index: usize,
}

impl<'a> Iterator for KernelRegionIterator<'a> {
    type Item = KernelRegion;

    fn next(&mut self) -> Option<Self::Item> {
        match self.index {
            0 => {
                self.index += 1;
                Some(KernelRegion {
                    title: KernelRegionTitle::Text,
                    range: self.layout.text..self.layout.rodata,
                })
            }
            1 => {
                self.index += 1;
                Some(KernelRegion {
                    title: KernelRegionTitle::Rodata,
                    range: self.layout.rodata..self.layout.data,
                })
            }
            2 => {
                self.index += 1;
                Some(KernelRegion {
                    title: KernelRegionTitle::Data,
                    range: self.layout.data..self.layout.boot,
                })
            }
            3 => {
                self.index += 1;
                Some(KernelRegion {
                    title: KernelRegionTitle::Boot,
                    range: self.layout.boot..self.layout.end,
                })
            }
            _ => None,
        }
    }
}

/// 分区名称枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelRegionTitle {
    Text,
    Rodata,
    Data,
    Boot,
}

/// 分区条目
pub struct KernelRegion {
    pub title: KernelRegionTitle,
    pub range: core::ops::Range<usize>,
}

impl core::fmt::Display for KernelRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self.title {
            KernelRegionTitle::Text => ".text",
            KernelRegionTitle::Rodata => ".rodata",
            KernelRegionTitle::Data => ".data",
            KernelRegionTitle::Boot => ".boot",
        };
        write!(
            f,
            "{}: 0x{:x}..0x{:x}",
            name, self.range.start, self.range.end
        )
    }
}

/// 应用程序元数据头
#[repr(C)]
pub struct AppMeta {
    pub base: u64,
    pub step: u64,
    pub count: u64,
    pub first: u64,
}

impl AppMeta {
    /// 返回指向链接符号 `apps` 的静态引用
    pub fn locate() -> &'static Self {
        extern "C" {
            static apps: AppMeta;
        }
        unsafe { &apps }
    }

    /// 返回应用程序迭代器
    pub fn iter(&'static self) -> AppIterator {
        AppIterator {
            meta: self,
            index: 0,
        }
    }
}

/// 应用程序迭代器
pub struct AppIterator {
    meta: &'static AppMeta,
    index: u64,
}

impl Iterator for AppIterator {
    type Item = &'static [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.meta.count {
            return None;
        }

        // 读取地址数组
        // first 字段是地址数组的起始位置
        // 地址数组包含 count+1 个地址（每个 app 的起始地址 + 最后一个的结束地址）
        let current_index = self.index;
        self.index += 1;
        
        let (addr_i, addr_i1) = unsafe {
            
            // first 字段本身就是地址数组的第一个元素
            let addr_ptr = &self.meta.first as *const u64;
            (
                *addr_ptr.add(current_index as usize),
                *addr_ptr.add((current_index + 1) as usize),
            )
        };
        
        let pos = addr_i as usize;
        let size = (addr_i1 - addr_i) as usize;

        if self.meta.base != 0 {
            // 需要拷贝到固定槽位
            let dst = (self.meta.base + current_index * self.meta.step) as usize;
            
            // 拷贝 app 映像
            unsafe {
                let src_ptr = pos as *const u8;
                let dst_ptr = dst as *mut u8;
                core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, size);
                
                // 清零剩余空间
                let zero_start = dst + size;
                let zero_end = dst + 0x20_0000;
                let mut zero_ptr = zero_start as *mut u8;
                let zero_end_ptr = zero_end as *mut u8;
                while zero_ptr < zero_end_ptr {
                    core::ptr::write_volatile(zero_ptr, 0);
                    zero_ptr = zero_ptr.add(1);
                }
            }
            
            // 返回指向拷贝后位置的切片
            unsafe {
                Some(core::slice::from_raw_parts(dst as *const u8, size))
            }
        } else {
            // 直接返回原始位置
            unsafe {
                Some(core::slice::from_raw_parts(pos as *const u8, size))
            }
        }
    }
}

/// 定义内核启动入口 `_start`
/// 
/// # 参数
/// - `$entry`: 入口函数名（当前实现固定跳转到 `rust_main`）
/// - `stack`: 启动栈大小表达式
/// 
/// # 示例
/// ```no_run
/// linker::boot0!(rust_main; stack = 4 * 4096);
/// ```
#[macro_export]
macro_rules! boot0 {
    ($entry:ident; stack = $stack:expr) => {
        #[link_section = ".boot.stack"]
        static mut STACK: [u8; $stack] = [0; $stack];

        #[no_mangle]
        #[link_section = ".text.entry"]
        pub unsafe extern "C" fn _start() -> ! {
            core::arch::asm!(
                "la sp, __end",
                "j rust_main",
                options(noreturn)
            );
        }
    };
}

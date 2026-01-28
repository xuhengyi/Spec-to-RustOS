#![no_std]

// 检查 feature 互斥
#[cfg(all(feature = "kernel", feature = "user"))]
compile_error!("features `kernel` and `user` cannot be enabled at the same time");

// 引入生成的 syscall 号常量
#[allow(dead_code)]
mod syscalls;

// Re-export signal-defs 的类型
pub use signal_defs::{SignalAction, SignalNo, MAX_SIG};

/// Syscall 号包装类型
/// 
/// 使用 `#[repr(transparent)]` 确保 ABI 兼容性
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyscallId(pub usize);

impl From<usize> for SyscallId {
    fn from(value: usize) -> Self {
        SyscallId(value)
    }
}

/// 时钟 ID 包装类型
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClockId(pub usize);

impl ClockId {
    pub const CLOCK_REALTIME: ClockId = ClockId(0);
    pub const CLOCK_MONOTONIC: ClockId = ClockId(1);
    pub const CLOCK_PROCESS_CPUTIME_ID: ClockId = ClockId(2);
    pub const CLOCK_THREAD_CPUTIME_ID: ClockId = ClockId(3);
}

/// 时间结构体
/// 
/// 使用 `#[repr(C)]` 确保可用于 C ABI/FFI 场景
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeSpec {
    pub tv_sec: usize,
    pub tv_nsec: usize,
}

impl TimeSpec {
    pub const ZERO: TimeSpec = TimeSpec { tv_sec: 0, tv_nsec: 0 };
    pub const SECOND: TimeSpec = TimeSpec { tv_sec: 1, tv_nsec: 0 };
    pub const MILLSECOND: TimeSpec = TimeSpec { tv_sec: 0, tv_nsec: 1_000_000 };
    pub const MICROSECOND: TimeSpec = TimeSpec { tv_sec: 0, tv_nsec: 1_000 };
    pub const NANOSECOND: TimeSpec = TimeSpec { tv_sec: 0, tv_nsec: 1 };

    /// 从毫秒数创建 TimeSpec
    pub fn from_millsecond(millsecond: usize) -> Self {
        TimeSpec {
            tv_sec: millsecond / 1000,
            tv_nsec: (millsecond % 1000) * 1_000_000,
        }
    }
}

impl core::ops::Add for TimeSpec {
    type Output = TimeSpec;

    fn add(self, other: TimeSpec) -> TimeSpec {
        let mut tv_sec = self.tv_sec + other.tv_sec;
        let mut tv_nsec = self.tv_nsec + other.tv_nsec;
        
        // 处理纳秒溢出
        if tv_nsec >= 1_000_000_000 {
            tv_sec += tv_nsec / 1_000_000_000;
            tv_nsec %= 1_000_000_000;
        }
        
        TimeSpec { tv_sec, tv_nsec }
    }
}

impl core::fmt::Display for TimeSpec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}s {}ns", self.tv_sec, self.tv_nsec)
    }
}

/// 标准输入文件描述符
pub const STDIN: usize = 0;

/// 标准输出文件描述符
pub const STDOUT: usize = 1;

/// 标准调试输出文件描述符
pub const STDDEBUG: usize = 2;

#[cfg(feature = "user")]
mod user;

#[cfg(feature = "user")]
pub use user::*;

#[cfg(feature = "kernel")]
mod kernel;

#[cfg(feature = "kernel")]
pub use kernel::*;

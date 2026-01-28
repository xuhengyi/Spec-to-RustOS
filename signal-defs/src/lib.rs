#![no_std]

use core::convert::TryFrom;
use numeric_enum_macro::numeric_enum;

/// 信号处理动作结构体
/// 
/// 用于表示信号的处理动作，包含处理函数地址和信号掩码。
/// 使用 `#[repr(C)]` 确保可用于 C ABI/FFI 场景。
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SignalAction {
    /// 信号处理函数的地址
    pub handler: usize,
    /// 信号掩码
    pub mask: usize,
}

numeric_enum! {
    #[repr(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SignalNo {
        ERR = 0,
        SIGHUP = 1,
        SIGINT = 2,
        SIGQUIT = 3,
        SIGILL = 4,
        SIGTRAP = 5,
        SIGABRT = 6,
        SIGBUS = 7,
        SIGFPE = 8,
        SIGKILL = 9,
        SIGUSR1 = 10,
        SIGSEGV = 11,
        SIGUSR2 = 12,
        SIGPIPE = 13,
        SIGALRM = 14,
        SIGTERM = 15,
        SIGSTKFLT = 16,
        SIGCHLD = 17,
        SIGCONT = 18,
        SIGSTOP = 19,
        SIGTSTP = 20,
        SIGTTIN = 21,
        SIGTTOU = 22,
        SIGURG = 23,
        SIGXCPU = 24,
        SIGXFSZ = 25,
        SIGVTALRM = 26,
        SIGPROF = 27,
        SIGWINCH = 28,
        SIGIO = 29,
        SIGPWR = 30,
        SIGSYS = 31,
        SIGRTMIN = 32,
        SIGRT1 = 33,
        SIGRT2 = 34,
        SIGRT3 = 35,
        SIGRT4 = 36,
        SIGRT5 = 37,
        SIGRT6 = 38,
        SIGRT7 = 39,
        SIGRT8 = 40,
        SIGRT9 = 41,
        SIGRT10 = 42,
        SIGRT11 = 43,
        SIGRT12 = 44,
        SIGRT13 = 45,
        SIGRT14 = 46,
        SIGRT15 = 47,
        SIGRT16 = 48,
        SIGRT17 = 49,
        SIGRT18 = 50,
        SIGRT19 = 51,
        SIGRT20 = 52,
        SIGRT21 = 53,
        SIGRT22 = 54,
        SIGRT23 = 55,
        SIGRT24 = 56,
        SIGRT25 = 57,
        SIGRT26 = 58,
        SIGRT27 = 59,
        SIGRT28 = 60,
        SIGRT29 = 61,
        SIGRT30 = 62,
        SIGRT31 = 63,
    }
}

/// 传统信号的最大编号
/// 
/// 用于表达传统信号的上限，与 `SignalNo::SIGSYS = 31` 对齐。
pub const MAX_SIG: usize = 31;

impl From<usize> for SignalNo {
    /// 将 `usize` 转换为 `SignalNo`
    /// 
    /// 转换语义：
    /// 1. 先将输入 `num` 以 Rust `as` 规则转换为 `u8`（可能发生截断）
    /// 2. 再执行 `SignalNo::try_from(u8)`
    /// 3. 如果 `try_from` 成功则返回对应变体；否则返回 `SignalNo::ERR`
    fn from(num: usize) -> Self {
        let num_u8 = num as u8;
        SignalNo::try_from(num_u8).unwrap_or(SignalNo::ERR)
    }
}

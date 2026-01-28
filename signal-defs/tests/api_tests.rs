//! signal-defs crate 功能性验证测试
//! 
//! 这些测试验证 signal-defs crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。

use signal_defs::*;

#[test]
fn test_signal_action_basic() {
    // 测试 SignalAction 的基本功能
    let action1 = SignalAction {
        handler: 0x1000,
        mask: 0x2000,
    };
    
    // 测试字段访问
    assert_eq!(action1.handler, 0x1000);
    assert_eq!(action1.mask, 0x2000);
}

#[test]
fn test_signal_action_default() {
    // 测试 SignalAction 的 Default trait
    let action = SignalAction::default();
    assert_eq!(action.handler, 0);
    assert_eq!(action.mask, 0);
}

#[test]
fn test_signal_action_clone() {
    // 测试 SignalAction 的 Clone trait
    let action1 = SignalAction {
        handler: 0x1000,
        mask: 0x2000,
    };
    let action2 = action1.clone();
    assert_eq!(action1.handler, action2.handler);
    assert_eq!(action1.mask, action2.mask);
}

#[test]
fn test_signal_action_copy() {
    // 测试 SignalAction 的 Copy trait
    let action1 = SignalAction {
        handler: 0x1000,
        mask: 0x2000,
    };
    let action2 = action1; // Copy trait 允许直接赋值
    assert_eq!(action1.handler, action2.handler);
    assert_eq!(action1.mask, action2.mask);
}

#[test]
fn test_signal_action_debug() {
    // 测试 SignalAction 的 Debug trait
    let action = SignalAction {
        handler: 0x1000,
        mask: 0x2000,
    };
    let debug_str = format!("{:?}", action);
    assert!(debug_str.contains("SignalAction"));
}

#[test]
fn test_max_sig() {
    // 测试 MAX_SIG 常量
    assert_eq!(MAX_SIG, 31);
}

#[test]
fn test_signal_no_basic() {
    // 测试 SignalNo 枚举的基本值
    assert_eq!(SignalNo::ERR as u8, 0);
    assert_eq!(SignalNo::SIGHUP as u8, 1);
    assert_eq!(SignalNo::SIGINT as u8, 2);
    assert_eq!(SignalNo::SIGQUIT as u8, 3);
    assert_eq!(SignalNo::SIGILL as u8, 4);
    assert_eq!(SignalNo::SIGTRAP as u8, 5);
    assert_eq!(SignalNo::SIGABRT as u8, 6);
    assert_eq!(SignalNo::SIGBUS as u8, 7);
    assert_eq!(SignalNo::SIGFPE as u8, 8);
    assert_eq!(SignalNo::SIGKILL as u8, 9);
    assert_eq!(SignalNo::SIGUSR1 as u8, 10);
    assert_eq!(SignalNo::SIGSEGV as u8, 11);
    assert_eq!(SignalNo::SIGUSR2 as u8, 12);
    assert_eq!(SignalNo::SIGPIPE as u8, 13);
    assert_eq!(SignalNo::SIGALRM as u8, 14);
    assert_eq!(SignalNo::SIGTERM as u8, 15);
}

#[test]
fn test_signal_no_standard_signals() {
    // 测试标准信号编号（16-31）
    assert_eq!(SignalNo::SIGSTKFLT as u8, 16);
    assert_eq!(SignalNo::SIGCHLD as u8, 17);
    assert_eq!(SignalNo::SIGCONT as u8, 18);
    assert_eq!(SignalNo::SIGSTOP as u8, 19);
    assert_eq!(SignalNo::SIGTSTP as u8, 20);
    assert_eq!(SignalNo::SIGTTIN as u8, 21);
    assert_eq!(SignalNo::SIGTTOU as u8, 22);
    assert_eq!(SignalNo::SIGURG as u8, 23);
    assert_eq!(SignalNo::SIGXCPU as u8, 24);
    assert_eq!(SignalNo::SIGXFSZ as u8, 25);
    assert_eq!(SignalNo::SIGVTALRM as u8, 26);
    assert_eq!(SignalNo::SIGPROF as u8, 27);
    assert_eq!(SignalNo::SIGWINCH as u8, 28);
    assert_eq!(SignalNo::SIGIO as u8, 29);
    assert_eq!(SignalNo::SIGPWR as u8, 30);
    assert_eq!(SignalNo::SIGSYS as u8, 31);
}

#[test]
fn test_signal_no_rt_signals() {
    // 测试实时信号编号（32-63）
    assert_eq!(SignalNo::SIGRTMIN as u8, 32);
    assert_eq!(SignalNo::SIGRT1 as u8, 33);
    assert_eq!(SignalNo::SIGRT2 as u8, 34);
    assert_eq!(SignalNo::SIGRT3 as u8, 35);
    assert_eq!(SignalNo::SIGRT4 as u8, 36);
    assert_eq!(SignalNo::SIGRT5 as u8, 37);
    assert_eq!(SignalNo::SIGRT6 as u8, 38);
    assert_eq!(SignalNo::SIGRT7 as u8, 39);
    assert_eq!(SignalNo::SIGRT8 as u8, 40);
    assert_eq!(SignalNo::SIGRT9 as u8, 41);
    assert_eq!(SignalNo::SIGRT10 as u8, 42);
    assert_eq!(SignalNo::SIGRT11 as u8, 43);
    assert_eq!(SignalNo::SIGRT12 as u8, 44);
    assert_eq!(SignalNo::SIGRT13 as u8, 45);
    assert_eq!(SignalNo::SIGRT14 as u8, 46);
    assert_eq!(SignalNo::SIGRT15 as u8, 47);
    assert_eq!(SignalNo::SIGRT16 as u8, 48);
    assert_eq!(SignalNo::SIGRT17 as u8, 49);
    assert_eq!(SignalNo::SIGRT18 as u8, 50);
    assert_eq!(SignalNo::SIGRT19 as u8, 51);
    assert_eq!(SignalNo::SIGRT20 as u8, 52);
    assert_eq!(SignalNo::SIGRT21 as u8, 53);
    assert_eq!(SignalNo::SIGRT22 as u8, 54);
    assert_eq!(SignalNo::SIGRT23 as u8, 55);
    assert_eq!(SignalNo::SIGRT24 as u8, 56);
    assert_eq!(SignalNo::SIGRT25 as u8, 57);
    assert_eq!(SignalNo::SIGRT26 as u8, 58);
    assert_eq!(SignalNo::SIGRT27 as u8, 59);
    assert_eq!(SignalNo::SIGRT28 as u8, 60);
    assert_eq!(SignalNo::SIGRT29 as u8, 61);
    assert_eq!(SignalNo::SIGRT30 as u8, 62);
    assert_eq!(SignalNo::SIGRT31 as u8, 63);
}

#[test]
fn test_signal_no_from_usize() {
    // 测试 SignalNo 的 From<usize> trait
    let sig0 = SignalNo::from(0);
    assert_eq!(sig0, SignalNo::ERR);
    
    let sig1 = SignalNo::from(1);
    assert_eq!(sig1, SignalNo::SIGHUP);
    
    let sig9 = SignalNo::from(9);
    assert_eq!(sig9, SignalNo::SIGKILL);
    
    let sig15 = SignalNo::from(15);
    assert_eq!(sig15, SignalNo::SIGTERM);
    
    let sig31 = SignalNo::from(31);
    assert_eq!(sig31, SignalNo::SIGSYS);
    
    // 测试实时信号
    let sig32 = SignalNo::from(32);
    assert_eq!(sig32, SignalNo::SIGRTMIN);
    
    let sig33 = SignalNo::from(33);
    assert_eq!(sig33, SignalNo::SIGRT1);
    
    let sig63 = SignalNo::from(63);
    assert_eq!(sig63, SignalNo::SIGRT31);
}

#[test]
fn test_signal_no_from_usize_invalid() {
    // 测试无效的信号编号应该返回 ERR
    let sig_invalid1 = SignalNo::from(64);
    assert_eq!(sig_invalid1, SignalNo::ERR);
    
    let sig_invalid2 = SignalNo::from(100);
    assert_eq!(sig_invalid2, SignalNo::ERR);
    
    let sig_invalid3 = SignalNo::from(255);
    assert_eq!(sig_invalid3, SignalNo::ERR);
}

#[test]
fn test_signal_no_eq() {
    // 测试 SignalNo 的 Eq 和 PartialEq trait
    let sig1 = SignalNo::SIGHUP;
    let sig2 = SignalNo::SIGHUP;
    let sig3 = SignalNo::SIGINT;
    
    assert_eq!(sig1, sig2);
    assert_ne!(sig1, sig3);
    assert_eq!(sig1, SignalNo::from(1));
}

#[test]
fn test_signal_no_copy() {
    // 测试 SignalNo 的 Copy trait
    let sig1 = SignalNo::SIGKILL;
    let sig2 = sig1; // Copy trait 允许直接赋值
    assert_eq!(sig1, sig2);
}

#[test]
fn test_signal_no_clone() {
    // 测试 SignalNo 的 Clone trait
    let sig1 = SignalNo::SIGTERM;
    let sig2 = sig1.clone();
    assert_eq!(sig1, sig2);
}

#[test]
fn test_signal_no_debug() {
    // 测试 SignalNo 的 Debug trait
    let sig = SignalNo::SIGHUP;
    let debug_str = format!("{:?}", sig);
    assert!(debug_str.contains("SIGHUP") || debug_str.contains("SignalNo"));
}

#[test]
fn test_signal_no_ordering() {
    // 测试信号编号的数值顺序
    assert!((SignalNo::ERR as u8) < (SignalNo::SIGHUP as u8));
    assert!((SignalNo::SIGHUP as u8) < (SignalNo::SIGINT as u8));
    assert!((SignalNo::SIGSYS as u8) < (SignalNo::SIGRTMIN as u8));
    assert!((SignalNo::SIGRTMIN as u8) < (SignalNo::SIGRT31 as u8));
}

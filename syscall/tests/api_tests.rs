//! syscall crate 功能性验证测试
//! 
//! 这些测试验证 syscall crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。

use syscall::*;

#[test]
fn test_syscall_id_basic() {
    // 测试 SyscallId 的基本功能
    let id1 = SyscallId::from(64);
    let id2 = SyscallId(64);
    assert_eq!(id1, id2);
    
    // 测试 From<usize> trait
    let id3: SyscallId = 93.into();
    assert_eq!(id3.0, 93);
}

#[test]
fn test_syscall_id_constants() {
    // 验证常用的系统调用号常量存在且正确
    assert_eq!(SyscallId::WRITE.0, 64);
    assert_eq!(SyscallId::READ.0, 63);
    assert_eq!(SyscallId::EXIT.0, 93);
    assert_eq!(SyscallId::CLOCK_GETTIME.0, 113);
    assert_eq!(SyscallId::GETPID.0, 172);
    assert_eq!(SyscallId::GETTID.0, 178);
    assert_eq!(SyscallId::SCHED_YIELD.0, 124);
}

#[test]
fn test_io_constants() {
    // 测试 IO 文件描述符常量
    assert_eq!(STDIN, 0);
    assert_eq!(STDOUT, 1);
    assert_eq!(STDDEBUG, 2);
}

#[test]
fn test_clock_id_constants() {
    // 测试 ClockId 常量
    assert_eq!(ClockId::CLOCK_REALTIME.0, 0);
    assert_eq!(ClockId::CLOCK_MONOTONIC.0, 1);
    assert_eq!(ClockId::CLOCK_PROCESS_CPUTIME_ID.0, 2);
    assert_eq!(ClockId::CLOCK_THREAD_CPUTIME_ID.0, 3);
}

#[test]
fn test_time_spec_basic() {
    // 测试 TimeSpec 的基本功能
    let zero = TimeSpec::ZERO;
    assert_eq!(zero.tv_sec, 0);
    assert_eq!(zero.tv_nsec, 0);
    
    let second = TimeSpec::SECOND;
    assert_eq!(second.tv_sec, 1);
    assert_eq!(second.tv_nsec, 0);
    
    let millisecond = TimeSpec::MILLSECOND;
    assert_eq!(millisecond.tv_sec, 0);
    assert_eq!(millisecond.tv_nsec, 1_000_000);
    
    let microsecond = TimeSpec::MICROSECOND;
    assert_eq!(microsecond.tv_sec, 0);
    assert_eq!(microsecond.tv_nsec, 1_000);
    
    let nanosecond = TimeSpec::NANOSECOND;
    assert_eq!(nanosecond.tv_sec, 0);
    assert_eq!(nanosecond.tv_nsec, 1);
}

#[test]
fn test_time_spec_from_millisecond() {
    // 测试 TimeSpec::from_millsecond
    let ts1 = TimeSpec::from_millsecond(1000);
    assert_eq!(ts1.tv_sec, 1);
    assert_eq!(ts1.tv_nsec, 0);
    
    let ts2 = TimeSpec::from_millsecond(1500);
    assert_eq!(ts2.tv_sec, 1);
    assert_eq!(ts2.tv_nsec, 500_000_000);
    
    let ts3 = TimeSpec::from_millsecond(500);
    assert_eq!(ts3.tv_sec, 0);
    assert_eq!(ts3.tv_nsec, 500_000_000);
}

#[test]
fn test_time_spec_add() {
    // 测试 TimeSpec 的加法运算
    let ts1 = TimeSpec {
        tv_sec: 1,
        tv_nsec: 500_000_000,
    };
    let ts2 = TimeSpec {
        tv_sec: 2,
        tv_nsec: 600_000_000,
    };
    let result = ts1 + ts2;
    assert_eq!(result.tv_sec, 4);
    assert_eq!(result.tv_nsec, 100_000_000);
    
    // 测试纳秒溢出
    let ts3 = TimeSpec {
        tv_sec: 0,
        tv_nsec: 800_000_000,
    };
    let ts4 = TimeSpec {
        tv_sec: 0,
        tv_nsec: 300_000_000,
    };
    let result2 = ts3 + ts4;
    assert_eq!(result2.tv_sec, 1);
    assert_eq!(result2.tv_nsec, 100_000_000);
}

#[test]
fn test_time_spec_display() {
    // 测试 TimeSpec 的 Display trait
    let ts = TimeSpec {
        tv_sec: 123,
        tv_nsec: 456_789_012,
    };
    let s = format!("{}", ts);
    assert!(s.contains("123"));
    assert!(s.contains("456789012"));
}

#[test]
fn test_time_spec_ordering() {
    // 测试 TimeSpec 的排序
    let ts1 = TimeSpec {
        tv_sec: 1,
        tv_nsec: 0,
    };
    let ts2 = TimeSpec {
        tv_sec: 2,
        tv_nsec: 0,
    };
    let ts3 = TimeSpec {
        tv_sec: 1,
        tv_nsec: 500_000_000,
    };
    
    assert!(ts1 < ts2);
    assert!(ts1 < ts3);
    assert!(ts3 < ts2);
    assert_eq!(ts1, ts1);
}

#[test]
fn test_signal_no_from() {
    // 测试 SignalNo 的 From trait
    let sig0 = SignalNo::from(0);
    assert_eq!(sig0, SignalNo::ERR);
    
    let sig1 = SignalNo::from(1);
    assert_eq!(sig1, SignalNo::SIGHUP);
    
    let sig9 = SignalNo::from(9);
    assert_eq!(sig9, SignalNo::SIGKILL);
}

#[test]
fn test_signal_action_default() {
    // 测试 SignalAction 的默认值
    let action = SignalAction::default();
    assert_eq!(action.handler, 0);
    assert_eq!(action.mask, 0);
}

#[test]
fn test_max_sig() {
    // 测试 MAX_SIG 常量
    assert_eq!(MAX_SIG, 31);
}

#[cfg(feature = "user")]
#[test]
fn test_open_flags() {
    // 测试 OpenFlags bitflags
    use syscall::*;
    
    let rdonly = OpenFlags::RDONLY;
    assert_eq!(rdonly.bits(), 0);
    
    let wronly = OpenFlags::WRONLY;
    assert_eq!(wronly.bits(), 1);
    
    let rdwr = OpenFlags::RDWR;
    assert_eq!(rdwr.bits(), 2);
    
    let create = OpenFlags::CREATE;
    assert_eq!(create.bits(), 512);
    
    let trunc = OpenFlags::TRUNC;
    assert_eq!(trunc.bits(), 1024);
    
    // 测试组合标志
    let flags = OpenFlags::WRONLY | OpenFlags::CREATE | OpenFlags::TRUNC;
    assert!(flags.contains(OpenFlags::WRONLY));
    assert!(flags.contains(OpenFlags::CREATE));
    assert!(flags.contains(OpenFlags::TRUNC));
}

#[cfg(feature = "user")]
#[test]
fn test_user_api_exists() {
    // 验证用户态 API 函数存在（不实际调用，因为需要内核支持）
    // 这些测试主要验证函数签名正确
    use syscall::*;
    
    // 验证函数存在且可编译
    let _write_fn: fn(usize, &[u8]) -> isize = write;
    let _read_fn: fn(usize, &[u8]) -> isize = read;
    let _open_fn: fn(&str, OpenFlags) -> isize = open;
    let _close_fn: fn(usize) -> isize = close;
    let _exit_fn: fn(i32) -> isize = exit;
    let _sched_yield_fn: fn() -> isize = sched_yield;
    let _clock_gettime_fn: fn(ClockId, *mut TimeSpec) -> isize = clock_gettime;
    let _fork_fn: fn() -> isize = fork;
    let _exec_fn: fn(&str) -> isize = exec;
    let _wait_fn: fn(*mut i32) -> isize = wait;
    let _waitpid_fn: fn(isize, *mut i32) -> isize = waitpid;
    let _getpid_fn: fn() -> isize = getpid;
    let _kill_fn: fn(isize, SignalNo) -> isize = kill;
    let _sigaction_fn: fn(SignalNo, *const SignalAction, *const SignalAction) -> isize = sigaction;
    let _sigprocmask_fn: fn(usize) -> isize = sigprocmask;
    let _sigreturn_fn: fn() -> isize = sigreturn;
    let _thread_create_fn: fn(usize, usize) -> isize = thread_create;
    let _gettid_fn: fn() -> isize = gettid;
    let _waittid_fn: fn(usize) -> isize = waittid;
    let _semaphore_create_fn: fn(usize) -> isize = semaphore_create;
    let _semaphore_up_fn: fn(usize) -> isize = semaphore_up;
    let _semaphore_down_fn: fn(usize) -> isize = semaphore_down;
    let _mutex_create_fn: fn(bool) -> isize = mutex_create;
    let _mutex_lock_fn: fn(usize) -> isize = mutex_lock;
    let _mutex_unlock_fn: fn(usize) -> isize = mutex_unlock;
    let _condvar_create_fn: fn() -> isize = condvar_create;
    let _condvar_signal_fn: fn(usize) -> isize = condvar_signal;
    let _condvar_wait_fn: fn(usize, usize) -> isize = condvar_wait;
}

extern crate alloc;

use alloc::vec::Vec;
use bitflags::bitflags;
use crate::{SyscallId, ClockId, TimeSpec, SignalNo, SignalAction};

bitflags! {
    /// 文件打开标志
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1;
        const RDWR = 2;
        const CREATE = 512;
        const TRUNC = 1024;
    }
}

/// 最小 syscall 原语模块
pub mod native {
    use super::SyscallId;

    /// 发起不带参数的系统调用
    /// 
    /// # Safety
    /// 调用方必须确保 syscall id 和参数有效
    #[cfg(target_arch = "riscv64")]
    #[inline]
    pub unsafe fn syscall0(id: SyscallId) -> isize {
        let ret: isize;
        core::arch::asm!(
            "ecall",
            in("a7") id.0,
            lateout("a0") ret,
            options(nostack, preserves_flags)
        );
        ret
    }

    #[cfg(not(target_arch = "riscv64"))]
    #[inline]
    pub unsafe fn syscall0(_id: SyscallId) -> isize {
        // 在非 RISC-V 架构上返回 -1（未实现）
        -1
    }

    /// 发起带 1 个参数的系统调用
    #[cfg(target_arch = "riscv64")]
    #[inline]
    pub unsafe fn syscall1(id: SyscallId, a0: usize) -> isize {
        let ret: isize;
        core::arch::asm!(
            "ecall",
            in("a0") a0,
            in("a7") id.0,
            lateout("a0") ret,
            options(nostack, preserves_flags)
        );
        ret
    }

    #[cfg(not(target_arch = "riscv64"))]
    #[inline]
    pub unsafe fn syscall1(_id: SyscallId, _a0: usize) -> isize {
        -1
    }

    /// 发起带 2 个参数的系统调用
    #[cfg(target_arch = "riscv64")]
    #[inline]
    pub unsafe fn syscall2(id: SyscallId, a0: usize, a1: usize) -> isize {
        let ret: isize;
        core::arch::asm!(
            "ecall",
            in("a0") a0,
            in("a1") a1,
            in("a7") id.0,
            lateout("a0") ret,
            options(nostack, preserves_flags)
        );
        ret
    }

    #[cfg(not(target_arch = "riscv64"))]
    #[inline]
    pub unsafe fn syscall2(_id: SyscallId, _a0: usize, _a1: usize) -> isize {
        -1
    }

    /// 发起带 3 个参数的系统调用
    #[cfg(target_arch = "riscv64")]
    #[inline]
    pub unsafe fn syscall3(id: SyscallId, a0: usize, a1: usize, a2: usize) -> isize {
        let ret: isize;
        core::arch::asm!(
            "ecall",
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a7") id.0,
            lateout("a0") ret,
            options(nostack, preserves_flags)
        );
        ret
    }

    #[cfg(not(target_arch = "riscv64"))]
    #[inline]
    pub unsafe fn syscall3(_id: SyscallId, _a0: usize, _a1: usize, _a2: usize) -> isize {
        -1
    }

    /// 发起带 4 个参数的系统调用
    #[cfg(target_arch = "riscv64")]
    #[inline]
    pub unsafe fn syscall4(id: SyscallId, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
        let ret: isize;
        core::arch::asm!(
            "ecall",
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            in("a7") id.0,
            lateout("a0") ret,
            options(nostack, preserves_flags)
        );
        ret
    }

    #[cfg(not(target_arch = "riscv64"))]
    #[inline]
    pub unsafe fn syscall4(_id: SyscallId, _a0: usize, _a1: usize, _a2: usize, _a3: usize) -> isize {
        -1
    }

    /// 发起带 5 个参数的系统调用
    #[cfg(target_arch = "riscv64")]
    #[inline]
    pub unsafe fn syscall5(id: SyscallId, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
        let ret: isize;
        core::arch::asm!(
            "ecall",
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            in("a4") a4,
            in("a7") id.0,
            lateout("a0") ret,
            options(nostack, preserves_flags)
        );
        ret
    }

    #[cfg(not(target_arch = "riscv64"))]
    #[inline]
    pub unsafe fn syscall5(_id: SyscallId, _a0: usize, _a1: usize, _a2: usize, _a3: usize, _a4: usize) -> isize {
        -1
    }

    /// 发起带 6 个参数的系统调用
    #[cfg(target_arch = "riscv64")]
    #[inline]
    pub unsafe fn syscall6(id: SyscallId, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
        let ret: isize;
        core::arch::asm!(
            "ecall",
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            in("a4") a4,
            in("a5") a5,
            in("a7") id.0,
            lateout("a0") ret,
            options(nostack, preserves_flags)
        );
        ret
    }

    #[cfg(not(target_arch = "riscv64"))]
    #[inline]
    pub unsafe fn syscall6(_id: SyscallId, _a0: usize, _a1: usize, _a2: usize, _a3: usize, _a4: usize, _a5: usize) -> isize {
        -1
    }
}

/// 写入数据到文件描述符
pub fn write(fd: usize, buffer: &[u8]) -> isize {
    unsafe {
        native::syscall3(
            SyscallId::WRITE,
            fd,
            buffer.as_ptr() as usize,
            buffer.len(),
        )
    }
}

/// 从文件描述符读取数据
/// 
/// # Safety
/// 调用方必须确保 buffer 指向的内存是可写的
pub fn read(fd: usize, buffer: &[u8]) -> isize {
    unsafe {
        native::syscall3(
            SyscallId::READ,
            fd,
            buffer.as_ptr() as usize,
            buffer.len(),
        )
    }
}

/// 打开文件
pub fn open(path: &str, flags: OpenFlags) -> isize {
    unsafe {
        native::syscall2(
            SyscallId::OPEN,
            path.as_ptr() as usize,
            flags.bits() as usize,
        )
    }
}

/// 关闭文件描述符
pub fn close(fd: usize) -> isize {
    unsafe {
        native::syscall1(SyscallId::CLOSE, fd)
    }
}

/// 退出进程
pub fn exit(exit_code: i32) -> isize {
    unsafe {
        native::syscall1(SyscallId::EXIT, exit_code as usize)
    }
}

/// 让出 CPU
pub fn sched_yield() -> isize {
    unsafe {
        native::syscall0(SyscallId::SCHED_YIELD)
    }
}

/// 获取时钟时间
pub fn clock_gettime(clockid: ClockId, tp: *mut TimeSpec) -> isize {
    unsafe {
        native::syscall2(SyscallId::CLOCK_GETTIME, clockid.0, tp as usize)
    }
}

/// 创建子进程
pub fn fork() -> isize {
    unsafe {
        native::syscall0(SyscallId::FORK)
    }
}

/// 执行程序
pub fn exec(path: &str) -> isize {
    let mut c_path = Vec::with_capacity(path.len() + 1);
    c_path.extend_from_slice(path.as_bytes());
    c_path.push(0);
    unsafe {
        native::syscall1(SyscallId::EXECVE, c_path.as_ptr() as usize)
    }
}

/// 等待子进程退出
/// 
/// 如果返回 -2，会调用 sched_yield() 并重试
pub fn wait(exit_code_ptr: *mut i32) -> isize {
    waitpid(-1, exit_code_ptr)
}

/// 等待指定进程退出
/// 
/// 如果返回 -2，会调用 sched_yield() 并重试
pub fn waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    loop {
        let ret = unsafe {
            native::syscall3(
                SyscallId::WAIT4,
                pid as usize,
                exit_code_ptr as usize,
                0,
            )
        };
        if ret != -2 {
            return ret;
        }
        sched_yield();
    }
}

/// 获取当前进程 ID
pub fn getpid() -> isize {
    unsafe {
        native::syscall0(SyscallId::GETPID)
    }
}

/// 发送信号
pub fn kill(pid: isize, signum: SignalNo) -> isize {
    unsafe {
        native::syscall2(SyscallId::KILL, pid as usize, signum as u8 as usize)
    }
}

/// 设置信号处理动作
pub fn sigaction(signum: SignalNo, action: *const SignalAction, old_action: *const SignalAction) -> isize {
    unsafe {
        native::syscall3(
            SyscallId::SIGACTION,
            signum as u8 as usize,
            action as usize,
            old_action as usize,
        )
    }
}

/// 设置信号掩码
pub fn sigprocmask(mask: usize) -> isize {
    unsafe {
        native::syscall1(SyscallId::SIGPROCMASK, mask)
    }
}

/// 从信号处理函数返回
pub fn sigreturn() -> isize {
    unsafe {
        native::syscall0(SyscallId::RT_SIGRETURN)
    }
}

/// 创建线程
pub fn thread_create(entry: usize, arg: usize) -> isize {
    unsafe {
        native::syscall2(SyscallId::THREAD_CREATE, entry, arg)
    }
}

/// 获取当前线程 ID
pub fn gettid() -> isize {
    unsafe {
        native::syscall0(SyscallId::GETTID)
    }
}

/// 等待线程退出
pub fn waittid(tid: usize) -> isize {
    unsafe {
        native::syscall1(SyscallId::WAITTID, tid)
    }
}

/// 创建信号量
pub fn semaphore_create(res_count: usize) -> isize {
    unsafe {
        native::syscall1(SyscallId::SEMOP, res_count)
    }
}

/// 信号量 V 操作
pub fn semaphore_up(sem_id: usize) -> isize {
    unsafe {
        native::syscall1(SyscallId::SEMOP, sem_id)
    }
}

/// 信号量 P 操作
pub fn semaphore_down(sem_id: usize) -> isize {
    unsafe {
        native::syscall1(SyscallId::SEMOP, sem_id)
    }
}

/// 创建互斥锁
pub fn mutex_create(blocking: bool) -> isize {
    unsafe {
        native::syscall1(SyscallId::MUTEX_CREATE, blocking as usize)
    }
}

/// 锁定互斥锁
pub fn mutex_lock(mutex_id: usize) -> isize {
    unsafe {
        native::syscall1(SyscallId::MUTEX_LOCK, mutex_id)
    }
}

/// 解锁互斥锁
pub fn mutex_unlock(mutex_id: usize) -> isize {
    unsafe {
        native::syscall1(SyscallId::MUTEX_UNLOCK, mutex_id)
    }
}

/// 创建条件变量
pub fn condvar_create() -> isize {
    unsafe {
        native::syscall0(SyscallId::CONDVAR_CREATE)
    }
}

/// 唤醒条件变量上的一个等待者
pub fn condvar_signal(condvar_id: usize) -> isize {
    unsafe {
        native::syscall1(SyscallId::CONDVAR_SIGNAL, condvar_id)
    }
}

/// 在条件变量上等待
pub fn condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    unsafe {
        native::syscall2(SyscallId::CONDVAR_WAIT, condvar_id, mutex_id)
    }
}

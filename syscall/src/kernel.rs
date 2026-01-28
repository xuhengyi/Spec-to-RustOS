use spin::Once;
use crate::SyscallId;

/// 系统调用调用者信息
#[derive(Debug, Clone, Copy)]
pub struct Caller {
    pub entity: usize,
    pub flow: usize,
}

/// 系统调用结果
#[derive(Debug, Clone, Copy)]
pub enum SyscallResult {
    Done(isize),
    Unsupported(SyscallId),
}

/// 进程管理 trait
pub trait Process: Send + Sync {
    fn fork(&self, caller: Caller) -> isize;
    fn exec(&self, caller: Caller, path: *const u8) -> isize;
    fn exit(&self, caller: Caller, exit_code: i32) -> isize;
    fn wait(&self, caller: Caller, exit_code_ptr: *mut i32) -> isize;
    fn waitpid(&self, caller: Caller, pid: isize, exit_code_ptr: *mut i32) -> isize;
    fn getpid(&self, caller: Caller) -> isize;
}

/// IO 操作 trait
pub trait IO: Send + Sync {
    fn read(&self, caller: Caller, fd: usize, buf: *mut u8, count: usize) -> isize;
    fn write(&self, caller: Caller, fd: usize, buf: *const u8, count: usize) -> isize;
    fn open(&self, caller: Caller, path: *const u8, flags: u32) -> isize;
    fn close(&self, caller: Caller, fd: usize) -> isize;
}

/// 内存管理 trait
pub trait Memory: Send + Sync {
    fn mmap(&self, caller: Caller, addr: usize, len: usize, prot: usize, flags: usize, fd: isize, offset: usize) -> isize;
    fn munmap(&self, caller: Caller, addr: usize, len: usize) -> isize;
}

/// 调度 trait
pub trait Scheduling: Send + Sync {
    fn sched_yield(&self, caller: Caller) -> isize;
}

/// 时钟 trait
pub trait Clock: Send + Sync {
    fn clock_gettime(&self, caller: Caller, clockid: usize, tp: *mut crate::TimeSpec) -> isize;
}

/// 信号 trait
pub trait Signal: Send + Sync {
    fn kill(&self, caller: Caller, pid: isize, signum: u8) -> isize;
    fn sigaction(&self, caller: Caller, signum: u8, action: *const crate::SignalAction, old_action: *mut crate::SignalAction) -> isize;
    fn sigprocmask(&self, caller: Caller, mask: usize) -> isize;
    fn sigreturn(&self, caller: Caller) -> isize;
}

/// 线程管理 trait
pub trait Thread: Send + Sync {
    fn thread_create(&self, caller: Caller, entry: usize, arg: usize) -> isize;
    fn gettid(&self, caller: Caller) -> isize;
    fn waittid(&self, caller: Caller, tid: usize) -> isize;
}

/// 同步原语 trait
pub trait SyncMutex: Send + Sync {
    fn semaphore_create(&self, caller: Caller, res_count: usize) -> isize;
    fn semaphore_up(&self, caller: Caller, sem_id: usize) -> isize;
    fn semaphore_down(&self, caller: Caller, sem_id: usize) -> isize;
    fn mutex_create(&self, caller: Caller, blocking: bool) -> isize;
    fn mutex_lock(&self, caller: Caller, mutex_id: usize) -> isize;
    fn mutex_unlock(&self, caller: Caller, mutex_id: usize) -> isize;
    fn condvar_create(&self, caller: Caller) -> isize;
    fn condvar_signal(&self, caller: Caller, condvar_id: usize) -> isize;
    fn condvar_wait(&self, caller: Caller, condvar_id: usize, mutex_id: usize) -> isize;
}

// Handler 存储（使用 Once 确保一次性初始化）
static PROCESS_HANDLER: Once<&'static dyn Process> = Once::new();
static IO_HANDLER: Once<&'static dyn IO> = Once::new();
static MEMORY_HANDLER: Once<&'static dyn Memory> = Once::new();
static SCHEDULING_HANDLER: Once<&'static dyn Scheduling> = Once::new();
static CLOCK_HANDLER: Once<&'static dyn Clock> = Once::new();
static SIGNAL_HANDLER: Once<&'static dyn Signal> = Once::new();
static THREAD_HANDLER: Once<&'static dyn Thread> = Once::new();
static SYNC_MUTEX_HANDLER: Once<&'static dyn SyncMutex> = Once::new();

/// 初始化进程管理 handler
pub fn init_process(handler: &'static dyn Process) {
    PROCESS_HANDLER.call_once(|| handler);
}

/// 初始化 IO handler
pub fn init_io(handler: &'static dyn IO) {
    IO_HANDLER.call_once(|| handler);
}

/// 初始化内存管理 handler
pub fn init_memory(handler: &'static dyn Memory) {
    MEMORY_HANDLER.call_once(|| handler);
}

/// 初始化调度 handler
pub fn init_scheduling(handler: &'static dyn Scheduling) {
    SCHEDULING_HANDLER.call_once(|| handler);
}

/// 初始化时钟 handler
pub fn init_clock(handler: &'static dyn Clock) {
    CLOCK_HANDLER.call_once(|| handler);
}

/// 初始化信号 handler
pub fn init_signal(handler: &'static dyn Signal) {
    SIGNAL_HANDLER.call_once(|| handler);
}

/// 初始化线程管理 handler
pub fn init_thread(handler: &'static dyn Thread) {
    THREAD_HANDLER.call_once(|| handler);
}

/// 初始化同步原语 handler
pub fn init_sync_mutex(handler: &'static dyn SyncMutex) {
    SYNC_MUTEX_HANDLER.call_once(|| handler);
}

/// 处理系统调用
pub fn handle(caller: Caller, id: SyscallId, args: [usize; 6]) -> SyscallResult {
    match id {
        // IO syscalls
        SyscallId::READ => {
            if let Some(handler) = IO_HANDLER.get() {
                SyscallResult::Done(handler.read(caller, args[0], args[1] as *mut u8, args[2]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::WRITE => {
            if let Some(handler) = IO_HANDLER.get() {
                SyscallResult::Done(handler.write(caller, args[0], args[1] as *const u8, args[2]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::OPEN => {
            if let Some(handler) = IO_HANDLER.get() {
                SyscallResult::Done(handler.open(caller, args[0] as *const u8, args[1] as u32))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::CLOSE => {
            if let Some(handler) = IO_HANDLER.get() {
                SyscallResult::Done(handler.close(caller, args[0]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        // Process syscalls
        SyscallId::FORK => {
            if let Some(handler) = PROCESS_HANDLER.get() {
                SyscallResult::Done(handler.fork(caller))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::EXECVE => {
            if let Some(handler) = PROCESS_HANDLER.get() {
                SyscallResult::Done(handler.exec(caller, args[0] as *const u8))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::EXIT => {
            if let Some(handler) = PROCESS_HANDLER.get() {
                SyscallResult::Done(handler.exit(caller, args[0] as i32))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::WAIT4 => {
            if let Some(handler) = PROCESS_HANDLER.get() {
                // args[0] = pid, args[1] = exit_code_ptr
                SyscallResult::Done(handler.waitpid(caller, args[0] as isize, args[1] as *mut i32))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::GETPID => {
            if let Some(handler) = PROCESS_HANDLER.get() {
                SyscallResult::Done(handler.getpid(caller))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        // Scheduling syscalls
        SyscallId::SCHED_YIELD => {
            if let Some(handler) = SCHEDULING_HANDLER.get() {
                SyscallResult::Done(handler.sched_yield(caller))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        // Clock syscalls
        SyscallId::CLOCK_GETTIME => {
            if let Some(handler) = CLOCK_HANDLER.get() {
                SyscallResult::Done(handler.clock_gettime(caller, args[0], args[1] as *mut crate::TimeSpec))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        // Signal syscalls
        SyscallId::KILL => {
            if let Some(handler) = SIGNAL_HANDLER.get() {
                SyscallResult::Done(handler.kill(caller, args[0] as isize, args[1] as u8))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::SIGACTION => {
            if let Some(handler) = SIGNAL_HANDLER.get() {
                SyscallResult::Done(handler.sigaction(caller, args[0] as u8, args[1] as *const crate::SignalAction, args[2] as *mut crate::SignalAction))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::SIGPROCMASK => {
            if let Some(handler) = SIGNAL_HANDLER.get() {
                SyscallResult::Done(handler.sigprocmask(caller, args[0]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::RT_SIGRETURN => {
            if let Some(handler) = SIGNAL_HANDLER.get() {
                SyscallResult::Done(handler.sigreturn(caller))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        // Thread syscalls
        SyscallId::THREAD_CREATE => {
            if let Some(handler) = THREAD_HANDLER.get() {
                SyscallResult::Done(handler.thread_create(caller, args[0], args[1]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::GETTID => {
            if let Some(handler) = THREAD_HANDLER.get() {
                SyscallResult::Done(handler.gettid(caller))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::WAITTID => {
            if let Some(handler) = THREAD_HANDLER.get() {
                SyscallResult::Done(handler.waittid(caller, args[0]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        // Sync mutex syscalls
        SyscallId::SEMOP => {
            if let Some(handler) = SYNC_MUTEX_HANDLER.get() {
                // 这里需要根据参数判断是 create/up/down，暂时使用 args[1] 作为操作类型
                // 实际实现可能需要不同的 syscall id
                SyscallResult::Done(handler.semaphore_create(caller, args[0]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::MUTEX_CREATE => {
            if let Some(handler) = SYNC_MUTEX_HANDLER.get() {
                SyscallResult::Done(handler.mutex_create(caller, args[0] != 0))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::MUTEX_LOCK => {
            if let Some(handler) = SYNC_MUTEX_HANDLER.get() {
                SyscallResult::Done(handler.mutex_lock(caller, args[0]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::MUTEX_UNLOCK => {
            if let Some(handler) = SYNC_MUTEX_HANDLER.get() {
                SyscallResult::Done(handler.mutex_unlock(caller, args[0]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::CONDVAR_CREATE => {
            if let Some(handler) = SYNC_MUTEX_HANDLER.get() {
                SyscallResult::Done(handler.condvar_create(caller))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::CONDVAR_SIGNAL => {
            if let Some(handler) = SYNC_MUTEX_HANDLER.get() {
                SyscallResult::Done(handler.condvar_signal(caller, args[0]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        SyscallId::CONDVAR_WAIT => {
            if let Some(handler) = SYNC_MUTEX_HANDLER.get() {
                SyscallResult::Done(handler.condvar_wait(caller, args[0], args[1]))
            } else {
                SyscallResult::Unsupported(id)
            }
        }
        _ => SyscallResult::Unsupported(id),
    }
}

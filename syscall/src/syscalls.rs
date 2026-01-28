// Auto-generated file from build.rs
// Do not edit manually

impl crate::SyscallId {
    pub const READ: crate::SyscallId = crate::SyscallId(63);
    pub const WRITE: crate::SyscallId = crate::SyscallId(64);
    pub const OPEN: crate::SyscallId = crate::SyscallId(56);
    pub const CLOSE: crate::SyscallId = crate::SyscallId(57);
    pub const EXIT: crate::SyscallId = crate::SyscallId(93);
    pub const EXIT_GROUP: crate::SyscallId = crate::SyscallId(94);
    pub const FORK: crate::SyscallId = crate::SyscallId(220);
    pub const EXECVE: crate::SyscallId = crate::SyscallId(221);
    pub const WAIT4: crate::SyscallId = crate::SyscallId(260);
    pub const WAITID: crate::SyscallId = crate::SyscallId(281);
    pub const GETPID: crate::SyscallId = crate::SyscallId(172);
    pub const GETTID: crate::SyscallId = crate::SyscallId(178);
    pub const KILL: crate::SyscallId = crate::SyscallId(129);
    pub const SIGACTION: crate::SyscallId = crate::SyscallId(134);
    pub const SIGPROCMASK: crate::SyscallId = crate::SyscallId(135);
    pub const RT_SIGRETURN: crate::SyscallId = crate::SyscallId(139);
    pub const SCHED_YIELD: crate::SyscallId = crate::SyscallId(124);
    pub const CLOCK_GETTIME: crate::SyscallId = crate::SyscallId(113);
    pub const CLONE: crate::SyscallId = crate::SyscallId(220);
    pub const SEMOP: crate::SyscallId = crate::SyscallId(65);
    pub const SEMGET: crate::SyscallId = crate::SyscallId(66);
    pub const SEMCTL: crate::SyscallId = crate::SyscallId(67);
    pub const MUTEX_CREATE: crate::SyscallId = crate::SyscallId(400);
    pub const MUTEX_LOCK: crate::SyscallId = crate::SyscallId(401);
    pub const MUTEX_UNLOCK: crate::SyscallId = crate::SyscallId(402);
    pub const CONDVAR_CREATE: crate::SyscallId = crate::SyscallId(403);
    pub const CONDVAR_SIGNAL: crate::SyscallId = crate::SyscallId(404);
    pub const CONDVAR_WAIT: crate::SyscallId = crate::SyscallId(405);
    pub const THREAD_CREATE: crate::SyscallId = crate::SyscallId(406);
    pub const WAITTID: crate::SyscallId = crate::SyscallId(407);
}

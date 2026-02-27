//! sync crate 功能性验证测试
//!
//! 这些测试验证 sync crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。
//!
//! ## 测试限制
//!
//! **单元测试**（当前文件）：
//! - ⚠️ 需要 RISC-V 64 位主机才能实际运行测试用例
//! - ⚠️ 在 x86_64 主机上：测试文件可编译，但用例被 `#[cfg(target_arch = "riscv64")]` 跳过（0 tests）
//! - ⚠️ 在 `riscv64gc-unknown-none-elf` 目标上：无法运行（no_std 环境，测试需要 std）
//!
//! **推荐运行方式**：
//! 1. **编译验证**：使用 `cargo check` 验证代码可以编译
//!    ```bash
//!    cargo check -p sync --target riscv64gc-unknown-none-elf
//!    ```
//!
//! 2. **集成测试**：在实际内核环境中验证功能
//!
//! 3. **RISC-V 64 位主机**：如果有 RISC-V 64 位主机，可以直接运行单元测试
//!    ```bash
//!    cargo test -p sync --test api_tests
//!    ```

#[cfg(target_arch = "riscv64")]
mod tests {
    use std::sync::Arc;
    use rcore_task_manage::ThreadId;
    use sync::{Condvar, Mutex, MutexBlocking, Semaphore};

    #[test]
    fn test_mutex_blocking_new() {
        let m = MutexBlocking::new();
        // 首次 lock 应成功
        let tid = ThreadId::from_usize(1);
        assert!(m.lock(tid));
        // 释放后无等待者，应返回 None
        assert!(m.unlock().is_none());
    }

    #[test]
    fn test_mutex_blocking_lock_unlock() {
        let m = MutexBlocking::new();
        let t1 = ThreadId::from_usize(10);
        let t2 = ThreadId::from_usize(20);

        assert!(m.lock(t1));
        // 已持锁，再次 lock(t2) 应失败，t2 进入等待队列
        assert!(!m.lock(t2));
        // 释放时应唤醒一个等待者，返回 Some(t2)
        let woken = m.unlock();
        assert_eq!(woken, Some(t2));
    }

    #[test]
    fn test_mutex_blocking_multiple_waiters() {
        let m = MutexBlocking::new();
        let t1 = ThreadId::from_usize(1);
        let t2 = ThreadId::from_usize(2);
        let t3 = ThreadId::from_usize(3);

        assert!(m.lock(t1));
        assert!(!m.lock(t2));
        assert!(!m.lock(t3));
        // 队列为 [t2, t3]，unlock 应返回 t2
        assert_eq!(m.unlock(), Some(t2));
        // 再次 unlock 应返回 t3
        assert_eq!(m.unlock(), Some(t3));
        // 无等待者，返回 None
        assert!(m.unlock().is_none());
    }

    #[test]
    fn test_condvar_new() {
        let cv = Condvar::new();
        // 初始无等待者，signal 返回 None
        assert!(cv.signal().is_none());
    }

    #[test]
    fn test_condvar_wait_no_sched_and_signal() {
        let cv = Condvar::new();
        let tid = ThreadId::from_usize(1);
        cv.wait_no_sched(tid);
        assert_eq!(cv.signal(), Some(tid));
        assert!(cv.signal().is_none());
    }

    #[test]
    fn test_condvar_multiple_waiters() {
        let cv = Condvar::new();
        let t1 = ThreadId::from_usize(1);
        let t2 = ThreadId::from_usize(2);
        cv.wait_no_sched(t1);
        cv.wait_no_sched(t2);
        assert_eq!(cv.signal(), Some(t1));
        assert_eq!(cv.signal(), Some(t2));
        assert!(cv.signal().is_none());
    }

    #[test]
    fn test_condvar_wait_with_mutex() {
        let cv = Condvar::new();
        let mutex: Arc<dyn Mutex> = Arc::new(MutexBlocking::new());
        let t1 = ThreadId::from_usize(1);
        let t2 = ThreadId::from_usize(2);

        assert!(mutex.lock(t1));
        assert!(!mutex.lock(t2));
        let (got_lock, woken) = cv.wait_with_mutex(t1, mutex.clone());
        assert_eq!(woken, Some(t2));
        assert!(!got_lock);
    }

    #[test]
    fn test_semaphore_new() {
        let s = Semaphore::new(3);
        let tid = ThreadId::from_usize(1);
        assert!(s.down(tid));
        assert!(s.down(tid));
        assert!(s.down(tid));
        assert!(!s.down(tid));
    }

    #[test]
    fn test_semaphore_down_up() {
        let s = Semaphore::new(1);
        let t1 = ThreadId::from_usize(1);
        let t2 = ThreadId::from_usize(2);

        assert!(s.down(t1));
        assert!(!s.down(t2));
        let woken = s.up();
        assert_eq!(woken, Some(t2));
    }

    #[test]
    fn test_semaphore_up_without_waiter() {
        let s = Semaphore::new(2);
        let tid = ThreadId::from_usize(1);
        assert!(s.down(tid));
        assert!(s.down(tid));
        assert!(s.up().is_none());
        assert!(s.up().is_none());
    }

    #[test]
    fn test_semaphore_multiple_waiters() {
        let s = Semaphore::new(0);
        let t1 = ThreadId::from_usize(1);
        let t2 = ThreadId::from_usize(2);
        let t3 = ThreadId::from_usize(3);

        assert!(!s.down(t1));
        assert!(!s.down(t2));
        assert!(!s.down(t3));
        assert_eq!(s.up(), Some(t1));
        assert_eq!(s.up(), Some(t2));
        assert_eq!(s.up(), Some(t3));
        assert!(s.up().is_none());
    }
}

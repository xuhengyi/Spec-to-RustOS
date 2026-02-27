#![no_std]

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::cell::{RefCell, RefMut};
use core::ops::{Deref, DerefMut};
use rcore_task_manage::ThreadId;
use spin::Lazy;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod arch_intr {
    pub fn intr_enabled() -> bool {
        riscv::register::sstatus::read().sie()
    }

    pub fn disable_intr() {
        unsafe {
            riscv::register::sstatus::clear_sie();
        }
    }

    pub fn enable_intr() {
        unsafe {
            riscv::register::sstatus::set_sie();
        }
    }
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
mod arch_intr {
    pub fn intr_enabled() -> bool {
        false
    }

    pub fn disable_intr() {}

    pub fn enable_intr() {}
}

struct IntrState {
    nesting: usize,
    prev_enabled: bool,
}

static INTR_STATE: Lazy<spin::Mutex<IntrState>> = Lazy::new(|| {
    spin::Mutex::new(IntrState {
        nesting: 0,
        prev_enabled: false,
    })
});

fn push_off() {
    let mut state = INTR_STATE.lock();
    if state.nesting == 0 {
        state.prev_enabled = arch_intr::intr_enabled();
        arch_intr::disable_intr();
    }
    state.nesting += 1;
}

fn pop_off() {
    let mut state = INTR_STATE.lock();
    if state.nesting == 0 {
        panic!("interrupt nesting underflow");
    }
    state.nesting -= 1;
    if state.nesting == 0 && state.prev_enabled {
        arch_intr::enable_intr();
    }
}

pub struct UPIntrFreeCell<T> {
    inner: RefCell<T>,
}

pub struct UPIntrRefMut<'a, T> {
    borrow: RefMut<'a, T>,
}

impl<T> UPIntrFreeCell<T> {
    pub unsafe fn new(value: T) -> Self {
        Self {
            inner: RefCell::new(value),
        }
    }

    pub fn exclusive_access(&self) -> UPIntrRefMut<'_, T> {
        push_off();
        match self.inner.try_borrow_mut() {
            Ok(borrow) => UPIntrRefMut { borrow },
            Err(_) => {
                pop_off();
                panic!("UPIntrFreeCell already borrowed");
            }
        }
    }

    pub fn exclusive_session<F, V>(&self, f: F) -> V
    where
        F: FnOnce(&mut T) -> V,
    {
        let mut guard = self.exclusive_access();
        f(&mut guard)
    }
}

impl<'a, T> Deref for UPIntrRefMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.borrow
    }
}

impl<'a, T> DerefMut for UPIntrRefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.borrow
    }
}

impl<'a, T> Drop for UPIntrRefMut<'a, T> {
    fn drop(&mut self) {
        pop_off();
    }
}

pub trait Mutex {
    fn lock(&self, tid: ThreadId) -> bool;
    fn unlock(&self) -> Option<ThreadId>;
}

struct MutexBlockingInner {
    locked: bool,
    waiting: VecDeque<ThreadId>,
}

pub struct MutexBlocking {
    inner: UPIntrFreeCell<MutexBlockingInner>,
}

impl MutexBlocking {
    pub fn new() -> Self {
        Self {
            inner: unsafe {
                UPIntrFreeCell::new(MutexBlockingInner {
                    locked: false,
                    waiting: VecDeque::new(),
                })
            },
        }
    }
}

impl Mutex for MutexBlocking {
    fn lock(&self, tid: ThreadId) -> bool {
        self.inner.exclusive_session(|inner| {
            if inner.locked {
                inner.waiting.push_back(tid);
                false
            } else {
                inner.locked = true;
                true
            }
        })
    }

    fn unlock(&self) -> Option<ThreadId> {
        self.inner.exclusive_session(|inner| {
            if !inner.locked {
                panic!("unlock on unlocked mutex");
            }
            if let Some(tid) = inner.waiting.pop_front() {
                Some(tid)
            } else {
                inner.locked = false;
                None
            }
        })
    }
}

pub struct Condvar {
    waiting: UPIntrFreeCell<VecDeque<ThreadId>>,
}

impl Condvar {
    pub fn new() -> Self {
        Self {
            waiting: unsafe { UPIntrFreeCell::new(VecDeque::new()) },
        }
    }

    pub fn signal(&self) -> Option<ThreadId> {
        self.waiting
            .exclusive_session(|queue| queue.pop_front())
    }

    pub fn wait_no_sched(&self, tid: ThreadId) -> bool {
        self.waiting
            .exclusive_session(|queue| queue.push_back(tid));
        false
    }

    pub fn wait_with_mutex(
        &self,
        tid: ThreadId,
        mutex: Arc<dyn Mutex>,
    ) -> (bool, Option<ThreadId>) {
        let woken_tid = mutex.unlock().unwrap();
        let got_lock = mutex.lock(tid);
        (got_lock, Some(woken_tid))
    }
}

pub struct Semaphore {
    inner: UPIntrFreeCell<SemaphoreInner>,
}

struct SemaphoreInner {
    count: isize,
    waiting: VecDeque<ThreadId>,
}

impl Semaphore {
    pub fn new(res_count: usize) -> Self {
        Self {
            inner: unsafe {
                UPIntrFreeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    waiting: VecDeque::new(),
                })
            },
        }
    }

    pub fn down(&self, tid: ThreadId) -> bool {
        self.inner.exclusive_session(|inner| {
            inner.count -= 1;
            if inner.count < 0 {
                inner.waiting.push_back(tid);
                false
            } else {
                true
            }
        })
    }

    pub fn up(&self) -> Option<ThreadId> {
        self.inner.exclusive_session(|inner| {
            inner.count += 1;
            inner.waiting.pop_front()
        })
    }
}

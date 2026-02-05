//! task-manage: 任务标识符类型、任务存储与就绪队列抽象
//!
//! 提供 `ProcId`、`ThreadId`、`CoroId` 等单调递增的 ID 类型，
//! 以及 `Manage`、`Schedule` trait 抽象任务存储与调度。

#![no_std]

extern crate alloc;

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::sync::atomic::{AtomicUsize, Ordering::SeqCst};

// =============================================================================
// 任务标识符类型 (ProcId, ThreadId, CoroId)
// =============================================================================

macro_rules! impl_id_type {
    ($name:ident, $counter:ident) => {
        /// 单调递增的任务标识符
        #[derive(Clone, Copy)]
        pub struct $name(usize);

        static $counter: AtomicUsize = AtomicUsize::new(0);

        impl $name {
            /// 创建新的单调递增 ID
            #[inline]
            pub fn new() -> Self {
                let v = $counter.fetch_add(1, SeqCst);
                Self(v)
            }

            /// 从原始值构造
            #[inline]
            pub fn from_usize(v: usize) -> Self {
                Self(v)
            }

            /// 获取原始值
            #[inline]
            pub fn get_usize(self) -> usize {
                self.0
            }
        }

        impl Default for $name {
            #[inline]
            fn default() -> Self {
                Self::new()
            }
        }

        impl PartialEq for $name {
            #[inline]
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }

        impl Eq for $name {}

        impl PartialOrd for $name {
            #[inline]
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for $name {
            #[inline]
            fn cmp(&self, other: &Self) -> Ordering {
                self.0.cmp(&other.0)
            }
        }

        impl Hash for $name {
            #[inline]
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.0.hash(state);
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }
    };
}

impl_id_type!(ProcId, PROC_ID_COUNTER);
impl_id_type!(ThreadId, THREAD_ID_COUNTER);
impl_id_type!(CoroId, CORO_ID_COUNTER);

// =============================================================================
// 泛型任务存储接口 Manage
// =============================================================================

/// 泛型任务存储 trait：按 ID 进行 CRUD 操作
pub trait Manage<T, I: Copy + Ord> {
    /// 存储 item 到 id 下
    fn insert(&mut self, id: I, item: T);
    /// 删除 id 下的项
    fn delete(&mut self, id: I);
    /// 获取 id 对应的可变引用
    fn get_mut(&mut self, id: I) -> Option<&mut T>;
}

// =============================================================================
// 泛型就绪队列接口 Schedule
// =============================================================================

/// 泛型就绪队列 trait：表示可运行队列
pub trait Schedule<I: Copy + Ord> {
    /// 将 id 加入队列
    fn add(&mut self, id: I);
    /// 从队列取出一个 id
    fn fetch(&mut self) -> Option<I>;
}

// =============================================================================
// Feature: proc - 进程父子关系与管理
// =============================================================================

#[cfg(feature = "proc")]
mod proc_feature {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    /// 进程树父子关系追踪
    pub struct ProcRel {
        parent: ProcId,
        children: Vec<ProcId>,
        dead_children: Vec<(ProcId, isize)>,
    }

    impl ProcRel {
        pub fn new(parent_pid: ProcId) -> Self {
            Self {
                parent: parent_pid,
                children: Vec::new(),
                dead_children: Vec::new(),
            }
        }

        pub fn add_child(&mut self, child_pid: ProcId) {
            self.children.push(child_pid);
        }

        pub fn del_child(&mut self, child_pid: ProcId, exit_code: isize) {
            if let Some(pos) = self.children.iter().position(|&c| c == child_pid) {
                self.children.remove(pos);
                self.dead_children.push((child_pid, exit_code));
            }
        }

        pub fn wait_any_child(&mut self) -> Option<(ProcId, isize)> {
            if self.children.is_empty() && self.dead_children.is_empty() {
                return None;
            }
            if !self.dead_children.is_empty() {
                return Some(self.dead_children.remove(0));
            }
            // 有活跃子进程且无已死子进程：返回 sentinel
            Some((ProcId::from_usize(usize::MAX - 1), -1))
        }

        pub fn wait_child(&mut self, child_pid: ProcId) -> Option<(ProcId, isize)> {
            if let Some(pos) = self.dead_children.iter().position(|(c, _)| *c == child_pid) {
                return Some(self.dead_children.remove(pos));
            }
            if self.children.contains(&child_pid) {
                return Some((ProcId::from_usize(usize::MAX - 1), -1));
            }
            None
        }
    }

    /// 进程管理辅助：结合存储、调度与父子关系
    pub struct PManager<P, MP> {
        manager: Option<MP>,
        relations: BTreeMap<ProcId, ProcRel>,
        current: Option<ProcId>,
        _phantom: core::marker::PhantomData<P>,
    }

    impl<P, MP> PManager<P, MP>
    where
        MP: Manage<P, ProcId> + Schedule<ProcId>,
    {
        pub fn new() -> Self {
            Self {
                manager: None,
                relations: BTreeMap::new(),
                current: None,
                _phantom: core::marker::PhantomData,
            }
        }

        pub fn set_manager(&mut self, manager: MP) {
            self.manager = Some(manager);
        }

        fn manager(&mut self) -> &mut MP {
            self.manager.as_mut().expect("must call set_manager first")
        }

        pub fn add(&mut self, id: ProcId, task: P, parent: ProcId) {
            let m = self.manager();
            m.insert(id, task);
            m.add(id);
            self.relations
                .entry(parent)
                .or_insert_with(|| ProcRel::new(parent))
                .add_child(id);
            self.relations
                .entry(id)
                .or_insert_with(|| ProcRel::new(parent));
        }

        pub fn find_next(&mut self) -> Option<&mut P> {
            let self_ptr: *mut Self = self;
            loop {
                let id = unsafe { (*self_ptr).manager().fetch() };
                let id = match id {
                    Some(id) => id,
                    None => {
                        unsafe { (*self_ptr).current = None };
                        return None;
                    }
                };
                if let Some(task) = unsafe { (*self_ptr).manager().get_mut(id) } {
                    unsafe { (*self_ptr).current = Some(id) };
                    return Some(task);
                }
            }
        }

        pub fn current(&mut self) -> Option<&mut P> {
            let id = self.current?;
            self.manager().get_mut(id)
        }

        pub fn get_task(&mut self, id: ProcId) -> Option<&mut P> {
            self.manager().get_mut(id)
        }

        pub fn make_current_suspend(&mut self) {
            if let Some(id) = self.current.take() {
                self.manager().add(id);
            }
        }

        pub fn make_current_exited(&mut self, exit_code: isize) {
            let exiting_pid = match self.current.take() {
                Some(id) => id,
                None => return,
            };

            let m = self.manager();
            m.delete(exiting_pid);

            // 更新父进程关系
            if let Some(rel) = self.relations.get_mut(&exiting_pid) {
                let parent = rel.parent;
                if let Some(parent_rel) = self.relations.get_mut(&parent) {
                    parent_rel.del_child(exiting_pid, exit_code);
                }
            }

            // reparent 子进程到 init (ProcId::from_usize(0))
            let init_pid = ProcId::from_usize(0);
            if let Some(rel) = self.relations.get(&exiting_pid) {
                let children: Vec<ProcId> = rel.children.clone();
                for child_pid in children {
                    if let Some(child_rel) = self.relations.get_mut(&child_pid) {
                        child_rel.parent = init_pid;
                    }
                    if let Some(init_rel) = self.relations.get_mut(&init_pid) {
                        init_rel.add_child(child_pid);
                    }
                }
            }
            if let Some(rel) = self.relations.get_mut(&exiting_pid) {
                rel.children.clear();
            }
        }

        pub fn wait(&mut self, child_pid: ProcId) -> Option<(ProcId, isize)> {
            let current_pid = self.current?;
            let rel = self.relations.get_mut(&current_pid)?;
            if child_pid.get_usize() == usize::MAX {
                rel.wait_any_child()
            } else {
                rel.wait_child(child_pid)
            }
        }
    }

    impl<P, MP> Default for PManager<P, MP>
    where
        MP: Manage<P, ProcId> + Schedule<ProcId>,
    {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(feature = "proc")]
pub use proc_feature::{PManager, ProcRel};

// =============================================================================
// Feature: thread - 进程-线程关系与 combined 管理
// =============================================================================

#[cfg(feature = "thread")]
mod thread_feature {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    /// 进程-线程关系：进程树 + 线程集合
    pub struct ProcThreadRel {
        parent: ProcId,
        children: Vec<ProcId>,
        dead_children: Vec<(ProcId, isize)>,
        threads: Vec<ThreadId>,
        dead_threads: Vec<(ThreadId, isize)>,
    }

    impl ProcThreadRel {
        pub fn new(parent: ProcId) -> Self {
            Self {
                parent,
                children: Vec::new(),
                dead_children: Vec::new(),
                threads: Vec::new(),
                dead_threads: Vec::new(),
            }
        }

        pub fn add_child(&mut self, child_pid: ProcId) {
            self.children.push(child_pid);
        }

        pub fn del_child(&mut self, child_pid: ProcId, exit_code: isize) {
            if let Some(pos) = self.children.iter().position(|&c| c == child_pid) {
                self.children.remove(pos);
                self.dead_children.push((child_pid, exit_code));
            }
        }

        pub fn wait_any_child(&mut self) -> Option<(ProcId, isize)> {
            if self.children.is_empty() && self.dead_children.is_empty() {
                return None;
            }
            if !self.dead_children.is_empty() {
                return Some(self.dead_children.remove(0));
            }
            Some((ProcId::from_usize(usize::MAX - 1), -1))
        }

        pub fn wait_child(&mut self, child_pid: ProcId) -> Option<(ProcId, isize)> {
            if let Some(pos) = self.dead_children.iter().position(|(c, _)| *c == child_pid) {
                return Some(self.dead_children.remove(pos));
            }
            if self.children.contains(&child_pid) {
                return Some((ProcId::from_usize(usize::MAX - 1), -1));
            }
            None
        }

        pub fn add_thread(&mut self, tid: ThreadId) {
            self.threads.push(tid);
        }

        pub fn del_thread(&mut self, tid: ThreadId, exit_code: isize) {
            if let Some(pos) = self.threads.iter().position(|&t| t == tid) {
                self.threads.remove(pos);
                self.dead_threads.push((tid, exit_code));
            }
        }

        pub fn wait_thread(&mut self, thread_tid: ThreadId) -> Option<isize> {
            if let Some(pos) = self.dead_threads.iter().position(|(t, _)| *t == thread_tid) {
                return Some(self.dead_threads.remove(pos).1);
            }
            if self.threads.contains(&thread_tid) {
                return Some(-2);
            }
            None
        }
    }

    /// 进程+线程联合管理
    pub struct PThreadManager<P, T, MT, MP> {
        thread_manager: Option<MT>,
        proc_manager: Option<MP>,
        relations: BTreeMap<ProcId, ProcThreadRel>,
        tid2pid: BTreeMap<ThreadId, ProcId>,
        current: Option<ThreadId>,
        _phantom: core::marker::PhantomData<(P, T)>,
    }

    impl<P, T, MT, MP> PThreadManager<P, T, MT, MP>
    where
        MT: Manage<T, ThreadId> + Schedule<ThreadId>,
        MP: Manage<P, ProcId>,
    {
        pub fn new() -> Self {
            Self {
                thread_manager: None,
                proc_manager: None,
                relations: BTreeMap::new(),
                tid2pid: BTreeMap::new(),
                current: None,
                _phantom: core::marker::PhantomData,
            }
        }

        pub fn set_manager(&mut self, manager: MT) {
            self.thread_manager = Some(manager);
        }

        pub fn set_proc_manager(&mut self, proc_manager: MP) {
            self.proc_manager = Some(proc_manager);
        }

        fn thread_manager(&mut self) -> &mut MT {
            self.thread_manager
                .as_mut()
                .expect("must call set_manager first")
        }

        fn proc_manager(&mut self) -> &mut MP {
            self.proc_manager
                .as_mut()
                .expect("must call set_proc_manager first")
        }

        pub fn add_proc(&mut self, id: ProcId, proc: P, parent: ProcId) {
            let pm = self.proc_manager();
            pm.insert(id, proc);
            self.relations
                .entry(parent)
                .or_insert_with(|| ProcThreadRel::new(parent))
                .add_child(id);
            self.relations
                .entry(id)
                .or_insert_with(|| ProcThreadRel::new(parent));
        }

        pub fn add(&mut self, id: ThreadId, task: T, pid: ProcId) {
            let tm = self.thread_manager();
            tm.insert(id, task);
            tm.add(id);
            self.tid2pid.insert(id, pid);
            self.relations
                .entry(pid)
                .or_insert_with(|| ProcThreadRel::new(ProcId::from_usize(0)))
                .add_thread(id);
        }

        pub fn find_next(&mut self) -> Option<&mut T> {
            let self_ptr: *mut Self = self;
            loop {
                let id = unsafe { (*self_ptr).thread_manager().fetch() };
                let id = match id {
                    Some(id) => id,
                    None => {
                        unsafe { (*self_ptr).current = None };
                        return None;
                    }
                };
                if let Some(task) = unsafe { (*self_ptr).thread_manager().get_mut(id) } {
                    unsafe { (*self_ptr).current = Some(id) };
                    return Some(task);
                }
            }
        }

        pub fn make_current_suspend(&mut self) {
            if let Some(id) = self.current.take() {
                self.thread_manager().add(id);
            }
        }

        pub fn make_current_blocked(&mut self) {
            self.current = None;
        }

        pub fn make_current_exited(&mut self, exit_code: isize) {
            let exiting_tid = match self.current.take() {
                Some(id) => id,
                None => return,
            };

            let pid = *self.tid2pid.get(&exiting_tid).expect("tid2pid must have entry");
            let tm = self.thread_manager();
            tm.delete(exiting_tid);

            let active_count = {
                let rel = self.relations.get_mut(&pid).expect("relations must have entry");
                rel.del_thread(exiting_tid, exit_code);
                rel.threads.len()
            };
            if active_count == 0 {
                self.del_proc(pid, exit_code);
            }
        }

        pub fn re_enque(&mut self, id: ThreadId) {
            self.thread_manager().add(id);
        }

        pub fn current(&mut self) -> Option<&mut T> {
            let id = self.current?;
            self.thread_manager().get_mut(id)
        }

        pub fn get_task(&mut self, id: ThreadId) -> Option<&mut T> {
            self.thread_manager().get_mut(id)
        }

        pub fn get_proc(&mut self, id: ProcId) -> Option<&mut P> {
            self.proc_manager().get_mut(id)
        }

        pub fn del_proc(&mut self, id: ProcId, exit_code: isize) {
            let parent = self.relations.get(&id).map(|r| r.parent);
            let thread_ids: alloc::vec::Vec<ThreadId> = self
                .relations
                .get(&id)
                .map(|r| r.threads.clone())
                .unwrap_or_default();

            let pm = self.proc_manager();
            pm.delete(id);

            for tid in &thread_ids {
                self.tid2pid.remove(tid);
            }
            self.relations.remove(&id);

            if let Some(parent_pid) = parent {
                if let Some(parent_rel) = self.relations.get_mut(&parent_pid) {
                    parent_rel.del_child(id, exit_code);
                }
            }
        }

        pub fn wait(&mut self, child_pid: ProcId) -> Option<(ProcId, isize)> {
            let current_tid = self.current?;
            let pid = *self.tid2pid.get(&current_tid)?;
            let rel = self.relations.get_mut(&pid)?;
            if child_pid.get_usize() == usize::MAX {
                rel.wait_any_child()
            } else {
                rel.wait_child(child_pid)
            }
        }

        pub fn waittid(&mut self, thread_tid: ThreadId) -> Option<isize> {
            let current_tid = self.current?;
            let pid = *self.tid2pid.get(&current_tid)?;
            self.relations.get_mut(&pid)?.wait_thread(thread_tid)
        }

        pub fn thread_count(&self, id: ProcId) -> usize {
            self.relations
                .get(&id)
                .map(|r| r.threads.len())
                .unwrap_or(0)
        }

        pub fn get_thread(&mut self, id: ProcId) -> Option<&alloc::vec::Vec<ThreadId>> {
            self.relations.get(&id).map(|r| &r.threads)
        }

        pub fn get_current_proc(&mut self) -> Option<&mut P> {
            let current_tid = self.current?;
            let pid = *self.tid2pid.get(&current_tid)?;
            self.proc_manager().get_mut(pid)
        }
    }

    impl<P, T, MT, MP> Default for PThreadManager<P, T, MT, MP>
    where
        MT: Manage<T, ThreadId> + Schedule<ThreadId>,
        MP: Manage<P, ProcId>,
    {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(feature = "thread")]
pub use thread_feature::{ProcThreadRel, PThreadManager};

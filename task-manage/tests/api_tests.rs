//! task-manage crate 功能性验证测试
//! 
//! 这些测试验证 task-manage crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。

use std::collections::HashMap;
use std::collections::VecDeque;
use rcore_task_manage::*;

// 简单的 Manage trait 实现用于测试
struct TestManager<T> {
    items: HashMap<usize, T>,
}

impl<T> TestManager<T> {
    fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }
}

impl<T> Manage<T, usize> for TestManager<T> {
    fn insert(&mut self, id: usize, item: T) {
        self.items.insert(id, item);
    }
    
    fn delete(&mut self, id: usize) {
        self.items.remove(&id);
    }
    
    fn get_mut(&mut self, id: usize) -> Option<&mut T> {
        self.items.get_mut(&id)
    }
}

// 简单的 Schedule trait 实现用于测试
struct TestScheduler<I> {
    queue: VecDeque<I>,
}

impl<I: Copy + Ord> TestScheduler<I> {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }
}

impl<I: Copy + Ord> Schedule<I> for TestScheduler<I> {
    fn add(&mut self, id: I) {
        self.queue.push_back(id);
    }
    
    fn fetch(&mut self) -> Option<I> {
        self.queue.pop_front()
    }
}

#[test]
fn test_proc_id_new() {
    // 测试 ProcId::new()
    let id1 = ProcId::new();
    let id2 = ProcId::new();
    let id3 = ProcId::new();
    
    // 验证 ID 是递增的
    assert!(id1.get_usize() < id2.get_usize());
    assert!(id2.get_usize() < id3.get_usize());
}

#[test]
fn test_proc_id_from_usize() {
    // 测试 ProcId::from_usize()
    let id = ProcId::from_usize(42);
    assert_eq!(id.get_usize(), 42);
}

#[test]
fn test_proc_id_get_usize() {
    // 测试 ProcId::get_usize()
    let id = ProcId::from_usize(100);
    assert_eq!(id.get_usize(), 100);
}

#[test]
fn test_proc_id_eq() {
    // 测试 ProcId 的 Eq trait
    let id1 = ProcId::from_usize(42);
    let id2 = ProcId::from_usize(42);
    let id3 = ProcId::from_usize(43);
    
    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_proc_id_ord() {
    // 测试 ProcId 的 Ord trait
    let id1 = ProcId::from_usize(10);
    let id2 = ProcId::from_usize(20);
    
    assert!(id1 < id2);
    assert!(id2 > id1);
}

#[test]
fn test_proc_id_clone_copy() {
    // 测试 ProcId 的 Clone 和 Copy trait
    let id1 = ProcId::from_usize(42);
    let id2 = id1; // Copy
    let id3 = id1.clone(); // Clone
    
    assert_eq!(id1, id2);
    assert_eq!(id1, id3);
}

#[test]
fn test_thread_id_new() {
    // 测试 ThreadId::new()
    let id1 = ThreadId::new();
    let id2 = ThreadId::new();
    let id3 = ThreadId::new();
    
    // 验证 ID 是递增的
    assert!(id1.get_usize() < id2.get_usize());
    assert!(id2.get_usize() < id3.get_usize());
}

#[test]
fn test_thread_id_from_usize() {
    // 测试 ThreadId::from_usize()
    let id = ThreadId::from_usize(42);
    assert_eq!(id.get_usize(), 42);
}

#[test]
fn test_thread_id_get_usize() {
    // 测试 ThreadId::get_usize()
    let id = ThreadId::from_usize(100);
    assert_eq!(id.get_usize(), 100);
}

#[test]
fn test_thread_id_eq() {
    // 测试 ThreadId 的 Eq trait
    let id1 = ThreadId::from_usize(42);
    let id2 = ThreadId::from_usize(42);
    let id3 = ThreadId::from_usize(43);
    
    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_thread_id_ord() {
    // 测试 ThreadId 的 Ord trait
    let id1 = ThreadId::from_usize(10);
    let id2 = ThreadId::from_usize(20);
    
    assert!(id1 < id2);
    assert!(id2 > id1);
}

#[test]
fn test_thread_id_clone_copy() {
    // 测试 ThreadId 的 Clone 和 Copy trait
    let id1 = ThreadId::from_usize(42);
    let id2 = id1; // Copy
    let id3 = id1.clone(); // Clone
    
    assert_eq!(id1, id2);
    assert_eq!(id1, id3);
}

#[test]
fn test_coro_id_new() {
    // 测试 CoroId::new()
    let id1 = CoroId::new();
    let id2 = CoroId::new();
    let id3 = CoroId::new();
    
    // 验证 ID 是递增的
    assert!(id1.get_usize() < id2.get_usize());
    assert!(id2.get_usize() < id3.get_usize());
}

#[test]
fn test_coro_id_from_usize() {
    // 测试 CoroId::from_usize()
    let id = CoroId::from_usize(42);
    assert_eq!(id.get_usize(), 42);
}

#[test]
fn test_coro_id_get_usize() {
    // 测试 CoroId::get_usize()
    let id = CoroId::from_usize(100);
    assert_eq!(id.get_usize(), 100);
}

#[test]
fn test_coro_id_eq() {
    // 测试 CoroId 的 Eq trait
    let id1 = CoroId::from_usize(42);
    let id2 = CoroId::from_usize(42);
    let id3 = CoroId::from_usize(43);
    
    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_coro_id_ord() {
    // 测试 CoroId 的 Ord trait
    let id1 = CoroId::from_usize(10);
    let id2 = CoroId::from_usize(20);
    
    assert!(id1 < id2);
    assert!(id2 > id1);
}

#[test]
fn test_coro_id_clone_copy() {
    // 测试 CoroId 的 Clone 和 Copy trait
    let id1 = CoroId::from_usize(42);
    let id2 = id1; // Copy
    let id3 = id1.clone(); // Clone
    
    assert_eq!(id1, id2);
    assert_eq!(id1, id3);
}

#[test]
fn test_manage_trait_insert() {
    // 测试 Manage trait 的 insert 方法
    let mut manager: TestManager<String> = TestManager::new();
    manager.insert(1, "item1".to_string());
    manager.insert(2, "item2".to_string());
    
    assert_eq!(manager.get_mut(1), Some(&mut "item1".to_string()));
    assert_eq!(manager.get_mut(2), Some(&mut "item2".to_string()));
}

#[test]
fn test_manage_trait_delete() {
    // 测试 Manage trait 的 delete 方法
    let mut manager: TestManager<String> = TestManager::new();
    manager.insert(1, "item1".to_string());
    manager.insert(2, "item2".to_string());
    
    manager.delete(1);
    assert_eq!(manager.get_mut(1), None);
    assert_eq!(manager.get_mut(2), Some(&mut "item2".to_string()));
}

#[test]
fn test_manage_trait_get_mut() {
    // 测试 Manage trait 的 get_mut 方法
    let mut manager: TestManager<i32> = TestManager::new();
    manager.insert(1, 100);
    manager.insert(2, 200);
    
    let item = manager.get_mut(1);
    assert_eq!(item, Some(&mut 100));
    
    // 测试修改
    if let Some(item) = manager.get_mut(1) {
        *item = 300;
    }
    assert_eq!(manager.get_mut(1), Some(&mut 300));
}

#[test]
fn test_schedule_trait_add() {
    // 测试 Schedule trait 的 add 方法
    let mut scheduler: TestScheduler<usize> = TestScheduler::new();
    scheduler.add(1);
    scheduler.add(2);
    scheduler.add(3);
    
    assert_eq!(scheduler.fetch(), Some(1));
    assert_eq!(scheduler.fetch(), Some(2));
    assert_eq!(scheduler.fetch(), Some(3));
}

#[test]
fn test_schedule_trait_fetch() {
    // 测试 Schedule trait 的 fetch 方法
    let mut scheduler: TestScheduler<usize> = TestScheduler::new();
    
    // 空队列应该返回 None
    assert_eq!(scheduler.fetch(), None);
    
    scheduler.add(1);
    scheduler.add(2);
    
    assert_eq!(scheduler.fetch(), Some(1));
    assert_eq!(scheduler.fetch(), Some(2));
    assert_eq!(scheduler.fetch(), None);
}

#[test]
fn test_schedule_trait_fifo_order() {
    // 测试 Schedule trait 的 FIFO 顺序
    let mut scheduler: TestScheduler<usize> = TestScheduler::new();
    
    scheduler.add(1);
    scheduler.add(2);
    scheduler.add(3);
    
    // 验证 FIFO 顺序
    assert_eq!(scheduler.fetch(), Some(1));
    assert_eq!(scheduler.fetch(), Some(2));
    assert_eq!(scheduler.fetch(), Some(3));
}

#[test]
fn test_id_types_hash() {
    // 测试 ID 类型的 Hash trait
    use std::collections::HashSet;
    
    let mut set = HashSet::new();
    set.insert(ProcId::from_usize(1));
    set.insert(ProcId::from_usize(2));
    set.insert(ProcId::from_usize(1)); // 重复的应该被忽略
    
    assert_eq!(set.len(), 2);
    assert!(set.contains(&ProcId::from_usize(1)));
    assert!(set.contains(&ProcId::from_usize(2)));
}

#[test]
fn test_id_types_debug() {
    // 测试 ID 类型的 Debug trait
    let proc_id = ProcId::from_usize(42);
    let thread_id = ThreadId::from_usize(100);
    let coro_id = CoroId::from_usize(200);
    
    let proc_debug = format!("{:?}", proc_id);
    let thread_debug = format!("{:?}", thread_id);
    let coro_debug = format!("{:?}", coro_id);
    
    assert!(proc_debug.contains("ProcId"));
    assert!(thread_debug.contains("ThreadId"));
    assert!(coro_debug.contains("CoroId"));
}

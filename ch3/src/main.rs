#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;
use core::slice;

use kernel_context::LocalContext;
use linker::{AppMeta, KernelLayout};
use rcore_console::{init_console, log, print, println, set_log_level, Console};
use riscv::register::{scause, sie, time};
use sbi_rt::{NoReason, Shutdown, SystemFailure};
use syscall::{Caller, ClockId, SyscallId, SyscallResult, TimeSpec, STDDEBUG, STDOUT};

// 使用 linker crate 的 boot0! 宏导出 _start 入口
linker::boot0!(rust_main; stack = 4 * 4096);

// 内联应用程序集
global_asm!(include_str!(env!("APP_ASM")));

// 用户栈大小
const USER_STACK_SIZE: usize = 4096;
// 最大应用数量
const MAX_APP_NUM: usize = 16;

// 用户栈（为每个应用静态分配）
static mut USER_STACKS: [[u8; USER_STACK_SIZE]; MAX_APP_NUM] = [[0u8; USER_STACK_SIZE]; MAX_APP_NUM];

// 任务控制块
struct TaskControlBlock {
    /// 用户上下文
    context: LocalContext,
    /// 任务是否已完成
    finished: bool,
}

impl TaskControlBlock {
    /// 创建一个新的任务控制块
    fn new(entry: usize, stack_top: usize) -> Self {
        let mut context = LocalContext::user(entry);
        // 设置用户栈顶
        *context.sp_mut() = stack_top;
        Self {
            context,
            finished: false,
        }
    }
}

// SBI console 实现
struct SbiConsole;

impl Console for SbiConsole {
    fn put_char(&self, c: u8) {
        #[allow(deprecated)]
        sbi_rt::legacy::console_putchar(c as usize);
    }
}

/// 内核主入口
#[unsafe(no_mangle)]
extern "C" fn rust_main() -> ! {
    // 1. 清零 BSS
    unsafe { KernelLayout::locate().zero_bss() };

    // 2. 初始化 console/log
    init_console(&SbiConsole);
    set_log_level(option_env!("LOG"));

    // 3. 初始化 syscall 子系统
    syscall::init_io(&SyscallContext);
    syscall::init_process(&SyscallContext);
    syscall::init_scheduling(&SyscallContext);
    syscall::init_clock(&SyscallContext);

    // 4. 枚举应用并初始化任务
    let app_meta = AppMeta::locate();
    let mut num_apps = 0usize;

    // 静态分配任务数组
    static mut TASKS: [Option<TaskControlBlock>; MAX_APP_NUM] = [const { None }; MAX_APP_NUM];

    // 初始化任务
    for (i, app) in app_meta.iter().enumerate() {
        if i >= MAX_APP_NUM {
            break;
        }
        let entry = app.as_ptr() as usize;
        let stack_top = unsafe { USER_STACKS[i].as_ptr().add(USER_STACK_SIZE) as usize };
        log::debug!("App {}: entry = {:#x}, size = {:#x}", i, entry, app.len());
        unsafe {
            TASKS[i] = Some(TaskControlBlock::new(entry, stack_top));
        }
        num_apps = i + 1;
    }

    log::info!("Found {} applications", num_apps);

    if num_apps == 0 {
        log::info!("No applications to run, shutting down");
        sbi_rt::system_reset(Shutdown, NoReason);
        unreachable!()
    }

    // 5. 开启 supervisor timer interrupt
    unsafe { sie::set_stimer() };

    // 6. 轮转调度
    let mut current = 0usize;
    loop {
        // 找到下一个未完成的任务
        let mut found = false;
        for _ in 0..num_apps {
            let task = unsafe { TASKS[current].as_mut().unwrap() };
            if !task.finished {
                found = true;
                break;
            }
            current = (current + 1) % num_apps;
        }

        if !found {
            // 所有任务都完成了
            log::info!("All tasks finished, shutting down");
            sbi_rt::system_reset(Shutdown, NoReason);
            unreachable!()
        }

        let task = unsafe { TASKS[current].as_mut().unwrap() };

        // 7. 设置时间片定时器（非 coop 模式）
        #[cfg(not(feature = "coop"))]
        {
            sbi_rt::set_timer(time::read64() + 12500);
        }

        // 8. 执行任务
        let _sstatus = unsafe { task.context.execute() };

        // 9. 处理 trap
        let scause = scause::read();
        match scause.cause() {
            scause::Trap::Interrupt(scause::Interrupt::SupervisorTimer) => {
                // 时间片耗尽，禁用定时器，切换到下一个任务
                sbi_rt::set_timer(u64::MAX);
                log::trace!("Task {} timeout, switching", current);
                current = (current + 1) % num_apps;
            }
            scause::Trap::Exception(scause::Exception::UserEnvCall) => {
                // syscall
                let id = SyscallId::from(task.context.a(7));
                let args = [
                    task.context.a(0),
                    task.context.a(1),
                    task.context.a(2),
                    task.context.a(3),
                    task.context.a(4),
                    task.context.a(5),
                ];
                let caller = Caller { entity: 0, flow: 0 };
                let result = syscall::handle(caller, id, args);

                match result {
                    SyscallResult::Done(ret) => {
                        if id == SyscallId::EXIT {
                            // 任务退出
                            let exit_code = task.context.a(0) as i32;
                            log::info!("Task {} exited with code {}", current, exit_code);
                            task.finished = true;
                            current = (current + 1) % num_apps;
                        } else if id == SyscallId::SCHED_YIELD {
                            // 让出 CPU
                            *task.context.a_mut(0) = ret as usize;
                            task.context.move_next();
                            current = (current + 1) % num_apps;
                        } else {
                            // 其他 syscall，继续执行同一任务
                            *task.context.a_mut(0) = ret as usize;
                            task.context.move_next();
                        }
                    }
                    SyscallResult::Unsupported(id) => {
                        log::error!("Task {}: unsupported syscall {:?}", current, id);
                        task.finished = true;
                        current = (current + 1) % num_apps;
                    }
                }
            }
            _ => {
                // 其他异常，杀死任务
                log::error!(
                    "Task {} killed by {:?} at {:#x}",
                    current,
                    scause.cause(),
                    task.context.pc()
                );
                task.finished = true;
                current = (current + 1) % num_apps;
            }
        }
    }
}

// syscall 宿主实现
struct SyscallContext;

impl syscall::IO for SyscallContext {
    fn write(&self, _caller: Caller, fd: usize, buf: *const u8, count: usize) -> isize {
        match fd {
            STDOUT | STDDEBUG => {
                let buffer = unsafe { slice::from_raw_parts(buf, count) };
                let s = unsafe { core::str::from_utf8_unchecked(buffer) };
                print!("{}", s);
                count as isize
            }
            _ => {
                log::warn!("Unsupported fd: {}", fd);
                -1
            }
        }
    }

    fn read(&self, _caller: Caller, _fd: usize, _buf: *mut u8, _count: usize) -> isize {
        // ch3 不支持 read
        -1
    }

    fn open(&self, _caller: Caller, _path: *const u8, _flags: u32) -> isize {
        // ch3 不支持 open
        -1
    }

    fn close(&self, _caller: Caller, _fd: usize) -> isize {
        // ch3 不支持 close
        -1
    }
}

impl syscall::Process for SyscallContext {
    fn exit(&self, _caller: Caller, _exit_code: i32) -> isize {
        // exit 在 rust_main 中处理
        0
    }

    fn fork(&self, _caller: Caller) -> isize {
        -1
    }

    fn exec(&self, _caller: Caller, _path: *const u8) -> isize {
        -1
    }

    fn wait(&self, _caller: Caller, _exit_code_ptr: *mut i32) -> isize {
        -1
    }

    fn waitpid(&self, _caller: Caller, _pid: isize, _exit_code_ptr: *mut i32) -> isize {
        -1
    }

    fn getpid(&self, _caller: Caller) -> isize {
        0
    }
}

impl syscall::Scheduling for SyscallContext {
    fn sched_yield(&self, _caller: Caller) -> isize {
        // sched_yield 在 rust_main 中处理
        0
    }
}

impl syscall::Clock for SyscallContext {
    fn clock_gettime(&self, _caller: Caller, clock_id: usize, tp: *mut TimeSpec) -> isize {
        if clock_id == ClockId::CLOCK_MONOTONIC.0 {
            // 获取当前时间
            let time_val = time::read64();
            // 假设时钟频率为 10MHz（QEMU 默认）
            const CLOCK_FREQ: u64 = 10_000_000;
            let tv_sec = time_val / CLOCK_FREQ;
            let tv_nsec = (time_val % CLOCK_FREQ) * 1_000_000_000 / CLOCK_FREQ;
            let timespec = TimeSpec {
                tv_sec: tv_sec as usize,
                tv_nsec: tv_nsec as usize,
            };
            unsafe {
                *tp = timespec;
            }
            0
        } else {
            log::warn!("Unsupported clock_id: {}", clock_id);
            -1
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    sbi_rt::system_reset(Shutdown, SystemFailure);
    unreachable!()
}

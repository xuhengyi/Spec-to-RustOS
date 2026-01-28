//! ch2 - 批处理内核
//!
//! 一个最小的 RISC-V S-mode 批处理内核。嵌入一个或多个用户应用程序，
//! 顺序执行它们，处理 ecall 系统调用，并在所有应用完成后通过 SBI 关机。

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::hint::black_box;
use core::mem::MaybeUninit;
use core::panic::PanicInfo;

use kernel_context::LocalContext;
use linker::{AppMeta, KernelLayout};
use rcore_console::{init_console, log, print, println, set_log_level};
use riscv::register::scause::{self, Exception, Trap};
use sbi_rt::{NoReason, Shutdown, SystemFailure};
use syscall::{Caller, SyscallId, SyscallResult, STDOUT, STDDEBUG};

// 使用 linker crate 提供的 boot0! 宏定义启动入口
// 定义 4 页大小的启动栈
linker::boot0!(rust_main; stack = 4 * 4096);

// 嵌入用户应用程序镜像
global_asm!(include_str!(env!("APP_ASM")));

/// SBI 控制台实现
struct SbiConsole;

impl rcore_console::Console for SbiConsole {
    fn put_char(&self, c: u8) {
        #[allow(deprecated)]
        sbi_rt::legacy::console_putchar(c as usize);
    }
}

/// IO 系统调用处理实现
struct SbiIO;

impl syscall::IO for SbiIO {
    fn write(&self, _caller: Caller, fd: usize, buf: *const u8, count: usize) -> isize {
        // 处理 STDOUT 和 STDDEBUG
        if fd == STDOUT || fd == STDDEBUG {
            // 直接将用户缓冲区转换为字符串（假设为有效 UTF-8）
            // 注意：这是不安全的，但 ch2 章节有意简化
            let slice = unsafe { core::slice::from_raw_parts(buf, count) };
            let s = unsafe { core::str::from_utf8_unchecked(slice) };
            print!("{}", s);
            count as isize
        } else {
            // 不支持的文件描述符
            -1
        }
    }

    fn read(&self, _caller: Caller, _fd: usize, _buf: *mut u8, _count: usize) -> isize {
        // ch2 不支持 read
        -1
    }

    fn open(&self, _caller: Caller, _path: *const u8, _flags: u32) -> isize {
        // ch2 不支持 open
        -1
    }

    fn close(&self, _caller: Caller, _fd: usize) -> isize {
        // ch2 不支持 close
        -1
    }
}

/// Process 系统调用处理实现
struct BatchProcess;

impl syscall::Process for BatchProcess {
    fn exit(&self, _caller: Caller, _status: i32) -> isize {
        // 退出由主循环处理，这里只返回 0
        0
    }

    fn fork(&self, _caller: Caller) -> isize {
        // ch2 不支持 fork
        -1
    }

    fn exec(&self, _caller: Caller, _path: *const u8) -> isize {
        // ch2 不支持 exec
        -1
    }

    fn wait(&self, _caller: Caller, _exit_code: *mut i32) -> isize {
        // ch2 不支持 wait
        -1
    }

    fn waitpid(&self, _caller: Caller, _pid: isize, _exit_code: *mut i32) -> isize {
        // ch2 不支持 waitpid
        -1
    }

    fn getpid(&self, _caller: Caller) -> isize {
        // ch2 不支持 getpid
        -1
    }
}

/// 内核主函数，永不返回
#[no_mangle]
extern "C" fn rust_main() -> ! {
    // 首先清零 BSS 段
    unsafe {
        KernelLayout::locate().zero_bss();
    }

    // 初始化控制台
    init_console(&SbiConsole);
    // 设置日志级别
    set_log_level(option_env!("LOG"));

    // 打印启动信息
    println!();
    log::info!("Hello, rCore-Tutorial ch2!");

    // 初始化系统调用子系统
    syscall::init_io(&SbiIO);
    syscall::init_process(&BatchProcess);

    log::info!("Starting batch execution...");

    // 获取应用程序元数据并遍历执行
    let app_meta = AppMeta::locate();
    for (i, app) in app_meta.iter().enumerate() {
        log::info!("Running app {}: {} bytes", i, app.len());
        run_app(app.as_ptr() as usize);
    }

    log::info!("All applications finished. Shutting down...");

    // 所有应用程序执行完毕，请求关机
    sbi_rt::system_reset(Shutdown, NoReason);
    unreachable!()
}

/// 运行单个用户应用程序
fn run_app(entry: usize) {
    // 创建用户上下文
    let mut ctx = LocalContext::user(entry);

    // 为该应用程序分配用户栈（256 * 8 = 2KB）
    let mut user_stack: MaybeUninit<[usize; 256]> = MaybeUninit::uninit();
    // 使用 black_box 防止编译器优化掉栈
    let stack_ptr = black_box(user_stack.as_mut_ptr() as usize + core::mem::size_of::<[usize; 256]>());

    // 设置用户栈指针
    *ctx.sp_mut() = stack_ptr;

    // 应用程序执行循环
    loop {
        // 执行用户上下文，返回时获取 sstatus
        let _sstatus = unsafe { ctx.execute() };

        // 读取陷阱原因
        let scause = scause::read();

        match scause.cause() {
            Trap::Exception(Exception::UserEnvCall) => {
                // 用户态系统调用
                // 从寄存器读取系统调用号和参数
                let id = SyscallId::from(ctx.a(7));
                let args = [ctx.a(0), ctx.a(1), ctx.a(2), ctx.a(3), ctx.a(4), ctx.a(5)];

                // 检查是否是 exit 系统调用
                if id == SyscallId::EXIT {
                    let exit_code = ctx.a(0) as i32;
                    log::info!("Application exited with code {}", exit_code);
                    // 执行指令缓存同步
                    unsafe { asm!("fence.i") };
                    return;
                }

                // 分发系统调用
                let result = syscall::handle(Caller { entity: 0, flow: 0 }, id, args);

                match result {
                    SyscallResult::Done(ret) => {
                        // 将返回值写入 a0
                        *ctx.a_mut(0) = ret as usize;
                        // 推进 PC 到下一条指令
                        ctx.move_next();
                    }
                    SyscallResult::Unsupported(id) => {
                        log::error!("Unsupported syscall: {:?}", id);
                        // 不支持的系统调用，终止应用
                        unsafe { asm!("fence.i") };
                        return;
                    }
                }
            }
            other => {
                // 其他陷阱，终止应用
                log::error!(
                    "Unexpected trap {:?}, sepc = {:#x}, stval = {:#x}",
                    other,
                    riscv::register::sepc::read(),
                    riscv::register::stval::read()
                );
                // 执行指令缓存同步
                unsafe { asm!("fence.i") };
                return;
            }
        }
    }
}

/// Panic 处理器
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    sbi_rt::system_reset(Shutdown, SystemFailure);
    loop {}
}

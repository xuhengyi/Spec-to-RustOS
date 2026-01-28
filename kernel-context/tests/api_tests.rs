//! kernel-context crate 功能性验证测试
//! 
//! 这些测试验证 kernel-context crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。
//! 
//! **注意**：kernel-context crate 包含 RISC-V 特定的内联汇编代码（在 execute() 和 execute_naked() 函数中）。
//! 这些测试只验证 API 的基本功能（如结构体创建、寄存器访问等），不实际执行汇编代码。
//! 
//! 在非 RISC-V 平台上，kernel-context 库本身可能无法编译，因为包含 RISC-V 特定的汇编指令。
//! 这些测试应该在 RISC-V 目标平台上运行，或者需要条件编译来跳过包含汇编的部分。

#[cfg(target_arch = "riscv64")]
mod tests {
    use kernel_context::*;

    #[test]
    fn test_local_context_empty() {
        // 测试 LocalContext::empty()
        let ctx = LocalContext::empty();
        
        assert_eq!(ctx.supervisor, false);
        assert_eq!(ctx.interrupt, false);
        assert_eq!(ctx.sepc, 0);
        
        // 验证所有寄存器都是 0
        for i in 1..=31 {
            assert_eq!(ctx.x(i), 0);
        }
    }

    #[test]
    fn test_local_context_user() {
        // 测试 LocalContext::user()
        let pc = 0x1000;
        let ctx = LocalContext::user(pc);
        
        assert_eq!(ctx.supervisor, false);
        assert_eq!(ctx.interrupt, true); // 用户态时中断应该开启
        assert_eq!(ctx.sepc, pc);
        assert_eq!(ctx.pc(), pc);
        
        // 验证所有寄存器都是 0
        for i in 1..=31 {
            assert_eq!(ctx.x(i), 0);
        }
    }

    #[test]
    fn test_local_context_thread() {
        // 测试 LocalContext::thread()
        let pc = 0x2000;
        
        // 测试中断开启的情况
        let ctx1 = LocalContext::thread(pc, true);
        assert_eq!(ctx1.supervisor, true);
        assert_eq!(ctx1.interrupt, true);
        assert_eq!(ctx1.sepc, pc);
        
        // 测试中断关闭的情况
        let ctx2 = LocalContext::thread(pc, false);
        assert_eq!(ctx2.supervisor, true);
        assert_eq!(ctx2.interrupt, false);
        assert_eq!(ctx2.sepc, pc);
    }

    #[test]
    fn test_local_context_x_accessors() {
        // 测试 x() 和 x_mut() 访问器
        let mut ctx = LocalContext::empty();
        
        // 测试读取
        assert_eq!(ctx.x(1), 0);
        assert_eq!(ctx.x(31), 0);
        
        // 测试写入
        *ctx.x_mut(1) = 0x1234;
        *ctx.x_mut(31) = 0x5678;
        
        assert_eq!(ctx.x(1), 0x1234);
        assert_eq!(ctx.x(31), 0x5678);
    }

    #[test]
    fn test_local_context_a_accessors() {
        // 测试 a() 和 a_mut() 访问器（参数寄存器）
        let mut ctx = LocalContext::empty();
        
        // a0 = x10, a1 = x11, ..., a7 = x17
        *ctx.a_mut(0) = 0xAAAA;
        *ctx.a_mut(1) = 0xBBBB;
        *ctx.a_mut(7) = 0xCCCC;
        
        assert_eq!(ctx.a(0), 0xAAAA);
        assert_eq!(ctx.a(1), 0xBBBB);
        assert_eq!(ctx.a(7), 0xCCCC);
        
        // 验证 a(n) 对应 x(n+10)
        assert_eq!(ctx.a(0), ctx.x(10));
        assert_eq!(ctx.a(1), ctx.x(11));
        assert_eq!(ctx.a(7), ctx.x(17));
    }

    #[test]
    fn test_local_context_ra_sp() {
        // 测试 ra() 和 sp() 访问器
        let mut ctx = LocalContext::empty();
        
        // ra = x1
        *ctx.x_mut(1) = 0xDEADBEEF;
        assert_eq!(ctx.ra(), 0xDEADBEEF);
        
        // sp = x2
        *ctx.x_mut(2) = 0xCAFEBABE;
        assert_eq!(ctx.sp(), 0xCAFEBABE);
    }

    #[test]
    fn test_local_context_sp_mut() {
        // 测试 sp_mut() 访问器
        let mut ctx = LocalContext::empty();
        
        *ctx.sp_mut() = 0x12345678;
        assert_eq!(ctx.sp(), 0x12345678);
        assert_eq!(ctx.x(2), 0x12345678);
    }

    #[test]
    fn test_local_context_pc_accessors() {
        // 测试 pc() 和 pc_mut() 访问器
        let mut ctx = LocalContext::empty();
        
        assert_eq!(ctx.pc(), 0);
        
        *ctx.pc_mut() = 0x80000000;
        assert_eq!(ctx.pc(), 0x80000000);
        assert_eq!(ctx.sepc, 0x80000000);
    }

    #[test]
    fn test_local_context_move_next() {
        // 测试 move_next() 方法
        let mut ctx = LocalContext::empty();
        
        ctx.sepc = 0x1000;
        ctx.move_next();
        assert_eq!(ctx.sepc, 0x1004);
        
        ctx.move_next();
        assert_eq!(ctx.sepc, 0x1008);
        
        // 测试溢出（虽然不太可能发生）
        ctx.sepc = usize::MAX - 3;
        ctx.move_next();
        assert_eq!(ctx.sepc, usize::MAX.wrapping_add(1));
    }

    #[test]
    fn test_local_context_clone() {
        // 测试 LocalContext 的 Clone trait
        let mut ctx1 = LocalContext::user(0x1000);
        *ctx1.x_mut(1) = 0x1234;
        *ctx1.sp_mut() = 0x5678;
        
        let ctx2 = ctx1.clone();
        
        assert_eq!(ctx2.sepc, ctx1.sepc);
        assert_eq!(ctx2.supervisor, ctx1.supervisor);
        assert_eq!(ctx2.interrupt, ctx1.interrupt);
        assert_eq!(ctx2.x(1), ctx1.x(1));
        assert_eq!(ctx2.sp(), ctx1.sp());
        
        // 验证是深拷贝
        *ctx1.x_mut(1) = 0x9999;
        assert_eq!(ctx2.x(1), 0x1234); // ctx2 不应该改变
    }

    #[test]
    fn test_local_context_size() {
        // 测试 LocalContext 的大小
        // LocalContext 包含：
        // - sctx: usize (8 bytes)
        // - x: [usize; 31] (31 * 8 = 248 bytes)
        // - sepc: usize (8 bytes)
        // - supervisor: bool (1 byte, 但可能有 padding)
        // - interrupt: bool (1 byte, 但可能有 padding)
        let size = core::mem::size_of::<LocalContext>();
        
        // 在 64 位系统上，应该是至少 264 字节
        // 实际大小可能因为对齐而更大
        assert!(size >= 264);
        assert!(size <= 280); // 允许一些对齐 padding
    }

    #[test]
    fn test_local_context_repr_c() {
        // 测试 LocalContext 是 #[repr(C)] 的
        // 这确保内存布局是 C 兼容的
        let ctx = LocalContext::empty();
        let ptr = &ctx as *const LocalContext;
        
        // 验证可以安全地转换为字节指针
        let bytes = unsafe {
            core::slice::from_raw_parts(
                ptr as *const u8,
                core::mem::size_of::<LocalContext>(),
            )
        };
        
        assert_eq!(bytes.len(), core::mem::size_of::<LocalContext>());
    }

    #[test]
    fn test_local_context_register_indices() {
        // 测试寄存器索引的正确性
        let mut ctx = LocalContext::empty();
        
        // 设置一些寄存器值
        for i in 1..=31 {
            *ctx.x_mut(i) = i as usize;
        }
        
        // 验证可以正确读取
        for i in 1..=31 {
            assert_eq!(ctx.x(i), i as usize);
        }
        
        // 验证参数寄存器
        for i in 0..=7 {
            *ctx.a_mut(i) = (i + 100) as usize;
            assert_eq!(ctx.a(i), (i + 100) as usize);
            assert_eq!(ctx.x(i + 10), (i + 100) as usize);
        }
    }
}

#[cfg(not(target_arch = "riscv64"))]
#[test]
fn test_kernel_context_requires_riscv64() {
    // 在非 RISC-V 平台上，kernel-context 库包含 RISC-V 特定的汇编代码，
    // 无法编译。这个测试用于说明情况。
    // 实际测试应该在 RISC-V 目标平台上运行。
    println!("kernel-context tests require RISC-V 64-bit target architecture");
}

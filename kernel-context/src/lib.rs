#![no_std]

//! kernel-context: RISC-V Supervisor-mode thread context representation and execution

use core::arch::global_asm;

/// RISC-V local thread context representation.
/// 
/// This structure holds the architectural register state (x1..x31), 
/// the saved program counter (sepc), and privilege/interrupt flags.
#[repr(C)]
#[derive(Clone)]
pub struct LocalContext {
    /// Integer registers x1..x31 (x[0] = x1, x[30] = x31)
    /// Offsets: x[0] at 0, x[1] at 8, ..., x[30] at 240
    pub x: [usize; 31],
    /// Saved program counter (sepc) - offset 248
    pub sepc: usize,
    /// Whether returning to supervisor mode
    pub supervisor: bool,
    /// Whether interrupts are enabled
    pub interrupt: bool,
}

impl LocalContext {
    /// Create an empty context with all fields zeroed.
    pub fn empty() -> Self {
        Self {
            x: [0; 31],
            sepc: 0,
            supervisor: false,
            interrupt: false,
        }
    }

    /// Create a user-mode context starting at `pc`.
    /// Sets `supervisor == false` and `interrupt == true`.
    pub fn user(pc: usize) -> Self {
        Self {
            x: [0; 31],
            sepc: pc,
            supervisor: false,
            interrupt: true,
        }
    }

    /// Create a supervisor-mode thread context starting at `pc`.
    /// Sets `supervisor == true` and `interrupt` to the provided value.
    pub fn thread(pc: usize, interrupt: bool) -> Self {
        Self {
            x: [0; 31],
            sepc: pc,
            supervisor: true,
            interrupt,
        }
    }

    /// Access the n-th integer register slot (1-based indexing).
    pub fn x(&self, n: usize) -> usize {
        assert!(n >= 1 && n <= 31, "register index must be in range [1, 31]");
        self.x[n - 1]
    }

    /// Mutably access the n-th integer register slot (1-based indexing).
    pub fn x_mut(&mut self, n: usize) -> &mut usize {
        assert!(n >= 1 && n <= 31, "register index must be in range [1, 31]");
        &mut self.x[n - 1]
    }

    /// Access the n-th argument register.
    /// Maps `a0..` onto integer slots `x10..` (i.e., `a(0)` is `x10`).
    pub fn a(&self, n: usize) -> usize {
        assert!(n <= 7, "argument register index must be in range [0, 7]");
        self.x(10 + n)
    }

    /// Mutably access the n-th argument register.
    pub fn a_mut(&mut self, n: usize) -> &mut usize {
        assert!(n <= 7, "argument register index must be in range [0, 7]");
        self.x_mut(10 + n)
    }

    /// Return the value of `x1` (return address register).
    pub fn ra(&self) -> usize {
        self.x(1)
    }

    /// Read the value of `x2` (stack pointer).
    pub fn sp(&self) -> usize {
        self.x(2)
    }

    /// Mutably access `x2` (stack pointer).
    pub fn sp_mut(&mut self) -> &mut usize {
        self.x_mut(2)
    }

    /// Return the saved program counter.
    pub fn pc(&self) -> usize {
        self.sepc
    }

    /// Mutably access the saved program counter.
    pub fn pc_mut(&mut self) -> &mut usize {
        &mut self.sepc
    }

    /// Advance the saved PC by 4 bytes using wrapping arithmetic.
    pub fn move_next(&mut self) {
        self.sepc = self.sepc.wrapping_add(4);
    }

    /// Execute the context, switching into it using RISC-V `sret`-based control transfer.
    #[cfg(target_arch = "riscv64")]
    pub unsafe fn execute(&mut self) -> usize {
        // Compute sstatus value based on supervisor and interrupt flags
        // SPP bit (bit 8): 0 = return to U-mode, 1 = return to S-mode
        // SPIE bit (bit 5): previous interrupt enable (restored to SIE on sret)
        let mut sstatus: usize;
        core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
        
        if self.supervisor {
            sstatus |= 1 << 8; // Set SPP (return to S-mode)
        } else {
            sstatus &= !(1 << 8); // Clear SPP (return to U-mode)
        }
        if self.interrupt {
            sstatus |= 1 << 5; // Set SPIE (enable interrupts after sret)
        } else {
            sstatus &= !(1 << 5); // Clear SPIE (disable interrupts after sret)
        }

        // Call the assembly routine
        extern "C" {
            fn __execute_context(ctx: *mut LocalContext, sstatus: usize) -> usize;
        }
        __execute_context(self, sstatus)
    }

    #[cfg(not(target_arch = "riscv64"))]
    pub unsafe fn execute(&mut self) -> usize {
        panic!("execute() is only available on RISC-V 64-bit targets");
    }
}

// Assembly code for context switching
// 
// LocalContext layout:
// - x[0] = x1 = ra: offset 0
// - x[1] = x2 = sp: offset 8
// - x[2] = x3 = gp: offset 16
// - x[3] = x4 = tp: offset 24
// - x[4] = x5 = t0: offset 32
// - x[5] = x6 = t1: offset 40
// - x[6] = x7 = t2: offset 48
// - x[7] = x8 = s0: offset 56
// - x[8] = x9 = s1: offset 64
// - x[9] = x10 = a0: offset 72
// - x[10] = x11 = a1: offset 80
// - x[11] = x12 = a2: offset 88
// - x[12] = x13 = a3: offset 96
// - x[13] = x14 = a4: offset 104
// - x[14] = x15 = a5: offset 112
// - x[15] = x16 = a6: offset 120
// - x[16] = x17 = a7: offset 128
// - x[17] = x18 = s2: offset 136
// - x[18] = x19 = s3: offset 144
// - x[19] = x20 = s4: offset 152
// - x[20] = x21 = s5: offset 160
// - x[21] = x22 = s6: offset 168
// - x[22] = x23 = s7: offset 176
// - x[23] = x24 = s8: offset 184
// - x[24] = x25 = s9: offset 192
// - x[25] = x26 = s10: offset 200
// - x[26] = x27 = s11: offset 208
// - x[27] = x28 = t3: offset 216
// - x[28] = x29 = t4: offset 224
// - x[29] = x30 = t5: offset 232
// - x[30] = x31 = t6: offset 240
// - sepc: offset 248
#[cfg(target_arch = "riscv64")]
global_asm!(r#"
.section .text
.globl __execute_context
.globl __trap_handler
.align 4

# __execute_context(ctx: *mut LocalContext, sstatus: usize) -> usize
# a0 = ctx pointer, a1 = sstatus to set
# Returns sstatus in a0 after trap
__execute_context:
    # Save kernel's callee-saved registers on stack
    addi sp, sp, -112
    sd ra, 0(sp)
    sd s0, 8(sp)
    sd s1, 16(sp)
    sd s2, 24(sp)
    sd s3, 32(sp)
    sd s4, 40(sp)
    sd s5, 48(sp)
    sd s6, 56(sp)
    sd s7, 64(sp)
    sd s8, 72(sp)
    sd s9, 80(sp)
    sd s10, 88(sp)
    sd s11, 96(sp)
    
    # Save kernel sp to sscratch (for trap handler to restore)
    csrw sscratch, sp
    
    # Save ctx pointer in s0 (will be restored after trap)
    mv s0, a0
    
    # Set up trap handler
    la t0, __trap_handler
    csrw stvec, t0
    
    # Set sstatus and sepc
    csrw sstatus, a1
    ld t0, 248(a0)      # sepc
    csrw sepc, t0
    
    # Now we need to restore user registers from context
    # But we're using a0 as ctx pointer, so save ctx to sscratch temporarily
    # Actually sscratch has kernel sp, we need another approach
    
    # Let's swap: put ctx in sscratch, kernel sp in s0 (but s0 will be overwritten)
    # Better approach: use kernel stack to pass ctx address
    # Store ctx on kernel stack, then retrieve in trap handler
    
    # Actually, let's just use a different register flow:
    # 1. Store ctx address at a fixed location (or use sscratch cleverly)
    # 2. After sret, trap handler reads ctx from that location
    
    # Simplest approach for ch2: store ctx address at [kernel_sp - 8]
    sd a0, -8(sp)
    
    # Now load all user registers from context (a0 = ctx)
    ld x1, 0(a0)        # ra
    ld x3, 16(a0)       # gp
    ld x4, 24(a0)       # tp
    ld x5, 32(a0)       # t0
    ld x6, 40(a0)       # t1
    ld x7, 48(a0)       # t2
    ld x8, 56(a0)       # s0
    ld x9, 64(a0)       # s1
    # a0 loaded last
    ld x11, 80(a0)      # a1
    ld x12, 88(a0)      # a2
    ld x13, 96(a0)      # a3
    ld x14, 104(a0)     # a4
    ld x15, 112(a0)     # a5
    ld x16, 120(a0)     # a6
    ld x17, 128(a0)     # a7
    ld x18, 136(a0)     # s2
    ld x19, 144(a0)     # s3
    ld x20, 152(a0)     # s4
    ld x21, 160(a0)     # s5
    ld x22, 168(a0)     # s6
    ld x23, 176(a0)     # s7
    ld x24, 184(a0)     # s8
    ld x25, 192(a0)     # s9
    ld x26, 200(a0)     # s10
    ld x27, 208(a0)     # s11
    ld x28, 216(a0)     # t3
    ld x29, 224(a0)     # t4
    ld x30, 232(a0)     # t5
    ld x31, 240(a0)     # t6
    
    # Load sp and a0 last
    ld x2, 8(a0)        # sp
    ld x10, 72(a0)      # a0
    
    # Return to user/supervisor mode
    sret

.align 4
__trap_handler:
    # User/supervisor code trapped back to kernel
    # sscratch contains kernel sp
    # First, swap sp with sscratch to get kernel sp
    csrrw sp, sscratch, sp
    # Now sp = kernel sp, sscratch = user sp
    
    # Save user sp temporarily
    sd t0, -16(sp)      # Save t0 first so we can use it
    csrr t0, sscratch   # Get user sp
    sd t0, -24(sp)      # Save user sp
    
    # Load ctx pointer (stored at kernel_sp - 8 before sret)
    ld t0, -8(sp)       # t0 = ctx
    
    # Retrieve saved user sp
    ld t1, -24(sp)
    sd t1, 8(t0)        # Save user sp to ctx.x[1]
    
    # Retrieve saved t0 (user's t0)
    ld t1, -16(sp)
    sd t1, 32(t0)       # Save user t0 to ctx.x[4]
    
    # Now save other user registers to context
    sd x1, 0(t0)        # ra
    # sp already saved above
    sd x3, 16(t0)       # gp
    sd x4, 24(t0)       # tp
    # t0 already saved above
    sd x6, 40(t0)       # t1
    sd x7, 48(t0)       # t2
    sd x8, 56(t0)       # s0
    sd x9, 64(t0)       # s1
    sd x10, 72(t0)      # a0
    sd x11, 80(t0)      # a1
    sd x12, 88(t0)      # a2
    sd x13, 96(t0)      # a3
    sd x14, 104(t0)     # a4
    sd x15, 112(t0)     # a5
    sd x16, 120(t0)     # a6
    sd x17, 128(t0)     # a7
    sd x18, 136(t0)     # s2
    sd x19, 144(t0)     # s3
    sd x20, 152(t0)     # s4
    sd x21, 160(t0)     # s5
    sd x22, 168(t0)     # s6
    sd x23, 176(t0)     # s7
    sd x24, 184(t0)     # s8
    sd x25, 192(t0)     # s9
    sd x26, 200(t0)     # s10
    sd x27, 208(t0)     # s11
    sd x28, 216(t0)     # t3
    sd x29, 224(t0)     # t4
    sd x30, 232(t0)     # t5
    sd x31, 240(t0)     # t6
    
    # Save sepc
    csrr t1, sepc
    sd t1, 248(t0)
    
    # Restore kernel's callee-saved registers
    ld ra, 0(sp)
    ld s0, 8(sp)
    ld s1, 16(sp)
    ld s2, 24(sp)
    ld s3, 32(sp)
    ld s4, 40(sp)
    ld s5, 48(sp)
    ld s6, 56(sp)
    ld s7, 64(sp)
    ld s8, 72(sp)
    ld s9, 80(sp)
    ld s10, 88(sp)
    ld s11, 96(sp)
    addi sp, sp, 112
    
    # Return sstatus in a0
    csrr a0, sstatus
    
    ret
"#);

#[cfg(feature = "foreign")]
pub mod foreign {
    //! Foreign address space execution facility
    
    use super::LocalContext;
    
    /// Portal cache record expected to be mapped in a shared ("public") address space.
    #[repr(C)]
    pub struct PortalCache {
        pub satp: usize,
        pub sepc: usize,
        pub a0: usize,
        pub sstatus: usize,
    }

    impl PortalCache {
        pub fn init(&mut self, satp: usize, pc: usize, a0: usize, supervisor: bool, interrupt: bool) {
            self.satp = satp;
            self.sepc = pc;
            self.a0 = a0;
            let mut sstatus = 0usize;
            if supervisor { sstatus |= 1 << 8; }
            if interrupt { sstatus |= 1 << 5; }
            self.sstatus = sstatus;
        }

        pub fn address(&self) -> usize {
            self as *const Self as usize
        }
    }

    pub struct ForeignContext {
        pub context: LocalContext,
        pub satp: usize,
    }

    impl ForeignContext {
        #[cfg(target_arch = "riscv64")]
        pub unsafe fn execute<P: ForeignPortal, K: SlotKey>(&mut self, portal: &mut P, key: K) -> usize {
            let orig_supervisor = self.context.supervisor;
            let orig_interrupt = self.context.interrupt;
            self.context.supervisor = true;
            self.context.interrupt = false;
            let entry = portal.transit_entry();
            let cache = portal.transit_cache(key);
            cache.init(self.satp, self.context.pc(), self.context.a(0), orig_supervisor, orig_interrupt);
            self.context.supervisor = orig_supervisor;
            self.context.interrupt = orig_interrupt;
            *self.context.a_mut(0) = cache.a0;
            cache.sstatus
        }

        #[cfg(not(target_arch = "riscv64"))]
        pub unsafe fn execute<P: ForeignPortal, K: SlotKey>(&mut self, _portal: &mut P, _key: K) -> usize {
            panic!("execute() is only available on RISC-V 64-bit targets");
        }
    }

    pub trait SlotKey {
        fn index(self) -> usize;
    }

    impl SlotKey for () {
        fn index(self) -> usize { 0 }
    }

    impl SlotKey for usize {
        fn index(self) -> usize { self }
    }

    pub struct TpReg;

    impl SlotKey for TpReg {
        #[cfg(target_arch = "riscv64")]
        fn index(self) -> usize {
            let tp: usize;
            unsafe { core::arch::asm!("mv {}, tp", out(reg) tp); }
            tp
        }

        #[cfg(not(target_arch = "riscv64"))]
        fn index(self) -> usize { 0 }
    }

    pub trait ForeignPortal {
        unsafe fn transit_entry(&self) -> usize;
        unsafe fn transit_cache<K: SlotKey>(&mut self, key: K) -> &mut PortalCache;
    }

    pub trait MonoForeignPortal {
        unsafe fn transit_address(&self) -> usize;
        fn text_offset(&self) -> usize;
        fn cache_offset(&self, index: usize) -> usize;
    }

    impl<T: MonoForeignPortal> ForeignPortal for T {
        unsafe fn transit_entry(&self) -> usize {
            self.transit_address() + self.text_offset()
        }

        unsafe fn transit_cache<K: SlotKey>(&mut self, key: K) -> &mut PortalCache {
            let addr = self.transit_address() + self.cache_offset(key.index());
            &mut *(addr as *mut PortalCache)
        }
    }

    #[repr(C)]
    pub struct MultislotPortal {
        slots: usize,
    }

    impl MultislotPortal {
        pub fn calculate_size(slots: usize) -> usize {
            const PORTAL_CODE_SIZE: usize = 256;
            core::mem::size_of::<usize>() + PORTAL_CODE_SIZE + slots * core::mem::size_of::<PortalCache>()
        }

        pub unsafe fn init_transit(transit: *mut u8, slots: usize) -> &'static mut Self {
            let portal = &mut *(transit as *mut Self);
            portal.slots = slots;
            portal
        }
    }

    impl MonoForeignPortal for MultislotPortal {
        unsafe fn transit_address(&self) -> usize {
            self as *const Self as usize
        }

        fn text_offset(&self) -> usize {
            core::mem::size_of::<usize>()
        }

        fn cache_offset(&self, index: usize) -> usize {
            const PORTAL_CODE_SIZE: usize = 256;
            core::mem::size_of::<usize>() + PORTAL_CODE_SIZE + index * core::mem::size_of::<PortalCache>()
        }
    }
}

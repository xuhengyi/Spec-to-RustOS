#![no_std]
#![no_main]

use core::arch::naked_asm;
use core::panic::PanicInfo;
use sbi_rt::{legacy::console_putchar, system_reset, NoReason, Shutdown, SystemFailure};

/// Stack size: 4096 bytes
const STACK_SIZE: usize = 4096;

/// Stack region in .bss.uninit
#[link_section = ".bss.uninit"]
static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

/// Supervisor entry symbol.
/// Sets up stack and jumps to rust_main.
#[unsafe(naked)]
#[no_mangle]
#[link_section = ".text.entry"]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        // Set stack pointer to the top of STACK
        "la sp, {stack}",
        "li t0, {stack_size}",
        "add sp, sp, t0",
        // Jump to rust_main
        "call {rust_main}",
        stack = sym STACK,
        stack_size = const STACK_SIZE,
        rust_main = sym rust_main,
    )
}

/// Main entry point after stack is set up.
/// Prints "Hello, world!" and requests shutdown.
#[no_mangle]
extern "C" fn rust_main() -> ! {
    // Print "Hello, world!" via SBI legacy console output
    for byte in b"Hello, world!" {
        console_putchar(*byte as usize);
    }
    
    // Request system shutdown with NoReason
    system_reset(Shutdown, NoReason);
    
    // This should never be reached, but satisfy the type system
    unreachable!()
}

/// Panic handler: request shutdown with SystemFailure
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    system_reset(Shutdown, SystemFailure);
    
    // This should never be reached
    unreachable!()
}

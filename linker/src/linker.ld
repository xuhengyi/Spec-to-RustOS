OUTPUT_ARCH(riscv)
ENTRY(_start)

SECTIONS
{
    . = 0x80200000;

    .text : {
        *(.text.entry)
        *(.text .text.*)
    }

    . = ALIGN(4K);
    __rodata = .;
    .rodata : {
        *(.rodata .rodata.*)
    }

    . = ALIGN(4K);
    __data = .;
    .data : {
        *(.data .data.*)
    }

    .bss : {
        __sbss = .;
        *(.bss .bss.*)
        __ebss = .;
    }

    . = ALIGN(4K);
    __boot = .;
    .boot : {
        KEEP(*(.boot.stack))
        *(.boot .boot.*)
    }

    . = ALIGN(4K);
    __end = .;
}

__start = 0x80200000;

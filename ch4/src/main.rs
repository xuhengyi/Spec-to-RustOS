#![no_std]
#![no_main]

extern crate alloc;

use core::arch::global_asm;
use core::panic::PanicInfo;
use core::ptr::NonNull;

use kernel_context::foreign::{ForeignPortal, MultislotPortal};
use kernel_context::LocalContext;
use kernel_vm::page_table::{Pte, Sv39, VAddr, VmFlags, PPN, VPN};
use kernel_vm::{AddressSpace, PageManager};
use linker::{AppMeta, KernelLayout, KernelRegionTitle};
use rcore_console::{init_console, log, print, println, set_log_level, test_log, Console};
use riscv::register::{scause, satp, stval};
use sbi_rt::{legacy, NoReason, Shutdown, SystemFailure};
use syscall::{
    Caller, ClockId, SyscallId, SyscallResult, TimeSpec, STDDEBUG, STDOUT,
};
use xmas_elf::ElfFile;

linker::boot0!(rust_main; stack = 4 * 4096);

global_asm!(include_str!(env!("APP_ASM")));

// Portal trampoline: a0 = cache address
// PortalCache layout (offsets, 8-byte aligned):
//   0:  a0
//   8:  a1
//  16:  satp
//  24:  sstatus
//  32:  sepc
//  40:  stvec (saved)
//  48:  sscratch (saved)
global_asm!(r#"
.section .text.portal,"ax"
.globl __ch4_portal_code
.globl __ch4_portal_trap
.globl __ch4_portal_code_end
.align 4
__ch4_portal_code:
    # save a1 into cache
    sd   a1, 8(a0)

    # switch satp
    ld   a1, 16(a0)
    csrrw a1, satp, a1
    sd   a1, 16(a0)
    sfence.vma zero, zero

    # load sstatus/sepc for user
    ld   a1, 24(a0)
    csrw sstatus, a1
    ld   a1, 32(a0)
    csrw sepc, a1

    # save old stvec, then set stvec to portal trap entry
    csrr a1, stvec
    sd   a1, 40(a0)
    la   a1, __ch4_portal_trap
    csrw stvec, a1

    # save old sscratch, then set sscratch to cache address
    csrr a1, sscratch
    sd   a1, 48(a0)
    csrw sscratch, a0

    # restore a0/a1 for user
    ld   a1, 8(a0)
    ld   a0, 0(a0)
    sret

.align 4
__ch4_portal_trap:
    # sscratch holds cache address
    csrr t0, sscratch
    sd   a0, 0(t0)
    sd   a1, 8(t0)

    # restore sscratch (kernel sp)
    ld   a1, 48(t0)
    csrw sscratch, a1

    # restore satp (kernel)
    ld   a1, 16(t0)
    csrrw a1, satp, a1
    sd   a1, 16(t0)
    sfence.vma zero, zero

    # restore stvec
    ld   a1, 40(t0)
    csrw stvec, a1

    # restore a0/a1 for trap handler
    ld   a0, 0(t0)
    ld   a1, 8(t0)

    # jump to original trap handler
    ld   t0, 40(t0)
    jr   t0

__ch4_portal_code_end:
"#);

const PHYS_MEM_START: usize = 0x8000_0000;
const MEMORY: usize = 64 * 1024 * 1024;
const USER_STACK_PAGES: usize = 2;
const PAGE_SIZE: usize = 4096;
const PORTAL_CODE_SIZE: usize = 256;
// Portal VA = 0x1_0000 << 12 = 0x1000_0000 (256MB)
const PORTAL_VPN: usize = 0x1_0000;
// User stack: VPN 0xFFFE..0x10000, stack_top = 0x10000000 - 16 = 0xFFFFF0
const TOP_OF_USER_STACK_VPN: usize = 0x1_0000;

static mut CURRENT_SPACE: Option<*const AddressSpace<Sv39, Sv39Manager>> = None;

struct SbiConsole;

impl Console for SbiConsole {
    fn put_char(&self, c: u8) {
        #[allow(deprecated)]
        legacy::console_putchar(c as usize);
    }
}

#[repr(C)]
struct Sv39Manager {
    root_ptr: NonNull<Pte<Sv39>>,
    root_ppn: PPN<Sv39>,
    heap_ppn_start: usize,
    heap_ppn_end: usize,
}

impl Sv39Manager {
    fn new(root_ptr: NonNull<Pte<Sv39>>, root_ppn: PPN<Sv39>, heap_ppn_start: usize, heap_ppn_end: usize) -> Self {
        Self {
            root_ptr,
            root_ppn,
            heap_ppn_start,
            heap_ppn_end,
        }
    }

    fn in_heap(&self, ppn: PPN<Sv39>) -> bool {
        let v = ppn.val();
        v >= self.heap_ppn_start && v < self.heap_ppn_end
    }
}

impl PageManager<Sv39> for Sv39Manager {
    fn new_root() -> Self {
        let layout = core::alloc::Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        let ptr = NonNull::new(ptr).unwrap_or_else(|| alloc::alloc::handle_alloc_error(layout));
        unsafe { core::ptr::write_bytes(ptr.as_ptr(), 0, PAGE_SIZE) };
        let root_ptr = ptr.cast();
        let root_ppn = PPN::new(ptr.as_ptr() as usize >> 12);
        let layout = KernelLayout::locate();
        let heap_start = layout.end() >> 12;
        let heap_end = (PHYS_MEM_START + MEMORY) >> 12;
        Self::new(root_ptr, root_ppn, heap_start, heap_end)
    }

    fn root_ptr(&self) -> NonNull<Pte<Sv39>> {
        self.root_ptr
    }

    fn root_ppn(&self) -> PPN<Sv39> {
        self.root_ppn
    }

    fn p_to_v<T>(&self, ppn: PPN<Sv39>) -> NonNull<T> {
        let vaddr = ppn.val() << 12;
        NonNull::new(vaddr as *mut T).unwrap()
    }

    fn v_to_p<T>(&self, ptr: NonNull<T>) -> PPN<Sv39> {
        PPN::new(ptr.as_ptr() as usize >> 12)
    }

    fn allocate(&mut self, len: usize, _flags: &mut VmFlags<Sv39>) -> NonNull<u8> {
        let layout = core::alloc::Layout::from_size_align(len * PAGE_SIZE, PAGE_SIZE).unwrap();
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        let ptr = NonNull::new(ptr).unwrap_or_else(|| alloc::alloc::handle_alloc_error(layout));
        // Zero the allocated memory
        unsafe { core::ptr::write_bytes(ptr.as_ptr(), 0, len * PAGE_SIZE) };
        ptr
    }

    fn deallocate(&mut self, _pte: Pte<Sv39>, _len: usize) -> usize {
        todo!("ch4 does not free pages")
    }

    fn check_owned(&self, pte: Pte<Sv39>) -> bool {
        let ppn = pte.ppn();
        ppn.val() == self.root_ppn.val() || self.in_heap(ppn)
    }

    fn drop_root(&mut self) {
        todo!("ch4 does not drop root")
    }
}

fn kernel_space(
    layout: &KernelLayout,
    heap_ppn_start: PPN<Sv39>,
    heap_ppn_count: usize,
    portal_ppn: PPN<Sv39>,
    portal_vpn: usize,
) -> AddressSpace<Sv39, Sv39Manager> {
    
    let mut space = AddressSpace::<Sv39, Sv39Manager>::new();

    for region in layout.iter() {
        let start = region.range.start >> 12;
        let end = (region.range.end + PAGE_SIZE - 1) >> 12;
        if start >= end {
            continue;
        }
        let flags_str = match region.title {
            KernelRegionTitle::Text => "VRX",
            KernelRegionTitle::Rodata => "VR",
            KernelRegionTitle::Data => "VRW",
            KernelRegionTitle::Boot => "VRW",
        };
        let flags = VmFlags::build_from_str(flags_str);
        let range = VPN::new(start)..VPN::new(end);
        let pbase = PPN::new(start);
        space.map_extern(range, pbase, flags);
    }

    let heap_ppn_end = heap_ppn_start.val() + heap_ppn_count;
    if heap_ppn_end > heap_ppn_start.val() {
        let range = VPN::new(heap_ppn_start.val())..VPN::new(heap_ppn_end);
        space.map_extern(range, heap_ppn_start, VmFlags::build_from_str("VRW"));
    }

    // Portal must be at VA 0x10000000 (portal_vpn) and executable in S-mode.
    // RISC-V forbids S-mode instruction fetch from U pages, so do NOT set U here.
    let portal_page_range = VPN::new(portal_vpn)..VPN::new(portal_vpn + 1);
    space.map_extern(portal_page_range, portal_ppn, VmFlags::build_from_str("VRWX"));

    let satp_val = (8 << 60) | space.root_ppn().val();
    satp::write(satp_val);
    unsafe { core::arch::asm!("sfence.vma zero, zero"); }

    space
}

struct Process {
    context: LocalContext,
    space: AddressSpace<Sv39, Sv39Manager>,
    stack_top: usize,
}

fn load_elf(
    app: &[u8],
    kernel_space: &AddressSpace<Sv39, Sv39Manager>,
) -> Option<Process> {
    let elf = ElfFile::new(app).ok()?;
    const ET_EXEC: u16 = 2;
    const EM_RISCV: u16 = 243;
    if elf.header.pt2.type_().0 != ET_EXEC {
        return None;
    }
    let machine_val: u16 = unsafe { core::mem::transmute(elf.header.pt2.machine()) };
    if machine_val != EM_RISCV {
        return None;
    }

    let mut space = AddressSpace::<Sv39, Sv39Manager>::new();
    let entry = elf.header.pt2.entry_point() as usize;

    // First pass: collect all pages and their required flags (union of overlapping segments)
    let mut page_info: alloc::collections::BTreeMap<usize, (usize, bool, bool, bool)> = alloc::collections::BTreeMap::new(); // vpn -> (flags_bits, has_r, has_w, has_x)
    
    for ph in elf.program_iter() {
        if ph.get_type().ok()? != xmas_elf::program::Type::Load {
            continue;
        }
        let vaddr = ph.virtual_addr() as usize;
        let memsz = ph.mem_size() as usize;
        let flags = ph.flags();
        
        let vpn_start = vaddr >> 12;
        let vpn_end = (vaddr + memsz + PAGE_SIZE - 1) >> 12;
        
        for vpn_val in vpn_start..vpn_end {
            let entry = page_info.entry(vpn_val).or_insert((0, false, false, false));
            entry.1 |= flags.is_read();
            entry.2 |= flags.is_write();
            entry.3 |= flags.is_execute();
        }
    }
    
    // Track which VPNs have been mapped
    let mut mapped_vpns: alloc::collections::BTreeSet<usize> = alloc::collections::BTreeSet::new();

    for ph in elf.program_iter() {
        if ph.get_type().ok()? != xmas_elf::program::Type::Load {
            continue;
        }
        let vaddr = ph.virtual_addr() as usize;
        let offset = ph.offset() as usize;
        let filesz = ph.file_size() as usize;
        let memsz = ph.mem_size() as usize;

        let vpn_start = vaddr >> 12;
        let vpn_end = (vaddr + memsz + PAGE_SIZE - 1) >> 12;
        if vpn_end <= vpn_start {
            continue;
        }

        // Map pages individually to handle overlapping segments
        for vpn_val in vpn_start..vpn_end {
            if mapped_vpns.contains(&vpn_val) {
                // Page already mapped by previous segment, just copy data if needed
                let page_vaddr = vpn_val << 12;
                let page_offset_in_page = if vaddr > page_vaddr { vaddr - page_vaddr } else { 0 };
                let data_start_in_segment = if page_vaddr > vaddr { page_vaddr - vaddr } else { 0 };
                
                if data_start_in_segment < filesz {
                    // There's data to copy to this page
                    let copy_len = (filesz - data_start_in_segment).min(PAGE_SIZE - page_offset_in_page);
                    if copy_len > 0 {
                        // Translate VPN to physical address and copy data
                        let vaddr_obj = VAddr::<Sv39>::new(page_vaddr + page_offset_in_page);
                        if let Some(ptr) = space.translate::<u8>(vaddr_obj, VmFlags::build_from_str("W")) {
                            let src = &app[offset + data_start_in_segment..offset + data_start_in_segment + copy_len];
                            unsafe {
                                core::ptr::copy_nonoverlapping(src.as_ptr(), ptr.as_ptr(), copy_len);
                            }
                        }
                    }
                }
                continue;
            }
            
            // First time mapping this page - use combined flags from all overlapping segments
            mapped_vpns.insert(vpn_val);
            
            let (_, has_r, has_w, has_x) = page_info.get(&vpn_val).copied().unwrap_or((0, true, false, false));
            let flags_str = match (has_r, has_w, has_x) {
                (_, true, true) => "VRWXU",
                (_, false, true) => "VRXU",
                (_, true, false) => "VRWU",
                _ => "VRU",
            };
            let page_flags = VmFlags::build_from_str(flags_str);
            
            let page_vaddr = vpn_val << 12;
            let page_offset_in_page = if vaddr > page_vaddr { vaddr - page_vaddr } else { 0 };
            let data_start_in_segment = if page_vaddr > vaddr { page_vaddr - vaddr } else { 0 };
            
            let data = if data_start_in_segment < filesz {
                let copy_len = (filesz - data_start_in_segment).min(PAGE_SIZE - page_offset_in_page);
                &app[offset + data_start_in_segment..offset + data_start_in_segment + copy_len]
            } else {
                &[]
            };
            
            let range = VPN::new(vpn_val)..VPN::new(vpn_val + 1);
            space.map(range, data, page_offset_in_page, page_flags);
        }
    }

    let stack_vpn = TOP_OF_USER_STACK_VPN - USER_STACK_PAGES;
    let stack_range = VPN::new(stack_vpn)..VPN::new(stack_vpn + USER_STACK_PAGES);
    let stack_flags = VmFlags::build_from_str("VRWU");
    space.map(stack_range, &[], 0, stack_flags);
    // RISC-V ABI requires 16-byte stack alignment
    let stack_top = (TOP_OF_USER_STACK_VPN << 12) - 16;

    space.copy_leaf_pte_from(kernel_space, VPN::new(PORTAL_VPN));

    let mut context = LocalContext::user(entry);
    *context.sp_mut() = stack_top;

    Some(Process { context, space, stack_top })
}

#[no_mangle]
extern "C" fn rust_main() -> ! {
    unsafe { KernelLayout::locate().zero_bss() };
    init_console(&SbiConsole);
    set_log_level(option_env!("LOG"));
    test_log();

    let layout = KernelLayout::locate();
    let heap_start = layout.end();
    let heap_end = PHYS_MEM_START + MEMORY;
    let heap_size_full = heap_end.saturating_sub(layout.end());
    assert!(heap_size_full > 0, "no heap space: layout.end() >= layout.start() + MEMORY");
    let heap_size = heap_size_full.min(4 << 20);
    kernel_alloc::init(heap_start);
    let heap_region: &'static mut [u8] =
        unsafe { core::slice::from_raw_parts_mut(heap_start as *mut u8, heap_size) };
    unsafe { kernel_alloc::transfer(heap_region) };

    let heap_ppn_start = heap_start >> 12;
    let heap_ppn_count = heap_size >> 12;
    let portal_size = MultislotPortal::calculate_size(1);
    assert!(portal_size <= PAGE_SIZE, "portal size must fit in one page");
    let portal_layout = core::alloc::Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
    let portal_ptr = unsafe { alloc::alloc::alloc(portal_layout) };
    let portal_ptr = NonNull::new(portal_ptr).unwrap_or_else(|| alloc::alloc::handle_alloc_error(portal_layout));
    unsafe { core::ptr::write_bytes(portal_ptr.as_ptr(), 0, PAGE_SIZE) };
    let portal_base = portal_ptr.as_ptr() as *mut u8;
    let portal_ppn = PPN::new(portal_ptr.as_ptr() as usize >> 12);

    let kernel_space = kernel_space(
        &layout,
        PPN::new(heap_ppn_start),
        heap_ppn_count,
        portal_ppn,
        PORTAL_VPN,
    );

    let portal = unsafe { MultislotPortal::init_transit(portal_base, 1) };
    {
        extern "C" {
            fn __ch4_portal_code();
            fn __ch4_portal_code_end();
        }
        let src = __ch4_portal_code as *const u8;
        let len = (__ch4_portal_code_end as usize).saturating_sub(__ch4_portal_code as usize);
        assert!(len <= PORTAL_CODE_SIZE, "portal code too large");
        let dst = unsafe { portal_base.add(core::mem::size_of::<usize>()) };
        unsafe { core::ptr::copy_nonoverlapping(src, dst, len) };
        unsafe { core::arch::asm!("fence.i") };
    }

    let mut processes: alloc::vec::Vec<Process> = alloc::vec::Vec::new();
    for app in AppMeta::locate().iter() {
        if let Some(proc) = load_elf(app, &kernel_space) {
            processes.push(proc);
        }
    }

    log::info!("Loaded {} processes", processes.len());

    if processes.is_empty() {
        log::info!("No applications to run, shutting down");
        sbi_rt::system_reset(Shutdown, NoReason);
        unreachable!()
    }

    syscall::init_io(&SyscallHost);
    syscall::init_process(&SyscallHost);
    syscall::init_scheduling(&SyscallHost);
    syscall::init_clock(&SyscallHost);

    let kernel_satp = (8 << 60) | kernel_space.root_ppn().val();
    satp::write(kernel_satp);
    unsafe { core::arch::asm!("sfence.vma zero, zero"); }

    // Set stvec early so any exceptions are handled
    extern "C" {
        fn __trap_handler();
    }
    unsafe {
        core::arch::asm!("csrw stvec, {}", in(reg) __trap_handler as usize);
    }

    let current = 0usize;
    let caller = Caller { entity: 0, flow: 0 };

    loop {
        if current >= processes.len() {
            log::info!("All processes finished, shutting down");
            sbi_rt::system_reset(Shutdown, NoReason);
            unreachable!()
        }

        let proc = &mut processes[current];
        unsafe { CURRENT_SPACE = Some(&proc.space as *const _) };
        let user_satp = (8 << 60) | proc.space.root_ppn().val();
        
        let portal_va = PORTAL_VPN << 12;
        let portal_entry = portal_va + core::mem::size_of::<usize>();
        let cache_addr = portal_va + core::mem::size_of::<usize>() + PORTAL_CODE_SIZE;
        let cache = unsafe { portal.transit_cache(()) };
        let orig_supervisor = proc.context.supervisor;
        let orig_interrupt = proc.context.interrupt;
        cache.init(
            user_satp,
            proc.context.pc(),
            proc.context.a(0),
            proc.context.a(1),
            orig_supervisor,
            orig_interrupt,
        );

        *proc.context.pc_mut() = portal_entry;
        *proc.context.a_mut(0) = cache_addr;
        proc.context.supervisor = true;
        proc.context.interrupt = false;

        unsafe { proc.context.execute() };
        proc.context.supervisor = orig_supervisor;
        proc.context.interrupt = orig_interrupt;
        
        // After trap, switch back to kernel address space
        satp::write(kernel_satp);
        unsafe { core::arch::asm!("sfence.vma zero, zero"); }

        let trap_cause = scause::read();
        match trap_cause.cause() {
            scause::Trap::Exception(scause::Exception::UserEnvCall) => {
                let id = SyscallId::from(proc.context.a(7));
                let args = [
                    proc.context.a(0),
                    proc.context.a(1),
                    proc.context.a(2),
                    proc.context.a(3),
                    proc.context.a(4),
                    proc.context.a(5),
                ];
                let result = syscall::handle(caller, id, args);

                match result {
                    SyscallResult::Done(ret) => {
                        if id == SyscallId::EXIT {
                            processes.remove(current);
                        } else {
                            *proc.context.a_mut(0) = ret as usize;
                            proc.context.move_next();
                        }
                    }
                    SyscallResult::Unsupported(_) => {
                        log::error!("Unsupported syscall {:?}", id);
                        processes.remove(current);
                    }
                }
            }
            scause::Trap::Exception(scause::Exception::Breakpoint) => {
                // Skip the ebreak instruction and continue
                proc.context.move_next();
            }
            _ => {
                log::error!(
                    "Trap {:?} stval={:#x} pc={:#x}",
                    trap_cause.cause(),
                    stval::read(),
                    proc.context.pc()
                );
                processes.remove(current);
            }
        }
        unsafe { CURRENT_SPACE = None };
    }
}

struct SyscallHost;

impl syscall::IO for SyscallHost {
    fn write(&self, _caller: Caller, fd: usize, buf: *const u8, count: usize) -> isize {
        match fd {
            STDOUT | STDDEBUG => {
                let space = unsafe { CURRENT_SPACE.and_then(|p| p.as_ref()) };
                if let Some(space) = space {
                    let vaddr = VAddr::<Sv39>::new(buf as usize);
                    let flags = VmFlags::build_from_str("R");
                    if let Some(ptr) = space.translate(vaddr, flags) {
                        let s = unsafe {
                            core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr.as_ptr(), count))
                        };
                        print!("{}", s);
                        return count as isize;
                    }
                    log::warn!("translate failed for vaddr {:#x}", vaddr.val());
                }
                log::warn!("Invalid user buffer for write");
                return -1;
            }
            _ => {
                log::warn!("Unsupported fd: {}", fd);
                -1
            }
        }
    }

    fn read(&self, _caller: Caller, _fd: usize, _buf: *mut u8, _count: usize) -> isize {
        -1
    }

    fn open(&self, _caller: Caller, _path: *const u8, _flags: u32) -> isize {
        -1
    }

    fn close(&self, _caller: Caller, _fd: usize) -> isize {
        -1
    }
}

impl syscall::Process for SyscallHost {
    fn exit(&self, _caller: Caller, _exit_code: i32) -> isize {
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

impl syscall::Scheduling for SyscallHost {
    fn sched_yield(&self, _caller: Caller) -> isize {
        0
    }
}

impl syscall::Clock for SyscallHost {
    fn clock_gettime(&self, _caller: Caller, clock_id: usize, tp: *mut TimeSpec) -> isize {
        if clock_id == ClockId::CLOCK_MONOTONIC.0 {
            let time_val = riscv::register::time::read64();
            const CLOCK_FREQ: u64 = 10_000_000;
            let tv_sec = (time_val / CLOCK_FREQ) as usize;
            let tv_nsec = ((time_val % CLOCK_FREQ) * 1_000_000_000 / CLOCK_FREQ) as usize;
            let spec = TimeSpec { tv_sec, tv_nsec };
            let space = unsafe { CURRENT_SPACE.and_then(|p| p.as_ref()) };
            if let Some(space) = space {
                let vaddr = VAddr::<Sv39>::new(tp as usize);
                let flags = VmFlags::build_from_str("W");
                if let Some(ptr) = space.translate::<u8>(vaddr, flags) {
                    // Use unaligned write to handle potentially unaligned user buffers
                    let spec_bytes = unsafe {
                        core::slice::from_raw_parts(&spec as *const TimeSpec as *const u8, core::mem::size_of::<TimeSpec>())
                    };
                    unsafe {
                        core::ptr::copy_nonoverlapping(spec_bytes.as_ptr(), ptr.as_ptr(), spec_bytes.len());
                    }
                    return 0;
                }
            }
            log::warn!("Invalid user buffer for clock_gettime");
            return -1;
        }
        -1
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    sbi_rt::system_reset(Shutdown, SystemFailure);
    unreachable!()
}

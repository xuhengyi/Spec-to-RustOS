#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::{BTreeMap, VecDeque};
use core::arch::global_asm;
use core::panic::PanicInfo;
use core::ptr::NonNull;

use kernel_context::foreign::{ForeignPortal, MultislotPortal};
use kernel_context::foreign::{ForeignContext, SlotKey};
use kernel_vm::page_table::{Pte, Sv39, VAddr, VmFlags, PPN, VPN};
use kernel_vm::{AddressSpace, PageManager};
use linker::{AppMeta, KernelLayout, KernelRegionTitle};
use rcore_console::{init_console, log, print, println, set_log_level, test_log, Console};
use rcore_task_manage::{Manage, PManager, ProcId, Schedule};
use riscv::register::{scause, satp, stval};
use sbi_rt::{legacy, NoReason, Shutdown, SystemFailure};
use syscall::{
    Caller, ClockId, SyscallId, SyscallResult, TimeSpec, STDDEBUG, STDIN, STDOUT,
};
use xmas_elf::ElfFile;

linker::boot0!(rust_main; stack = 4 * 4096);

global_asm!(include_str!(env!("APP_ASM")));

// Portal trampoline: use ch4 portal (same layout)
global_asm!(r#"
.section .text.portal,"ax"
.globl __ch5_portal_code
.globl __ch5_portal_trap
.globl __ch5_portal_code_end
.align 4
__ch5_portal_code:
    sd   a1, 8(a0)
    ld   a1, 16(a0)
    csrrw a1, satp, a1
    sd   a1, 16(a0)
    sfence.vma zero, zero
    ld   a1, 24(a0)
    csrw sstatus, a1
    ld   a1, 32(a0)
    csrw sepc, a1
    csrr a1, stvec
    sd   a1, 40(a0)
    la   a1, __ch5_portal_trap
    csrw stvec, a1
    csrr a1, sscratch
    sd   a1, 48(a0)
    csrw sscratch, a0
    ld   a1, 8(a0)
    ld   a0, 0(a0)
    sret

.align 4
__ch5_portal_trap:
    csrr t0, sscratch
    sd   a0, 0(t0)
    sd   a1, 8(t0)
    ld   a1, 48(t0)
    csrw sscratch, a1
    ld   a1, 16(t0)
    csrrw a1, satp, a1
    sd   a1, 16(t0)
    sfence.vma zero, zero
    ld   a1, 40(t0)
    csrw stvec, a1
    ld   a0, 0(t0)
    ld   a1, 8(t0)
    ld   t0, 40(t0)
    jr   t0

__ch5_portal_code_end:
"#);

const PHYS_MEM_START: usize = 0x8000_0000;
const MEMORY: usize = 64 * 1024 * 1024;
const USER_STACK_PAGES: usize = 2;
const PAGE_SIZE: usize = 4096;
const PORTAL_CODE_SIZE: usize = 256;
const PORTAL_VPN: usize = 0x1_0000;
const TOP_OF_USER_STACK_VPN: usize = 0x1_0000;

static mut CURRENT_SPACE: Option<*const AddressSpace<Sv39, Sv39Manager>> = None;
static mut CURRENT_PID: Option<ProcId> = None;

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
        unsafe { core::ptr::write_bytes(ptr.as_ptr(), 0, len * PAGE_SIZE) };
        ptr
    }

    fn deallocate(&mut self, pte: Pte<Sv39>, len: usize) -> usize {
        let ppn = pte.ppn();
        if ppn.val() == self.root_ppn.val() {
            return 0; // 根页表由 drop_root 释放
        }
        let ptr = (ppn.val() << 12) as *mut u8;
        let layout =
            core::alloc::Layout::from_size_align(len * PAGE_SIZE, PAGE_SIZE).unwrap();
        unsafe { alloc::alloc::dealloc(ptr, layout) };
        len
    }

    fn check_owned(&self, pte: Pte<Sv39>) -> bool {
        let ppn = pte.ppn();
        ppn.val() == self.root_ppn.val() || self.in_heap(ppn)
    }

    fn drop_root(&mut self) {
        let ptr = self.root_ptr.as_ptr() as *mut u8;
        let layout = core::alloc::Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
        unsafe { alloc::alloc::dealloc(ptr, layout) };
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

    let portal_page_range = VPN::new(portal_vpn)..VPN::new(portal_vpn + 1);
    space.map_extern(portal_page_range, portal_ppn, VmFlags::build_from_str("VRWX"));

    let satp_val = (8 << 60) | space.root_ppn().val();
    satp::write(satp_val);
    unsafe { core::arch::asm!("sfence.vma zero, zero"); }

    space
}

pub struct Process {
    pub pid: ProcId,
    pub context: ForeignContext,
    pub space: AddressSpace<Sv39, Sv39Manager>,
    pub stack_top: usize,
}

impl Process {
    fn satp(&self) -> usize {
        (8 << 60) | self.space.root_ppn().val()
    }
}

struct ProcManager {
    store: BTreeMap<ProcId, Process>,
    ready: VecDeque<ProcId>,
}

impl ProcManager {
    fn new() -> Self {
        Self {
            store: BTreeMap::new(),
            ready: VecDeque::new(),
        }
    }
}

impl Manage<Process, ProcId> for ProcManager {
    fn insert(&mut self, id: ProcId, item: Process) {
        self.store.insert(id, item);
    }

    fn delete(&mut self, id: ProcId) {
        self.store.remove(&id);
    }

    fn get_mut(&mut self, id: ProcId) -> Option<&mut Process> {
        self.store.get_mut(&id)
    }
}

impl Schedule<ProcId> for ProcManager {
    fn add(&mut self, id: ProcId) {
        self.ready.push_back(id);
    }

    fn fetch(&mut self) -> Option<ProcId> {
        self.ready.pop_front()
    }
}

fn app_name_at(app_names: *const u8, index: usize) -> Option<&'static str> {
    let mut ptr = app_names;
    for i in 0..=index {
        let start = ptr;
        while unsafe { *ptr } != 0 {
            ptr = unsafe { ptr.add(1) };
        }
        if i == index {
            let len = (ptr as usize) - (start as usize);
            if len == 0 {
                return None;
            }
            return Some(unsafe {
                core::str::from_utf8_unchecked(core::slice::from_raw_parts(start, len))
            });
        }
        ptr = unsafe { ptr.add(1) };
    }
    None
}

fn get_app_by_name(name: &str) -> Option<&'static [u8]> {
    extern "C" {
        static app_names: u8;
    }
    let meta = AppMeta::locate();
    let count = meta.count as usize;
    for i in 0..count {
        let app_name = app_name_at(unsafe { &app_names }, i)?;
        if app_name == name {
            let mut apps = meta.iter();
            return apps.nth(i);
        }
    }
    None
}

fn list_app_names() {
    extern "C" {
        static app_names: u8;
    }
    let meta = AppMeta::locate();
    let count = meta.count as usize;
    for i in 0..count {
        if let Some(n) = app_name_at(unsafe { &app_names }, i) {
            print!(" {}", n);
        }
    }
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

    let mut page_info: alloc::collections::BTreeMap<usize, (usize, bool, bool, bool)> = alloc::collections::BTreeMap::new();

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
            let e = page_info.entry(vpn_val).or_insert((0, false, false, false));
            e.1 |= flags.is_read();
            e.2 |= flags.is_write();
            e.3 |= flags.is_execute();
        }
    }

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

        for vpn_val in vpn_start..vpn_end {
            if mapped_vpns.contains(&vpn_val) {
                let page_vaddr = vpn_val << 12;
                let page_offset_in_page = if vaddr > page_vaddr { vaddr - page_vaddr } else { 0 };
                let data_start_in_segment = if page_vaddr > vaddr { page_vaddr - vaddr } else { 0 };

                if data_start_in_segment < filesz {
                    let copy_len = (filesz - data_start_in_segment).min(PAGE_SIZE - page_offset_in_page);
                    if copy_len > 0 {
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
    let stack_top = (TOP_OF_USER_STACK_VPN << 12) - 16;

    space.copy_leaf_pte_from(kernel_space, VPN::new(PORTAL_VPN));

    let mut ctx = kernel_context::LocalContext::user(entry);
    *ctx.sp_mut() = stack_top;

    Some(Process {
        pid: ProcId::from_usize(usize::MAX),
        context: ForeignContext {
            context: ctx,
            satp: (8 << 60) | space.root_ppn().val(),
        },
        space,
        stack_top,
    })
}

static mut PROCESSOR: Option<PManager<Process, ProcManager>> = None;

struct SyscallContext;

impl syscall::IO for SyscallContext {
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
                }
                -1
            }
            _ => -1,
        }
    }

    fn read(&self, _caller: Caller, fd: usize, buf: *mut u8, count: usize) -> isize {
        if fd == STDIN && count > 0 {
            let space = unsafe { CURRENT_SPACE.and_then(|p| p.as_ref()) };
            if let Some(space) = space {
                let vaddr = VAddr::<Sv39>::new(buf as usize);
                let flags = VmFlags::build_from_str("W");
                if let Some(ptr) = space.translate::<u8>(vaddr, flags) {
                    let mut n = 0usize;
                    while n < count {
                        #[allow(deprecated)]
                        let c = legacy::console_getchar();
                        if c == usize::MAX {
                            break;
                        }
                        unsafe { *ptr.as_ptr().add(n) = c as u8 };
                        n += 1;
                    }
                    return n as isize;
                }
            }
            return -1;
        }
        -1
    }

    fn open(&self, _caller: Caller, _path: *const u8, _flags: u32) -> isize {
        -1
    }

    fn close(&self, _caller: Caller, _fd: usize) -> isize {
        -1
    }
}

impl syscall::Process for SyscallContext {
    fn exit(&self, caller: Caller, exit_code: i32) -> isize {
        let processor = unsafe { PROCESSOR.as_mut().unwrap() };
        processor.make_current_exited(exit_code as isize);
        0
    }

    fn fork(&self, caller: Caller) -> isize {
        let processor = unsafe { PROCESSOR.as_mut().unwrap() };
        let parent = match processor.current() {
            Some(p) => p,
            None => return -1,
        };
        let kernel_space = unsafe { KERNEL_SPACE.as_ref().unwrap() };
        let mut child_space = AddressSpace::new();
        parent.space.cloneself(&mut child_space);
        child_space.copy_leaf_pte_from(kernel_space, VPN::new(PORTAL_VPN));

        let mut child_ctx = kernel_context::LocalContext::empty();
        child_ctx.sepc = parent.context.context.sepc;
        child_ctx.x = parent.context.context.x;
        child_ctx.supervisor = parent.context.context.supervisor;
        child_ctx.interrupt = parent.context.context.interrupt;
        *child_ctx.sp_mut() = parent.context.context.sp();

        let child_pid = ProcId::new();
        *child_ctx.a_mut(0) = 0;
        let child = Process {
            pid: child_pid,
            context: ForeignContext {
                context: child_ctx,
                satp: (8 << 60) | child_space.root_ppn().val(),
            },
            space: child_space,
            stack_top: parent.stack_top,
        };

        let parent_pid = unsafe { CURRENT_PID.unwrap() };
        processor.add(child_pid, child, parent_pid);

        let child_pid_usize = child_pid.get_usize() as isize;
        child_pid_usize
    }

    fn exec(&self, caller: Caller, path: *const u8) -> isize {
        let space = unsafe { CURRENT_SPACE.and_then(|p| p.as_ref()) };
        let space = match space {
            Some(s) => s,
            None => return -1,
        };
        let vaddr = VAddr::<Sv39>::new(path as usize);
        let flags = VmFlags::build_from_str("R");
        let name_ptr = match space.translate::<u8>(vaddr, flags) {
            Some(p) => p,
            None => return -1,
        };
        let name = unsafe {
            let mut len = 0usize;
            while *name_ptr.as_ptr().add(len) != 0 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(name_ptr.as_ptr(), len))
        };

        let app = match get_app_by_name(name) {
            Some(a) => a,
            None => {
                log::error!("Application not found: {}", name);
                print!("Available applications:");
                list_app_names();
                println!();
                return -1;
            }
        };

        let kernel_space = unsafe { KERNEL_SPACE.as_ref().unwrap() };
        let new_proc = match load_elf(app, kernel_space) {
            Some(p) => p,
            None => return -1,
        };
        let processor = unsafe { PROCESSOR.as_mut().unwrap() };
        let current_pid = unsafe { CURRENT_PID.unwrap() };
        if let Some(proc) = processor.get_task(current_pid) {
            let mut old_space = core::mem::replace(&mut proc.space, new_proc.space);
            old_space.free_allocated_pages_and_root(Some(VPN::new(PORTAL_VPN)));
            proc.context = new_proc.context;
            proc.stack_top = new_proc.stack_top;
        }
        0
    }

    fn wait(&self, caller: Caller, exit_code_ptr: *mut i32) -> isize {
        -1
    }

    fn waitpid(&self, caller: Caller, pid: isize, exit_code_ptr: *mut i32) -> isize {
        let (pid, exit_code_ptr) = if exit_code_ptr.is_null() {
            (-1, pid as *mut i32)
        } else {
            (pid, exit_code_ptr)
        };
        let processor = unsafe { PROCESSOR.as_mut().unwrap() };
        let child_pid = if pid == -1 {
            ProcId::from_usize(usize::MAX)
        } else {
            ProcId::from_usize(pid as usize)
        };
        match processor.wait(child_pid) {
            Some((pid, -1)) if pid.get_usize() == usize::MAX - 1 => -2,
            Some((reaped_pid, code)) => {
                let space = unsafe { CURRENT_SPACE.and_then(|p| p.as_ref()) };
                if let (Some(space), Some(ptr)) = (space, NonNull::new(exit_code_ptr)) {
                    let vaddr = VAddr::<Sv39>::new(exit_code_ptr as usize);
                    let flags = VmFlags::build_from_str("W");
                    if let Some(dst) = space.translate::<i32>(vaddr, flags) {
                        unsafe { *dst.as_ptr() = code as i32 };
                    }
                }
                reaped_pid.get_usize() as isize
            }
            None => -1,
        }
    }

    fn getpid(&self, caller: Caller) -> isize {
        unsafe { CURRENT_PID.map(|p| p.get_usize() as isize).unwrap_or(-1) }
    }
}

impl syscall::Scheduling for SyscallContext {
    fn sched_yield(&self, caller: Caller) -> isize {
        let processor = unsafe { PROCESSOR.as_mut().unwrap() };
        processor.make_current_suspend();
        0
    }
}

impl syscall::Clock for SyscallContext {
    fn clock_gettime(&self, caller: Caller, clock_id: usize, tp: *mut TimeSpec) -> isize {
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
                    let spec_bytes = unsafe {
                        core::slice::from_raw_parts(&spec as *const TimeSpec as *const u8, core::mem::size_of::<TimeSpec>())
                    };
                    unsafe {
                        core::ptr::copy_nonoverlapping(spec_bytes.as_ptr(), ptr.as_ptr(), spec_bytes.len());
                    }
                    return 0;
                }
            }
            return -1;
        }
        -1
    }
}

static mut KERNEL_SPACE: Option<AddressSpace<Sv39, Sv39Manager>> = None;

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
    assert!(heap_size_full > 0, "no heap space");
    let heap_size = heap_size_full; // 使用全部可用内存作为内核堆
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

    let mut portal = unsafe { MultislotPortal::init_transit(portal_base, 1) };
    {
        extern "C" {
            fn __ch5_portal_code();
            fn __ch5_portal_code_end();
        }
        let src = __ch5_portal_code as *const u8;
        let len = (__ch5_portal_code_end as usize).saturating_sub(__ch5_portal_code as usize);
        assert!(len <= PORTAL_CODE_SIZE, "portal code too large");
        let dst = unsafe { portal_base.add(core::mem::size_of::<usize>()) };
        unsafe { core::ptr::copy_nonoverlapping(src, dst, len) };
        unsafe { core::arch::asm!("fence.i") };
    }

    unsafe { KERNEL_SPACE = Some(kernel_space) };

    let mut proc_manager = ProcManager::new();
    let mut processor = PManager::new();
    processor.set_manager(proc_manager);

    let init_app_name = option_env!("INIT_APP").unwrap_or("initproc");
    let initproc_app = match get_app_by_name(init_app_name) {
        Some(a) => a,
        None => {
            log::error!("{} not found", init_app_name);
            sbi_rt::system_reset(Shutdown, NoReason);
            unreachable!()
        }
    };

    let mut initproc = match load_elf(initproc_app, unsafe { KERNEL_SPACE.as_ref().unwrap() }) {
        Some(p) => p,
        None => {
            log::error!("Failed to load initproc");
            sbi_rt::system_reset(Shutdown, NoReason);
            unreachable!()
        }
    };

    let init_pid = ProcId::from_usize(0);
    initproc.pid = init_pid;
    processor.add(init_pid, initproc, init_pid);

    unsafe { PROCESSOR = Some(processor) };

    log::info!("Loaded initproc");

    syscall::init_io(&SyscallContext);
    syscall::init_process(&SyscallContext);
    syscall::init_scheduling(&SyscallContext);
    syscall::init_clock(&SyscallContext);

    let kernel_satp = (8 << 60) | unsafe { KERNEL_SPACE.as_ref().unwrap() }.root_ppn().val();
    satp::write(kernel_satp);
    unsafe { core::arch::asm!("sfence.vma zero, zero"); }

    extern "C" {
        fn __trap_handler();
    }
    unsafe {
        core::arch::asm!("csrw stvec, {}", in(reg) __trap_handler as usize);
    }

    let caller = Caller { entity: 0, flow: 0 };

    loop {
        let processor = unsafe { PROCESSOR.as_mut().unwrap() };
        let proc = match processor.find_next() {
            Some(p) => p,
            None => {
                log::info!("No runnable processes, shutting down");
                sbi_rt::system_reset(Shutdown, NoReason);
                unreachable!()
            }
        };

        let pid = proc.pid;
        unsafe { CURRENT_SPACE = Some(&proc.space as *const _) };
        unsafe { CURRENT_PID = Some(pid) };

        let portal_va = PORTAL_VPN << 12;
        let portal_entry = portal_va + core::mem::size_of::<usize>();
        let cache_addr = portal_va + core::mem::size_of::<usize>() + PORTAL_CODE_SIZE;
        let cache = unsafe { portal.transit_cache(()) };
        let orig_supervisor = proc.context.context.supervisor;
        let orig_interrupt = proc.context.context.interrupt;
        cache.init(
            proc.satp(),
            proc.context.context.pc(),
            proc.context.context.a(0),
            proc.context.context.a(1),
            orig_supervisor,
            orig_interrupt,
        );
        *proc.context.context.pc_mut() = portal_entry;
        *proc.context.context.a_mut(0) = cache_addr;
        proc.context.context.supervisor = true;
        proc.context.context.interrupt = false;
        unsafe { proc.context.context.execute() };
        proc.context.context.supervisor = orig_supervisor;
        proc.context.context.interrupt = orig_interrupt;

        satp::write(kernel_satp);
        unsafe { core::arch::asm!("sfence.vma zero, zero"); }

        let trap_cause = scause::read();
        match trap_cause.cause() {
            scause::Trap::Exception(scause::Exception::UserEnvCall) => {
                let id = SyscallId::from(proc.context.context.a(7));
                let args = [
                    proc.context.context.a(0),
                    proc.context.context.a(1),
                    proc.context.context.a(2),
                    proc.context.context.a(3),
                    proc.context.context.a(4),
                    proc.context.context.a(5),
                ];
                let result = syscall::handle(caller, id, args);

                match result {
                    SyscallResult::Done(ret) => {
                        if id == SyscallId::EXIT {
                            let processor = unsafe { PROCESSOR.as_mut().unwrap() };
                            processor.make_current_exited(ret);
                        } else {
                            *proc.context.context.a_mut(0) = ret as usize;
                            proc.context.context.move_next();
                            let processor = unsafe { PROCESSOR.as_mut().unwrap() };
                            processor.make_current_suspend();
                        }
                    }
                    SyscallResult::Unsupported(_) => {
                        log::error!("Unsupported syscall {:?}", id);
                        let processor = unsafe { PROCESSOR.as_mut().unwrap() };
                        processor.make_current_exited(-2);
                    }
                }
            }
            scause::Trap::Exception(scause::Exception::Breakpoint) => {
                proc.context.context.move_next();
            }
            _ => {
                log::error!(
                    "Trap {:?} stval={:#x} pc={:#x}",
                    trap_cause.cause(),
                    stval::read(),
                    proc.context.context.pc()
                );
                let processor = unsafe { PROCESSOR.as_mut().unwrap() };
                processor.make_current_exited(-3);
            }
        }
        unsafe { CURRENT_SPACE = None };
        unsafe { CURRENT_PID = None };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    sbi_rt::system_reset(Shutdown, SystemFailure);
    unreachable!()
}

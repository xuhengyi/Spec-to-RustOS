#![no_std]
#![no_main]

extern crate alloc;

use alloc::alloc::{alloc, alloc_zeroed, dealloc, handle_alloc_error};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::global_asm;
use core::panic::PanicInfo;
use core::ptr::NonNull;

use easy_fs::{BlockDevice, EasyFileSystem, FSManager, FileHandle, Inode, OpenFlags};
use kernel_context::foreign::{ForeignContext, MultislotPortal};
use kernel_vm::page_table::{Pte, Sv39, VAddr, VmFlags, PPN, VPN};
use kernel_vm::{AddressSpace, PageManager};
use linker::{KernelLayout, KernelRegionTitle};
use rcore_console::{init_console, log, print, println, set_log_level, test_log, Console};
use rcore_task_manage::{Manage, PManager, ProcId, Schedule};
use riscv::register::{scause, satp, stval};
use sbi_rt::{legacy, NoReason, Shutdown, SystemFailure};
use spin::{Lazy, Mutex};
use syscall::{
    Caller, ClockId, SyscallId, SyscallResult, TimeSpec, STDDEBUG, STDIN, STDOUT,
};
use virtio_drivers::{Hal, VirtIOBlk, VirtIOHeader};
use xmas_elf::header::{Machine, Type as ElfType};
use xmas_elf::program::Type as ProgramType;
use xmas_elf::ElfFile;

linker::boot0!(rust_main; stack = 4 * 4096);

global_asm!(r#"
.section .text.portal,"ax"
.globl __ch6_portal_code
.globl __ch6_portal_trap
.globl __ch6_portal_code_end
.align 4
__ch6_portal_code:
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
    la   a1, __ch6_portal_trap
    csrw stvec, a1
    csrr a1, sscratch
    sd   a1, 48(a0)
    csrw sscratch, a0
    ld   a1, 8(a0)
    ld   a0, 0(a0)
    sret

.align 4
__ch6_portal_trap:
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

__ch6_portal_code_end:
"#);

const PHYS_MEM_START: usize = 0x8000_0000;
const MEMORY: usize = 64 * 1024 * 1024;
const PAGE_SIZE: usize = 4096;
const USER_STACK_PAGES: usize = 2;
const PORTAL_CODE_SIZE: usize = 256;
const PORTAL_VPN: usize = (1 << 27) - 1;
const TOP_OF_USER_STACK_VPN: usize = PORTAL_VPN;
const VIRTIO0: usize = 0x1000_1000;
const USER_CSTR_MAX: usize = 4096;

pub const MMIO: &[(usize, usize)] = &[(VIRTIO0, 0x1000)];

static mut KERNEL_SPACE: Option<AddressSpace<Sv39, Sv39Manager>> = None;
static mut PROCESSOR: Option<PManager<Process, ProcManager>> = None;
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
    fn new(
        root_ptr: NonNull<Pte<Sv39>>,
        root_ppn: PPN<Sv39>,
        heap_ppn_start: usize,
        heap_ppn_end: usize,
    ) -> Self {
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
        let ptr = unsafe { alloc(layout) };
        let ptr = NonNull::new(ptr).unwrap_or_else(|| handle_alloc_error(layout));
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
        let ptr = unsafe { alloc(layout) };
        let ptr = NonNull::new(ptr).unwrap_or_else(|| handle_alloc_error(layout));
        unsafe { core::ptr::write_bytes(ptr.as_ptr(), 0, len * PAGE_SIZE) };
        ptr
    }

    fn deallocate(&mut self, pte: Pte<Sv39>, len: usize) -> usize {
        let ppn = pte.ppn();
        if ppn.val() == self.root_ppn.val() {
            return 0;
        }
        let ptr = (ppn.val() << 12) as *mut u8;
        let layout = core::alloc::Layout::from_size_align(len * PAGE_SIZE, PAGE_SIZE).unwrap();
        unsafe { dealloc(ptr, layout) };
        len
    }

    fn check_owned(&self, pte: Pte<Sv39>) -> bool {
        let ppn = pte.ppn();
        ppn.val() == self.root_ppn.val() || self.in_heap(ppn)
    }

    fn drop_root(&mut self) {
        let ptr = self.root_ptr.as_ptr() as *mut u8;
        let layout = core::alloc::Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
        unsafe { dealloc(ptr, layout) };
    }
}

fn kernel_space(
    layout: &KernelLayout,
    heap_ppn_start: PPN<Sv39>,
    heap_ppn_count: usize,
    portal_ppn: PPN<Sv39>,
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
        let range = VPN::new(start)..VPN::new(end);
        space.map_extern(range, PPN::new(start), VmFlags::build_from_str(flags_str));
    }

    let heap_ppn_end = heap_ppn_start.val() + heap_ppn_count;
    if heap_ppn_end > heap_ppn_start.val() {
        let range = VPN::new(heap_ppn_start.val())..VPN::new(heap_ppn_end);
        space.map_extern(range, heap_ppn_start, VmFlags::build_from_str("VRW"));
    }

    let portal_range = VPN::new(PORTAL_VPN)..VPN::new(PORTAL_VPN + 1);
    space.map_extern(portal_range, portal_ppn, VmFlags::build_from_str("VRWX"));

    for (base, len) in MMIO.iter().copied() {
        let start = base >> 12;
        let end = (base + len + PAGE_SIZE - 1) >> 12;
        if end > start {
            let range = VPN::new(start)..VPN::new(end);
            space.map_extern(range, PPN::new(start), VmFlags::build_from_str("VRW"));
        }
    }

    satp::write((8 << 60) | space.root_ppn().val());
    unsafe {
        core::arch::asm!("sfence.vma zero, zero");
    }

    space
}

pub mod virtio_block {
    use super::*;

    struct VirtioHal;

    impl Hal for VirtioHal {
        fn dma_alloc(pages: usize) -> usize {
            let layout = core::alloc::Layout::from_size_align(pages * PAGE_SIZE, PAGE_SIZE).unwrap();
            let ptr = unsafe { alloc_zeroed(layout) };
            if ptr.is_null() {
                return 0;
            }
            let paddr = Self::virt_to_phys(ptr as usize);
            if paddr == 0 {
                unsafe { dealloc(ptr, layout) };
            }
            paddr
        }

        fn dma_dealloc(paddr: usize, pages: usize) -> i32 {
            let vaddr = Self::phys_to_virt(paddr);
            if vaddr == 0 {
                return -1;
            }
            let layout = core::alloc::Layout::from_size_align(pages * PAGE_SIZE, PAGE_SIZE).unwrap();
            unsafe { dealloc(vaddr as *mut u8, layout) };
            0
        }

        fn phys_to_virt(paddr: usize) -> usize {
            paddr
        }

        fn virt_to_phys(vaddr: usize) -> usize {
            let space = unsafe { KERNEL_SPACE.as_ref() };
            let Some(space) = space else {
                return 0;
            };
            let addr = VAddr::<Sv39>::new(vaddr);
            if space
                .translate::<u8>(addr, VmFlags::build_from_str("R"))
                .is_some()
            {
                addr.val()
            } else {
                0
            }
        }
    }

    struct VirtIOBlock(Mutex<VirtIOBlk<'static, VirtioHal>>);

    impl VirtIOBlock {
        fn new() -> Self {
            let header = unsafe { &mut *(VIRTIO0 as *mut VirtIOHeader) };
            let blk = VirtIOBlk::<VirtioHal>::new(header).expect("failed to init virtio-blk");
            Self(Mutex::new(blk))
        }
    }

    impl BlockDevice for VirtIOBlock {
        fn read_block(&self, block_id: usize, buf: &mut [u8]) {
            self.0
                .lock()
                .read_block(block_id, buf)
                .expect("virtio read block failed");
        }

        fn write_block(&self, block_id: usize, buf: &[u8]) {
            self.0
                .lock()
                .write_block(block_id, buf)
                .expect("virtio write block failed");
        }
    }

    pub static BLOCK_DEVICE: Lazy<Arc<dyn BlockDevice>> = Lazy::new(|| Arc::new(VirtIOBlock::new()));
}

pub mod fs {
    use super::*;

    pub struct FileSystem {
        root: Arc<Inode>,
    }

    impl FileSystem {
        fn new(root: Inode) -> Self {
            Self { root: Arc::new(root) }
        }
    }

    impl FSManager for FileSystem {
        fn open(&self, path: &str, flags: OpenFlags) -> Option<Arc<FileHandle>> {
            let (readable, writable) = flags.read_write();

            if path == "/" || path == "." || path.is_empty() {
                return Some(Arc::new(FileHandle::new(
                    readable,
                    writable,
                    Arc::clone(&self.root),
                )));
            }

            if flags.contains(OpenFlags::CREATE) {
                if let Some(inode) = self.root.find(path) {
                    inode.clear();
                    return Some(Arc::new(FileHandle::new(readable, writable, inode)));
                }
                return self
                    .root
                    .create(path)
                    .map(|inode| Arc::new(FileHandle::new(readable, writable, inode)));
            }

            self.root.find(path).map(|inode| {
                if flags.contains(OpenFlags::TRUNC) {
                    inode.clear();
                }
                Arc::new(FileHandle::new(readable, writable, inode))
            })
        }

        fn find(&self, path: &str) -> Option<Arc<Inode>> {
            if path == "/" || path == "." || path.is_empty() {
                return Some(Arc::clone(&self.root));
            }
            self.root.find(path)
        }

        fn link(&self, _src: &str, _dst: &str) -> isize {
            -1
        }

        fn unlink(&self, _path: &str) -> isize {
            -1
        }

        fn readdir(&self, path: &str) -> Option<Vec<String>> {
            if path == "/" || path == "." || path.is_empty() {
                return Some(self.root.readdir());
            }
            self.root.find(path).map(|inode| inode.readdir())
        }
    }

    pub static FS: Lazy<FileSystem> = Lazy::new(|| {
        let efs = EasyFileSystem::open(Arc::clone(&virtio_block::BLOCK_DEVICE));
        let root = EasyFileSystem::root_inode(&efs);
        FileSystem::new(root)
    });

    pub fn read_all(file: Arc<FileHandle>) -> Vec<u8> {
        let mut data = Vec::new();
        let Some(inode) = file.inode.as_ref() else {
            return data;
        };
        let mut offset = 0usize;
        let mut buf = [0u8; 512];
        loop {
            let len = inode.read_at(offset, &mut buf);
            if len == 0 {
                break;
            }
            data.extend_from_slice(&buf[..len]);
            offset += len;
        }
        data
    }
}

fn duplicate_file_handle(file: &FileHandle) -> FileHandle {
    let mut cloned = match file.inode.as_ref() {
        Some(inode) => FileHandle::new(file.readable(), file.writable(), Arc::clone(inode)),
        None => FileHandle::empty(file.readable(), file.writable()),
    };
    cloned.offset = file.offset;
    cloned
}

fn new_stdio_fd_table() -> Vec<Option<Arc<Mutex<FileHandle>>>> {
    vec![
        Some(Arc::new(Mutex::new(FileHandle::empty(true, false)))),
        Some(Arc::new(Mutex::new(FileHandle::empty(false, true)))),
        Some(Arc::new(Mutex::new(FileHandle::empty(false, true)))),
    ]
}

fn clone_fd_table(src: &[Option<Arc<Mutex<FileHandle>>>]) -> Vec<Option<Arc<Mutex<FileHandle>>>> {
    src.iter()
        .map(|entry| {
            entry.as_ref().map(|handle| {
                let guard = handle.lock();
                Arc::new(Mutex::new(duplicate_file_handle(&guard)))
            })
        })
        .collect()
}

pub struct Process {
    pub pid: ProcId,
    pub context: ForeignContext,
    pub space: AddressSpace<Sv39, Sv39Manager>,
    pub stack_top: usize,
    pub fd_table: Vec<Option<Arc<Mutex<FileHandle>>>>,
}

impl Process {
    fn satp(&self) -> usize {
        (8 << 60) | self.space.root_ppn().val()
    }

    fn from_elf(elf_data: &[u8], kernel_space: &AddressSpace<Sv39, Sv39Manager>) -> Option<Self> {
        let elf = ElfFile::new(elf_data).ok()?;
        if elf.header.pt2.type_().as_type() != ElfType::Executable {
            return None;
        }
        if elf.header.pt2.machine().as_machine() != Machine::RISC_V {
            return None;
        }

        let mut space = AddressSpace::<Sv39, Sv39Manager>::new();
        let entry = elf.header.pt2.entry_point() as usize;

        let mut page_flags: BTreeMap<usize, (bool, bool, bool)> = BTreeMap::new();
        for ph in elf.program_iter() {
            if ph.get_type().ok()? != ProgramType::Load {
                continue;
            }
            let vaddr = ph.virtual_addr() as usize;
            let memsz = ph.mem_size() as usize;
            if memsz == 0 {
                continue;
            }
            let flags = ph.flags();
            let vpn_start = vaddr >> 12;
            let vpn_end = (vaddr + memsz + PAGE_SIZE - 1) >> 12;
            for vpn in vpn_start..vpn_end {
                let entry = page_flags.entry(vpn).or_insert((false, false, false));
                entry.0 |= flags.is_read();
                entry.1 |= flags.is_write();
                entry.2 |= flags.is_execute();
            }
        }

        let mut mapped_vpns = BTreeSet::new();

        for ph in elf.program_iter() {
            if ph.get_type().ok()? != ProgramType::Load {
                continue;
            }

            let vaddr = ph.virtual_addr() as usize;
            let offset = ph.offset() as usize;
            let filesz = ph.file_size() as usize;
            let memsz = ph.mem_size() as usize;
            if memsz == 0 {
                continue;
            }

            let vpn_start = vaddr >> 12;
            let vpn_end = (vaddr + memsz + PAGE_SIZE - 1) >> 12;

            for vpn in vpn_start..vpn_end {
                let page_vaddr = vpn << 12;
                let page_offset = vaddr.saturating_sub(page_vaddr);
                let data_start = page_vaddr.saturating_sub(vaddr);

                if mapped_vpns.contains(&vpn) {
                    if data_start < filesz {
                        let copy_len = (filesz - data_start).min(PAGE_SIZE - page_offset);
                        if copy_len > 0 {
                            let dst = space.translate::<u8>(
                                VAddr::<Sv39>::new(page_vaddr + page_offset),
                                VmFlags::build_from_str("W"),
                            )?;
                            let src = &elf_data
                                [offset + data_start..offset + data_start + copy_len];
                            unsafe {
                                core::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_ptr(), copy_len)
                            };
                        }
                    }
                    continue;
                }

                mapped_vpns.insert(vpn);

                let (has_r, has_w, has_x) = page_flags
                    .get(&vpn)
                    .copied()
                    .unwrap_or((true, false, false));
                let vm_flags = match (has_r, has_w, has_x) {
                    (_, true, true) => VmFlags::build_from_str("VRWXU"),
                    (_, false, true) => VmFlags::build_from_str("VRXU"),
                    (_, true, false) => VmFlags::build_from_str("VRWU"),
                    _ => VmFlags::build_from_str("VRU"),
                };

                let data = if data_start < filesz {
                    let copy_len = (filesz - data_start).min(PAGE_SIZE - page_offset);
                    &elf_data[offset + data_start..offset + data_start + copy_len]
                } else {
                    &[]
                };

                let range = VPN::new(vpn)..VPN::new(vpn + 1);
                space.map(range, data, page_offset, vm_flags);
            }
        }

        let stack_vpn = TOP_OF_USER_STACK_VPN - USER_STACK_PAGES;
        let stack_range = VPN::new(stack_vpn)..VPN::new(stack_vpn + USER_STACK_PAGES);
        space.map(stack_range, &[], 0, VmFlags::build_from_str("VRWU"));
        let stack_top = VAddr::<Sv39>::new(TOP_OF_USER_STACK_VPN << 12)
            .val()
            .wrapping_sub(16);

        space.copy_leaf_pte_from(kernel_space, VPN::new(PORTAL_VPN));

        let mut context = kernel_context::LocalContext::user(entry);
        *context.sp_mut() = stack_top;

        Some(Self {
            pid: ProcId::from_usize(usize::MAX),
            context: ForeignContext {
                context,
                satp: (8 << 60) | space.root_ppn().val(),
            },
            space,
            stack_top,
            fd_table: new_stdio_fd_table(),
        })
    }

    fn fork(&self, kernel_space: &AddressSpace<Sv39, Sv39Manager>) -> Option<Self> {
        let mut child_space = AddressSpace::<Sv39, Sv39Manager>::new();
        self.space.cloneself(&mut child_space);
        child_space.copy_leaf_pte_from(kernel_space, VPN::new(PORTAL_VPN));

        let mut child_ctx = self.context.context.clone();
        *child_ctx.a_mut(0) = 0;

        let child_pid = alloc_pid_nonzero();

        Some(Self {
            pid: child_pid,
            context: ForeignContext {
                context: child_ctx,
                satp: (8 << 60) | child_space.root_ppn().val(),
            },
            space: child_space,
            stack_top: self.stack_top,
            fd_table: clone_fd_table(&self.fd_table),
        })
    }

    fn exec(&mut self, elf_data: &[u8], kernel_space: &AddressSpace<Sv39, Sv39Manager>) -> isize {
        let Some(new_proc) = Self::from_elf(elf_data, kernel_space) else {
            return -1;
        };

        let mut old_space = core::mem::replace(&mut self.space, new_proc.space);
        old_space.free_allocated_pages_and_root(Some(VPN::new(PORTAL_VPN)));
        self.context = new_proc.context;
        self.stack_top = new_proc.stack_top;
        0
    }

    fn alloc_fd(&mut self, file: Arc<Mutex<FileHandle>>) -> usize {
        for fd in 3..self.fd_table.len() {
            if self.fd_table[fd].is_none() {
                self.fd_table[fd] = Some(file);
                return fd;
            }
        }
        self.fd_table.push(Some(file));
        self.fd_table.len() - 1
    }

    fn get_fd(&self, fd: usize) -> Option<Arc<Mutex<FileHandle>>> {
        self.fd_table.get(fd).and_then(|f| f.as_ref()).cloned()
    }

    fn close_fd(&mut self, fd: usize) -> isize {
        if fd >= self.fd_table.len() {
            return -1;
        }
        if self.fd_table[fd].is_none() {
            return -1;
        }
        self.fd_table[fd] = None;
        0
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

fn alloc_pid_nonzero() -> ProcId {
    loop {
        let pid = ProcId::new();
        if pid.get_usize() != 0 {
            return pid;
        }
    }
}

fn current_space() -> Option<&'static AddressSpace<Sv39, Sv39Manager>> {
    unsafe { CURRENT_SPACE.and_then(|p| p.as_ref()) }
}

fn current_process_mut() -> Option<&'static mut Process> {
    let pid = unsafe { CURRENT_PID? };
    unsafe { PROCESSOR.as_mut() }?.get_task(pid)
}

fn read_user_bytes(
    space: &AddressSpace<Sv39, Sv39Manager>,
    ptr: *const u8,
    len: usize,
) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    let flags = VmFlags::build_from_str("R");
    for i in 0..len {
        let vaddr = VAddr::<Sv39>::new(ptr as usize + i);
        let src = space.translate::<u8>(vaddr, flags)?;
        out.push(unsafe { *src.as_ptr() });
    }
    Some(out)
}

fn write_user_bytes(
    space: &AddressSpace<Sv39, Sv39Manager>,
    ptr: *mut u8,
    data: &[u8],
) -> bool {
    let flags = VmFlags::build_from_str("W");
    for (i, byte) in data.iter().copied().enumerate() {
        let vaddr = VAddr::<Sv39>::new(ptr as usize + i);
        let dst = match space.translate::<u8>(vaddr, flags) {
            Some(p) => p,
            None => return false,
        };
        unsafe { *dst.as_ptr() = byte };
    }
    true
}

fn read_user_cstr(space: &AddressSpace<Sv39, Sv39Manager>, ptr: *const u8) -> Option<String> {
    let flags = VmFlags::build_from_str("R");
    let mut buf = Vec::new();
    for i in 0..USER_CSTR_MAX {
        let vaddr = VAddr::<Sv39>::new(ptr as usize + i);
        let src = space.translate::<u8>(vaddr, flags)?;
        let b = unsafe { *src.as_ptr() };
        if b == 0 {
            return String::from_utf8(buf).ok();
        }
        buf.push(b);
    }
    None
}

fn print_available_apps() {
    if let Some(apps) = fs::FS.readdir("/") {
        print!("Available applications:");
        for app in apps {
            print!(" {}", app);
        }
        println!();
    }
}

struct SyscallContext;

impl syscall::IO for SyscallContext {
    fn write(&self, _caller: Caller, fd: usize, buf: *const u8, count: usize) -> isize {
        if count == 0 {
            return 0;
        }

        let Some(space) = current_space() else {
            return -1;
        };

        if fd == STDOUT || fd == STDDEBUG {
            let Some(handle) = current_process_mut().and_then(|p| p.get_fd(fd)) else {
                return -1;
            };
            if !handle.lock().writable() {
                return -1;
            }
            let Some(data) = read_user_bytes(space, buf, count) else {
                return -1;
            };
            for byte in data.iter().copied() {
                print!("{}", byte as char);
            }
            return count as isize;
        }

        let Some(data) = read_user_bytes(space, buf, count) else {
            return -1;
        };

        let Some(file) = current_process_mut().and_then(|p| p.get_fd(fd)) else {
            return -1;
        };

        let mut file = file.lock();
        if !file.writable() {
            return -1;
        }
        let Some(inode) = file.inode.as_ref() else {
            return -1;
        };

        let written = inode.write_at(file.offset, &data);
        file.offset += written;
        written as isize
    }

    fn read(&self, _caller: Caller, fd: usize, buf: *mut u8, count: usize) -> isize {
        if count == 0 {
            return 0;
        }

        let Some(space) = current_space() else {
            return -1;
        };

        if fd == STDIN {
            let Some(handle) = current_process_mut().and_then(|p| p.get_fd(fd)) else {
                return -1;
            };
            if !handle.lock().readable() {
                return -1;
            }

            let mut in_buf = Vec::with_capacity(count);
            while in_buf.len() < count {
                #[allow(deprecated)]
                let ch = legacy::console_getchar();
                if ch == usize::MAX {
                    if in_buf.is_empty() {
                        continue;
                    }
                    break;
                }
                in_buf.push(ch as u8);
            }
            if write_user_bytes(space, buf, &in_buf) {
                return in_buf.len() as isize;
            }
            return -1;
        }

        let Some(file) = current_process_mut().and_then(|p| p.get_fd(fd)) else {
            return -1;
        };

        let mut file = file.lock();
        if !file.readable() {
            return -1;
        }
        let Some(inode) = file.inode.as_ref() else {
            return -1;
        };

        let mut out = vec![0u8; count];
        let read_len = inode.read_at(file.offset, &mut out);
        file.offset += read_len;

        if write_user_bytes(space, buf, &out[..read_len]) {
            read_len as isize
        } else {
            -1
        }
    }

    fn open(&self, _caller: Caller, path: *const u8, flags: u32) -> isize {
        let Some(space) = current_space() else {
            return -1;
        };
        let Some(path) = read_user_cstr(space, path) else {
            return -1;
        };
        let flags = OpenFlags::from_bits_truncate(flags);
        let Some(file) = fs::FS.open(path.as_str(), flags) else {
            return -1;
        };

        let kernel_file = Arc::new(Mutex::new(duplicate_file_handle(&file)));
        let Some(proc) = current_process_mut() else {
            return -1;
        };
        proc.alloc_fd(kernel_file) as isize
    }

    fn close(&self, _caller: Caller, fd: usize) -> isize {
        let Some(proc) = current_process_mut() else {
            return -1;
        };
        proc.close_fd(fd)
    }
}

impl syscall::Process for SyscallContext {
    fn fork(&self, _caller: Caller) -> isize {
        let Some(kernel_space) = (unsafe { KERNEL_SPACE.as_ref() }) else {
            return -1;
        };
        let Some(processor) = (unsafe { PROCESSOR.as_mut() }) else {
            return -1;
        };
        let Some(parent) = processor.current() else {
            return -1;
        };

        let Some(child) = parent.fork(kernel_space) else {
            return -1;
        };
        let child_pid = child.pid;

        let parent_pid = unsafe { CURRENT_PID.unwrap() };
        processor.add(child_pid, child, parent_pid);
        child_pid.get_usize() as isize
    }

    fn exec(&self, _caller: Caller, path: *const u8) -> isize {
        let Some(space) = current_space() else {
            return -1;
        };
        let Some(path) = read_user_cstr(space, path) else {
            return -1;
        };

        let Some(file) = fs::FS.open(path.as_str(), OpenFlags::RDONLY) else {
            log::error!("Application not found: {}", path);
            print_available_apps();
            return -1;
        };
        let elf_data = fs::read_all(file);

        let Some(kernel_space) = (unsafe { KERNEL_SPACE.as_ref() }) else {
            return -1;
        };

        let Some(processor) = (unsafe { PROCESSOR.as_mut() }) else {
            return -1;
        };
        let pid = unsafe { CURRENT_PID.unwrap() };
        let Some(proc) = processor.get_task(pid) else {
            return -1;
        };

        proc.exec(&elf_data, kernel_space)
    }

    fn exit(&self, _caller: Caller, exit_code: i32) -> isize {
        exit_code as isize
    }

    fn wait(&self, caller: Caller, exit_code_ptr: *mut i32) -> isize {
        self.waitpid(caller, -1, exit_code_ptr)
    }

    fn waitpid(&self, _caller: Caller, pid: isize, exit_code_ptr: *mut i32) -> isize {
        if pid < -1 {
            return -1;
        }

        let child_pid = if pid == -1 {
            ProcId::from_usize(usize::MAX)
        } else {
            ProcId::from_usize(pid as usize)
        };

        let Some(processor) = (unsafe { PROCESSOR.as_mut() }) else {
            return -1;
        };
        match processor.wait(child_pid) {
            Some((sentinel, -1)) if sentinel.get_usize() == usize::MAX - 1 => -2,
            Some((reaped_pid, code)) => {
                if !exit_code_ptr.is_null() {
                    let Some(space) = current_space() else {
                        return -1;
                    };
                    let code_bytes = (code as i32).to_ne_bytes();
                    if !write_user_bytes(space, exit_code_ptr as *mut u8, &code_bytes) {
                        return -1;
                    }
                }
                reaped_pid.get_usize() as isize
            }
            None => -1,
        }
    }

    fn getpid(&self, _caller: Caller) -> isize {
        unsafe { CURRENT_PID.map(|p| p.get_usize() as isize).unwrap_or(-1) }
    }
}

impl syscall::Scheduling for SyscallContext {
    fn sched_yield(&self, _caller: Caller) -> isize {
        let Some(processor) = (unsafe { PROCESSOR.as_mut() }) else {
            return -1;
        };
        processor.make_current_suspend();
        0
    }
}

impl syscall::Clock for SyscallContext {
    fn clock_gettime(&self, _caller: Caller, clock_id: usize, tp: *mut TimeSpec) -> isize {
        if clock_id != ClockId::CLOCK_MONOTONIC.0 {
            return -1;
        }

        let ticks = riscv::register::time::read64();
        const CLOCK_FREQ: u64 = 10_000_000;
        let ts = TimeSpec {
            tv_sec: (ticks / CLOCK_FREQ) as usize,
            tv_nsec: ((ticks % CLOCK_FREQ) * 1_000_000_000 / CLOCK_FREQ) as usize,
        };

        let Some(space) = current_space() else {
            return -1;
        };

        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&ts as *const TimeSpec).cast::<u8>(),
                core::mem::size_of::<TimeSpec>(),
            )
        };
        if write_user_bytes(space, tp as *mut u8, bytes) {
            0
        } else {
            -1
        }
    }
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
    let heap_size = heap_end.saturating_sub(heap_start);
    assert!(heap_size > 0, "no heap space");

    kernel_alloc::init(heap_start);
    let heap_region = unsafe { core::slice::from_raw_parts_mut(heap_start as *mut u8, heap_size) };
    unsafe { kernel_alloc::transfer(heap_region) };

    let portal_size = MultislotPortal::calculate_size(1);
    assert!(portal_size <= PAGE_SIZE, "portal transit too large");
    let portal_layout = core::alloc::Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
    let portal_ptr = unsafe { alloc_zeroed(portal_layout) };
    let portal_ptr =
        NonNull::new(portal_ptr).unwrap_or_else(|| handle_alloc_error(portal_layout));
    let portal_base = portal_ptr.as_ptr();
    let portal_ppn = PPN::new(portal_ptr.as_ptr() as usize >> 12);

    let kernel_space = kernel_space(
        &layout,
        PPN::new(heap_start >> 12),
        heap_size >> 12,
        portal_ppn,
    );

    let _portal_init = unsafe { MultislotPortal::init_transit(portal_base, 1) };
    unsafe {
        extern "C" {
            fn __ch6_portal_code();
            fn __ch6_portal_code_end();
        }
        let src = __ch6_portal_code as *const u8;
        let len = (__ch6_portal_code_end as usize).saturating_sub(__ch6_portal_code as usize);
        assert!(len <= PORTAL_CODE_SIZE, "portal code too large");
        let dst = portal_base.add(core::mem::size_of::<usize>());
        core::ptr::copy_nonoverlapping(src, dst, len);
        core::arch::asm!("fence.i");
    }

    unsafe { KERNEL_SPACE = Some(kernel_space) };

    // Use the portal alias mapped at PORTAL_VPN, so user and kernel agree on transit addresses.
    let portal_va = VAddr::<Sv39>::new(PORTAL_VPN << 12).val();
    let portal = unsafe { &mut *(portal_va as *mut MultislotPortal) };

    let mut processor = PManager::<Process, ProcManager>::new();
    processor.set_manager(ProcManager::new());

    let initproc = match fs::FS.open("initproc", OpenFlags::RDONLY) {
        Some(file) => {
            let elf = fs::read_all(file);
            match Process::from_elf(&elf, unsafe { KERNEL_SPACE.as_ref().unwrap() }) {
                Some(proc) => proc,
                None => {
                    log::error!("failed to parse initproc ELF");
                    sbi_rt::system_reset(Shutdown, NoReason);
                    unreachable!()
                }
            }
        }
        None => {
            log::error!("initproc not found in easy-fs image");
            print_available_apps();
            sbi_rt::system_reset(Shutdown, NoReason);
            unreachable!()
        }
    };

    let init_pid = ProcId::from_usize(0);
    let mut initproc = initproc;
    initproc.pid = init_pid;
    processor.add(init_pid, initproc, init_pid);

    unsafe { PROCESSOR = Some(processor) };

    syscall::init_io(&SyscallContext);
    syscall::init_process(&SyscallContext);
    syscall::init_scheduling(&SyscallContext);
    syscall::init_clock(&SyscallContext);

    let kernel_satp = (8 << 60) | unsafe { KERNEL_SPACE.as_ref().unwrap() }.root_ppn().val();
    satp::write(kernel_satp);
    unsafe { core::arch::asm!("sfence.vma zero, zero") };

    unsafe {
        extern "C" {
            fn __trap_handler();
        }
        core::arch::asm!("csrw stvec, {}", in(reg) __trap_handler as usize);
    }

    let caller = Caller { entity: 0, flow: 0 };

    loop {
        let processor = unsafe { PROCESSOR.as_mut().unwrap() };
        let proc = match processor.find_next() {
            Some(proc) => proc,
            None => {
                println!("no task");
                sbi_rt::system_reset(Shutdown, NoReason);
                unreachable!()
            }
        };

        unsafe {
            CURRENT_SPACE = Some(&proc.space as *const _);
            CURRENT_PID = Some(proc.pid);
        }

        unsafe {
            proc.context.execute(portal, ());
        }

        satp::write(kernel_satp);
        unsafe { core::arch::asm!("sfence.vma zero, zero") };

        let trap_cause = scause::read();
        match trap_cause.cause() {
            scause::Trap::Exception(scause::Exception::UserEnvCall) => {
                proc.context.context.move_next();

                let id = SyscallId::from(proc.context.context.a(7));
                let args = [
                    proc.context.context.a(0),
                    proc.context.context.a(1),
                    proc.context.context.a(2),
                    proc.context.context.a(3),
                    proc.context.context.a(4),
                    proc.context.context.a(5),
                ];

                match syscall::handle(caller, id, args) {
                    SyscallResult::Done(ret) => {
                        if id == SyscallId::EXIT {
                            let processor = unsafe { PROCESSOR.as_mut().unwrap() };
                            processor.make_current_exited(ret);
                        } else {
                            *proc.context.context.a_mut(0) = ret as usize;
                            let processor = unsafe { PROCESSOR.as_mut().unwrap() };
                            processor.make_current_suspend();
                        }
                    }
                    SyscallResult::Unsupported(_) => {
                        let processor = unsafe { PROCESSOR.as_mut().unwrap() };
                        processor.make_current_exited(-2);
                    }
                }
            }
            _ => {
                log::error!(
                    "trap {:?} stval={:#x} sepc={:#x}",
                    trap_cause.cause(),
                    stval::read(),
                    proc.context.context.pc()
                );
                let processor = unsafe { PROCESSOR.as_mut().unwrap() };
                processor.make_current_exited(-3);
            }
        }

        unsafe {
            CURRENT_SPACE = None;
            CURRENT_PID = None;
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    sbi_rt::system_reset(Shutdown, SystemFailure);
    unreachable!()
}

//! kernel-vm: 内核虚拟内存/页表管理最小抽象
//!
//! 提供 `PageManager` trait 与 `AddressSpace` 容器，基于 `page-table` crate 完成映射建立、地址翻译与地址空间克隆。

#![no_std]

extern crate alloc;

pub extern crate page_table;

use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;
use core::ptr::NonNull;
use page_table::{
    Decorator, PageTable, PageTableFormatter, Pte, Update, VAddr, Visitor, VmFlags, VmMeta, PPN,
    Pos, VPN,
};

// ============== PageManager ==============

/// 物理页管理与页表页可访问性抽象。
///
/// 调用方实现时必须满足 spec 中规定的前置条件，否则 `AddressSpace` 的行为可能 panic 或导致未定义行为。
pub trait PageManager<Meta: VmMeta> {
    /// 创建新的根页表页，并返回已持有该根的 `PageManager` 实例。
    fn new_root() -> Self
    where
        Self: Sized;

    /// 返回指向根页表页首个 `Pte<Meta>` 的非空指针。
    fn root_ptr(&self) -> NonNull<Pte<Meta>>;

    /// 返回根页表的物理页号。
    fn root_ppn(&self) -> PPN<Meta>;

    /// 将物理页号转换为当前地址空间中可访问的虚拟指针。
    fn p_to_v<T>(&self, ppn: PPN<Meta>) -> NonNull<T>;

    /// 将当前地址空间中的指针转换为物理页号。
    fn v_to_p<T>(&self, ptr: NonNull<T>) -> PPN<Meta>;

    /// 分配 `len` 个连续物理页，返回在当前地址空间中连续可访问的虚拟内存起始指针。
    /// 实现者可根据需要修改 `flags`（例如补充 VALID）。
    fn allocate(&mut self, len: usize, flags: &mut VmFlags<Meta>) -> NonNull<u8>;

    /// 回收从 `pte` 指示的起始物理页起、长度为 `len` 的页序列；返回值语义由实现者定义。
    fn deallocate(&mut self, pte: Pte<Meta>, len: usize) -> usize;

    /// 指示 `pte` 指向的物理页是否由本 `PageManager` 拥有（用于决定是否可进入下级页表页）。
    fn check_owned(&self, pte: Pte<Meta>) -> bool;

    /// 释放根页表页（与 `new_root` 对应）。
    fn drop_root(&mut self);
}

// ============== AddressSpace ==============

/// 地址空间容器：持有根页表与已映射虚拟区间记录。
pub struct AddressSpace<Meta: VmMeta, M: PageManager<Meta>> {
    pub areas: Vec<Range<VPN<Meta>>>,
    manager: M,
}

impl<Meta: VmMeta, M: PageManager<Meta>> AddressSpace<Meta, M> {
    /// 创建新的地址空间：`areas` 为空，根页表由 `M::new_root()` 创建。
    pub fn new() -> Self
    where
        M: PageManager<Meta>,
    {
        let manager = M::new_root();
        Self {
            areas: Vec::new(),
            manager,
        }
    }

    /// 返回根页表的物理页号。
    pub fn root_ppn(&self) -> PPN<Meta> {
        self.manager.root_ppn()
    }

    /// 以 `PageTable` 视图访问根页表，用于遍历/修改页表。
    pub fn root(&self) -> PageTable<Meta> {
        unsafe { PageTable::from_root(self.manager.root_ptr()) }
    }

    /// 将虚拟页号区间 `range` 映射到从 `pbase` 开始的连续物理页，并记录到 `areas`。
    ///
    /// 前置条件：`range` 非空；目标页表项未映射；遍历路径上的页表页由本 `PageManager` 拥有且可访问。
    pub fn map_extern(
        &mut self,
        range: Range<VPN<Meta>>,
        pbase: PPN<Meta>,
        flags: VmFlags<Meta>,
    ) {
        assert!(
            range.end.val() > range.start.val(),
            "map_extern: range must be non-empty"
        );
        let count = range.end.val() - range.start.val();
        let root_ptr = self.manager.root_ptr();

        let mut decorator = MapExternDecorator {
            vpn: range.start,
            ppn: pbase,
            flags,
            manager: &mut self.manager,
        };

        for i in 0..count {
            let vpn = range.start.val() + i;
            let vpn = VPN::new(vpn);
            let ppn = pbase.val() + i;
            let ppn = PPN::new(ppn);
            decorator.vpn = vpn;
            decorator.ppn = ppn;

            let mut pt = unsafe { PageTable::from_root(root_ptr) };
            pt.walk_mut(Pos::new(vpn, 0), &mut decorator);
        }

        self.areas.push(range);
    }

    /// 分配物理页、拷贝数据并建立映射：将 `data` 从偏移 `offset` 拷贝到新分配的页，前后零填充，再建立 `range` 到新物理页的映射。
    ///
    /// 前置条件：`count << Meta::PAGE_BITS >= data.len() + offset`。
    pub fn map(
        &mut self,
        range: Range<VPN<Meta>>,
        data: &[u8],
        offset: usize,
        mut flags: VmFlags<Meta>,
    ) {
        let count = range.end.val() - range.start.val();
        let size = count << Meta::PAGE_BITS;
        assert!(
            size >= data.len() + offset,
            "map: size must be >= data.len() + offset"
        );

        let ptr = self.manager.allocate(count, &mut flags);
        let base = ptr.as_ptr();

        // [0..offset) 零填充
        unsafe {
            core::ptr::write_bytes(base, 0, offset);
        }
        // [offset..offset+data.len()) 拷贝 data
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), base.add(offset), data.len());
        }
        // [offset+data.len()..size) 零填充
        let tail_start = offset + data.len();
        let tail_len = size - tail_start;
        unsafe {
            core::ptr::write_bytes(base.add(tail_start), 0, tail_len);
        }

        let pbase = self.manager.v_to_p(unsafe { NonNull::new_unchecked(base) });
        self.map_extern(range, pbase, flags);
    }

    /// 从 `src` 地址空间复制 VPN 对应的叶子 PTE 到本地址空间。
    /// 用于 ch4 将 kernel 的 portal PTE 复制到 process，确保 process 看到同一物理页。
    pub fn copy_leaf_pte_from(&mut self, src: &Self, vpn: VPN<Meta>) {
        let mut src_pte: Option<Pte<Meta>> = None;
        let mut get_visitor = GetPteVisitor {
            target: vpn,
            result: &mut src_pte,
            manager: &src.manager,
        };
        let pt = src.root();
        pt.walk(Pos::new(vpn, 0), &mut get_visitor);
        if let Some(pte) = src_pte {
            let root_ptr = self.manager.root_ptr();
            let mut set_decorator = SetPteDecorator {
                target: vpn,
                pte,
                manager: &mut self.manager,
            };
            let mut pt = unsafe { PageTable::from_root(root_ptr) };
            pt.walk_mut(Pos::new(vpn, 0), &mut set_decorator);
        }
    }

    /// 在页表中查询 `addr` 所在页的映射并检查权限；满足时返回当前地址空间中该页的指针（加 `addr.offset()`）。
    pub fn translate<T>(
        &self,
        addr: VAddr<Meta>,
        flags: VmFlags<Meta>,
    ) -> Option<NonNull<T>> {
        let vpn = addr.floor();
        let mut result: Option<(PPN<Meta>, VmFlags<Meta>)> = None;
        let mut visitor = TranslateVisitor {
            target: vpn,
            result: &mut result,
            manager: &self.manager,
        };
        let pt = self.root();
        pt.walk(Pos::new(vpn, 0), &mut visitor);

        let (ppn, pte_flags) = result?;
        if !pte_flags.contains(flags) {
            return None;
        }
        let base = self.manager.p_to_v::<u8>(ppn);
        let byte_offset = addr.offset();
        let ptr = unsafe { NonNull::new_unchecked(base.as_ptr().add(byte_offset) as *mut T) };
        Some(ptr)
    }

    /// 释放本地址空间中由 `map()` 分配的物理页，并释放根页表页。
    /// 用于 exec 等场景在替换地址空间前回收旧空间占用的内核堆。
    /// `skip_vpn`：若某 area 包含此 VPN，则跳过（用于 portal 等从内核复制的页）。
    pub fn free_allocated_pages_and_root(&mut self, skip_vpn: Option<VPN<Meta>>) {
        let mut pte_buf = None;
        for range in core::mem::take(&mut self.areas) {
            if let Some(skip) = skip_vpn {
                if range.start.val() <= skip.val() && skip.val() < range.end.val() {
                    continue; // 跳过 portal 等外部映射
                }
            }
            let count = range.end.val() - range.start.val();
            if count == 0 {
                continue;
            }
            let vpn0 = range.start;
            let mut get_visitor = GetPteVisitor {
                target: vpn0,
                result: &mut pte_buf,
                manager: &self.manager,
            };
            let pt = self.root();
            pt.walk(Pos::new(vpn0, 0), &mut get_visitor);
            if let Some(pte) = pte_buf.take() {
                self.manager.deallocate(pte, count);
            }
        }
        self.manager.drop_root();
    }

    /// 将本地址空间的 `areas` 中每个虚拟区间在 `new_addrspace` 中重新分配物理页、拷贝数据并建立同等映射。
    pub fn cloneself(&self, new_addrspace: &mut AddressSpace<Meta, M>) {
        for range in &self.areas {
            let count = range.end.val() - range.start.val();
            if count == 0 {
                continue;
            }

            // 从本地址空间读取该区间首页的 PTE，得到 ppn 与 flags
            let vpn0 = range.start;
            let mut src_pte: Option<(PPN<Meta>, VmFlags<Meta>)> = None;
            let mut visitor = TranslateVisitor {
                target: vpn0,
                result: &mut src_pte,
                manager: &self.manager,
            };
            let pt = self.root();
            pt.walk(Pos::new(vpn0, 0), &mut visitor);

            let (src_ppn, flags) = match src_pte {
                Some(x) => x,
                None => continue,
            };

            let size = count << Meta::PAGE_BITS;
            let src_ptr = self.manager.p_to_v::<u8>(src_ppn);

            let mut flags_clone = flags;
            let new_ptr = new_addrspace
                .manager
                .allocate(count, &mut flags_clone);
            let dst_ptr = new_ptr.as_ptr();
            unsafe {
                core::ptr::copy_nonoverlapping(src_ptr.as_ptr(), dst_ptr, size);
            }

            let new_pbase = new_addrspace
                .manager
                .v_to_p(unsafe { NonNull::new_unchecked(dst_ptr) });
            new_addrspace.map_extern(range.clone(), new_pbase, flags);
        }
    }
}

impl<Meta: VmMeta, M: PageManager<Meta>> Default for AddressSpace<Meta, M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Meta: VmMeta, M: PageManager<Meta>> fmt::Debug for AddressSpace<Meta, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let root_ppn = self.root_ppn();
        writeln!(f, "root: {:#x?}", root_ppn.val())?;
        let pt = self.root();
        let formatter = PageTableFormatter {
            pt,
            f: |ppn: PPN<Meta>| self.manager.p_to_v(ppn),
        };
        fmt::Debug::fmt(&formatter, f)
    }
}

// ============== copy_leaf_pte 用 Visitor/Decorator ==============

struct GetPteVisitor<'a, Meta: VmMeta, M: PageManager<Meta>> {
    target: VPN<Meta>,
    result: &'a mut Option<Pte<Meta>>,
    manager: &'a M,
}

impl<Meta: VmMeta, M: PageManager<Meta>> Visitor<Meta> for GetPteVisitor<'_, Meta, M> {
    fn arrive(&mut self, pte: Pte<Meta>, target: Pos<Meta>) -> Pos<Meta> {
        if target.vpn == self.target && pte.is_valid() {
            *self.result = Some(pte);
        }
        Pos::stop()
    }

    fn meet(
        &mut self,
        _level: usize,
        pte: Pte<Meta>,
        _target: Pos<Meta>,
    ) -> Option<NonNull<Pte<Meta>>> {
        if self.manager.check_owned(pte) {
            Some(self.manager.p_to_v(pte.ppn()))
        } else {
            None
        }
    }

    fn block(&mut self, _level: usize, _pte: Pte<Meta>, _target: Pos<Meta>) -> Pos<Meta> {
        Pos::stop()
    }
}

struct SetPteDecorator<'a, Meta: VmMeta, M: PageManager<Meta>> {
    target: VPN<Meta>,
    pte: Pte<Meta>,
    manager: &'a mut M,
}

impl<Meta: VmMeta, M: PageManager<Meta>> Decorator<Meta> for SetPteDecorator<'_, Meta, M> {
    fn arrive(&mut self, pte: &mut Pte<Meta>, target: Pos<Meta>) -> Pos<Meta> {
        if target.vpn == self.target {
            *pte = self.pte;
        }
        Pos::stop()
    }

    fn meet(
        &mut self,
        _level: usize,
        pte: Pte<Meta>,
        _target: Pos<Meta>,
    ) -> Option<NonNull<Pte<Meta>>> {
        if self.manager.check_owned(pte) {
            Some(self.manager.p_to_v(pte.ppn()))
        } else {
            None
        }
    }

    fn block(&mut self, _level: usize, _pte: Pte<Meta>, _target: Pos<Meta>) -> Update<Meta> {
        let mut flags = unsafe { VmFlags::from_raw(Meta::VALID_FLAG) };
        let ptr = self.manager.allocate(1, &mut flags);
        let ppn = self.manager.v_to_p(ptr);
        let pte = unsafe { VmFlags::from_raw(Meta::VALID_FLAG) }.build_pte(ppn);
        Update::Pte(pte, self.manager.p_to_v(ppn))
    }
}

// ============== map_extern 用 Decorator ==============

struct MapExternDecorator<'a, Meta: VmMeta, M: PageManager<Meta>> {
    vpn: VPN<Meta>,
    ppn: PPN<Meta>,
    flags: VmFlags<Meta>,
    manager: &'a mut M,
}

impl<Meta: VmMeta, M: PageManager<Meta>> Decorator<Meta> for MapExternDecorator<'_, Meta, M> {
    fn arrive(&mut self, pte: &mut Pte<Meta>, _target: Pos<Meta>) -> Pos<Meta> {
        assert!(!pte.is_valid(), "map_extern: target PTE already mapped");
        *pte = self.flags.build_pte(self.ppn);
        Pos::stop()
    }

    fn meet(
        &mut self,
        _level: usize,
        pte: Pte<Meta>,
        _target: Pos<Meta>,
    ) -> Option<NonNull<Pte<Meta>>> {
        if self.manager.check_owned(pte) {
            Some(self.manager.p_to_v(pte.ppn()))
        } else {
            todo!("map_extern: mapping not complete (need to create page table page)")
        }
    }

    fn block(&mut self, _level: usize, _pte: Pte<Meta>, _target: Pos<Meta>) -> Update<Meta> {
        // 遇到无效 PTE 时分配新页表页并返回 Update::Pte，供 walk_mut 写入并继续遍历
        let mut flags = unsafe { VmFlags::from_raw(Meta::VALID_FLAG) };
        let ptr = self.manager.allocate(1, &mut flags);
        let ppn = self.manager.v_to_p(ptr);
        let pte = unsafe { VmFlags::from_raw(Meta::VALID_FLAG) }.build_pte(ppn);
        Update::Pte(pte, self.manager.p_to_v(ppn))
    }
}

// ============== translate 用 Visitor ==============

struct TranslateVisitor<'a, Meta: VmMeta, M: PageManager<Meta>> {
    target: VPN<Meta>,
    result: &'a mut Option<(PPN<Meta>, VmFlags<Meta>)>,
    manager: &'a M,
}

impl<Meta: VmMeta, M: PageManager<Meta>> Visitor<Meta> for TranslateVisitor<'_, Meta, M> {
    fn arrive(&mut self, pte: Pte<Meta>, target: Pos<Meta>) -> Pos<Meta> {
        if target.vpn == self.target && pte.is_valid() {
            *self.result = Some((pte.ppn(), pte.flags()));
        }
        Pos::stop()
    }

    fn meet(
        &mut self,
        _level: usize,
        pte: Pte<Meta>,
        _target: Pos<Meta>,
    ) -> Option<NonNull<Pte<Meta>>> {
        if self.manager.check_owned(pte) {
            Some(self.manager.p_to_v(pte.ppn()))
        } else {
            None
        }
    }

    fn block(&mut self, _level: usize, _pte: Pte<Meta>, _target: Pos<Meta>) -> Pos<Meta> {
        Pos::stop()
    }
}

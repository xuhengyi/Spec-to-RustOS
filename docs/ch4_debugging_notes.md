# Ch4 调试问题总结

本文档按**发现时间顺序**记录 ch4 虚拟内存管理章节调试过程中遇到的所有问题，包括具体现象、错误信息、解决方案及待办项。

---

## 问题 1：ELF 段重叠导致 map_extern panic

**发现阶段**：初次运行 ch4，ELF 加载时

### 错误信息
```
panicked at kernel-vm/src/lib.rs:358:9:
map_extern: target PTE already mapped
```

### 具体问题
用户程序 ELF 的 LOAD 段不是页对齐的，多个段可能共享同一虚拟页。例如：
- 第一个 LOAD 段（.text）：VA 0x10000，结束于 0x15d88
- 第二个 LOAD 段（.rodata）：VA 0x15d80，起始于页内

两个段都覆盖 VPN 0x15，按段顺序映射时第二次 `map_extern` 会尝试映射已存在的 PTE，触发 panic。

### 解决方案
修改 `load_elf`，用 `BTreeSet<usize>` 记录已映射的 VPN，逐页映射：
```rust
let mut mapped_vpns: BTreeSet<usize> = BTreeSet::new();
for vpn_val in vpn_start..vpn_end {
    if mapped_vpns.contains(&vpn_val) {
        // 页已映射，仅复制数据
        continue;
    }
    mapped_vpns.insert(vpn_val);
    space.map(...);
}
```

### 状态
已解决

---

## 问题 2：重叠段权限冲突导致 StorePageFault

**发现阶段**：修复问题 1 后，用户程序执行时

### 错误信息
```
Trap Exception(StorePageFault) stval=0x189b0 pc=0x1373a
```

### 具体问题
同一页被多个段共享时，权限可能不同：
- .rodata：只读 (R)
- .data/.bss：读写 (RW)

若先按只读段映射，后按读写段映射，页表仍为只读，写入会触发 StorePageFault。

### 解决方案
在映射前收集每个 VPN 的权限并集：
```rust
let mut page_info: BTreeMap<usize, (bool, bool, bool)> = BTreeMap::new();
for ph in elf.program_iter() {
    for vpn_val in vpn_start..vpn_end {
        let entry = page_info.entry(vpn_val).or_insert((false, false, false));
        entry.0 |= flags.is_read();
        entry.1 |= flags.is_write();
        entry.2 |= flags.is_execute();
    }
}
// 映射时使用合并后的权限
```

### 状态
已解决

---

## 问题 3：Trap wrapper 使用用户栈导致 syscall 参数错误

**发现阶段**：用户程序首次 syscall 时

### 错误信息
```
Unsupported fd: 2149670992
```
或类似异常大的 fd 值（如 0x800F0210）

### 具体问题
`__ch4_trap_wrapper` 最初是 Rust 函数。trap 后 `satp` 已切回内核，但 `sp` 仍是用户栈。Rust 函数用 `sp` 访问栈，用户栈地址在内核页表中无效，导致读到错误数据，syscall 参数（fd、buf 等）被破坏。

### 解决方案
改为纯汇编实现，避免使用栈：
```asm
.globl __ch4_trap_wrapper
__ch4_trap_wrapper:
    j __trap_handler
```

### 状态
已解决

---

## 问题 4：clock_gettime 未对齐访问

**发现阶段**：调用 clock_gettime 的测试用例

### 错误信息
```
misaligned pointer dereference: address must be a multiple of 0x8 but is 0x81305f57
```

### 具体问题
用户传入的 `TimeSpec` 缓冲区可能未 8 字节对齐，直接用 `*ptr = spec` 会触发未对齐访问。

### 解决方案
使用 `core::ptr::copy_nonoverlapping` 按字节拷贝：
```rust
let spec_bytes = unsafe {
    core::slice::from_raw_parts(&spec as *const TimeSpec as *const u8, 
                                core::mem::size_of::<TimeSpec>())
};
unsafe { core::ptr::copy_nonoverlapping(spec_bytes.as_ptr(), ptr.as_ptr(), spec_bytes.len()); }
```

### 状态
已解决

---

## 问题 5：用户进程以 supervisor 模式返回

**发现阶段**：用户程序执行后立即异常

### 错误信息
```
Trap Exception(InstructionPageFault) stval=0x115d0 pc=0x115d0
```
所有进程以 exit code 1 退出。

### 具体问题
1. `LocalContext::thread(portal_entry_va, false)` 设置 `supervisor=true`
2. Portal cache 的 sstatus 中 SPP=1
3. sret 返回到 S 模式
4. S 模式访问带 U 标志的用户页会触发 InstructionPageFault

### 解决方案
在 trap 返回后修正 context：
```rust
proc.context = jump_ctx;
proc.context.supervisor = false;
proc.context.interrupt = true;
```

### 状态
已解决

---

## 问题 6：Portal 只恢复 sepc 和 a0，未恢复 sp

**发现阶段**：修正 supervisor 模式后

### 错误信息
```
Trap Exception(StorePageFault) stval=0x10000017 pc=0x114a6
```
或
```
Trap Exception(StorePageFault) stval=0x81275bb8 pc=0x10002
```

### 具体问题
原始 portal 代码只恢复 sepc、a0、sstatus，不恢复 sp。用户代码继续使用旧 sp（内核栈或错误值），写入时访问无效地址。

### 解决方案
扩展 portal cache，在 portal 中恢复 user_sp、user_ra：
```asm
ld sp, 48(a0)     # user_sp
ld ra, 56(a0)     # user_ra
```
并在 `__ch4_run_to_portal` 中把 user_sp、user_ra 写入 extended cache。

### 状态
已解决

---

## 问题 7：直接切换 satp 导致 InstructionPageFault

**发现阶段**：尝试在 `__ch4_run_to_user` 中直接切换 satp 时

### 错误信息
```
Trap Exception(InstructionPageFault) stval=0x802034d8 pc=0x802034d8
```

### 具体问题
执行 `csrw satp, user_satp` 后，PC 仍指向内核代码（如 0x802034d8）。用户页表不映射内核地址，下一条取指触发 InstructionPageFault。

### 解决方案
必须通过 portal 作为“蹦床”：
1. sret 到 portal（共享 VA 0x1000_0000，内核和用户页表都映射）
2. portal 中切换 satp 到用户空间
3. portal 再 sret 到用户代码

### 状态
已解决

---

## 问题 8：__trap_handler 依赖的 ctx 指针未保存

**发现阶段**：在 `__ch4_run_to_portal` 中加 ebreak 调试时

### 错误信息
ebreak 等 trap 触发后无输出，程序卡住；或 trap handler 无法正确保存用户寄存器。

### 具体问题
`__trap_handler` 从 `kernel_sp - 8` 读取 ctx 指针，但 `__ch4_run_to_portal` 未把 ctx 存到该位置。

### 解决方案
在 `__ch4_run_to_portal` 中增加：
```asm
sd a0, -8(sp)   # 保存 ctx 指针供 trap handler 使用
```

### 状态
已解决

---

## 问题 9：Breakpoint 导致进程被错误终止

**发现阶段**：用 ebreak 调试时

### 错误信息
调试用 ebreak 触发后，进程被移除，输出 "All processes finished"。

### 具体问题
trap 处理逻辑把所有未显式处理的 trap 都当作错误，调用 `processes.remove(current)`。

### 解决方案
对 Breakpoint 执行 `move_next()` 后继续执行：
```rust
scause::Trap::Exception(scause::Exception::Breakpoint) => {
    proc.context.move_next();
}
```

### 状态
已解决

---

## 问题 10：sret 到 portal 后每次都从 portal 入口执行

**发现阶段**：在 portal 入口加 ebreak 时

### 现象
每次循环都触发 Breakpoint，无法“跳过” ebreak。

### 具体问题
`__ch4_run_to_portal` 的 sepc 固定为 portal_entry (0x1000_0008)，每次 sret 都从 portal 开头执行，这是预期行为。

### 解决方案
移除调试用 ebreak 后，portal 会完整执行并 sret 到用户。

### 状态
已理解，非 bug

---

## 问题 11：无 GDB 时程序挂起，用户 trap 路径未完全验证

**发现阶段**：移除 ebreak、设置 stvec 为 portal trap entry 后

### 错误信息
程序在 "Loaded 12 processes" 后挂起，无 "Hello, world!" 或 "All processes finished" 输出。

### 具体问题
- 用户 trap 时，stvec 指向 portal trap entry (0x1000_0140)
- 无 GDB 时 `execute()` 从不返回，系统挂起
- 根本原因：见问题 18（Portal 页 U 标志导致 S 模式无法取指）

### GDB 已验证
- stvec = `__trap_handler`（内核地址）时，portal 内 ebreak 能正确 trap 并返回
- 在 portal 入口加 ebreak 时，能 trap 并返回，说明 `__trap_handler` 路径正常
- 断点 portal_trap_entry (0x10000140) 可命中，scause=0x8 (UserEnvCall)，说明用户能 trap 到 portal
- 内核页表可正确访问 portal (0x10000008)，`x/4i` 反汇编成功

### 状态
已解决（见问题 18）

---

## 问题 12：copy_leaf_pte_from 覆盖用户 portal 的 U 标志

**发现阶段**：GDB 调试用户 trap 路径时（2025-02）

### 错误信息
用户态 trap 时 InstructionPageFault，无法跳转到 portal trap entry。

### 具体问题
`copy_leaf_pte_from` 将内核 portal PTE 复制到用户空间。若内核 portal 带 U 标志，会与问题 18 冲突；若不带 U，用户态 ecall 后 trap 进入 S 模式，portal trap 在 S 模式执行，无需 U。正确做法见问题 18。

### 状态
已解决（见问题 18）

---

## 问题 13：Extended cache 未正确初始化

**发现阶段**：GDB 调试 StorePageFault 时（2025-02）

### 错误信息
```
Trap Exception(StorePageFault) stval=0x1000003a sepc=0x1000003a
```

### 具体问题
Portal 从 `cache_addr = 0x10000108`（偏移 264）读取 user_satp、user_sepc、user_a0、user_sstatus，但 `cache.init()` 只写入 MultislotPortal 的 slot（偏移 8）。extended cache 处仅写了 user_sp 和 user_ra，前 4 个字段未写入。Portal 读到垃圾值作为 user_sepc，sret 后跳到错误地址（如 0x1000003a），用户态 store 时触发页错误。

### 解决方案
在 execute 前，显式写入 extended cache（偏移 264）：
```rust
ext.add(0).write(user_satp);
ext.add(1).write(entry);
ext.add(2).write(proc.context.a(0));
ext.add(3).write(sstatus);
```

### 状态
已解决

---

## 问题 14：Kernel portal 映射 VPN 错误

**发现阶段**：GDB 显示 FAULT at sepc=0x10000008 时（2025-02）

### 错误信息
GDB 显示 FAULT at sepc=0x10000008（portal 入口），无法执行 portal 第一条指令。

### 具体问题
`kernel_space` 中 portal 使用 `VPN::new(portal_ppn.val())` 映射，即 VA = portal_ppn << 12。portal 物理页号通常较小，导致 portal 被映射到错误 VA，而非预期的 0x1000_0000。sret 到 portal 时取指失败。

### 解决方案
```rust
// 正确：portal 必须在 VA 0x1000_0000 (portal_vpn) 供 kernel/user 共享访问
let portal_page_range = VPN::new(portal_vpn)..VPN::new(portal_vpn + 1);
```

### 状态
已解决

---

## 问题 15：Portal stvec 设置顺序

**发现阶段**：分析 double fault 时（2025-02）

### 具体问题
必须在切换 satp 到用户空间**之前**将 stvec 设为 portal trap entry。若在切换之后设置，用户 trap 会跳转到 kernel stvec，而用户页表未映射内核地址，导致 double fault。

### 解决方案
在 portal 代码中，在 `csrw satp` 和 `sret` 之前设置 stvec：
```asm
lui t0, 0x10000
addi t0, t0, 320
csrw stvec, t0
```

### 状态
已解决

---

## 问题 16：用户栈对齐

**发现阶段**：排查 StorePageFault 时（2025-02）

### 具体问题
RISC-V ABI 要求 sp 16 字节对齐，否则可能触发未对齐访问。

### 解决方案
```rust
let stack_top = (TOP_OF_USER_STACK_VPN << 12) - 16;
```

### 状态
已解决

---

## 问题 17：TRAP_ENTRY_OFFSET 与 user_ra 重叠

**发现阶段**：排查 trap 入口不可执行时（2025-02）

### 具体问题
user_ra 写入 offset 320 会覆盖 trap 入口代码，导致 trap entry 不可用。

### 解决方案
将 TRAP_ENTRY_OFFSET 改为 384，避免与 extended cache 重叠。

### 状态
已解决

---

## 问题 18：Portal 页 U 标志导致 S 模式无法取指（最顽固问题）

**发现阶段**：修复问题 14 后，`kernel_space.translate(portal_vaddr, X)` 返回 true，页表显示 portal 可执行，但 `cargo qemu --ch 4` 仍报 InstructionPageFault at 0x10000008（2025-02）

### 错误信息
```
Trap Exception(InstructionPageFault) stval=0x10000008 pc=0x10000008
```

### 调试过程与困惑点
1. **页表检查正常**：`kernel_space.translate(portal_vaddr, VmFlags::build_from_str("X"))` 返回 `Some`，说明内核页表认为 portal 页有执行权限。
2. **satp 怀疑**：曾怀疑 trap 时 satp 被意外设为用户页表，导致取指失败，但逻辑上 execute 前 satp 应为 kernel_satp。
3. **地址计算怀疑**：反复核对 portal_entry = portal_va + 8、cache_addr 等，均正确。
4. **关键突破**：查阅 RISC-V 特权架构规范得知：**S 模式永远不能从 U 页（PTE.U=1）取指**，即使 SUM 位为 1 也不允许。SUM 仅放宽 S 模式对 U 页的 load/store，不涉及取指。

### 根本原因
Portal 代码在 `sret` 后、切换 satp 到用户空间之前，仍在 S 模式执行。若 portal 页映射为 `VRWXU`（带 U 标志），则 S 模式从该页取指违反 RISC-V 规范，触发 InstructionPageFault。用户 ecall 后 trap 到 portal，也是在 S 模式执行 portal trap 代码，同样不能从 U 页取指。

### 解决方案
将 kernel 中 portal 页映射改为 **VRWX**（去掉 U）：
```rust
space.map_extern(portal_page_range, portal_ppn, VmFlags::build_from_str("VRWX"));
```
`copy_leaf_pte_from` 将无 U 的 PTE 复制到用户空间，用户 trap 时以 S 模式执行 portal，可正常取指。用户代码本身在 U 模式运行于用户页（VRXU 等），与 portal 页分离。

### 状态
已解决

---

## 汇总表

| 序号 | 问题简述 | 错误类型 | 状态 |
|------|----------|----------|------|
| 1 | ELF 段重叠 | map_extern panic | 已解决 |
| 2 | 重叠段权限冲突 | StorePageFault | 已解决 |
| 3 | Trap wrapper 用用户栈 | syscall 参数错误 | 已解决 |
| 4 | clock_gettime 未对齐 | misaligned pointer | 已解决 |
| 5 | 用户以 S 模式返回 | InstructionPageFault | 已解决 |
| 6 | Portal 未恢复 sp | StorePageFault | 已解决 |
| 7 | 直接切换 satp | InstructionPageFault | 已解决 |
| 8 | ctx 指针未保存 | trap handler 异常 | 已解决 |
| 9 | Breakpoint 误判为致命错误 | 进程被移除 | 已解决 |
| 10 | sret 到 portal 入口 | 预期行为 | 已理解 |
| 11 | 无 GDB 时 execute 不返回 | 程序挂起 | 已解决（见 18） |
| 12 | copy_leaf_pte_from 覆盖 U | InstructionPageFault | 已解决（见 18） |
| 13 | Extended cache 未初始化 | StorePageFault | 已解决 |
| 14 | Kernel portal VPN 错误 | InstructionPageFault | 已解决 |
| 15 | stvec 设置顺序 | double fault | 已解决 |
| 16 | 用户栈对齐 | 未对齐访问 | 已解决 |
| 17 | TRAP_ENTRY_OFFSET 重叠 | trap entry 不可用 | 已解决 |
| 18 | Portal 页 U 标志导致 S 模式无法取指 | InstructionPageFault | 已解决 |

---

## 当前待验证项

- [x] 验证每个用户进程页表中 portal (0x1000_0000) 是否正确映射
- [x] 用户 trap 时 portal trap entry 是否可访问（Portal 须为 VRWX 无 U）
- [x] 用户程序能否正确执行并输出
- [x] 所有 12 个测试用例是否通过

---

## GDB -ex 调试验证记录 (2025-02)

### 脚本
- `./scripts/gdb_ch4_debug.sh`：sret 前断点，检查 portal 映射，单步执行
- `./scripts/gdb_ch4_trace.sh`：追踪 sret→portal→user→trap 完整路径
- `./scripts/gdb_ch4_trace_user.sh`：完整追踪 __execute_context→portal sret→user，可检测 FAULT
- `./scripts/gdb_ch4_portal_step.sh`：单步执行 portal trap entry
- `./scripts/gdb_ch4_nobreak.sh`：仅连接 GDB 不设断点

### 验证结果
| 检查项 | 结果 |
|--------|------|
| sret 前 `x/4i 0x10000008` | 成功，内核页表已映射 portal |
| stepi 后 portal 指令执行 | 成功，pc 从 0x10000008 推进到 0x10000018 |
| portal_trap_entry 断点 | 可命中，scause=0x8 (UserEnvCall)，sepc=0x1140e |
| portal 入口 ebreak | 能 trap 并返回，`__trap_handler` 路径正常 |
| portal 入口 FAULT（问题 14 修复前） | sepc=0x10000008，无法执行 portal 第一条指令 |
| portal 入口 FAULT（问题 14 修复后） | 无 FAULT，可到达 portal sret |
| portal sret 断点 | 可命中，stepi 后进入用户态 pc=0x10000 |
| 完整测试无 GDB | 问题 18 修复后正常完成 |

### 关键地址（需随构建用 objdump/nm 确认）
- `__execute_context` 中 sret：0x80215bea（示例，以 objdump 为准）
- `__trap_handler`：0x80215bf0
- portal 入口：0x10000008
- portal sret：0x1000003e（portal 入口 + 0x36）
- portal trap entry：0x10000180（TRAP_ENTRY_OFFSET=384）
- 用户程序入口：0x10000

---

## 调试建议

1. **GDB 调试**：使用 `./scripts/gdb_ch4_debug.sh` 或 `./scripts/gdb_ch4_trace.sh`
2. **关键断点**：`__execute_context` 的 sret (0x80215bea)、portal 入口 0x10000008、portal sret 0x1000003e、portal trap entry 0x10000180
3. **检查项**：portal 映射（须 VRWX 无 U，见问题 18）、cache 内容、user_sp、user_sepc、stvec 值

---

## 未解决的问题（待进一步排查）

当前 ch4 已通过 `cargo qemu --ch 4` 完整测试，上述问题均已解决。若后续出现新问题，可参考本文档的调试思路与 GDB 脚本。

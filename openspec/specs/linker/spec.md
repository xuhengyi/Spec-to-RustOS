# Capability: linker

该 capability 定义内核链接脚本（linker script）及其导出的链接符号，并提供依赖这些符号的启动入口与布局查询能力。

## Requirements

### Requirement: Export linker script bytes (`SCRIPT`)
`linker` crate MUST 通过 `SCRIPT: &[u8]` 导出一份用于 RISC-V 内核的链接脚本文本。

该链接脚本 MUST：
- 将 `.text` 段起始地址固定在 `0x80200000`；
- 定义并导出符号 `__start`、`__rodata`、`__data`、`__sbss`、`__ebss`、`__boot`、`__end`；
- 包含 `.boot` 段，且 MUST `KEEP(*(.boot.stack))`，以确保启动栈不会被链接器丢弃。

#### Scenario: Consumer uses `SCRIPT` via build.rs
- **WHEN** 某个依赖 `linker` 的 crate 在 `build.rs` 中将 `SCRIPT` 写入 `${OUT_DIR}/linker.ld` 并传递 `-T${OUT_DIR}/linker.ld`
- **THEN** 最终链接产物 MUST 采用该脚本的段布局，并导出上述符号以供运行时查询

### Requirement: Define boot entry (`boot0!`)
`linker` crate MUST 提供宏 `boot0!` 用于定义内核启动入口 `_start`。

`boot0!` MUST：
- 定义一个 `#[no_mangle]` 的 `unsafe extern "C" fn _start() -> !`；
- 将 `_start` 放入链接段 `.text.entry`；
- 在链接段 `.boot.stack` 中定义静态可变启动栈 `STACK`，其大小由调用方传入的表达式决定；
- 在 `_start` 中通过汇编将栈指针 `sp` 设置为 `__end`，随后无条件跳转到符号 `rust_main`。

#### Scenario: Kernel defines an entry with a boot stack
- **WHEN** 内核 crate 使用 `linker::boot0!(rust_main; stack = 4 * 4096);`
- **THEN** 链接产物 MUST 含有 `_start` 符号与 `.boot.stack` 段内容，并在启动时在启动栈上跳转执行 `rust_main`

### Requirement: Locate kernel layout (`KernelLayout`)
`linker` crate MUST 提供 `KernelLayout` 以读取并暴露内核在内存中的静态布局信息。

`KernelLayout::locate()` MUST 通过读取链接符号地址定位布局，并填充以下字段（语义对应符号地址）：
- `text` = `__start`
- `rodata` = `__rodata`
- `data` = `__data`
- `sbss` = `__sbss`
- `ebss` = `__ebss`
- `boot` = `__boot`
- `end` = `__end`

`KernelLayout` MUST 提供：
- `start()`：返回内核起始地址（`__start`）
- `end()`：返回内核结束地址（`__end`）
- `len()`：返回 `end - start`
- `iter()`：返回按固定顺序遍历的内核分区迭代器（Text → Rodata → Data → Boot）

#### Scenario: Kernel prints memory map
- **WHEN** 内核在运行早期调用 `KernelLayout::locate()` 并遍历 `layout.iter()`
- **THEN** 迭代器 MUST 依序产出四个不重叠的地址区间，分别覆盖 `.text/.rodata/.data(+.bss)/.boot` 的范围

### Requirement: Zero `.bss` (`KernelLayout::zero_bss`)
`KernelLayout::zero_bss()` MUST 将地址区间 `[__sbss, __ebss)` 清零。

该过程 MUST 使用 volatile 写入以确保对其他处理器核可见（若适用）。

#### Scenario: Early boot clears BSS before using static mut
- **WHEN** 内核在初始化阶段调用 `unsafe { KernelLayout::locate().zero_bss() }`
- **THEN** `[__sbss, __ebss)` 区间内的所有字节 MUST 变为 `0`

### Requirement: Enumerate linked applications (`AppMeta` / `AppIterator`)
`linker` crate MUST 提供 `AppMeta` 与 `AppIterator` 用于遍历“被链接进内核镜像的应用程序”。

`AppMeta::locate()` MUST 返回指向链接符号 `apps` 的静态引用，其中 `apps` 的内存布局 MUST 满足：
- `base: u64`
- `step: u64`
- `count: u64`
- `first: u64`（紧随其后是一个长度为 `count + 1` 的地址数组，用于给出每个 app 的起始地址，并以最后一个地址作为末尾边界）

`AppMeta::iter()` 返回的迭代器在每次 `next()` 时 MUST：
- 计算第 `i` 个 app 的 `(pos, size)`，其中 `pos = addr[i]` 且 `size = addr[i+1] - addr[i]`
- 若 `base != 0`：将 `[pos, pos+size)` 拷贝到 `dst = base + i*step`，并将 `dst+size .. dst+0x20_0000` 清零；返回指向 `[dst, dst+size)` 的切片
- 若 `base == 0`：直接返回指向 `[pos, pos+size)` 的切片

#### Scenario: Kernel loads apps into fixed slots
- **WHEN** 链接产物提供了 `apps` 符号，且其 `base != 0`、每个 `dst..dst+0x20_0000` 都是可写的有效内存
- **THEN** 遍历 `AppMeta::locate().iter()` MUST 逐个将 app 映像拷贝到对应槽位并返回其切片视图

## Public API

### Constants
- `SCRIPT: &[u8]`: 链接脚本文本（字节序列）。

### Macros
- `boot0!($entry:ident; stack = $stack:expr)`: 定义 `_start` 并设置启动栈；当前实现会跳转到符号 `rust_main`。

### Types
- `KernelLayout`: 内核布局信息与操作。
- `KernelRegionIterator<'a>`: 内核分区迭代器。
- `KernelRegionTitle`: 分区名称枚举（`Text`/`Rodata`/`Data`/`Boot`）。
- `KernelRegion`: 分区条目（`title` + `range: Range<usize>`）。
- `AppMeta`: 应用程序元数据头。
- `AppIterator`: 应用程序迭代器（`Iterator<Item = &'static [u8]>`）。

### Functions / Methods
- `KernelLayout::locate() -> KernelLayout`
- `KernelLayout::start(&self) -> usize`
- `KernelLayout::end(&self) -> usize`
- `KernelLayout::len(&self) -> usize`
- `unsafe KernelLayout::zero_bss(&self)`
- `KernelLayout::iter(&self) -> KernelRegionIterator<'_>`
- `AppMeta::locate() -> &'static AppMeta`
- `AppMeta::iter(&'static self) -> AppIterator`

## Build Configuration
- **build.rs（consumer）**: consumer 的 `build.rs` SHOULD 将 `linker::SCRIPT` 写入 `${OUT_DIR}/linker.ld`（或等价路径），并通过 `println!("cargo:rustc-link-arg=-T{}", path)` 传给链接器。
- **环境变量**: `${OUT_DIR}`（由 Cargo 提供）。
- **生成文件**: `linker.ld`（建议命名；由 consumer 生成）。

## Dependencies / Preconditions
- **External crates**: 无。
- **Workspace crates**: 无（该 crate 本身不依赖 workspace 其他 crate）。
- **Link-time symbols**:
  - MUST 由 `SCRIPT` 导出：`__start/__rodata/__data/__sbss/__ebss/__boot/__end`
  - MUST 由最终链接产物提供（若使用 `AppMeta`）：`apps`，且其内存布局与本 spec 的 `AppMeta::locate()` 要求一致
- **Target / Toolchain**: 链接脚本声明 `OUTPUT_ARCH(riscv)`；consumer MUST 使用与之匹配的目标平台与链接器能力（例如支持 `-T` 传入脚本）。


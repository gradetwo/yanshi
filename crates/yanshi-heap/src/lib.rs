//! **把 glibc 堆里已空闲的 arena 归还给 OS（RSS 修复 paint-rss）**。
//!
//! 为什么要单独立一个 crate：工作区其余 crate 全部 `#![forbid(unsafe_code)]`，
//! 而 `malloc_trim` 是 C FFI、调用本身必须是 `unsafe`。把它隔离在这里，
//! 本 crate 只含这一处 `unsafe` 调用，对外只暴露一个安全函数；
//! `yanshi-server` 调它时仍保持"本 crate 零 unsafe"。
//!
//! 为什么需要它（定位报告 §3.2）：`close`/`delete` 丢掉文档后，`Document`
//! 及其缓存确实已释放，但 glibc malloc 只把 ~40% 及时归还 OS，约 60%
//! 滞留堆 arena（可复用、不归还）：关完 12 个文档静置后仍滞留 2,073MB。
//! `malloc_trim(0)` 要求 glibc 把堆顶连续空闲收缩归还；若 arena 已碎片化、
//! 归还不足，再评估换 jemalloc/mimalloc——不要先换分配器。

// glibc 的 malloc_trim（仅 Linux；不引入 libc 依赖，一个 extern 声明就够了）。
// 注意：extern 块不生成 rustdoc，所以这里用 // 而不是 ///（否则 clippy 的
// unused_doc_comments 会告警，见 yanshi-server 首次合入时的教训）。
#[cfg(target_os = "linux")]
extern "C" {
    fn malloc_trim(pad: usize) -> std::os::raw::c_int;
}

/// **把堆里已空闲的 arena 归还给 OS（Linux：调 `malloc_trim(0)`；别处：空操作）**。
///
/// 返回 `true` 表示确实归还了一部分（glibc 的返回值语义）。调用点只在
/// "刚丢掉一整份文档"之后（`close` / `delete` / 预算卸载），不是热路径，
/// 每次至多毫秒级；调用本身不触碰任何 Rust 对象、不改变任何分配语义——
/// 归还的只是**已空闲**的堆页，活数据不受影响。
#[cfg(target_os = "linux")]
pub fn release_heap_to_os() -> bool {
    // SAFETY：`malloc_trim(0)` 是 glibc 的公开 API；`pad = 0` 表示"尽量归还"，
    // 无非法输入、无指针传递，调用本身不触碰任何 Rust 对象。
    unsafe { malloc_trim(0) == 1 }
}

/// **把堆里已空闲的 arena 归还给 OS（非 Linux：空操作）**。
#[cfg(not(target_os = "linux"))]
pub fn release_heap_to_os() -> bool {
    false
}

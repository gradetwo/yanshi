//! wasm 目标守卫：限制会编译进 wasm32 的代码里对「宿主专属 std」的使用。
//!
//! 背景：`std::time::Instant::now()` 在 `wasm32-unknown-unknown` 上**不受支持并 panic**，
//! 而 `cargo test` 在宿主上跑，完全看不到这个问题 —— 上一轮一个诊断探针就这样让
//! 浏览器端每次渲染都崩，原生却全绿。
//!
//! 本守卫采用「**带理由的白名单绊线**」：统计各 crate 里 `Instant::now` / `SystemTime::now`
//! 的出现次数，只有与白名单**完全一致**才通过。任何新增用法都会让测试失败，
//! 迫使作者显式确认「它是否按目标平台门控」——而不是悄悄溜进去。
//!
//! 它是**启发式**而非证明：真正的运行时证明由 `scripts/wasm-smoke.sh` 提供
//! （在 node 里真跑一次 wasm 渲染）。两层一起用：这一层在写代码时即时反馈，那一层保底。

use std::path::{Path, PathBuf};

/// 允许出现的宿主时间 API 用法（文件 → 次数 → 理由）。
fn allowlist() -> Vec<(&'static str, usize, &'static str)> {
    vec![
        // **★ 第 184 轮新增 ✓**：**阶段一第 2 条的**成本调查**探针 ✓。
        // **∴ 它**做什么 ✗**：**分别**计时「**f16 往返**」**与「**整条逐像素路**」✗
        //   ⇒ **∴ 于是**得出「**f16 占 22.6%（**上界 ✓）」** ✓
        //     ⇒ **★ 而**那个数**否掉了**「**引入 wide／手写 SIMD ✓」**这个方向 ✓ ★**
        // **∴ 为什么它**可以在这里**✗**：**它**位于
        //   `#[cfg(all(test, not(target_arch = wasm32)))]` **里** ✓
        //   ⇒ **∴ 即**：**测试**代码**不参与** wasm 构建**✗** ⇒ **∴ 永不**编译进客户端 ✓
        // **∴ 与** `geometry.rs` **那条**同理由 ✓**（**那里**也是** `#[cfg(test)]` **的微基准 ✓）**
        (
            "crates/yanshi-render/src/rows.rs",
            2,
            "`#[cfg(all(test, not(target_arch = wasm32)))]` 测试模块 split_probe 里的 2 处 \
             `Instant::now()`（分别计时 f16 往返与整条逐像素路，用于定要不要 SIMD）：\
             测试代码不参与 wasm 构建，永不编译进客户端",
        ),
        (
            "crates/yanshi-render/src/render.rs",
            9,
            "（1）stage_probe 模块里的 Instant::now()：1 处代码 + 1 处说明注释，代码那处位于
             #[cfg(not(target_arch = wasm32))] 分支，wasm32 走编译期空操作；
             （2）#[cfg(test)] 测试模块里的 5 处 Instant::now()（液化成本探针与三个微基准）——
             测试代码不参与 wasm 构建，永不编译进客户端；
             （3）并行分带的直接证据探针（第 869 轮）：1 处 Instant::now() 与 1 处
             SystemTime::now()，只在 YANSHI_PARALLEL_PROBE 打开时执行，且位于
             #[cfg(not(target_arch = wasm32))] 的并行路径内 —— wasm32 上那条路径整体不参与编译",
        ),
        (
            "crates/yanshi-render/src/geometry.rs",
            2,
            "`#[cfg(test)]` 测试模块 `blur_work_does_not_grow_with_radius` 里的 2 处 `Instant::now()`：\
             同一张图分别用 radius 8 与 radius 128 计时、用**同一次运行的比值**判「每像素 O(1)」\
             —— 测试代码不参与 wasm 构建，永不编译进客户端",
        ),
        (
            "crates/yanshi-core/src/ids.rs",
            2,
            "`SystemTime::now()` 用于 ULID 进程种子与 `now_ms()`：目前只被服务端/宿主路径调用 \
             （客户端由 JS 提供 id 与 timestamp，见 scripts/wasm-smoke.sh 的写入路径）。\
             属于**潜在风险**：若内核将来调用 `now_ms()`，在 wasm32 上会 panic —— \
             scripts/wasm-smoke.sh 的写入路径用例就是为盯住这件事而存在",
        ),
        (
            "crates/yanshi-core/src/blob.rs",
            3,
            "第 1504／1611 轮的**存储写入分段计时**（`put_timing`，受 `YANSHI_OPEN_TIMING` 开关控制）：\
             2 处 `Instant::now()` 与 1 处 `SystemTime::now()`（后者是 9ff42c5 为「与外部报告对表」加的 \
             `epoch_ms`）。它们都在 **`FsBlobStore::put_inner`** 里 —— 那是**文件系统**存储。\
             而 wasm 内核用的是 **`MemoryBlobStore`**（见 crates/yanshi-wasm/src/kernel.rs 的 `store` 字段），\
             **从不调 `FsBlobStore`** ⇒ 运行时不会被调到。\
             属于**潜在风险**：`blob` 模块是**无条件**编译的 ⇒ 这段代码**会编进 wasm32**；\
             若将来门面也用文件系统存储，就必须先把它挪进 `#[cfg(not(target_arch = \"wasm32\"))]` 分支。\
             scripts/wasm-smoke.sh 的写入路径用例是这件事的守卫",
        ),
    ]
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate 应位于 <repo>/crates/yanshi-render")
        .to_path_buf()
}

/// 会编译进 wasm 的 crate 源码目录。
fn wasm_facing_sources() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = Vec::new();
    for crate_name in ["yanshi-core", "yanshi-render", "yanshi-wasm"] {
        collect(
            &root.join("crates").join(crate_name).join("src"),
            &mut files,
        );
    }
    files
}

fn collect(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn host_only_time_apis_are_explicitly_accounted_for() {
    let root = repo_root();
    let allow = allowlist();
    let mut observed: Vec<(String, usize)> = Vec::new();
    for file in wasm_facing_sources() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let count = text.matches("Instant::now").count() + text.matches("SystemTime::now").count();
        if count > 0 {
            let relative = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            observed.push((relative, count));
        }
    }
    observed.sort();

    let mut expected: Vec<(String, usize)> = allow
        .iter()
        .map(|(path, count, _)| ((*path).to_owned(), *count))
        .collect();
    expected.sort();

    assert_eq!(
        observed, expected,
        "wasm 目标代码里出现了未登记的宿主时间 API 用法。\n\
         若确实需要，请确认它位于 `#[cfg(not(target_arch = \"wasm32\"))]` 分支，\n\
         然后在本测试的白名单里显式登记（并写明理由）；同时跑一遍 scripts/wasm-smoke.sh。"
    );

    for (path, count, reason) in allow {
        assert!(
            observed.contains(&(path.to_owned(), count)),
            "白名单条目已过期（{path} 期望 {count} 处）：{reason}"
        );
    }
}

/// 白名单里的每条理由都必须非空 —— 不允许"因为要过测试"这种无信息登记。
#[test]
fn allowlist_entries_carry_a_reason() {
    for (path, count, reason) in allowlist() {
        assert!(count > 0, "{path} 的次数应为正数");
        assert!(
            reason.len() >= 20,
            "{path} 的理由过短，无法说明为何安全：{reason:?}"
        );
    }
}

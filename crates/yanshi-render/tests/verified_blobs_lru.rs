// **★★★ 判据：`VERIFIED_BLOBS` 的**容量上限 ＋ LRU 淘汰** ✗ ★★★**（**第 693 轮 ✓）
//
// **∴ 背景 ✗**（**目标第 6 条 ✓）：
//   **∴ 要求 ✗**：**「**`VERIFIED_BLOBS` 加容量上限／LRU**（现在只增不减，长跑会缓慢增长）」** ✓
//     ＋ **∴ 而**第 693 轮查明 ✗**：**实现**早已完整**✗
//       ＋ **∴ `blob_verified` ⇒ `touch`**✗（**∴ 顺带触碰 ⇒ **∴ 不会**成为下一个被淘汰的 ✓）
//         ＋ **∴ `mark_blob_verified` ⇒ `insert(blob, max)`**✗（**∴ 超限淘汰**最久未用 ✓）
//           ＋ **∴ 默认上限 **4096**✗ ＋ `YANSHI_VERIFIED_BLOBS_MAX` 可覆盖 ✓
//             ⇒ **★ 但**判据**完全缺失**✗ ★**（**∴ grep 全仓库测试文件 ⇒ 无结果 ✓）
//               ⇒ **∴ 所以本文件补上**回归保护** ✓ ★
//
// **★★★ 变异检验（**必须能红 ✓）★★★**
//   **∴ 把 `insert` 里的**上限检查去掉**✗ ⇒ **∴ 长度**会无限增长 ⇒ **∴ 判据**必红** ✓
//     ＋ **∴ 把** LRU 换成** FIFO**✗（**∴ 不更新访问顺序 ✓）⇒ **∴ 第二个测试**必红** ✓ ★
//
// **∴ 注意 ✗**：**`VERIFIED_BLOBS` 是**全局状态**✗ ＋ **∴ 测试**默认并行** ✓
//   ＋ **∴ 所以**本文件的两个测试**合并成一个**✗（**∴ 避免**互相干扰 ✓）★
use yanshi_render::backend_note::{blob_verified, mark_blob_verified, verified_blobs_len};

#[test]
fn verified_blobs_respects_the_cap_and_evicts_lru() {
    // **∴ 用小上限 ⇒ 便于观测 ✓**（**∴ 环境变量**必须在**任何插入之前**设好 ✓）★
    std::env::set_var("YANSHI_VERIFIED_BLOBS_MAX", "2");

    // **∴ ① 上限生效 ✗**：**插 5 个 ⇒ **∴ 长度 ≤ 2** ✓
    for name in ["a", "b", "c", "d", "e"] {
        mark_blob_verified(name);
    }
    let len = verified_blobs_len();
    println!("  ★ 上限 2 下插入 5 个 ⇒ 当前长度 {len}");
    assert!(len <= 2, "上限为 2 时长度不应超过 2（实际 {len}）");

    // **∴ ② LRU 语义 ✗**：**最近的**两个**应当**还在**✗ 而**最早的**应当**已被淘汰** ✓ ★
    //   **∴ 上面插入顺序 ✗**：a b c d e ⇒ **∴ 若**是 LRU ⇒ **∴ 保留 d、e** ✓
    let d_ok = blob_verified("d");
    let e_ok = blob_verified("e");
    let a_ok = blob_verified("a");
    println!("  ★ LRU 检查：d={d_ok} e={e_ok} a={a_ok}（期望 d、e 真；a 假）");
    assert!(d_ok && e_ok, "最近插入的两个必须仍在（d={d_ok} e={e_ok}）");
    assert!(!a_ok, "最早插入的 a 应已被淘汰（实际 a={a_ok}）");

    // **∴ ③ 触碰会**更新**访问顺序 ✗**（**∴ 证明**真的是 LRU**✗ 而**不是 FIFO** ✓）★
    //   **∴ 现在**：d、e 在表内（**∴ 顺序**d 早于 e ✓）
    //     ＋ **∴ 触碰 d ⇒ **∴ 那**么**下一个被淘汰的**应当**是 e**✗ 而**不是 d** ✓
    assert!(blob_verified("d"), "d 触碰前应在表内");
    mark_blob_verified("f"); // **∴ 触发一次淘汰 ✓
    let d_after = blob_verified("d");
    let e_after = blob_verified("e");
    println!("  ★ 触碰 d 后插入 f：d={d_after} e={e_after}（期望 d 真；e 假 = 真 LRU）");
    assert!(d_after, "被触碰过的 d 不应被淘汰（实际 d={d_after}）");
    assert!(!e_after, "未被触碰的 e 应被淘汰（实际 e={e_after}）");
}

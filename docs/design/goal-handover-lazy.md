# 交接：图层合成解耦 ＋ 工程加载慢 ＋ 各环节 lazy（目标第 166 轮时点 ✓）

> **纪律（每轮照做 ✓）**：① **先测量再改** ✗（本会话 19 次靠实测／读码**改掉了自己的结论** ✓）；
> ② **判据先行 ＋ 变异检验** ✓；③ **探针一律写文件**（`/tmp/…-trace.log` ✓，**不用 `eprintln`** ✗）；
> ④ **插入代码按行号 ＋ 相邻行断言**（**不许凭记忆写锚点** ✗ —— 栽过 9 次 ✓）；
> ⑤ **门禁（fmt／clippy／`cargo test --workspace`／覆盖率守卫）全绿才提交** ✓（**已拦下 5 次** ✓）；
> ⑥ **英文提交 ＋ 每轮推送** ✓；⑦ **对照必须同材料** ✗（险些误判"倒退 64%" ✓）。

## 一、已完成的战果（**全部实测 ✓**）

| 场景 | 最初 ✗ | 现在 ✓ | 判据 |
|---|---|---|---|
| 打开 4K／8K | 1834／8935 ms | **0.055／0.066 ms** | `tool-open-fast` 家族 ✓ |
| 重复保存 4K／8K | 1280／9174 ms | **5.80／596 ms** | `tool-export-idempotent`（5 条 ✓）／`tool-thumbnail-on-demand`（4 条 ✓） |
| 导出结果缓存 | — | 4K **5.9 ms** | 第 409–412 轮 ✓（**键含 `head_seq`** ✓） |
| 缩略图按需 | — | 复用整幅 PNG 快路径 | 第 415–416 轮 ✓ |
| **位图缓存（8K 大图）** | 每次重复解压 **253 MiB** ✗ | **0 B** ✓ | `tool-bitmap-decode-scope` ①′（**已转绿并退役** ✓） |
| 8K 短笔（同材料 `bench_8k`） | 590 ms | **474 ms**（−20% ✓） | — |

## 二、当前靶子：**8K 首次解码一份整幅位图 ~333～432 ms** ✗

**证据链（第 441／445／447 轮 ✓）**：
* 8K 短笔 288 ms ⇒ **`preview_ms` 281.8** ✓ ⇒ 真身是 `Workspace::render_region(脏区)` ✓（路径**正确** ✓）；
* 1 个 64²（1 tile）区域：**112 ms（4K）／333～679 ms（8K）** ✗ ⇒ **与区域面积无关** ✗；
* **空角只要 1.99 ms** ✓ ⇒ **∴ 不是全局固定项** ✗ ⇒ **成本随"区域相交的位图"** ✓；
* **`missed_bytes`：渲 64² 而解 265 MB ＝ 16200× 区域像素** ✗✓（**8K 一份整幅明文 132.7 MB** ✓）；
* `blit_rgba8` **已只遍历相交行列** ✓（`buffer.rs:261` 自带注释 ✓）⇒ **不是 blit** ✗；
* **解码是单线程** ✗（`png.rs` `decode_png`／`zlib_decompress` 无 thread／rayon ✓）。

**为何不能简单加速** ✗：整体式 **deflate 流** ⇒ **取任意一段必须先解整条** ✓；
**多线程 inflate 无路** ✗（`flate2` ＋ `zlib-rs` 纯 Rust／wasm ✓；C 库在 wasm 不可行 ✓ —— 第 450 轮 ✓）。

## 三、∴ 解决方案 **(b1) 位图分块存储** —— **数据层已完成 ✓，只剩接线**

**已落地**（`crates/yanshi-render/src/bitmap_tiles.rs` ✓，**7 条判据 ＋ 3 次变异验证** ✓）：
| 项 | 内容 |
|---|---|
| `BITMAP_TILE = 256` ✓ | **与 `tile::DEFAULT_TILE_SIZE` 对齐** ✓（渲染 tile 与位图 tile 同尺寸 ✓） |
| `BitmapIndex { v, tile, width, height, tiles[] }` ✓ | **版本化 ✓**；`is_consistent()` **防"块表与尺寸不符"** ✓ |
| `tiles_for_rect` ✓ | **64² ⇒ 1 块** ✓（**变异验证 ✓**） |
| `assemble_region` ✓ | **由块拼区域 ≡ 整幅同位块（逐字节 ✓，变异验证 ✓）** |
| `split_into_tiles` ✓ | **行优先切 256² ✓，边缘块实际尺寸 ✓；往返恒等（变异验证 ✓）** |

**待接线（两步，机械活 ✓）**：
1. **导入侧** ✓ `crates/yanshi-server/src/tools.rs:4374 write_import_image`：
   `store.get(blob_hash)` ⇒ 解码 ⇒ `split_into_tiles` ⇒ 逐块 `store.put` ⇒ 写**索引 blob** ⇒
   payload 增 **`"tiles": <index_hash>`** ✓（**旧读方忽略未知字段 ⇒ 兼容自动成立** ✓）；
2. **渲染侧** ✓ `crates/yanshi-render/src/render.rs:1854 fetch_raster_patch`（调用点在 `:1315` ✓）：
   payload 有 `tiles` ⇒ 取索引 ⇒ **`tiles_for_rect(请求区域)`** ⇒ 只取／解这些块 ⇒ **`assemble_region`** ⇒ 填 `layer_buffer` ✓；
   **无 `tiles` ⇒ 原路整幅** ✓（**不许改变旧工程行为** ✗）。

**验收** ✓：**`tool-bitmap-decode-scope` ①（"解码字节 ≤ K × 区域像素 × 4"）应从 16200× 转绿到 ~30×** ✓
＋ ①′／② 仍绿 ✓ ＋ **937＋ 测试全绿** ✓ ＋ **复测同材料**（8K 短笔 474 ms → ?）✓。

## 四、判据与已知红

* **脚本判据 +10 条**；**Rust 判据 +7 条**（`bitmap_tiles` ✓）＋ 3 条（`layer_fingerprint` ✓）；
* **已知红 7 条**（`scripts/criteria-known-red.txt` ✓，**每条写明转绿条件** ✓）：
  `tool-impasto-plateau`／`kernel-brush-parity`／`tool-reference-overlay`／`tool-example-acceptance`／
  `browser-reference-overlay`／`tool-below-reuse`（**below 缓存未实现** ✓）／**`tool-bitmap-decode-scope` ①**（**分块存储 ✓**）。

## 五、其它待做（按价值）

| 优先 | 项 | 量 | 说明 |
|---|---|---|---|
| 1 | **(b1) 接线** | — | 本文件第三节 ✓ |
| 2 | **`create_layer` 8K** | **9779 ms／+869 MB** ✗ | **空层不应预分整幅** ⇒ 目标第 1 条（**判据："新建空层 ⇒ RSS 增幅 < 1 MB"** ✓） |
| 3 | **below 缓存（第 4 条）** | 合成 **531 ms** ✗ | 设计写实（`docs/design/lazy-composite-design.md` ✓）；**前置件（`layer_content_fingerprint` ＋ `below_reuse` 计数）已落地 ✓**；**C3 已登记已知红 ✓** |
| 4 | **首次保存／整幅渲染** | 4K **1389 ms**／8K ~3.5 s ✗ | 分段：填充 94 ＋ **图层 600** ＋ **合成 531** ＋ 裁剪 182 ＋ **量化 414（固有 ✓）** |
| 5 | 小缺陷（外部报告） | — | 缺陷 2 **已修** ✓；缺陷 3／4／5／7／8 待做（都小 ✓）；缺陷 6 **须两面实测** ✗ |

## 六、对外部报告（apple2011）的裁决 ✓

| 项 | 裁决 |
|---|---|
| **缺陷 1**（TileCache 容量 ⇒ 全量重光栅化 ⇒ 52×） | **现象为真** ✓（两套材料复现 ✓）；**归因错误** ✗（`raster_ms` 两边 **0.1 ms** ✓；其复现脚本**只做算术** ✗）；**建议（+191 MB）四次实测无支持** ✗ |
| **缺陷 2**（8K 位图超单份上限 ⇒ 不缓存） | **为真且已修** ✓（**其机制由我独立证实** ✓ —— **而"自适应预算"正是缺陷 1 的建议，放对了缓存 ✓**） |
| 报告文字 vs 交付文件 | **画幅不符** ✗（`arnolfini_4k` 称 3000×4000 ⇒ 实为 3000×2600 ✓）；**4K 一栏不是真画布** ✗（`create_layer` 0.211 ms／+80 KB ✓） |

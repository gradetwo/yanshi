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

## 三、∴ 解决方案 **(b1) 位图分块存储** —— ✅ **已全链落地（第 206 轮 ✓）**

> **状态** ✓：**数据层 8 判据 ＋ 4 变异 ✓｜导入侧 ✓｜字段贯通 ✓｜渲染侧分支 ✓｜收益判据实测通过 ✓**
> **实测收益** ✓：**64² 渲染取用 ≤ 512 KiB（1 块 ＝ 256 KiB）**，**整幅旧口径 1 MiB（8K 253 MiB）** ✗
> **尚缺** ✗：**新路的"区域 ≡ 整幅逐字节"专门测试** ＋ **端到端挂钟复测** ＋ **判据①（脚本，走旧工程）口径需注明** ✓

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
**接线点原文（第 167 轮读 ✓）**：
```rust
fn write_import_image(ctx, args) {
    let bitmap = require_object(args, "bitmap")?.clone();   // 含 blob_hash ＋ mime_type ＋ size
    let region = parse_bbox(…)?;
    let mut payload = json!({ …, "bitmap": bitmap, "width": region.w, "height": region.h });
    …                                                       // ← **切块 ＋ 索引 ＋ payload["tiles"] 插在这里**
    let result = ctx.commit(AtomKind::ImportImage, payload)?;
}
```
**阈值取舍（第 167 轮定 ✓）**：**只在 `max(width,height) ≥ 512` 时才切块** ✓
（**收益**：小图整幅解码本来就便宜，切块反而多花 blob＋索引开销 ✗；
**代价**：引入两条路径 ✓ ⇒ **由判据②"逐字节相同"守住 ✓**）。
**失败语义** ✓：**任一步失败 ⇒ 跳过** ✓ ⇒ **不写 `tiles` ⇒ 渲染走原路** ✓（**不阻断导入 ✗、不留半个索引 ✗**）。

1. **导入侧** ✓ `crates/yanshi-server/src/tools.rs:4374 write_import_image`：
   `store.get(blob_hash)` ⇒ 解码 ⇒ `split_into_tiles` ⇒ 逐块 `store.put` ⇒ 写**索引 blob** ⇒
   payload 增 **`"tiles": <index_hash>`** ✓（**旧读方忽略未知字段 ⇒ 兼容自动成立** ✓）；
2. **渲染侧** ✓ —— **⚠️ 不要改 `fetch_raster_patch`** ✗：它返回**整幅** RGBA8（`-> Result<Option<(u32,u32,Vec<u8>)>>` ✓），
   调用方**整块 blit** ✗ ⇒ **∴ 分块路接在**调用侧**（`render.rs:1315` 一带 ✓）**，把既有几行包进 `else` ✓：
   ```rust
   if let Some(idx_hash) = object.data.get("tiles") {      // 字段读法同 object.data.get("params")（:944 ✓）
       // 取索引 blob ⇒ 解析 BitmapIndex（**版本不符 ⇒ 落回 else** ✓）
       // ⇒ tiles_for_rect(本块请求区域) ⇒ 只取这些块 ⇒ assemble_region ⇒ 得**该区域**像素
       // ⇒ blit 到**区域坐标**（⚠️ **不是整块 `offset`** ✗）
   } else {
       let entry = bitmaps.get_or_decode(…)?;              // **原路整幅，一字不动** ✓（旧工程零改动 ✓）
       layer_buffer.blit_rgba8(offset…, entry.0, entry.1, &entry.2, opacity);
   }
   ```
   **三条不许踩错** ✓：① **失败／缺块 ⇒ 落回 `else`** ✓（**绝不画半个 ✗**）；② **区域坐标** ✗（不是 `offset` ✓）；
   ③ **`else` 分支原封不动** ✓。

   原计划描述（仍是这一步 ✓）：2. **渲染侧** ✓ `crates/yanshi-render/src/render.rs:1854 fetch_raster_patch`（调用点在 `:1315` ✓）：
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

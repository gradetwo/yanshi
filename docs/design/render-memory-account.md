# 整幅渲染的内存账目（DEF-03 / 8K OOM 的根因调查）

日期：2026-10-15
基线：`c45e4b30`（main）
判据：`scripts/tool-render-memory-budget.mjs`（**当前已知红**，见 `scripts/criteria-known-red.txt`）

## 一、已经被实测钉死的账目

判据量的是「4K ＋ 4 层真实素材，**整幅渲染那一步的 RSS 增量**」（数据取自 `GET /health` 的
`rss_bytes`，所以不必新增 API、也不必读 `/proc`）。

| 场景 | 实测 | 说明 |
|---|---|---|
| 空载 ／ 4K 空文档（未渲染） | 4 MB ／ **5 MB** | 图层像素是**懒分配**的 |
| 导入 4 层小素材（900×640 ≈ 2.3 MB/张） | 151 MB | 每张 ≈ 50 MB（解码后的 tile 存储） |
| **4K ＋ 4 个空层 ⇒ 整幅渲染** | 72 ⇒ **1032 MB**（增量 ≈ **960 MB**） | ≈ 4 × 240 MB ⇒ **每层一个全幅 f32 工作缓冲** |
| 单层 4K 空文档 ⇒ 整幅渲染 | +52 MB | 说明渲染路**本身**是分块的 |
| 判据实测（4 层真实素材） | 增量 **895–909 MB** | 稳定复现 |

**结论**：这笔账与**图层数**成正比、与渲染面积的关系弱 ⇒ **每层一个全幅（或按 chunks 分带）的
f32 工作缓冲，且多层同时在用**。

## 二、代码位置（已读）

* 生产路径：`render_region`（`crates/yanshi-render/src/render.rs:1056` 起）
  * 当 `workers > 1 && region_pixels >= PARALLEL_MIN_PIXELS` ⇒ 走
    `parallel_impl::render_region(...)`（**按行分块**）
  * 之后 `self.buffer_pool.begin_render_with(chunks, 2)`（`render.rs:4058`）
    ⇒ **池的并发因子 = chunks × 2**
* 合成循环：`for layer in state.alive_layers()`（`render.rs:2058` 起），每层
  `self.buffer_pool.acquire(origin_x, origin_y, width, height)`（`render.rs:2099`）
* 池的预算公式（`buffer_pool.rs`）：`max(MIN_POOLED_BYTES=192 MiB,
  并发 × 每并发块数 × 本轮最大单块)`，**已加**绝对上限 `MAX_RETAINED_BYTES = 384 MiB`
* 缓冲区大小：`Buffer::new` 里是 `vec![0.0; w*h*4]`（**f32** ⇒ 4096² 一张就是 **268 MB**）

## 三、可调的旋钮（按"收益/风险"排序）

1. **`chunks`（并发分块数）** —— 直接影响同时在用的块数（＝ `chunks × 2`）。
   现在由 `available_parallelism()` 一类的并行度决定，**没有按内存预算夹住**。
   ⇒ 建议：按「单块字节 × 2 × chunks ≤ 预算」反推 `chunks`（大画布自动降并发）。
   **风险**：4K 全幅渲染的**墙钟**会变慢（分块少了）⇒ **必须两本账量**（墙钟 ＋ CPU，
   中位数与极差），别只看内存。
2. **工作缓冲从 f32 降到 f16** —— 仓库已有 `crates/yanshi-render/src/half.rs`。
   单块 268 MB ⇒ 134 MB。**风险**：会改变渲染的数值 ⇒ 现有像素级判据可能红，
   属于"精度换内存"的取舍，需要先量清楚对画质的影响。
3. **真正的分带合成** —— 让合成**一条带一条带**地做（`begin_render(1)`，
   每带合成完立刻释放），峰值 ≈ 一条带。**风险**：改动面最大，且要确认
   src-over 的可结合性（它可结合 ✓）与图层缓存的交互（读画布类笔刷已禁用缓存 ✓）。
4. **`MAX_RETAINED_BYTES`（已加 384 MiB）** —— 只影响"归还后保留多少"，
   **对这笔"同时在用"的账没有帮助**（实测 909 ⇒ 895 MB，属噪声）⇒ 保留它是为了**有界性**，
   **不要把它当成这笔账的解**。

## 四、下一步（建议按这个顺序，且每步都要两本账）

1. 先做**旋钮 1**（按内存夹 `chunks`）——改动小、方向明确、可回退；
   用 `tool-render-memory-budget.mjs` 看内存、用 `tool-cpu-baseline-real.mjs` 看墙钟/CPU，
   并在**同一台机器**上取中位数与极差（本仓既有做法）；
2. 若还不够，再评估**旋钮 2（f16）**，但那要先量对画质的影响；
3. **旋钮 3** 留到前两步都不够时再动。

## 五、已撤回的说法（别再引用）

「`import_image` 一张 900×640 小图吃 **1.7 GB**」—— **复现不出来**（干净重测是 512² 文档
**+33 MB**、4K 文档 **+50 MB**）。那条 968 MB 的读数是**不可复现**的，已撤回。

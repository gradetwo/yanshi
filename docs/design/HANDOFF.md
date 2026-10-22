# YANSHI 交接摘要（**第 356 轮重写**；旧版只到第 275 轮 ⇒ 已整份替换）

> **∴ 只写「怎么查」，不写「查到的数字」** ✓（内核 ②：数字会过期 ✓）
> ⇒ 需要数字时**跑命令** ✓；确实没有公式的（例如外部实测的结论），**带时间戳**写在台账里 ✓。
>
> **权威台账** = `docs/design/brush-capability-gap.md`（按节追加；本文件只做索引与下一步）。

## 一、当前状态（全部用命令，不写死值）

```bash
git rev-parse --short HEAD && git rev-parse main l4-sync   # 三个应相同
git status --porcelain                                     # 应为空
gh run list --limit 1 --json headSha,status,conclusion      # headSha 应等于 HEAD
ls scripts/{tool,browser,mcp,serve,check}-*.mjs | wc -l     # 判据「CI 口径」条数
ls scripts/{tool,mcp}-*.mjs | wc -l                         # 判据「自检口径」条数
wc -l < scripts/criteria-known-red.txt                     # 已知红条数（理由在 AGENTS.md）
```

**本段交接时的在飞项**：`texture.jitter` 那个提交的 CI run 刚派发（`gh run list --limit 1` 自查结果）。

## 二、最近这一段做了什么（按提交信息可查：`git log --oneline -12`）

| 事 | 证据／入口 |
|---|---|
| 纹理填充：`texture={type:bitmap,…}` **端到端接通** ＋ **接受 PNG** ＋ **写文档前就校验 blob**（尺寸/MIME 双数报错） | `crates/yanshi-server/tests/fill_region_texture.rs`（8 条）｜台账「三百六十一」「三百六十九」 |
| **`texture.jitter`**（防「墙纸重复」，外部第 76 次要的就是它） | 同上测试文件（jitter=0 逐字节平铺 / jitter=1 周期被打散 / 1.5 报错）｜台账「三百七十四」 |
| `fill_region` 收 **`shape.kind` 别名** ＋ **`path` 形状**（`d`/`subpaths` 都行，`d` 单独给也能推断） | 同上｜台账「三百六十九」结尾 |
| `create_mask{from_selection}` 被自己 schema 拦死 ⇒ 已修 | `tests/create_mask_from_selection.rs`｜台账「三百六十三」 |
| 反选选区裁剪（外部 `B2`）：**复核不复现** ＋ 补判据 | `tests/selection_invert_clip.rs`｜台账「三百六十五」 |
| 零像素笔触文案（外部 `B3`）：结论句跟真实原因走 ＋ 去掉重复开头句 | `tests/zero_ink_advice.rs`｜台账「三百七十一」 |
| `blob_gc` 内存闸（外部 `B4`）：**本来就有闸，缺判据** ⇒ 上限改成 `YANSHI_GC_MEMORY_BUDGET_MB`（缺省 1536）＋ 判据 | `tests/blob_gc_budget.rs`｜台账「三百七十」 |
| `update_layer` 收 **`z_index`**（外部 `bug_report.md` BUG-20，ROI 第 1 条） | `tests/layer_z_index.rs`｜台账「三百七十二」 |
| `scatter_strokes.area.points` 写进声明（同报告 BUG-04 第 1 条；他们说的「会被拒绝」**实测不成立**） | `tools_registry.rs` 的 `area` 参数｜台账「三百七十二」 |
| 双渲染器一致性脚本一直**空转**（写死 400×300，CI 起 320×240 ⇒ 0 条比对）⇒ 改成问 `describe_canvas` | `scripts/kernel-brush-parity.mjs`｜台账「三百六十七」 |
| `raster_patch_fixed_cost` 的 CI 假红（163%）⇒ 比值改成**按轮配对** | `crates/yanshi-render/src/bitmap_tiles.rs`｜台账「三百七十三」 |
| 纹理填充的 **JS 判据**（CI 分片里会跑） | `scripts/tool-fill-texture.mjs` |

## 三、下一步（**按优先级**；每条都写了「怎么算完成」）

1. **apple2011 的 jitter 复测**：`docs/FOR-APPLE2011.md` 末尾已回信（写明 `jitter` 0.2/0.3/0.5 怎么试）。
   等他们下一次巡检回来；**每轮第一件事**是比对 `YANSHI_ISSUES.md` 的 md5（见第四节仪式）。
2. **`bug_report.md` 剩下的 ROI 项**（`~/patch/bug_report.md` §3.2）：
   - `BUG-12`：`get_document` 的 `preview_size` 只有 64/128/256 ⇒ 支持任意分辨率或加「低开销区域预览」。
   - `BUG-15 残留`：`validate_args` 不下沉到嵌套 `data.*`（`additionalProperties:true`）⇒ 嵌套键写错仍静默忽略。
   - `BUG-19 残留`：`open_document` 时做一次 blob 引用完整性扫描并回报缺失清单。
   - `BUG-04 残留`：`scatter_strokes` **没接 `clip_to_selection`**；散点**半径可越出多边形**。
   - `BUG-01`：把「当前原子数／预计 fold 成本」暴露到文档元数据（他们要在超阈值前主动切分）。
3. **parity 吃底图那半**：机制未隔离（台账「三百六十七（续）」）。下一步：把服务端的**第二遍掩膜落墨**
   与门面的单遍对齐再比（本机取证：region 底图 2652 字节 vs 全画布底图 6564 字节 ⇒ 不是"底图裁小了"）。
4. **`bitmap` 选区的裁剪覆盖度仍 0.0**（台账里挂着）：`ensure_bitmap_masks_registered` 在 `bitmap_coverage`
   里没被调用 ⇒ 下一步加 stderr 追踪，看注册表里到底有没有那个 key。
5. **`export_png` 的错误体仍只有 `blob_hash`**：纹理那条路已经到不了它（早校验 ✓），但**别的**解码失败仍只报哈希。

**完成标准（不许放宽）**：代码改了 ＋ 判据红了又绿 ＋ 变异红过 ＋ CI 绿（红都在 `scripts/criteria-known-red.txt` 里）。

## 四、和 `apple2011` 的固定仪式（**每轮第一件事**）

```bash
scp -o BatchMode=yes apple2011:/home/crow/work/art/YANSHI_ISSUES.md /tmp/art/issues_latest.md
md5sum /tmp/art/issues_latest.md /tmp/art/issues_seen.md     # 相同 ⇒ 只回一句「未变」
# 变了 ⇒ 读新增节（diff 标题）⇒ 分三类：我的 bug（判据＋变异立刻修）／引擎真问题（复现或写清缺什么）／误判（附证据）
cp /tmp/art/issues_latest.md /tmp/art/issues_seen.md
```

回信写在 `docs/FOR-APPLE2011.md`（他们按 §分节引用）。**不许说「不存在」** ⇒ 要写构型差异与「我还缺什么才能复现」。

## 五、流程陷阱（本场血债，**必须先读**）

1. **本机跑判据**一律 `bash scripts/run-criteria-local.sh scripts/tool-xxx.mjs`
   （验二进制新鲜度 → 自动重建 → 起临时服务 → `trap` 清理）。**别手搓**、别复用旧 `target/release`。
2. **push 走 HTTPS**：仓库的 `origin` 是 SSH，而**本场 SSH 22 端口一直 banner 超时** ⇒ 用
   `git push "https://x-access-token:$(gh auth token)@github.com/gradetwo/yanshi.git" main`
   然后 `git update-ref refs/remotes/origin/main "$(git rev-parse HEAD)"`（本地 ref 才跟得上）。
3. **CI 卡住 ≠ 判据挂住**：若某个 run 的 `in_progress` 超过半小时、且日志接口回 `BlobNotFound`
   （`curl -sL -H "Authorization: Bearer $(gh auth token)" .../jobs/<id>/logs`）⇒ **runner 丢作业** ⇒ 取消它，别查产品。
4. **`criteria` 各分片快慢差很大**（含 `browser-*` 的分片要串 7 条 chromium 判据）⇒ 20+ 分钟是正常。
   **认判据名，不认分片号**（分片号会随判据数重排）。
5. **本机内存 3.8 GiB**：`cargo test --workspace`／`clippy --workspace` 尽量交给 CI；
   本机只跑单 crate ＋ 过滤的测试、`node scripts/*.mjs`、`cargo build --release -p yanshi-http`。
6. **别用 `pkill -f "yanshi-serve --bind …"`**（会杀掉自己的 shell）⇒ 用 `fuser -k <port>/tcp`。
7. **采样点先算边界**：本场两次踩（一次是矩形，一次是**画布 64×64**）⇒ `sample_colors` 对画布外的点回
   `outside:true`／`rgba:null` ⇒ 断言会拿到 `null` 而不是"没画"。

## 六、外部报告的当前分级（据 `bug_report.md`，22 项旧 BUG 在最新 commit 上）

| 级别 | 项 |
|---|---|
| **已修，不再收录** | `BUG-02`、`03`、`13`、`16`、`17`、`18` |
| **仍完全存在** | `BUG-12`（缩略图分辨率）、`BUG-20`（**已在本场修掉 `z_index` 那一半** ⇒ 现在只剩「层级参数分裂」的文档口径） |
| **部分存在**（本场已动） | `BUG-04`（声明已补 `points`；还差 `clip_to_selection` 与半径越界）、`BUG-06`（本体不动，`get_histogram` 已够用） |
| **部分存在**（未动） | `BUG-01`、`05`、`07`、`08`、`09`、`10`、`11`、`14`、`15`、`19`、`21`、`22` |

## 七、这份文件自己会过期

**⇒ 别信这里的数字**，只信第一节的命令 ✓；**结论与新发现一律追加到台账** ✓（`docs/design/brush-capability-gap.md`）。

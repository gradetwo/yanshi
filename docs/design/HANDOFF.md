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
| `scatter_strokes` 接上 **`clip_to_selection`**（同报告 BUG-04 残留第 3 条） | `tests/scatter_clip_to_selection.rs`｜台账「三百七十五」 |
| 散点**整块**（两端笔尖方框）留在多边形内 ＋ `skipped_outside` 计数 | 同文件第 2 条｜台账「三百七十七」 |
| **位图选区**能裁剪了（`parse_geom` 认 `blob_hash`）＋ 蒙版多格式解码 ＋ 解不开时**响亮报错** | `tests/bitmap_selection_clip.rs`｜台账「三百七十八」 |
| 位图选区的 **`area`** 报真实像素数（同族第三处） | 同文件第 2 条｜台账「三百七十八（续）」 |
| **打开文档时报缺失 blob**（`blob_integrity`） | `tests/open_blob_integrity.rs`｜台账「三百八十」 |
| **`texture.type:"region"`**（就地拍成位图＝从画布取纹理） | `tests/fill_region_texture.rs` 最后一条｜台账「三百八十一」 |
| `texture.tiles` ＋ **平铺过多告警**（把外部第 78 次的选源原则做进工具） | 同文件第 9 条｜台账「三百七十九」 |
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
   - `BUG-04 残留`：`scatter_strokes` 的 `clip_to_selection` **已接上**（判据 `tests/scatter_clip_to_selection.rs`）；**还差**散点半径可越出多边形（逐像素遮罩）。
   - `BUG-01`：把「当前原子数／预计 fold 成本」暴露到文档元数据（他们要在超阈值前主动切分）。
3. **parity 吃底图那半**：机制未隔离（台账「三百六十七（续）」）。下一步：把服务端的**第二遍掩膜落墨**
   与门面的单遍对齐再比（本机取证：region 底图 2652 字节 vs 全画布底图 6564 字节 ⇒ 不是"底图裁小了"）。
4. **`bitmap` 选区的裁剪覆盖度仍 0.0**（台账里挂着）：`ensure_bitmap_masks_registered` 在 `bitmap_coverage`
   里没被调用 ⇒ 下一步加 stderr 追踪，看注册表里到底有没有那个 key。
5. **`export_png` 的错误体仍只有 `blob_hash`**：纹理那条路已经到不了它（早校验 ✓），但**别的**解码失败仍只报哈希。

**完成标准（不许放宽）**：代码改了 ＋ 判据红了又绿 ＋ 变异红过 ＋ CI 绿（红都在 `scripts/criteria-known-red.txt` 里）。

## 四、和 `apple2011` 的固定仪式（**每轮第一件事**）

```bash
# 基线**不许放 /tmp** ✗（2026-10-22 下午 /tmp 被清理 ⇒ 基线连同 md5 一起没了，白丢一轮 ✓）
BASE=~/.local/share/yanshi-external
scp -o BatchMode=yes apple2011:/home/crow/work/art/YANSHI_ISSUES.md "$BASE/issues_latest.md"
md5sum "$BASE/issues_latest.md" "$BASE/issues_seen.md"      # 相同 ⇒ 只回一句「未变」
# 变了 ⇒ 读新增节（diff 标题）⇒ 分三类：我的 bug（判据＋变异立刻修）／引擎真问题（复现或写清缺什么）／误判（附证据）
cp "$BASE/issues_latest.md" "$BASE/issues_seen.md"
# 绝对路径拉不到时，`$BASE/state.json` 里存着最近一次读到的 md5（可比对、也可据此判断"到底变没变"）
```

**⚠ 网络**：2026-10-22 下午起，本机到 `apple2011` 的 **22 端口**一直 `No route to host` / `Connection closed`
（`ping` 通、GitHub 的 SSH 同样超时；**HTTPS 正常**）⇒ 拉不到就**如实记「这一轮没拉到」**，
**不许**把"没变"当成默认结论 ✓。

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

## 八、最近这几轮补的能力（**纹理合成线**，接手先读这条）

- `texture.mode:"patch"` **已实现**（合成：左→右/上→下填块、每块 K 候选取重叠区最小 SSD、重叠区**线性过渡**）；
  `quilt_patches` 用**行号定位**接线（字符串锚点会被 `cargo fmt` 重排 ⇒ 失败过两次）。
- `texture.seam:"min_error"` **按行最小误差硬切**（带平滑约束的 DP **还没做**）。
- `texture.layer_id`：`region` 取纹理**只拍那一层**（缺省拍整幅合成）。
- `texture.tiles` ＋ 两条 warning：**平铺次数 ≥12**（特征会被复制）、**候选池 <64**（棋盘格/人形）。
- 判据全在 `crates/yanshi-server/tests/fill_region_texture.rs`（**17 条**，每条都有变异记录）。
- **下一轮**：① DP 平滑切缝；② 外部提的两条更硬判据（**自相关无峰**、**形状泄漏**）。

## 九、**推送成功的判据**（第 417 轮的坑）

**⇒ 事实**：`git push … | tail -1` 那天**连不上 github**（`Failed to connect … after 133 s`）⇒ 推送失败 ✗，
而我随后跑了 `git update-ref refs/remotes/origin/main HEAD` ⇒ **本地 ref 显示"已同步"** ⇒ **假绿** ✗
（`git status -sb` 显示 `## main...origin/main` 没有 ahead/behind ⇒ 看起来推上去了 ✓ 实际没有 ✗）。

**⇒ 规矩**：push 之后**必须**用远端说话，不许用本地 ref：
```bash
git ls-remote "<url>" refs/heads/main | cut -c1-12   # 应等于 git rev-parse --short HEAD
```
（`update-ref` 只用来让本地 `origin/main` 跟上，**不能当推送证据** ✓。）

## 十、**下一步**（2026-10-23 更新，含实测与判据）

**本轮已完成（等 CI：`457b1351` 的 run 一直 `queued` —— GitHub 无 runner，不是仓库问题）**：
- **真 bug 修掉**：`color_mixing` 一直被 `write_brush_stroke` 读、**spec 从未声明** ⇒ `validate_args` 拒收 ⇒
  **API 根本调不到**（第 562 轮修，`tools_registry.rs:2184`）⇒ 那条端到端判据从 `exit(3)`（不可判）
  变成 `exit(0)` ✓，并补了 `linear` 的三条断言 ✓（变异：Linear 当 Rgb ⇒ `rgb(100,100,155)` ⇒ exit 1 ✓）。
- **实测**（同一条黄→蓝渐变的中段色）：`rgb` **rgb(100,100,155)**｜`linear` **rgb(168,168,205)**（亮 **68 级**）｜
  `pigment` **rgb(0,84,105)**（分离度 56 → **105**）。
- 文档三处补齐（两份 guide ＋ `docs/tools.md`）。

**色彩体系 Phase 1（用户裁定的最高优先级）现在只剩一件事**：
把线性的混色接到**笔刷管线**（`vendor/hokusai-core/src/stroke.rs:1128` 的 `hsv_to_rgb` 之后一路 sRGB 混合），
而不是只做 `color_to` 渐变那一段 ✓。**代价与收益都已知**：中段会亮 **68 级**（≈27% 满量程），
且**所有笔触的重放结果都会变**（用户已拍板接受 ✓）⇒ 需要按规划 ④ **重标定像素类判据** ✓。
**建议做法**：先在 `color_mixing` 默认值上**保持 `rgb`**（不动老文档 ✓），把"默认也线性"当**独立决策** ✓。

**已作废（别再照着做）**：`batch` 合并渲染、`silent` 省预览、`defer_preview`（会破坏 `wait_for_render` 调度契约）；
"用正则做参数面机检"（假红 38 处，见台账「四百一十」）。

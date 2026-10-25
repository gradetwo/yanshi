# 工作交接（截至 2026-10-25 16:05 Asia/Shanghai）

**仓库**：`/home/crow/yanshi`（分支 `main`）｜**HEAD**：`ca58be70`｜推送：`git push git@github.com:gradetwo/yanshi.git main`

## 一、每轮第一件事：比两份外部报告

| 报告 | 远端路径 | 本地基线 | 比法 |
|---|---|---|---|
| `art`（apple2011）| `apple2011:/home/crow/work/art/YANSHI_ISSUES.md` | `~/.local/share/yanshi-external/issues_seen.md` | `scp` 下来后 `md5sum` **两侧都打印**，相同才说「未变」 |
| `qoder` | `apple2011:~/work/qoder/yanshi_issue.md` | `~/.local/share/yanshi-external/qoder_issue_seen.md` | 同上 |

**⇒ 2026-10-25 16:00 实测（两侧 md5 相同 ✓，基线已更新）**：
- `art`：`42a993d78aafabfd279b9db47e3da1c3`（740,964 B）⇒ **只多了一条定时检查**（他复述 `c7739c89`／`2e70615b`／`1788080d`，无新缺口）。
- `qoder`：`7dcd42c3a6e79e94eabc15b94b8e0bcb`（141,575 B）⇒ **未变**。

**⇒ 规矩（我犯过）**：报「未变」**必须附两侧 md5**。此前 `qoder` 根本没被真正比对过（旧基线是 33 KB 的**截断抓取**），我报了十几轮"未变"，全是**断言恒真**。

## 二、当前状态

```
CI：35656ee9 **已推，待跑**（三条外部期 1 补丁线合并；见 §二-7 与台账 §622）
    784362cc **success ✓**（run 40183983484；12 个可跑 job 全 success，brush parity skipped）
    ⇒ 这一跑验过：`browser-project-download`／`brush-panel`／`layout`／`i18n` 全 ✓（前三条已移出 known-red）
      ＋ 新判据 `check-blank-overwrite-guard` ✓（shard 1：通过 57｜意外失败 0｜已知红 3｜跳过 3）
    dbe1160b failure ⇒ 唯一意外失败是 `browser-i18n`（运行时 `<option>` 7 条中文）⇒ 784362cc 修
    bf0c6e5a success ✓（D172 闭环 ＋ `HANDOFF.md` 白名单）
    2e70615b success ✓（dab ×3 注释；**补丁线收工**）
```

**已落地（都过了 CI 或本机验证）**：
1. **0.3.0 发布完整**：版本号、注解标签 `v0.3.0`、5 个资产（4 平台包 ＋ `SHA256SUMS`）。三层验证：哈希重下载一致 → 包内 509 条目 → 解包执行 `--version`。
2. **发布流水线缺陷已修**：`checksums` 作业没 checkout 却跑 `gh` ⇒ 加 `--repo`（`9ddb92d3`）＋ 判据 `check-release-workflow-gh-repo`（变异可红、已进 `pre-commit`）。
3. **外部补丁包 F ＋ H 已合并**（`61a7e64a`，CI success）：F 的资产根解析一次（仓库外启动不再 404）；H 的退化点改闭合等弧长环（32×61 → 46×46），相位哈希改 `to_bits()` 混列。A2 未合（作者自述"未证明不伤 below 缓存"）。
4. **D172 完整闭环**（`ca58be70`，本轮）：
   - 完整包参数**早就有**，名字是 **`include_bitmaps`**（交接上一版写的 `include_blobs` 是错的；`tools_registry.rs:1971`、`service.rs:3219`）。
   - `yanshi-timelapse`：缺**必需** blob ⇒ **`exit(2)`**（不再产出一条"看起来正常的空影片"）；**`tiles` 解码索引缺失不致命**（渲染侧会退回整幅，`render.rs:4350`）。
   - `real_package_shrinks`：导出传 `include_bitmaps: true`，断 `omitted == 0`、`blob_count > 0`、回放侧「缺 blob 0 个」。
   - 新判据 `a_missing_tile_index_alone_is_not_fatal`：只缺 tiles ⇒ `rc=0`，但 stderr 仍要看见「N 个 tiles 索引缺失」（豁免 ≠ 静默）。
   - 三条变异都实测过（见 §五-3）。
5. **判据补强（前几轮）**：台账引用 430 处（棘轮 333→0）、注释里的断言不算、临时目录守卫必须被绑定、示例 205→203＋静态生成、`omitted` 可见判据、D12 knife 判据。
6. **第 1131 轮（本轮，未推）**：
   - **D188 量级**写进 `yanshi-timelapse` 降采样警告（「差随原子数增长：200 图块 ≈0.6 lv/通道 ⇒ 22,926 原子 ≈16 lv」）＋ 断言；变异「随规模变化」⇒ 红 ✓。
   - **四条浏览器 known-red 归因**：`browser-project-download`／`browser-brush-panel`／`browser-i18n` 是 `run-criteria.sh` 的**一刀切 `render=client`** 把它们换成了"本地内核 + 服务端资产"这种不存在的组合 ⇒ 加 `RENDER_PARAM` 例外；**CI 实测**：`project-download ✓`（shard 1）、`brush-panel ✓`（shard 3）、`layout ✓`（shard 3）。**而 `i18n` 本机是假绿** ✗（CI shard 1 报 7 条中文：`正常/正片叠底/滤色/叠加/变暗/变亮`）⇒ 那是**运行时**建的 `<option>`（`modeLabels`，`viewer-app.js:5740`），本机那次还没建出来 ⇒ 已加 7 条 `I18N_EN_TEXT` 词条，CDP 探 DOM 复核为 `Normal/Multiply/Screen/Overlay/Darken/Lighten/Add` ✓。
   - **`browser-layout` 判据改成结构量**：旧断「400% 溢出舞台 > 10px」量的是**视口宽**（CI 854px）✗；产品机制是**视口平移**，不是 `.stage` 滚动条 ⇒ 改断 `state.zoom ≈ 4` ＋ `viewport.w < 文档宽`（适配时 == 文档宽）○ 本机 client 模式通过 ✓。
   - **known-red 18 → 14**（上面四条移出，理由写在名单里）。
   - **Y-01 复查＋新判据**：`viewer-app.js:2245` 的 `skipped-blank-over-ink` 已按**内容**挡住"全透明覆盖"⇒ 审计说的"新序号空帧"已覆盖；残余只是"`knownSeq` 自身可能旧"（记在台账 §621-四）。另加 **`scripts/check-blank-overwrite-guard.mjs`**：守「守卫在、且先判后落帧、且跳过有计数、行为侧判据仍在读」；变异（`if (false)`）⇒ 红 ✓、复原 ⇒ 绿 ✓；自测 5 例 ✓。

7. **三条外部期 1 补丁线已合并**（`35656ee9`；台账 §622 有逐项账）：
   - **paint 期 1**（7 patch：A1–A6／B1–B6／C1–C3，14 条新判据）；
   - **photo 期 1**（1 patch：上传上限 64→256MiB、lensfun 6→12 支 ＋ `lens_match` 回显、裁剪量回显、C5-8/C5-9）；
   - **photo G08 notes**（1 patch：`japanese_airy` notes 一句话）。
   **整合三处**：`tool-ink-mass` 补 exit(3)；CI 补 `cargo build --release -p yanshi-mcp`（否则三条 MCP 判据静默跳过）；
   CI 补 stable-only 的 `raw` 测试步（否则 `raw_import.rs` 编成空文件）。
   本机：workspace clippy ＋ fmt 绿；server lib 186、定向 32、raw/render/http(86)/mcp 全过；**14 条新判据全 RC=0**。
   **全量 workspace 测试留 CI**（提交信息已写）。
   **⇒ 首跑 CI 红了 6 条，已全部修**（不是补丁写错，是衔接；逐条见台账 §622-补）：
   `tool-swatch`（blending 是涂抹类）、`tool-brush-aliases`（同类，另需**有变化**的底图）、
   `tool-example-acceptance`（示例用了 Blender）、`tool-export-missing-body`（导出目录 env 与服务端沙箱不同）、
   `tool-engine-capabilities`（`medium_params` 少 `color_space`）、`tool-fit-background-field`（`created`/`opened` 是框架字段）。
   顺带补了 swatch 的 `background`/`read_canvas` 透传（错误消息早就建议"先铺底再试"，但原来没有入口 ✗）。

8. **照片线期 1 扩展（0002）＋ 期 3a 已合并**（`7fc8e658`；台账 §623）：
   - 期 1 扩展 8 项（Y-09/Y-06/G-09/Y-04/Y-05/Y-01/Y-07/**G-03 `Object.enabled` bypass**）；
   - 期 3a 7 项（高光护栏面积感知、`T_mid 0.09`、`DEHAZE_MAX 0.15`、暖调豁免、stats 回显）；
   - 整合三处：`push_response_warning` 默认 feature 死代码（加 `cfg(raw)`）、期 3a 文件 rustfmt、
     `browser-i18n` 的 WASM 状态三条词条（`已按设置跳过…`／`不可用`／`版本不匹配`）。
   - **证据缺口（如实记）**：期 3a 说明的「测试数字／回测对比／重放结果」三节仍是 `TODO` 占位 ⇒
     代码测试我跑了（189＋定向＋clippy 全绿），但**标定/回测表没有** ⇒ 要他们补，别把口述当实测。

9. **照片线期 2 已合并**（台账 §625）：A1 levels 显示口径（6 套预设翻回显示尺度）、
   A2 shadows_highlights 亮度比回写（保色度）、Y-02 MakerNote 方向交叉校验（冲突只进 notes）。
   本机：render filter 106／style_preset_package 5／raw decode 14／server levels_ 2 全过；clippy/fmt 绿。
   **已知须上游裁决**：auto backtest 触点 3（colorcast）R−B 只降 28.6% < 40% 门限（A2 保住色度的必然）
   ⇒ 门限重标或接受新行为；22 套预设目检在包里**未勾选**（我本机没有那 6 张 ARW 夹具）。
10. **Web 审计包已合并**（台账 §626）：15 个工具入口 + 参考图卡/任意尺寸导出/批量建议 + MIME 嗅探等。
   本机 CDP 验证 **ALL_STEPS_PASS**（把作者脚本里写死的 `/home/polar/...` 夹具换成仓内 PNG；
   每次用**全新 doc**，否则前置状态被消耗）；browser-i18n / ui-i18n / tool-surface-coverage 全绿。
   顺带修了 census 的 `saturation`/`amount` 分类（CI shard 2 第一次红的原因）。
11. **vector-tracing v3 已合**（完整包，台账 §627）：7 个 patch 从本仓 v1 直达 v3（旧差量包作废）；
   结果 blob `505da95` 与 `合并.diff` 逐字一致，`check-skill-tool-names` 73 处全过。
12. **照片线 G-09 补充件已合**（台账 §627）：hsl 校验域放宽到 hue ±360／sat 0..8（含 `clamp_params`
   镜像同幅放宽）；`selective_color`/hsl 嵌套键名与同名不同义写进 schema 文案。
   本机 hsl 3／preset_expander 13／presence 5 全过，clippy/fmt 绿，census（225 名）绿。
13. **期 3a 回填版**：作者重跑确认测试/回测/重放数字逐数复现；唯一修正「fmt 干净」不实 ⇒
   与本仓 `7fc8e658` 的 rustfmt 正好对上（台账 §627 末）。

14. **画画线期 2 已合**（台账 628）：`verify_package`、list_atoms 5000/total 修正、gradient 数组色、
    hex alpha 告警、batch 缺省静默/上限/水位、smart 前置、calligraphy 小 size、频率分离查表+并行（4.25×）。
   本机：server lib 203、20 个定向 suite、render 59、hokusai 35、timelapse 30、clippy workspace 绿。
15. **画画线期 2b 已合**（台账 629）：P2-9 color_space 三路；D1 batch `stroke_evidence`。
   与期 2 在 batch 声明/测试同锚点**手工解冲突**（两边都留；补回被公共上下文吃掉的收尾 `}` 两处）。
16. **画画线期 3 E2（v2）已合**（台账 630）：timelapse 分段重放 checkpoint；timelapse 30（含 segment 4）。
17. **vector-tracing v3 后续包（8 条回填）已合**（台账 631）：blob `4a5e1880` 与 `合并.diff` 逐字一致。
18. **四包衔接修复**（台账 632）：工具数 124/204（tools.rs+两 guide）、census 的 `mask_ids`、
   `tool-examples.md` 重新生成；**`tool-verify-package` 判据自身用 `tar --delete` 篡改手写 tar 会重排坏**
   ⇒ 改成解包-删-重打包（只删本体），判据转绿（不是产品问题）。

19. **首跑 CI 的四类红已修**（台账 633）：
   - `test (beta)`：`transport.rs` 4 个测试的 `Cleanup(std::env::temp_dir())` **会删整个 /tmp**
     ⇒ 并行测试互删资产 ⇒ 正例 404；改成**先 push 后缀再 Cleanup 子目录** ✓。
   - `tool-thin-stroke-not-global`：P2-13 已让 size=3.456 落墨（2354px）⇒ 旧「零墨」前提失效；
     改成「细笔必须落墨」+ 真零墨样本（2 点 0.4px）验消息 ✓。
   - `kernel-wasm-allowlist`：filter.rs 两处 wasm 串行回退登记（逐位一致）✓。
   - `tool-color-schema`：`gradient_blend.from` 的 Object→`Any`（实现收数组/hex）✓。

20. **画画线期 4 已合**（台账 634）：`export_png` 加 region/LOD（修法 A，8 patch）。
   Buffer/brush/font 步长感知（step==1 逐字等价）、`render_region_with_lod`、`export_png.lod 0..=3`、
   i64 div_ceil 手写、并行 8 参豁免。本机：render lod 3/above 3/quantize 2/tile_parallel 23，
   server export_png 9/tombstones 1/export_small 6/region_cache 1 全过；clippy/fmt 绿。
   **整合修**：期 2 的 `Buffer::from_f32_vec` 缺 `step` 字段（E0063）⇒ 补 `step:1`；测试 rustfmt 差异。
   **像素口径**：点采样 vs bilinear 超差，但 vs 全幅格点点采样**逐字节相同** ⇒ 总协调裁决**接受点采样**（设计已改）。
   已知：lod:1 暖提速 2.67×（未达面积比 ~4×，固定成本）；8K 真实文档缓存 hits 恒 0（落回告警，非本期）。

## 三、下一步（按优先序，都有精确入口）

-0. **读本轮 CI**（paint 期 2/2b/E2＋skill 后续＋衔接修复）：重点 `tool-surface-coverage`、
   `tool-schema-params-census`、`tool-examples-doc-check`、17 条新画线判据、browser 判据。

-0. **读本轮 CI**（照片期 2＋web＋census 的那一跑）：重点看 browser 判据、`tool-surface-coverage`、
   `tool-schema-params-census`、以及 A1/A2 是否顶到既有像素判据（`tool-effect-*` 等）。

0. **读 `35656ee9` 的 CI 结论**（新判据共 14 条，分片号会变 ⇒ 认判据名）。
   若红先看是不是：`raw` 测试步、release mcp、或某条新判据的环境假设。
   另：照片期 1 的已知未决（hex alpha 静默丢、`color_space` 不进 try_stroke/preview、80MP OOM、4 支镜头 unmatched）
   与 paint 期 1 的未决（WS 入口 uncovered、失败导出不删旧目标、B4 fail-closed 留裁决）都写在台账 §622。

1. **qoder 剩余条目**：`D139`（静默零覆盖，需他给完整参数；我 6 点网格未复现）、`D188` 的量级说明（差随原子数增长：200 图块 ≈0.6 lv/通道、22,926 原子 ≈16 lv ⇒ 写进那条 `⚠️` 警告）。
2. `shrink` blit（`art` 第 12 项）：最小包上对照判据实测 均值 1.89／>30 占 0.22% ⇒ 无缺口；要他用同判据跑 0.30 GB 的 `test1full`。
3. （可选，第二口径）`scripts/tool-export-bitmaps.mjs` 加一段：`include_bitmaps: true` 导出的包直接喂 `target/release/yanshi-timelapse` ⇒ `rc=0`。
4. **浏览器 known-red 只剩真产品缺陷**：客户端落笔一族（本地 IndexedDB 日志缺 `create_document` ⇒ `0×0`）、
   `browser-ui-check`（夹具/时序）、`cs-render-switch`（抖动）等。`browser-project-download`／`brush-panel`／`i18n`／`layout` **已移出名单且 CI 绿** ✓。
   `browser-export-full-document` 仍登记（产品那句「整幅，离线本地合成」已满足；只因**偶发内核超时**留着）。

## 四、纪律与环境（别重犯）

- **CI 一条并发**：`main` 上只跑一个 run；**一次 push 就够，别连推**。推前先查 `gh run list --limit 3 --json status --jq '[.[] | select(.status=="in_progress" or .status=="pending")] | length'` ⇒ 0 才派发。
- **推前必跑** `bash scripts/pre-commit.sh`（fmt／workspace clippy／台账引用／三条自测／发布工作流形态）。本轮已跑，全绿。
- **改 `.rs` 后本机**：`cargo clippy -p <crate> --all-targets -- -D warnings` ＋ 定向 `cargo test -p <crate> --test <name>`；全量交 CI。
- **二进制新鲜度**：跑数字前 `ls -l --time-style=full-iso <binary> <source>` ⇒ **源码不得比二进制新**（我拿变异版二进制量出「6.5% 纯黑」，真值 0.00%）。
- **`target` 体积**：本机 `target` 现约 **4.8 G**（16:00 清过一次 `target/debug` 2.8 G ＋ `/tmp` 里 10 个陈旧 chrome profile 1.45 G；清完磁盘 70% → 69%）。脚本是 `/home/crow/cleanup-dev-space.sh`，本轮给它补了 ③b 段（只删 **crow 所有且 >6 h 且没有 chromium 在跑**的 `*_chrome_*`）。
- **`paths-ignore`**：只改 `docs/**`、`**/*.md`、`scripts/criteria-known-red.txt` 的提交**不触发 CI**。
- **已知红**：权威名单是 `scripts/criteria-known-red.txt`（用 `grep -c` 数，别写死）；「CI 绿」＝红都在名单里。
- 中文用「」；中文不许插进 `param!(…)` 参数之间；含 `\"` 的字符串里不许用 `[^\\\"]*`。

## 五、我这场的坑（供新 session 少走）

1. **报告基线是截断抓取** ⇒ 十几轮"qoder 未变"无依据（已修基线）。
2. **`head -c 400` 截断 JSON** ⇒ 误报「没有 `total` 字段」（键按字母序，`total` 在最后）。
3. **D172 的两条新坑（`ca58be70`）**：
   - **"缺 blob"的审计不能直接用全文 64-hex 判致命**：`import_image` 的 `payload.tiles` 是**分块解码索引**，不在 `refs.blobs` 里、导出**故意不装**，而渲染侧取不到就退回整幅 ⇒ 把它算成缺 = **拒绝自己的包**（实测 6 个引用里 3 个是 tiles ⇒ `rc=2`）。修法：只把**非 tiles** 的缺失当致命，但仍打印 tiles 缺失数。
   - **断言的输入必须能触发它**：原测试用 `8B_Pencil#1`，这支笔**不可省** ⇒ `omitted == 0` 恒真 ⇒ 把 `include_bitmaps: true` 删掉测试**照样绿** ✗。换成 `2B_pencil.myb`（`export_small` 已证可省）后，删参数 ⇒ `omitted: 3` ⇒ 红 ✓。
   - **顺序**：`exit(2)` 必须在"导出侧能装全"之后（第 1089 轮先加 `exit(2)` ⇒ 拒收自产包 ⇒ 回归过）。现在的顺序是 导出侧 `include_bitmaps` ✓ → 测试/文档传 `true` ✓ → 才 `exit(2)` ✓。
4. **改行为前先 grep 谁依赖它**（这条欠账的代价是一轮红）。
5. **「一刀切给判据加参数」会把被测对象换掉**（第 1131 轮）：`run-criteria.sh` 给全体 `browser-*` 加 `render=client` 是为了让**需要内核**的判据拿到内核 ✓，但 `project-download`（服务端包）／`brush-panel`（服务端资产）／`i18n`（静态文案）**正好要服务端** ⇒ 加了参数就在测"本地内核 + 服务端资产"这种不存在的组合 ✗。本机 server 模式三条全过 ⇒ 用 `RENDER_PARAM` 例外 ✓。**⇒ 问句：这条判据的"被测对象"是哪个模式？**
6. **判据断的若是"溢出多少像素"，先问"这个量由谁决定"**（第 1131 轮）：`browser-layout` 的 400% 溢出 = **视口宽 - 画布宽**，而画布 CSS 尺寸本来就被限制在视口 × 缩放 ⇒ 溢出恒≈0 ⇒ 它永远只能报视口尺寸。产品机制是**视口平移**（`state.viewport`），不是 `.stage` 滚动条 ⇒ 判据要断 `state.zoom ≈ 4` 与 `viewport.w < 文档宽`。
7. **新增仓库**顶层**条目（含 `.md`）要同步加进 `scripts/tool-repo-root-clean.mjs` 的 `ALLOWED`**：
   `HANDOFF.md` 在 `ab7a1114` 提交后**第一次**跑 CI 才红（`2e70615b` 的 run 早于它）
   ⇒ 这类红**不在产品侧**，但会阻塞"CI 绿" ✓。判据自己会指出"有意的就加白名单"。

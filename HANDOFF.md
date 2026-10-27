# 工作交接（截至 2026-10-27，vector-tracing v5.1、MCP receipts 与 draw_text 真文本合并后）

**仓库**：`/home/crow/yanshi`（分支 `main`）｜**HEAD**：changes.patch 合并 + vector-tracing v5.2（见第 59／60 项；含产品代码 ⇒ 触发 CI）｜推送：`git push git@github.com:gradetwo/yanshi.git main`

## 一、每轮第一件事：比两份外部报告

| 报告 | 远端路径 | 本地基线 | 比法 |
|---|---|---|---|
| `art`（apple2011）| `apple2011:/home/crow/work/art/YANSHI_ISSUES.md` | `~/.local/share/yanshi-external/issues_seen.md` | `scp` 下来后 `md5sum` **两侧都打印**，相同才说「未变」 |
| `qoder` | `apple2011:~/work/qoder/yanshi_issue.md` | `~/.local/share/yanshi-external/qoder_issue_seen.md` | 同上 |

**⇒ 2026-10-27 实测（基线已更新）**：
- `art`：远端 `706a3499f00dd0cab673245631d42b32`（753,171 B）｜旧基线 `42a993d78aafabfd279b9db47e3da1c3`（740,964 B）⇒ **变了**：纯追加第 145–149 次检查。
  要点：他已在 0.30 GB 的 `test1full`（4,608×8,192、22,926 atom、8 层）上跑了 shrink 对照 ⇒
  **均值 12.29／`>30` 占 14.98%**（最小包 1.89／0.22%）；两版都是官方引擎、唯一差别是 `YANSHI_TIMELAPSE_SHRINK`。
  另有一次自我更正（他以为报告从未送达上游；实际我们每轮 `scp` 读它）。
- `qoder`：远端 `7dcd42c3a6e79e94eabc15b94b8e0bcb`（141,575 B）｜基线 `7dcd42c3a6e79e94eabc15b94b8e0bcb` ⇒ **未变** ✓（两侧相同）。
- **2026-10-27 本轮复核**：art `706a3499…`（753,171 B）／qoder `7dcd42c3…`（141,575 B）
  两侧 md5 与基线**相同** ⇒ 两份都未变 ✓。

**⇒ 规矩（我犯过）**：报「未变」**必须附两侧 md5**。此前 `qoder` 根本没被真正比对过（旧基线是 33 KB 的**截断抓取**），我报了十几轮"未变"，全是**断言恒真**。

## 二、当前状态

**⇒ 本轮（2026-10-27 下半场，两份审计报告修复中）**：
- **Batch 1（P0 鉴权）已合入并推上 CI**（`0bd7c06f`）：`atom_submit`／`create_document` 闸在落盘前、
  viewer 令牌不许提交原子、viewer 源链接不许复制成可写副本、`list_documents` 过写闸、密钥常数时间比较。
  CI 上 transport 新测试全过；那跑的 10 个红**没有一个是这次改动造成的**（都是合并遗留 ⇒ 下面三条已修）。
- **合并 changes.patch 带来的四个静态红已修**（`6ae48635`）：census 白名单补 `fill_region` 的
  texture 六个子键（逐个核对 `tools.rs` 真读它们）；`tool-examples.md` 是生成物 ⇒ 手写注释搬回
  `tools_examples.rs` 源处再重生成；`render-box-first-paint.md` 补指向 `performance-notes.md` 的指针；
  `tool-export-idempotent` 本机复跑绿（omitted=2，重放证明真在跑）。四条都经 `run-criteria-local.sh` 当场复验。
- **xiaomi 审计 H-2（单一 SW 卫生）已修**（`96ed7057`）：删 `web/sw.js.bak`（平行实现以 `.bak` 形态回潮，
  旧判据只查 `web/sw.js` 抓不到 ⇒ 现在拒 `web/sw.js*` 任一变体，变异验过红／绿）；`web/_headers` 的
  `/sw.js` 条目改指真正注册的 `/service-worker.js`；pwa.yml 里跑**已删除脚本**
  `tool-sw-stamp-idempotent.mjs` 的步骤换成 `check-service-worker-roles.mjs`；两个 HTML 的
  `&& false` 死注册行与过期注释删掉。
- **六个 browser 红的本机定性**（CI 日志逐条摘出的失败行见台账本轮追加）：
  - `browser-export-full-document` **已修（判据度量错）**：`#log` 是 div 前置追加，
    `textContent` 里**没有换行** ⇒ 旧判据 `split("\n")` 读日志是测量陷阱；改成
    `[...document.querySelectorAll("#log div")].find(...)` 逐条读。本机复跑绿，
    「离线默认导出日志没写明整幅」**不是产品缺陷**（CI 假红源自 `index_missing` 那条
    boot 错误多插一个 div，改变 find 命中序）。
  - `browser-p0-entries` **已修（三处）**：① 判据自己钉 `render=server`
    （`set_brush_dynamics` 在 PWA 本地模式诚实报 `not_implemented_locally` ⇒ 加
    `render=client` 会改变被测对象，与 dodge-burn 先例同族）；② 下拉选项数
    `!== 7` 改下界 `< 7`（BLEND_MODE_LABELS 已 7→13，权威比对在
    `browser-blend-modes`）；③ 渐变坐标从页面 `state().docSize` 推导，不再写死
    （写死 120×96 在 320×240 夹具下踩到的判据前提）。本机复跑绿。
  - `browser-dodge-burn` **CI 绿、本机 3/3 红 ⇒ 待归因**：红固定在①首条预测，
    拿到的是「将新建组 「图层 1 · 减淡加深」（放在「图层 1」上方）」——像夹具的
    SRC「底」层没建上。按规矩不静默入名单，先查是本机编排/二进制还是判据时序。
  - `browser-range-mask`／`browser-frequency-separation` 早先本机红**疑似复现夹具污染**
    （预开标签页把 A 判据的文档混进 B 的日志），等用无预开驱动重跑后再定性；
    `browser-fill-region` 用无预开驱动重跑**已绿**（前一种红确认是污染，不是产品）。
  - `tool-export-idempotent` **CI 独有红**：CI 恒 `kept_no_recipe=3／omitted=0`，
    本机 release 与 debug（同 CI 参数）都是 `omitted=2／kept_no_recipe=0` ⇒
    CI 环境特有的分类差异，未决。
- **外部补丁包 `photo-bugfix-gc-isolation后续包.zip` 已合入**（`39511fc6`，`git am` 零冲突，
  基线 `02506181`＝当时 HEAD）：双服务同 root 的两个跨进程竞态——R1 `put_inner` 的 tmp 名
  拼上进程 id（每进程 counter 从 0 起 ⇒ 并发同哈希共用 tmp ⇒ 先 rename 者拿走、后一方
  ENOENT ⇒ 503「CAS IO 失败」，正是 A2 梯度的 503）；R2 `delete_document` 只删年龄 ≥
  `DELETE_DOCUMENT_BLOB_GRACE_SECONDS`（60 秒）的 blob，新鲜的留给 TTL GC 当孤儿，回执新增
  `blobs_deferred_fresh`、`blobs_shared_kept` 改精确交集计数。说明.md 更正了 A2 日志的
  「启动期 GC」推断（全仓没有启动期 GC）。**本机验证**：fmt 干净；`clippy -p yanshi-core
  -p yanshi-server --all-targets -D warnings` 干净（pre-commit 的 workspace clippy 也过）；
  新测试 `delete_document_blob_grace` **2/2**；**独立变异复现**（门槛改 0 ⇒ ① FAILED、② 仍过；
  改回 60 ⇒ 2/2 ✓）；`yanshi-core blob` 13/13、`delete_with_subscriber` 1/1、
  `two_writers_seqs` 2/2 全绿。全量 `cargo test --workspace` 等 CI。
  诚实记录（作者自述＋我核对）：60 秒是启发式不是消除（put→import 超 60 秒仍可撞）；
  R1 无 Rust 单测（同进程撞不出跨进程撞名，写了是安慰剂），证据是盒上双服务复现红→绿；
  `run_gc` 根集只看本进程的同类缺口**未修**（要 7 天老 blob＋文档未加载两条件，窄得多）。
- **外部补丁包 `photo-bugfix-a2-interp后续包.zip` 已合入**（`d265897d`，只碰 `filter.rs`）：
  `shadows_highlights` 提亮侧 gap 按品红分量 `m=min(R−G,B−G)` 调制（`t=1/(1+(m/0.005)²)`），
  m≤0 与压暗侧逐位不动。本机定向验证：`-p yanshi-render --lib shadows_highlights` **15/15 绿**；
  **变异独立复现**（调制改恒等 ⇒ 仅 `magenta_pixel_lift_reduced` FAILED）；clippy 干净。
  全量 test 等 CI（合并补丁例外的最新口径：本机只跑 clippy＋定向 test）。
  诚实记录（作者自述，未随本包修）：+0.6 档判据 A 色卡保持率失守仍是设计文档 §6 已知未决项。

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

21. **CI 已绿**：`40244250728`（paint 期 2/2b/E2＋skill 后续＋衔接修复，`29d50948`）与
   `40246037263`（期 4，`96460361`）**全 job success**（`brush parity` 仍 skipped）✓。
   四个包（期 2、2b、E2、期 4）与 skill 后续均已合入 `main`，工作树干净。

22. **p3b-1 已合**（台账 635）：C1 护栏 G1/G2/G3 + WB 回显 9 字段；`tools_auto` 17 过/5 ignored。
   注意：**基线三触点本身未过**（colorcast 28.6% < 40%），本包不改变结论；01242/01244 服务端重跑未做。
23. **photo 期 4 已合**（台账 635）：`Op::ReorderObjects` + `reorder_objects`（125/205）；`region_fingerprint`
   补 `z_index` 修"排了但画面不动"；root 全套/ render 537 / reorder 5 全过。
24. **E3 v2 已合**（台账 635）：timelapse 分段账默认打印、进度末行、MCP 三段 ms；timelapse 31。
25. **paint RSS 已合**（台账 635）：每文档保留上限 + `yanshi-heap` 的 `malloc_trim` + C2 预算接 create；
   30 文档峰值 2.46GB（基线 19 就 OOM）。已知：每文档 ~131MB（未达 100MB 建议）、关完残留 1.7–3.5% arena。
26. **photo P-01 已合**（台账 635）：变更后预览 LOD ≤1600（打包轮 359s→74s）；不碰 preview.png/缩略图/preview_dirty。

27. **photo crossfade O4 已合**（台账 636）：selective_color 白档保底 ε=0.20（亮饱和像素重新
   吃到 whites 杠杆）；render filter 110、style_preset_package 5、clippy/fmt 绿。
   **不修 forest 唇口**（whites 钝杠杆/剂量倒置，阶段 3 另案）；4 套预设重放轻微变化（均值 ≤0.0014）。

28. **CI 已绿**：`40272398250`（`fe644770`：五包＋O4＋两处判据修正）**全 job success** ✓。
   此前 `40269293870` 的两处红都修了：`pinned_blob_eviction`（P-01 语义：显式 `render_document_preview`
   触发淘汰）、`tool-example-acceptance`（`reorder_objects` 示例须匹配夹具 `L1/o1`）。

29. **web-ef2494de 审计已合**（台账 637）：参考图位置/留存、直方图、整组撤销、对象重排；修两个真 bug
   （开机对象/标注面板空 + 面板按钮晚接线）；新增 `browser-boot-panels`；本机 5 条浏览器判据全 0。
30. **vector-tracing v4 后续已合**：10 条通用规则；blob `f7e0de88` 与合并 diff 逐字一致。
31. **font-identification 新 skill 已合**：100 行、零反引号；blob `f9179e33` 一致。
32. **展示册 v5：无仓库补丁，未合并**（只有 PDF/配方/notes；forest v5r3 预设在展示工作树、未提交；
   日系半量补丁未出包）。forest 预设是否进仓库需单定（基线是 fix-phase5 `2917122`）。
33. **p3b-2 已合**：C4 curves + A-03 p0.1 + tone 解耦/审美压；tools_auto 24、auto_retouch 10；
   **colorcast 触点仍红**（基线本就红，road 27.4% < 40%）。
34. **render-nondet 已合**：below/above 只写本带自有行 ⇒ 跨进程逐位一致；不修 padding 截断本身的 ≤13 微差。
35. **Y-03 已合**：`raw_options.tone` 缺省 standard；raw 16 套件、render 546、raw_import 9 全过。
   **★ 发的是设计初值表，不是标定定版**：§6.3 未做 ⇒ v5/预设/auto/harness 需重标，勿当最终基线。
36. **D1-a/b 已合**：`create_range_mask` + linear/radial gradient；工具总数 206（core 125）；定向全过。

37. **八包首跑 CI 两处红已修**（台账 638）：
   - `browser-boot-panels` 的 CDP 端口来源错（argv[3] 是 base、不是端口）⇒ 改读 `CDP_PORT`；
   - `raster_patch_fixed_cost_*` 的下界 0.4 在 beta 负载下抖红（实测 37.1%）⇒ 改 0.25。

38. **CI 已绿**：`40318415147`（`d552fe85`：八包＋两处判据修正）**全 job success** ✓。

39. **photo D2-2b 已合**（台账 639）：`black_and_white` 加 8 色档 `ranges`（[0,2] 乘子、
   饱和度缩放、灰像素不动点）；无键路逐位不变。render `bw_` 10、`tools_effect` 6、render lib 552。
40. **photo P-01 M2 已合**（台账 639）：`auto_retouch` 打包落成、一次终态预览；auto −51%
   （12.7s→6.2s，预览 11→1）。口径：这是 M1 基线上的再砍半，别与 M1 收益重复计。

41. **CI 已绿**：`40324482977`（`38962591`：D2-2b ＋ P-01 M2）**全 job success** ✓。

42. **Web 第三轮（范围遮罩/渐变蒙版）已合**（台账 640）：`#cardRangeMask`（亮度/色相带、
   烘焙挂载/取下/现状）、linear/radial 渐变笔、`browser-range-mask` 判据、改动选择器修漏报。
   整合三处：CDP 端口读 `CDP_PORT`、判据加进服务器渲染例外（不能注入 render=client）、
   本地模式回执改轮询（固定 2s 会读到空串）。本机 21 条 + 三条浏览器回归全 0。

   **首跑 CI 红（shard2 `ui-text-density`）已修**：渐变卡长句拆成短可见句 + `title=`，
   并补 `I18N_EN_TEXT`/`I18N_EN_ATTR` 两条词条（i18n 静态判据也查 title）。

43. **Web 第四轮（频率分离 + 混合模式清单）已合**（台账 641）：频率分离卡（5 层/四误差同源/
   五条拒绝路径）、混合模式 7→13 项（未知模式透传）、两条新判据。整合五处：CDP_PORT、
   服务器渲染例外、点击前等 init、长 hint 进 title、title 词条。
44. **渲染带截断已合**（台账 641）：分带按**声明外扩**（不钳）⇒ 并行收敛到串行（羽化 400：
   156,280 字节/max12 → 114 字节/max1；羽化 1000 应力 0 差）。不修 `MAX_EFFECT_PADDING` 本身。

45. **CI 已绿**：`40345152775`（`504b975a`：Web 频率分离/混合模式 ＋ 带截断）**全 job success** ✓
   （前一轮 `40340042575` 的文本密度修复也已 success ✓）。

46. **C1 stage1/2/3 已合**（台账 642）：S1 色度锚 + S2 Planck + S3 分轴反解 + 阈值定版/Q5 断代。
   **D-WB2 诚实结论：Ti 收敛、T 未收**（机制/光源歧义）⇒ 是否立 (b) 光源估计由 Wang 定。
47. **C1 Y-05 已合**：`wb=auto` 导入期真估计并落成 `default_white_balance`；护栏跳过有原因。
48. **dodge-burn 已合**：纯调整层从下方内容播种（D&B curves 不再 no-op）；only_layer/clipping 不播种。
49. **D1-c 已合**：`calibrate_primaries`（6 参数，不新增工具）；全零逐位单位阵。
50. **magenta-softknee 已合**：s_h 色度软拐点（品红 2.97%→0.05%）；+0.6/korean 3x 为已知限制。
51. **airy-half-dose 已合**：japanese_airy 半量烘焙为 strength-1 缺省。

52. **首跑 CI 三红已修**（同一根因）：D1-c 的 `calibrate_primaries` 让调整数 25→26，
   `docs/design/implementation-notes.md` 的算子表/总数行没同步 ⇒ `doc_consistency` 在
   test stable/beta/GPU 全红；补行 + 改 26 后 3 通过 ✓。

53. **CI 已绿**：`40412680604`（`04c7165e`：八包＋文档计数修复）**全 job success** ✓。

54. **vector-tracing v5 后续包已合**（`e544fa3a`，基线 `1231047b`，`git am` 干净）：
   `skills/yanshi-vector-tracing/SKILL.md` 142 → 185 行（+50/−7）——Phase 3 十三条量测规则、
   Phase 4 真渐变张力解决（组装 SVG 多止点 `linearGradient`，明确**不是**引擎 `gradient_fill`）、
   新增 `## Iteration Economy (Time Discipline)`、交付核验脚本条目与 5 条失败模式。
   **校验（本机，全部 ✓）**：包内定稿 blob `5ab17e85`／sha256 `d7391244…` 与本仓应用后**逐字一致**；
   `check-skill-tool-names.mjs` ✓（7 文档／79 标识符／206 工具／259 参数）；
   通用性 grep 零命中（`sofa/chest/wordmark` 5 处是 v4 原有）；`##` 12 个；`scripts/pre-commit.sh` 全绿。
   **纯 `*.md` ⇒ 命中 `paths-ignore`，不触发 CI**（证据只是本机那几条，不是 CI）。
   留待 v5.1/v6：交付核验脚本本体、patch–描边方法级修法、宝箱立体感收尾、冷启动实测。

55. **MCP receipts 第一批已合**（`26f4c801`，作者 `9f0072c`）：`export_svg` 回执加 `sha256`
   （`BlobHash::from_bytes(svg.as_bytes())`，与 `export_png` 同款、与落盘字节同源）＋
   `list_atoms` 加 `hidden_non_stroke`（缺省过滤藏掉的非笔迹原子数；**计数位置在全部其余
   过滤器之后**，不变量 `total + hidden_non_stroke` = 加 `include_non_stroke=true` 的 `total`）。
   **校验（本机 ✓）**：四个 pre/post blob 与 `合并.diff` 逐位一致；`cargo fmt -p yanshi-server` 干净；
   `clippy -p yanshi-server --all-targets -D warnings` 干净；定向 `list_atoms_truncation` 5/5、
   `vector_layer` 2/2；**两处变异都红**（`hidden_non_stroke` 写 0 ⇒ 0≠2；sha 换成常量 ⇒ 不匹配）；
   活体判据 `tool-list-atoms` ✓、`tool-export-svg-validate-not-overrejecting` ✓。
   **CI 已绿**：`40444468624`（`ececf129`）⇒ **12 个 job 全 success**（`brush parity` skipped，已知）✓。
   **全量测试留给 CI**（本次改了 `crates/**` ⇒ 会跑）——已由上面那跑验过。
   未在本包：③ `draw_text` 出真 `<text>`（只写了设计文档，未实现）。

56. **vector-tracing v5.1 后续包已合**（`d68d6ce3`，作者 `26a9f7d`，基线 `bae5f7e`，`git am` 干净）：
   `skills/yanshi-vector-tracing/SKILL.md` 185 → 222 行（+52/−15）——冷启动第二轮 7 图
   36 条缺口归并的 33 条规则（弧命令预转贝塞尔、Catmull-Rom 参数化、量测边内收/AA 补偿、
   共享边与遮挡顺序、Text Assembly 四条扩展、Hausdorff 采样口径、Known Failure Modes 新增 3 条等）。
   **校验（本机 ✓）**：`git apply --check` 干净；pre/post blob `5ab17e85` → `151e55e1`、
   sha256 `4eff25f6…` 与包内定稿逐字一致；222 行、13 个 `##`（与 v5 相同）；
   `node scripts/check-skill-tool-names.mjs` ✓（7 文档｜92 标识符｜206 工具｜259 参数）；
   新增行通用性 grep 零命中；提交钩子跑了完整 `scripts/pre-commit.sh`（fmt／workspace
   clippy／台账／四条自测／发布工作流形态）**全绿**（clippy 走缓存 1.43 s）。
   **衔接修（本包与 `26f4c801` 的直接冲突）**：包内 Phase 0 原写「`export_svg` 回执从不带
   sha256（按设计，字段只在 PNG 路径）」＋ 对应 failure mode 一条——而 `26f4c801` 恰好给
   `export_svg` 加了 sha256（`tools_io.rs:1956`，`tests/vector_layer.rs` 已断言、CI 绿）
   ⇒ 已改写两处：正确导出（PNG/SVG）都带 sha256，只有失败调用没有；先看 `ok`。
   **两处读取核对（写进交接，不阻塞）**：① 包内「`draw_text` position 字典形态被静默忽略」
   属实但**只对自订字体路径**（`tools.rs:4124` 只认数组）——内置渲染路径
   （`crates/yanshi-render/src/object.rs:273-289`）对象与数组都收；② 包内 64 KiB stdio
   上限属实（`crates/yanshi-mcp/src/lib.rs:709`）。
   **须带回 Wang 的冲突**：打包说明的「狐狸 GT 再核查」认为 6/7 图 GT 逐位成对、与 Wang
   11:56「狐狸无 GT」相反；本包按 Wang 定义写规则、不把「狐狸无 GT」写成事实。此前该冲突
   已随 v5 包记过一次，仍未裁决。
   **CI**：只改 `.md` ⇒ `paths-ignore` 不触发；证据是本机本地校验。

57. **draw_text 真文本（子集内嵌）已合**（`1d251991`，作者 `019140c`，基线 `95a1098`，`git am` 干净）：
   9 文件 +1221/−37——`draw_text` 真字体路径 draw 时记 `data.text_provenance`（基线锚点 + TTF 子集 blob），
   `export_svg` 对有溯源的对象出真 `<text>`，子集以 `@font-face` 进 `<defs>`；builtin 文本仍走位图
   （单列 `texts_builtin_raster`）；回退有原因码 + 回执警告（不静默）。
   **校验（本机 ✓）**：8 个 pre-image blob 与补丁 `index` 行逐位一致；`git apply --check` 干净；
   定向测试 **22/22**（`text_provenance` 6、`text_object` 6、`raw_registry_guard_off` 2、
   `vector_layer` 2、`fonts` lib 5、position lib 1）；回归 **20/20**（`svg_q123_fixes` 9、
   `export_png` 6、`export_small` 5）；`clippy -p yanshi-server --all-targets -D warnings` 干净；
   fmt 干净。**变异 4 条都红**：拆 XML 转义、去回退计数、去字典 position 分支、按对象分族名。
   **诚实纠正**：包内注释说「去重表改成每次都插 ⇒ @font-face 变 2」是**错的**——
   `BTreeMap` 按族名去重，`contains_key` 守卫去掉也仍只有 1 个；真正的回归（每对象一个族名）
   才红 ⇒ 该测试有效，但变异描述要改。
   **端到端（旧 release 二进制〔11:27，补丁前〕 vs 新 debug 二进制）**：Inter "Hello"/"World"
   场景 **PNG sha256 两边相同 `90c373c1…`** ⇒ 引擎光栅路径逐字节未变；新 SVG `texts_vector=2`、
   `images_embedded=0`、2 个 `@font-face`、xmllint 通过；Chromium 渲染 vs 引擎 PNG
   **AE(5% fuzz) 136 px = 0.17%**、墨迹框差 1–2 px；CJK（Noto Sans CJK SC 60pt）
   引擎与 Chromium 墨迹框**同为 165×39 @ (22,22)**、AE 0.4%，`font-size=41.44` 与包内校准一致。
   字典 position 端到端回显 `[20,20]`；两次导出 sha256 相同；真字体含 `\n` 响亮拒绝；
   DejaVu（不在 allowlist）`text_svg=false` + `font_license_unknown` + 回执警告 ✓。
   **补的两处包外改动**：① 补丁**没带 `Cargo.lock`** ⇒ 本机 `cargo metadata` 重生成
   （+23 个 crate，无既有版本变动；`rustc-hash` 出现 1.1.0/2.1.3 双版本，依赖方标注消歧）；
   ② `allsorts` 许可证注释原写「MIT／Apache-2.0」⇒ crates.io 元数据只有 **Apache-2.0**，已改。
   **skill 文档同步**：`SKILL.md` 的 Text Assembly 首条／line 105／failure mode／position 语义四处
   随事实改写（真字体已自带真 `<text>`，手工组装只留给 builtin／非 allowlist／非平移变换）；
   `check-skill-tool-names` ✓（93 标识符｜206 工具｜**260 参数**）。
   **本机未覆盖**：Fredoka／Baloo 本机未装 ⇒ 只对 Inter 与 Noto CJK SC 做了端到端；
   GPOS 字距丢失是已知代价；全量测试留 CI。
   **★ 补丁漏改的第三处（本机测试口径没覆盖到 ⇒ CI 首跑才暴露）**：`raw_import.rs:594` 的
   `registry_diff_is_exactly_the_raw_options_group` 仍期待 raw-on `import_image` 恰 6 个参数
   （`bitmap/layer_id/medium/object_id/raw_options/region`），而补丁**同时**在两个变体注册了
   `text_provenance` ⇒ 实到 7 个。作者只更新了 `raw_registry_guard_off`（关变体），漏了开变体。
   **CI 首跑 `40460044012`（`125cce35`）**：`test (stable)` 的 **RAW-feature 步**红 ——
   `left: [.., "text_provenance"]`（7 项）｜`right: [..]`（6 项）｜该文件 11 passed / 1 failed；
   **其余 11 个 job 全绿**（含 `test (beta)` —— 因为 RAW 步带 `if: matrix.rust == 'stable'`），
   `brush parity` skip。**修复 `3c8cf8cd`**：期望集加入 `text_provenance`（与实到集合逐项一致），
   并把平台注释的「基线五项」改成「基线各项」（避免写会过期的数字）。
   **CI `40460340849`（`3c8cf8cd`）**：**13/13 job 全绿**（`rustfmt`／`clippy`／`test (stable)`／
   `test (beta)`／`GPU feature`／`wasm smoke`／criteria 6 片），仅 `brush parity` 按约定 skip
   ⇒ **本包合并完成，主干绿**。

58. **photo-bugfix 期 2 文档修正已合**（`b608933a`，作者 `a9bc410`，基线 `68adb93`，`git am` 干净、零冲突）：
   纯文档 1 文件 +11/−6（`docs/design/style-preset-system.md`）：把 A1（levels 改显示口径、6 套预设翻回）
   的「6 套翻回后渲染应与现状**逐像素一致**（转换可逆，浮点误差 <1e-4）」改正为
   「**只保端点、不保曲线形状**」（显示仿射曲线整体低于线性仿射曲线，暗部差最大），
   §9.1 续 与 续七 两处同文一起改。
   **校验（本机 ✓）**：pre-image blob `e2510dd…` 与补丁 `index` 行一致；`git apply --check` 干净；
   合入后 `git diff --check` 干净；全库 grep「逐像素一致／转换可逆」剩余命中都与 A1 无关
   （`style_preset_package` 的 strength=0 恒等断言、timelapse 段重放等，各自是别的机制）。
   **数字出处（未复测，已记录）**：japanese_airy mean|d| 6.45/255、L_p2 −0.026；
   korean（黑点 0.1）mean|d| 12.16、mid_sat +75% —— 出自作者「期 2 包说明」；
   **本机与仓库内都没有这些数**（`~/patch` 各 photo-* 包逐包 grep 也没有），只按作者实测记录。
   自洽性旁证：按「黑点 0.1 ⇒ 中点位移 ≈ 0.9·0.5+0.1 − 0.5 = 0.05（线性光口径编码后 ≈0.023）」
   粗算与 6.45/255≈0.025 同量级，修正方向与数值不矛盾 ✓。
   **CI**：只改 `docs/**` ⇒ `paths-ignore` 不触发；证据是本机校验（与本仓 56／57 的 docs 合入同口径）。

## 三、下一步（按优先序，都有精确入口）

-0. 待上游裁决的仍是：期 2 的 **auto backtest colorcast 门限**（A2 保色度 ⇒ R−B 只降 28.6% < 40%）、
   期 2 的 22 套预设目检（作者未勾选）、B4 剩余 4 支镜头识别、80MP 整幅导入需大内存机。

-0. **读本轮 CI**（照片期 2＋web＋census 的那一跑）：重点看 browser 判据、`tool-surface-coverage`、
   `tool-schema-params-census`、以及 A1/A2 是否顶到既有像素判据（`tool-effect-*` 等）。

0. **读 `35656ee9` 的 CI 结论**（新判据共 14 条，分片号会变 ⇒ 认判据名）。
   若红先看是不是：`raw` 测试步、release mcp、或某条新判据的环境假设。
   另：照片期 1 的已知未决（hex alpha 静默丢、`color_space` 不进 try_stroke/preview、80MP OOM、4 支镜头 unmatched）
   与 paint 期 1 的未决（WS 入口 uncovered、失败导出不删旧目标、B4 fail-closed 留裁决）都写在台账 §622。

1. **qoder 剩余条目**：`D139`（静默零覆盖，需他给完整参数；我 6 点网格未复现）、`D188` 的量级说明（差随原子数增长：200 图块 ≈0.6 lv/通道、22,926 原子 ≈16 lv ⇒ 写进那条 `⚠️` 警告）。
2. `shrink` blit（`art` 第 12 项）：**他已作答**——`test1full` 上 均值 **12.29**／`>30` 占 **14.98%**（最小包 1.89／0.22%）。
   **⇒ 我的读法：这不是默认路径的缺口**：`YANSHI_TIMELAPSE_SHRINK=1` 是显式 opt-in，源码与 stderr 都写明「代价是细节先丢后缩」
   （`crates/yanshi-timelapse/src/main.rs:362-369`；`real_package_shrinks` 只断「缩不放弃、缺 blob 0」，不断像素等价）。
   **待裁决**：是否把 12.29／14.98% 这种「按缩放比的量级」写进那条建议语（现在是定性的一句话），或干脆不改。
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

59. **changes.patch 已合（工作树 → 提交）**：外部裸补丁 `~/patch/changes.patch`（基线 `bfe6189e`，
   `git apply --check` 干净后落工作树），27 改 + 15 新。要点：
   W3C `SoftLight` 多项式分支修正（`((16cb−12)cb+4)cb`，含 ps 参考值/接缝/白提/变异对照四组单测）；
   feather 形状裁剪 scope∩canvas；`viewer.rs` 内联脚本让 `#useServerRender` **首帧说真话**；
   `api-local.js` `__deflate` 死锁修复；C/S 双模 `atomsOf` 走 `/api/atoms`；本地折叠种子 `layer_default`；
   `dodge_burn_setup` 回执（`effective`/`ineffective_reason`/`neutral_gray`/`mode`）；retouch/curves 播种行为改动；
   新判据 12 条 + `check-viewer-unique-ids` + `tool-dodge-burn-effective` + 3 篇设计文档；
   `run-criteria.sh` RENDER_PARAM 例外表扩充；`criteria-known-red.txt` 重写（4 条移出、
   offline-reload 改归「判据接线」、offline-brush 改归「判据度量」）。
   **本机验证**：`cargo clippy --workspace --all-targets -- -D warnings` 绿；
   定向判据 14 条：**11 绿、3 红**（`browser-fill-region` ②「宽 0 就地拦」读到空消息、
   `browser-dodge-burn` 英文模式预测/回执 2 条、`browser-frequency-separation` 3 条）——
   新判据抓到的是**产品缺口还是判据接线**待查 ⇒ 进两份审计报告的同批修复（第 61 项起）。
   CI 结果以 run 为准；若 3 红在 CI 复现，先归因再决定修产品或修判据，不静默入名单。

60. **skill-vector-tracing v5.2 后续包已合**（`cc4d1759`，作者 `625593e`，基线 `bfe6189`＝当时 HEAD，`git am` 干净）：
   单文件 `skills/yanshi-vector-tracing/SKILL.md` +90/−5（222→307 行）。
   **校验（本机 ✓）**：pre-image blob `9cab355…`、am 后 blob `a85dd5e…` 均与补丁 `index` 行一致；
   包内定稿 `SKILL.md` sha256 `fa3f8349…` 与说明一致；包内 `合并.diff` 与 patch 逐行一致。
   纯 skill 文档 ⇒ 不触发 CI（`paths-ignore`），随本提交的产品代码一起过 CI。

61. **photo-bugfix-gc-isolation 后续包已合**（`39511fc6`，作者 `e4cf9f0`，基线 `02506181`＝当时 HEAD，
   `git am` 零冲突）：R1 `put_inner` tmp 名拼进程 id（跨进程撞名 ⇒ 503）；R2 `delete_document`
   只删年龄 ≥60 秒的 blob（防别的服务 put→import 窗口），回执加 `blobs_deferred_fresh`。
   本机验证与独立变异见 §二 本轮条目与提交信息；**更正了 A2 日志的「启动期 GC」推断**（全仓没有启动期 GC）。
   未修记录：`run_gc` 根集同样只看本进程（7 天老 blob＋文档未加载才可能撞，窄 ⇒ 不在本包范围）。

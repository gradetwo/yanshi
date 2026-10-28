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

**⇒ 本轮（2026-10-27 下半场～10-28，两份审计报告修复中）**：
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
  - `browser-dodge-burn`／`browser-range-mask`／`browser-frequency-separation` **已定性、已修（判据侧接线）**：
    红因是**开机接线窗口**——下拉在 `loadDocumentData()` 的 `refreshLayers` 里就填充，而
    `setupRangeMaskPanel`／`setupFreqSepPanel`／`setupDodgeBurnPanel` 的监听器要到 `warmKernel()`
    尾部（`await initWasm()` 之后）才挂 ⇒ 窗口内程序化 change/click 是**静默空操作**。
    判据改成**先等前提、再下结论**：dodge-burn 用 `trioAndRead` 重试到回执出现；range-mask 用
    「翻 `rangeMaskKind` 到 color 看亮度行隐藏、翻回看恢复」探针（syncRows 是监听器 ⇒ 换行成功
    证明 click 也已挂）；freq-sep 用**差分探针**（`layer_empty`↔`layer_ok` 两次读数必须**不同**）——
    第一版「读数非空即算挂上」是**假阳性**（`renderFreqSepLayers` 开机就调 `renderFreqSepCurrent`），
    实测踩到后改正。**产品侧的窗口本身未修**（把 setup 提前要处理 `warmKernel` 第二个调用点
    与重复监听，需单独一轮）⇒ 已记台账 §64。
  - 同族另外两条：`browser-range-mask` ⑨／`browser-frequency-separation` ⑧ 的
    「本地内核没有这个端点」断臂**已按新契约重写**——`48f3190`（changes.patch 的 callTool 路由）
    把改文档的工具**一律送服务端**（`mutating: true` ∉ `LOCAL_READ_TOOLS`），勾掉「服务器渲染」
    也如此 ⇒ 旧断臂在服务器来源页**结构性不可达**（CI 证据：`3c8cf8cd` 之前绿、`be9ed57f` 红）。
    新断言：①界面不许谎称本地内核缺端点（反向假归因），②烘焙必须如实落服务端且卡片念服务端回执。
    `endpoint_not_local` 那条**真guard现在只在 localOnly PWA 里可触发** ⇒ 要保住它需一个
    单独的 PWA 夹具轮次（已记台账 §64，未开）。
  - `tool-delete-document` **已修（判据侧计时）**：GC 隔离包（`39511fc6`）把 `delete_document`
    改成只删年龄 ≥60 秒的 blob ⇒ 判据当场新建再删的 blob 永远是「新鲜的」⇒ 只可能拿到
    `blobs_deferred_fresh`。修法是删前把专属 blob 的 mtime 回拨 120 秒（判据自己造输入），
    并**双契约分支**：回拨成功 ⇒ 断 `deleted≥1`＋404；回拨失败 ⇒ 断 `deferred_fresh≥1`＋200。
    本机 `blobs_deleted=1`、释 166 B、404 ✓。
  - 上述四条**本机全绿**（无预开标签页驱动 `/tmp/browser-repro-noprefix.sh`＋debug 二进制）；
    以 CI 为准。
  - `tool-export-idempotent` **CI 独有红：真因已查实并修复 —— 同工作区的判据间污染**（台账 §70 ✓）：
    常驻观测 `kept_no_recipe_reasons` 下一跑 CI 即报 `omitted=0／原因={"reads_canvas":4}`；本机按
    CI 的分片编排把 `tool-brush-settings` 跑在前、同一工作区再跑本判据 ⇒ **逐字节复现同一红**
    （bytes=35840、`reads_canvas:4`）——`tool-brush-settings` 拿 `list_brushes` 的 `names[0]`
    （`100%_Opaque`）做 `set_brush_setting(smudge=0.375)` **不还原** ⇒ 覆盖文件落进工作区笔刷缓存 ⇒
    `resolve_asset` 先查缓存 ⇒ 本判据的"不透明"笔触全被读成**读画布** ⇒ `omitted=0`。
    修法＝给写方加**第⑤条自清理断言**（写回原值＋当场读回核对，失败则**本判据**红而非下游怪红 ✓）；
    复跑序列 ⇒ `omitted=2／reads_canvas=1`，与单独跑一致 ✓。
    （历史：debug/release、笔刷内容、git-lfs、笔触失败都逐一排除过；分类观测与判定共用一份实现
    `brush_source_is_replayable`／`brush_source_replay_rejection` ⇒ 无两份漂移 ✗；夹具补的
    `oil-03-paint` 一笔让 `reads_canvas` 分支**输入能触发** ✓；变异：摘掉分类计数行 ⇒ 恰好两条新
    断言红 ✓。顺带修的 `kept_missing += 1` 双写＝死写，诚实记录：无断言可钉，仅删重复行。）
  - `browser-frequency-separation` ⑧ **CI 复发红（`{page:""}`，run 40505569696；上一跑 40502062240 绿）
    ⇒ 判据侧硬化**：接线差分探针过了而卡片 20 秒仍空 ⇒ 与 fill-region ② 同族的"点击落在忙窗"候选。
    改成**有界补按**（每 1 秒补一次、两次都空才算红）＋失败详情带**方向证据**
    （`pick`／`click` 在位读数＋`layers`／`atoms` 差值 ⇒ 下次红能分清"没出门"与"出了门没念"）。
    本机复跑**当场在同族两臂（① 现状行、② 半径 0 拒绝）复现同型红** ⇒ 修复推广到**全部
    拒绝臂**（`runToInfo`：先清卡片再按——拒绝文案与上一臂相同时不清就是假阳性 ✓）；
    本机 13/13 绿。另：RW2 遗留的 `kernel-wasm-allowlist` 未登记已补（台账 §68）。
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
- **外部补丁包 `photo-bugfix-rw2-oom后续包.zip` 已合入**（`d686c622`）：81MP RW2 import OOM，
  分块扩散（halo=64 ⇒ 中心行逐位一致、峰值 5.6GB→1.6GB）＋位图缓存单飞（认领+Condvar，wasm 门控）。
  本机独立验证与两例变异复现见台账 §65；盒上 81MP 复现（7522MB 死 ⇒ 4054MB 活）为作者侧证据，
  本机无原图与大内存盒子 ⇒ 未独立复跑。全量 test 留 CI。
- **skill-vector-tracing v5.3 后续包已合入**（`764cba98`＋`4820c40e`）：SKILL.md 307→437 行，
  P0 修正（`draw_shape` 没有 `fill` 键——**已对 HEAD 的 `parse_shape` 独立核对**）＋第 4 轮 13 条晋升，
  B12/B13/B16 泛化门未过退回附录。校验见台账 §66。纯文档不触发 CI。
- **外部补丁 `viewer-app.patch` 已合入工作树并验证**（本轮 ✓，台账 §69）：预镜像 `4efddbc4` 逐字节相同 ⇒
  `git apply` 干净（85+/7−，七段各治一事）。**验证踩了两次 freshness 坑**：`viewer-app.js` 是
  `include_str!` 内嵌进二进制的 ⇒ 必须重建才测得到补丁；而 `cargo build --debug` 非法参数被管道吃成退出码 0 ⇒
  第一轮测的是昨天的旧二进制（8 项红恰好全在补丁要修的路径上 ⇒ 反证对象正确）。重建后（`29b7a014-dirty`）：
  `browser-ui-check` **8→6**（「回到此处」跳转后指纹回拨成功 ✓、隐藏图层 10726→0→10726 恢复 ✓，
  剩余 6 项全在 known-red 既有族里 ✓ 无新增回归）、**`browser-brush-preview` 本机转绿（EXIT=0）**——
  它正是 CI 在 `29b7a014` 上那条「换笔刷没有换预览」独有红，补丁的 `refreshPreview()+resync()` 段对症 ✓。
- **v0.3.1 已发布**（tag `v0.3.1` → `4b534cf1`，本轮 ✓）：版本号只改 `workspace.package.version`
  与 6 个 medium crate 自版本（0.3.0→0.3.1），crate 间依赖要求维持 `"0.3.0"`（^0.3.0 接受 patch ✓），
  Cargo.lock 由 `cargo update --workspace` 更新（21 个 yanshi 包 ⇒ 0.3.1，第三方零变动）。
  三个提交一并推上（污染修复 `e2a9ec41`、viewer 补丁 `8110e32c`、bump `4b534cf1`）⇒
  tag 触发 `release.yml`（四平台打包＋冒烟＋汇总校验和，产物由 CI runner 建，本机不产包 ✓）。
  **诚实记录**：bump 的提交信息写「workspace clippy 不在本机跑 ⇒ CI 把关」，但**提交钩子当场把
  全部闸门跑齐且全绿**（fmt／workspace clippy／台账／四条自测／发布工作流探针）⇒ 那句是**低估** ✗
  ⇒ 在此纠正 ✓。**发布证据链（都指向 `4b534cf1`）**：CI `40599927930` **success**（6 个 criteria 分片全绿，
  其中分片 6/6 里 `✓ tool-brush-settings` 之后 5 秒即 `✓ tool-export-idempotent` ⇒ ⑤ 清理的 CI 侧实证 ✓）；
  Release `40599947239` **success**，Release 5 个资产齐（4 平台包 ＋ `SHA256SUMS`，`gh release view v0.3.1` 实录）。
  同提交的 PWA `40599947304` **failure** —— **不是本版回归**：判据自己 90 秒看门狗把成功跑成了 `exit 3`，
  已查实并修复（见下一条与台账 §71）。
- **判据间污染查实：`tool-brush-settings` 不还原共享笔刷 ⇒ `tool-export-idempotent` 的"CI 独有红"真因**
  （本轮 ✓，台账 §70）：常驻观测下一跑即报 `原因={"reads_canvas":4}`；本机照 CI **分片编排**串跑两判据 ⇒
  逐字节复现（bytes=35840 一字不差）⇒ 定性完成：**不是环境差异，是共享工作区里写方不还完 `100%_Opaque` 的 smudge** ✗。
  修法＝写方加 ⑤ 自清理（写回原值＋当场读回核对）⇒ 串跑复验 `omitted=2／reads_canvas=1` 全绿 ✓。
- **PWA 工作流常红查实：判据自己的 90 秒看门狗把"成功"变成 `exit 3`**（本轮 ✓，台账 §71）：
  `browser-pwa-local.mjs` 四条断言全过、23:11:31 已打印 ✓，23:12:46 却打印「判据超时」退 3 ——
  根因是那条 `setTimeout` 是 **ref'd** 的，**成功路径从不 `clearTimeout`** ⇒ 事件循环被它占着到 90 秒整
  ⇒ 每次成功都变红 ✗（本机逐秒复现：`1:30.14 total` + 同一句「判据超时」）。修法＝收尾之后、
  成功打印之前 `clearTimeout`（收尾本身仍归看门狗管 ✓）⇒ 本机 `16.2s / 退出码 0` ✓。
- **部署后核对 `verify-live` 三处都坏，已修**（本轮 ✓，台账 §72）：① 名单是**手抄副本**（4 张，
  其中 2 张在 `4f9de7b0` 已删、而它漏改）⇒ 每次 `exit 3`；② origin 指向**官网** `yanshi.wangda.today`
  （PWA 在 `yanshi-online.wangda.today`）⇒ 那个地址的 `/showcase/…` 永远 404；③ 只在**手动触发**上跑，
  而部署**只**发生在 tag 上 ⇒ "部署了但从不核对"。修法：名单**从 `web/index.html` 提取**（唯一来源）＋
  origin 改 PWA 主机 ＋ `if: needs.deploy.outputs.skipped != '1'`（dry run／缺 secret 不跑）＋
  每张图 5 次 × 3 秒重试（吸收刚部署的边缘延迟；连不上仍退 3）。**CD 实况（重要）**：线上 PWA 仍是
  **10-15 的旧部署** —— `/showcase/*` 全部被 SPA 回落成 `index.html`（HTTP 200 ＋ 759113 字节 ＋ `text/html`），
  因为 10-17 起的 4 次 PWA 工作流**全失败**（10-17／10-24 各自步骤红、10-27 看门狗、10-27 本核对）
  ⇒ **∴ 没有任何一次成功部署** ✓。修好的核对对线上**如实报 exit 1**（不是它自己坏 ✓）。**待用户裁决**：
  是否现在手动触发一次真部署（`workflow_dispatch` ＋ `dry_run=false`）把线上刷新到当前 main。
- **外部补丁包 `photo-bugfix-orf-detect后续包.zip` 已合入**（`bc28716a`，`git am` 零冲突）：
  RAW 嗅探补 `has_raw_magic`/`sniff_format` 对 ORF（IIRS）识别。本机验证与独立变异
  （IIRS⇒IIRX ⇒ 必红、恢复 ⇒ 97 测试全绿）见提交信息与台账 §62。
- **外部补丁包 `photo-bugfix-gc-rootset后续包.zip` 已合入**（`2f1e7748`，`git am` 零冲突）：
  `gc_inputs` 的根集加**磁盘文档扫描**（`persist.list_documents()` → `load_atoms` →
  `all_blob_refs`，fail-closed），补上 §61 记的「`run_gc` 根集只看本进程」缺口。
  本机验证与变异（删掉磁盘扫描 ⇒ 复现作者声称的红；`gc_disk_doc_roots` 3/3、
  `blob_gc` 5/5 等）见提交信息与台账 §63。
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

62. **photo-bugfix-orf-detect 后续包已合**（`bc28716a`，基线 `be9ed57f`＝当时 HEAD，`git am` 零冲突）：
   ORF 的魔数是 **IIRS**（`0x5352`）而 `decode.rs` 的 `has_raw_magic`/`sniff_format` 只列了 IIRO ⇒
   olyC5060 的合法 ORF 在 `put_blob` 被判 `image/png`、`import` 带 `raw_options` 报 `invalid_argument`。
   修法：两处白名单各加 IIRS→orf，单测＋探针各加 IIRS 断言（假 IIRS 垃圾字节仍判 None）。
   **本机独立验证**：修前复现红、修后盒上 release 二进制 5 张全过（olyC5060 2608×1949、E-3、NEF、CR2、DNG）；
   `yanshi-raw` debug 全套 **97/0**；**变异 IIRS→IIRX ⇒ 目标单测红**（恢复后全绿）；fmt＋clippy 干净。
   全量 `cargo test --workspace` 留 CI。rawler 下游本来就能解（TIFF 读数不校验魔数值，按 Make 标签分发）。

63. **photo-bugfix-gc-rootset 后续包已合**（`2f1e7748`，基线 `bc28716a`＝当时 HEAD，`git am` 零冲突）：
   补上 §61 记的缺口——`run_gc` 的根集只看本进程内存文档 ⇒ **磁盘上已落盘、本进程从未加载**的文档
   引用的旧 blob（≥7 天 TTL）被判孤儿删掉，文档再打开缺像素。修法：`gc_inputs` 枚举落盘文档
   （跳过已在内存的）`load_atoms` 并日志根进 `extra_roots`，**读失败即报错不回收（fail-closed）**。
   **本机独立验证**：新测试 `gc_disk_doc_roots` **3/3**（修前 1 绿 2 红；**变异：撤磁盘扫描 ⇒ 主测试复红**；
   真孤儿照删）；双服务复现（盒上 release）修前 `expiring=1` 且 confirm 后文件消失、修后 `expiring=0` 文件保留；
   `delete_document_blob_grace` 2/2、`blob_gc` 5/5、`delete_with_subscriber` 1/1、`two_writers_seqs` 2/2 全绿。
   范围说明（作者自述＋我核对）：日志引用是活跃 Manifest 的超集 ⇒ 只并日志根就够；
   钉住 blob 纯内存、无落盘 ⇒ 不在本包范围。全量 test 留 CI。

64. **六个 browser 红收尾：开机接线窗口（判据侧已修，产品侧未修）＋ ⑧/⑨ 路由契约重定性**：
   - **接线窗口（真产品缺口，暂缓）**：面板监听器挂在 `warmKernel()` 尾部（`await initWasm()` 之后），
     而下拉在 `loadDocumentData()` 就填好 ⇒ 窗口内程序化 change/click **静默空操作**
     （用户手动点击不受影响，判据是程序化的 ⇒ 先红）。判据改成「等前提、不等结论」：
     `trioAndRead`（dodge-burn）、`rangeMaskKind` 翻色回亮度验换行（range-mask）、
     **差分探针**（freq-sep：`layer_empty`↔`layer_ok` 两次读数必须不同——「读数非空」是假阳性，
     因 `renderFreqSepLayers` 开机就调 `renderFreqSepCurrent`，实测踩到后改正）。
     **产品侧为何不随手修**：把 setup 提前要处理 `warmKernel` 第二个调用点
     （`viewer-app.js:13190`）与重复监听风险 ⇒ 需单独一轮，不塞进本轮。
   - **⑧/⑨ 的新契约**（`48f3190` changes.patch 的 callTool 路由：`!serverRenderPreferred()` 时也只有
     `LOCAL_READ_TOOLS`＝`!mutating` 的工具走本地 ⇒ `create_range_mask`/`frequency_separation`
     **一律落服务端**）⇒「本地内核没有这个端点」断臂在服务器来源页**结构性不可达**
     （CI 证据：`3c8cf8cd` 该臂绿在 `48f3190` 之前、`be9ed57f` 红其后）。判据改为断
     ①不许假归因（界面谎称本地内核缺端点 ⇒ 红）、②烘焙如实落服务端且卡片念服务端回执（覆盖数/半径）。
     **代价**：`endpoint_not_local` 那条真guard现在只在 localOnly PWA 里可触发 ⇒
     要保住它需要一个**单独的 PWA 夹具轮次**（未开，登记在此）。
   - **`tool-delete-document` 跟 §61 的 60 秒门槛走**：判据把专属 blob mtime 回拨 120 秒（自己造输入），
     双契约分支（回拨成功 ⇒ `deleted≥1`＋404；失败 ⇒ `deferred_fresh≥1`＋200）。本机
     `blobs_deleted=1`、释 166 B、404 ✓。
   - `browser-fill-region` ② **CI 又红一次（`40502062240`，本机先前绿）**：`info=""` ⇒
    与 §64 的接线窗口同族（`setupFillRegionPanel` 挂在 `warmKernel` 里、监听器可能尚未接上，
    固定 400 ms 一把读在慢机上也不够）。判据改成**有界重点击 + 轮询**（最多 30 s；宽 0 这一路
    不发请求 ⇒ 重点击不会写原子）。本机复跑全绿。
  - 四条修改（`browser-dodge-burn`／`browser-range-mask`／`browser-frequency-separation`／
     `tool-delete-document`）**本机全绿**（无预开驱动＋新 debug 二进制）；
     **CI 已验（run `40502062240`）**：shard 2/6 与 6/6 里 range-mask／frequency-separation／
     dodge-burn／delete-document **全部转绿** ✓；剩余红＝`tool-export-idempotent`（CI 独有，
     未决，见上）＋ `browser-fill-region`（本轮新修，见上一条）。

65. **photo-bugfix-rw2-oom 后续包已合**（`d686c622`，作者侧提交 `aac1e9d`，基线 `be9ed57`＝当时远端 HEAD，
   `git am` 零冲突；patch pre-image blob `3ef57e8`/`17bb456` 与合入前 HEAD 逐一相符）：
   81MP RW2（panaG9_P1000475，10402×7794）import 时 OOM-killer 杀服务（峰值 RSS 7522MB > 7.2GB 可用）。
   两处叠加：①主因 `highlight_chroma_recover` 全幅数组 4.19GB ⇒ **按行分块**（中心 256 行 + halo=步长和 64，
   reflect 钳读只污染将被丢弃的 halo 行 ⇒ 中心行逐位一致，块缓冲 ≈160MB）；
   ②次因 `BitmapCache::get_or_decode` 锁外解码无单飞 ⇒ 4 条带同键并发各跑一遍全幅 develop ⇒
   **在途认领 + Condvar 等候**（wasm 门控保持旧行为；RAII guard 兜底 panic 路径，等候者不饿死）。
   **本机独立验证**：fmt＋`clippy -p yanshi-raw -p yanshi-render --all-targets -D warnings` 干净；
   `chroma_pkg5` **5/5**（含 `tiled_matches_full_frame` 带触发断言，饱和行横跨 y=256/512 两块界）、
   `buffer_reuse_golden`+`highlight` 15/15、单飞单测 1/1、`--test bitmap_cache`+`tile_parallel` 23 过 1 忽略；
   **变异独立复现两例**：halo 改 0 ⇒ `tiled_matches_full_frame` FAILED；认领改恒真 ⇒ 单飞测试 FAILED；
   恢复后全绿、`MUTATION-TEST` 标记 grep 为 0、工作树干净。
   盒上证据（作者侧，本机无 81MP 原图与 7.2GB 盒子 ⇒ **未独立复跑**）：base 死在 import／7522MB，
   fix50 与 rebase 峰值 4041/4054MB、PNG sha256 三轮逐位相同（确定性）。
   作者自述边界（我核对认可）：预览降分辨率开发（`develop_raw_source_preview`）会改预览像素 ⇒ 另立项；
   `warp_rgb` identity 拷贝未动（本文件无 warp plan）；>100MP 或更小内存机器仍可能吃紧。
   全量 `cargo test --workspace` 留 CI。

66. **skill-vector-tracing v5.3 后续包已合**（`764cba98`＋`4820c40e`，基线 `be9ed57`，`git am` 两提交零冲突）：
   单文件 `skills/yanshi-vector-tracing/SKILL.md` 307→437 行（+152/−22）。
   **校验（本机 ✓）**：0001 pre-image `a85dd5e`＝HEAD、末态 blob `543e516`＝patch `index` 行；
   合入后文件 sha256 `1abc4398…` 与包内定稿 `SKILL.md` 一致；`check-skill-tool-names.mjs` ✓
   （7 文档／标识符 105／工具 206／参数 260 全能找到）。
   **P0 修正独立核对 ✓**：`object.rs::parse_shape`（HEAD 实测）确无 `fill` 键、`has_fill` 恒真起步 ⇒
   `data.fill` 被静默接受但无效果，「填充色就是 `color`；非 path 省略 `color` 默认填黑」属实。
   内容：第 4 轮 13 条 B 档晋升入正文（B20 正文已有原文）；**B12/B13/B16 泛化门未过退回附录 R4**
   （第二验证都在企鹅同一张图，不构成独立图类）；诚实记录随包（Round 4 时间目标未达、
   引擎静默接受未知字段未修——本轮只在 skill 侧改正）。
   纯 skill 文档 ⇒ 不触发 CI（`paths-ignore`），随同批产品代码过 CI。

67. **`kept_no_recipe` 的原因分类常驻进回执**（第 230 轮 ✓，`15d04dfe`，为 CI 独有红定位）：
   CI 恒 `kept_no_recipe=3／omitted=0`、本机恒 `omitted=2` —— debug/release、笔刷内容、
   git-lfs、笔触成功率都排除后，只剩"CI 上配方**因何**被判不可重放"不可观测 ⇒ 按
   「不再读代码猜」把观测做成**常驻**：`ExportStats`/回执/`BUILD-INFO` 增
   `kept_no_recipe_reasons`（`no_source`／`kind_not_brush`／`brush_absent`／`brush_unreadable`／
   `reads_canvas`／`unreferenced`／`all_replayable_but_kept`）。**分类与判定共用一份实现** ✓
   （`brush_source_replay_rejection` 抽出，bool 只是 `.is_none()` 包装 ⇒ 无两份漂移 ✗）。
   判据钉恒等式 `各原因之和 == kept_no_recipe` ＋ `reads_canvas ≥ 1`；**输入能触发**这一层
   靠夹具补一笔 `oil-03-paint`（`smudge=0.9` ⇒ 读画布 ✓ —— 从前本机 `kept_no_recipe=0`
   ⇒ 新断言会恒绿成安慰剂 ✗）。变异复现：摘掉分类计数行 ⇒ 恰好两条新断言红、其余不动 ✓。
   顺带：删第 229 轮临时 `YANSHI_TRACE_EXPORT_WANTED` 探针（观测已常驻）；修
   `service.rs` 的 `kept_missing += 1` **写两遍**（`00220ddd`＋`e7a4402d` 各加一行）——
   诚实记录：该路径整单 `Err`、统计不回执 ⇒ 是**死写**，无断言可钉，仅删重复。
   **下一跑 CI 看 `kept_no_recipe_reasons` 报的是哪个名字**，那是这个红的真答案。

68. **freq-sep 判据的「点击落忙窗」整族修复 ＋ RW2 遗留的 wasm 白名单登记**（`2043a4ea`＋本轮）：
   ① CI `40505569696` ⑧ 复发红 `{page:""}`（上一跑绿）＋本机复跑**当场在 ①/② 臂复现同族红**
   （`选到空层后现状行没跟着走`、`半径 0 拒绝拿到 page:""`）⇒ 定性坐实：**接线探针过后仍有
   忙窗把 click/change 吃调**（不是产品 ✗，旧判据固定 sleep／立即读是测量陷阱 ✓）。
   修法＝**有界补按＋先清卡片**（`runToInfo`：拒绝路径不发请求不写原子 ⇒ 补按零副作用 ✓；
   不清卡片的话半径 70 那臂会被半径 0 留下的**同款文案**假阳性放过 ✓——旧判据正是这样混过去的）；
   ⑧ 另加失败详情方向证据（`pick`/`click` 在位读数＋`layers`/`atoms` 差值）。
   本机 13/13 全绿（无预开驱动＋新 debug 二进制）。② RW2 合并（`d686c622`）新增的
   `render.rs` 单飞 wasm32 等待分支被 CI `40508963270` 的 `kernel-wasm-allowlist` 抓到未登记 ✓
   —— 本机合并验证跑了定向 test 与变异、**没跑这条主题判据** ⇒ 教训：合并验证要连同
   **相关主题的既有判据**一起跑，不只是新测试。已登记（调度/等待簿记、不改解码字节 ⇒ 像素逐位一致 ✓），
   本机该判据绿（9 处＝9 条）。

69. **viewer-app 补丁已合（前端一处七段）**（`~/patch/viewer-app.patch`，本轮 ✓）：
   单文件 `crates/yanshi-http/assets/viewer-app.js`（85+/7−），预镜像 `4efddbc4` 与当时文件**逐字节相同** ✓
   ⇒ `git apply` 干净落地。七段各治一件事：**wasm 借用中毒抛错 ⇒ 捕获后回收内核重开**（`resyncFallbacks`）、
   撤销栈 `{kind:"head"}` 与 `{kind:"atom"}` 条目分开处理、**「回到此处」先回拨 head 再撤销**、
   `atomIdOf` 统一 `get_log.atom_id` 与 `/api/atoms.id` 两种 id 字段（单源 ✓）、
   拾取用例缺 bbox 时**不再谎报「此处没有对象」**、内置画笔提交后**显式 `refreshPreview()+resync()`**。
   验证口径（纯 JS 资产，无 Rust 改动 ⇒ 不开 `cargo test --workspace` 例外）：`node --check` ✓、
   标识符逐处对读（`atomIdOf` 定义与三处调用、`head` 契约、`failureCode`、`resyncFallbacks` 被
   `browser-ui-check` 读数 ✓）、`check-viewer-unique-ids`（359 id 唯一/303 被引用 ✓）、
   `check-blank-overwrite-guard` ✓、`pwa-sync-viewer` 重新生成 `web/`（tracked 的 `web/index.html` 随补丁一起提交 ✓）、
   防分叉（`tool-pwa-assets` 的归一化字节比对部分）✓ —— 该判据本机 EXIT=1 只红在**六个介质 .wasm 产物过期**
   （本机构建产物问题，CI 重建 ⇒ 与本补丁无关 ✓，如实记录）。**教训（freshness 双坑）**：
   `viewer-app.js` 是 `include_str!` **内嵌进二进制**的 ⇒ 本机跑浏览器判据前必须**重建**才测得到补丁；
   而 `cargo build --debug` **不是合法参数** ⇒ usage 错误被 `| tail` 吃成退出码 0，**测了昨天的旧二进制**
   ✗（8 项红全部落在补丁正要修的路径上 ⇒ 反证补丁对象正确）。改完跑法＝先 `ls -l` 比时间戳再跑。
   浏览器判据结果：见本轮 §二 bullet 与提交信息。

70. **判据间污染：`tool-brush-settings` 改共享笔刷不还原 ⇒ 下游 `tool-export-idempotent` CI 独有红**（本轮 ✓）：
   `run-criteria.sh` 的分片是**一个临时工作区跑一串判据**，而 `set_brush_setting` 写的是**工作区笔刷缓存**
   （`resolve_asset` 三档里缓存**优先于** `--assets-dir` ⇒ 覆盖文件对同工作区后续**所有**判据生效 ✗）。
   `tool-brush-settings` 拿 `list_brushes` 的 `names[0]`（数字名排最前 ⇒ `100%_Opaque`）把 `smudge` 写成
   0.375 **不还完** ⇒ 同分片后面的 `tool-export-idempotent` 把该笔所有"不透明"笔触判成 `reads_canvas`
   ⇒ `omitted=0` ⇒ ③ 红。本机按同编排**逐字节复现**（bytes=35840、`原因={"reads_canvas":4}` 与 CI 一字不差 ✓）
   ⇒ **CI-only 的定性就此终结**：不是 CI 环境差异，是**分片内先后顺序**（本机一直单独跑 ⇒ 永远绿 ✗）。
   修法＝**写方自清理**：⑤ 写回原值＋当场读回核对（还原失败 ⇒ 本判据红 ✓，不给下游留查不到根的怪红 ✗），
   复跑「settings 在前、idempotent 在后」⇒ `omitted=2／reads_canvas=1` 全绿 ✓。
   **⑤ 的变异已实跑**（2026-10-28，补记 `e2a9ec41` 提交信息里那条"推理过、未执行"的变异）：
   把还原值改成 `target`（0.375）⇒ 同编排串跑两张输出：
   `✗ 清理读回一致：smudge = 0.375（原 0）` ⇒ settings 退 1 ⇒ 下游 idempotent 复红且**与 CI 那条一字不差**
   （`omitted=0／原因={"reads_canvas":4}`）⇒ 还原文件后同编排再跑：`✓ 清理读回一致：smudge = 0（原 0）`、
   `omitted=2` 全绿 ✓。CI 侧同源实证：`40599927930` 分片 6/6 里两判据相邻且都 ✓。
   **通用教训（与第 916/904/888 轮同族：测量工具要先被测量）**：
   ① 判据**改了共享状态就必须还原**，且还原要**被断言**（不还原＝给分片里所有下游判据埋雷 ✗）；
   ② 排查"CI 独有红"时，**CI 的编排方式本身**（共享工作区分片）是被测变量之一 ⇒
     本机复现要**照编排复现**（串跑），单独跑得出的"本机绿"不构成分歧证据 ✗。

71. **PWA 真实浏览器判据：自己的 90 秒看门狗把"成功"变成 `exit 3`**（2026-10-28 ✓）：
   CI `40599947304` 的分片日志把时序写得很清楚：23:11:31 打印
   `✓ PWA 在真实浏览器里成立：内核加载 ✓｜/health 如实 ✓｜未实现端点 501 ✓｜缺快照 needs_render ✓`，
   23:12:46 又打印 `❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时 ⇒ 退出码 3`。
   **根因**：第 620 轮那条"总超时守卫"的 `setTimeout(..., 90000)` 是 **ref'd** 的，
   成功路径**从不 `clearTimeout`** ⇒ 四条断言全过、成功文案已打印，进程仍被这个定时器占着
   ⇒ 空等满 90 秒 ⇒ 被自己杀掉退 3。**于是每次成功都变红**（PWA 历史失败自 10-17 起持续，
   上一次成功在 10-16 前后 ⇒ 与"看门狗是在 10-16 之后加的"一致 ✓）。
   **本机逐秒复现**：`node scripts/browser-pwa-local.mjs` ⇒ 末尾同一对文案，`1:30.14 total`。
   **修法**：`clearTimeout(__judgeTimeout)` 放在**收尾调用之后、成功打印之前** ——
   收尾（`socket.close`／`chrome.kill`／`server.close`）仍归看门狗管 ✓（收尾挂住时**正该** exit 3），
   之后已无待办 ⇒ 撤销它让进程自然退出。**复验**：同一命令 `16.2s`、**退出码 0** ✓。
   **变异（红）**：去掉这一行 ⇒ 回到 90 秒 `exit 3`（就是修改前的实测，不是推理）。
   **教训**："防挂死"的看门狗**本身就是 ref'd 句柄** ⇒ 它必须**在正常完成时被撤销**，
   否则"防挂死"会变成"保证挂满" ✗。凡是 `setTimeout` 做超时守卫的判据都要问一句：
   **成功路径会不会就是被它按住的那一个**？

72. **`verify-live` 三处坏：名单副本、origin、触发条件**（2026-10-28 ✓）：
   **症状**：手动触发的 PWA run `40602674041`（dry run，部署 job 已全绿）里 `verify-live` 报
   `❌ 判不了：本地缺 assets/showcase/degas-rehearsal-morning.png ⇒ 退出码 3`。
   **查实（三处，各自独立）**：
   ① **名单是副本** ✗：`verify-live-showcase.mjs` 手抄了四张；`4f9de7b0`（10-17，按用户要求换芭蕾成品）
      删了两张、同步改了 `tool-showcase-assets.mjs`，**漏改本文件** ⇒ 本地缺文件 ⇒ 永远 exit 3。
   ② **origin 错** ✗：脚本与工作流都写 `yanshi.wangda.today` —— 那是**官网**（首页 HTML 不同、
      图在 `/assets/showcase/`）；PWA 在 `yanshi-online.wangda.today`（`wrangler.toml` 的 `routes`）。
   ③ **触发条件反过来** ✗：核对只挂 `workflow_dispatch`，而部署发生在 **tag push** ⇒ 真部署从不核对。
   **修法**：名单从 `web/index.html` 的 `src="showcase/…"` 提取（唯一来源，抽不到 ⇒ exit 3）；
   origin 改 PWA 主机；`deploy` job 暴露 `skipped` 输出 ⇒ `verify-live` 用
   `needs.deploy.outputs.skipped != '1'`（dry run／缺 secret 都写 1 ⇒ 没部署就不核对 ✗）；
   每张图 5 次 × 3 秒重试（刚部署时边缘可能还在发旧版本），连不上仍 exit 3。
   **四层验证（本机）**：本地静态服务（`web/`）⇒ exit 0（2/2 张 ✓）；
   变异 a＝截断一张 ⇒ `✗ 100000 vs 265096` ⇒ exit 1；
   变异 b＝把另一张换成 HTML（模拟 SPA 回落）⇒ `✗ 不是 PNG` ⇒ exit 1（**HTTP 仍是 200** ⇒ 证明
   "只查状态码会假绿" ✓）；死端口 ⇒ 连着 5 次失败后 exit 3（不冒充内容错 ✓）。
   **CD 实况**：线上 PWA 是 **10-15 的旧部署**；`/showcase/*` 全部被 `not_found_handling =
   "single-page-application"` 回落成 `index.html`（200 ＋ 759113 字节 ＋ `text/html`）。
   原因：10-17 起的 PWA run 全失败（`2e711439`／`a720cad9`／`4b534cf1` 看门狗／`6a0d8ea9` 本核对）
   ⇒ 期间没有任何一次成功部署。修好的判据对线上如实报 exit 1 ✓（不是判据自己坏）。

73. **审计 Batch 2（前半）：解压炸弹与图层父链成环**（2026-10-28 ✓）：
   ① **`zlib_decompress` 没有输出上限** ✗：上传端只限**压缩**字节（缺省 256 MiB），
   几 MB 的流可以 inflate 成几十 GB ⇒ OOM。**修法**：
   `MAX_INFLATE_BYTES = 1 GiB + 1 MiB`，`take(cap + 1)` 有界读 ⇒ **超出即 `None`**
   （不返回前缀冒充成功 ✗）；上限口径 = `MAX_DECODE_PIXELS × 4 + MAX_DECODE_EDGE`（合法最大 PNG 扫描线）＋ 余量。
   **判据**：`zlib_decompress_refuses_output_over_the_cap`（恰好等于上限可解、少 1 字节即拒）＋
   **编译期恒等式**（`const _: () = assert!(…)`：上限 ≥ 最大合法扫描线且 < 2 GiB）。
   **变异**：`>` 改 `>=` ⇒ 运行期判据红 ✓；上限改成 `1 << 30`（少 1 MiB 余量）⇒ `cargo check` 报
   `E0080: evaluation panicked: assertion failed` ⇒ 退出码 101 ✓（都是实测 ✓）。
   ② **图层父链成环** ✗：`SetProperty(parent_id)` 与"墓碑 id 重建"都能造出 `A⇐B⇐A`，
   而 `tombstone_layer` 收集后代时没有去重 ⇒ `targets` **无限增长**（删除一个图层拖死进程 ✗）。
   **修法（三道）**：`check_layer_parent_cycle`（visited ＋ 64 层上限）同时挂**提交闸**
   `validate_layer_parent_cycles`（`CreateLayer.parent_id` 与 `SetProperty(key=parent_id)` 两条入口 ✓）
   与**折叠闸** `precondition`（重放期纵深防御 ✓）；`tombstone_layer` 加 `seen` 去重。
   **判据**：`commit_rejects_a_parent_id_rewrite_that_closes_a_cycle`、
   `commit_rejects_recreating_a_layer_under_its_own_descendant`、
   `a_parent_id_rewrite_that_closes_a_cycle_is_skipped_by_the_fold`、
   `tombstoning_over_a_preexisting_parent_cycle_terminates`、
   `check_layer_parent_cycle_rejects_cycles_and_depth_bombs`。
   **变异（四条，实测）**：删提交闸调用 ⇒ 两条 `commit_*` 红 ✓；删折叠闸 ⇒ 折叠测试红 ✓；
   删深度上限 ⇒ 深度断言红 ✓；删 `seen` ⇒ 终止测试**不返回**（`timeout 5` ⇒ 退出码 124、无 `test result` 行 ✓）。
   ③ **Batch 2 后半（已做，见 §74）**：`plan_dirty_with_log` 迭代化 ＋ `MAX_REVERT_DEPTH`、
   `buffer.rs` usize 饱和运算 ＋ 像素上限、`apply_filter`／padding 共用 clamp、
   `ATTEMPTS` 上限与 `/tmp/yanshi-attempts` 启动清理、`EXPORT_RESULT_CACHE`／`EXPORT_PROOF_CACHE` 上限。

74. **审计 Batch 2（后半）：递归／回绕／无界缓存**（2026-10-28 ✓）：
   ① **撤销链递归** ✗：`plan_dirty_with_log` 沿 `Revert.target` 递归 ⇒ 100 层链就能吃光栈。
   **修法**：先**迭代**剥掉内层 Revert（`MAX_REVERT_DEPTH = 64`），再按**跳数**一次性放大；
   `amplify_revert` 保留 1 跳时的旧文案（`撤销 {kind} 无几何线索`／`撤销几何修改`）⇒
   浅链的 dirty 集与文案**逐字不变** ✓。**判据**：`a_nested_revert_chain_resolves_iteratively`（3 跳
   仍与旧行为一致）、`a_revert_chain_deeper_than_the_cap_falls_back_to_full`（100 跳 ⇒ Full ＋ 文案含「超过」）。
   **变异**：`MAX_REVERT_DEPTH` 64→200 ⇒ 深链判据红（退出码 101 ✓，实测）。
   ② **Buffer u32 回绕** ✗：`width × height × 4` 在 u32 里回绕 ⇒ 小分配骗过大检查。
   **修法**：`MAX_BUFFER_PIXELS = 1 << 28`（与 `MAX_CANVAS_PIXELS` 同口径），
   `pixel_count` 用 `saturating_mul` ＋ 超限 panic；`doc_width/doc_height`、`len()`、
   `from_f32(_vec)`、`to_rgba8_blocks` 全部改饱和运算。
   **判据**：`oversized_buffers_are_rejected_before_allocating`（`#[should_panic]` 断文案）、
   `doc_extent_saturates_instead_of_wrapping`（2×2、`step = u32::MAX` ⇒ 文档尺寸 `u32::MAX`、`len()==4`）。
   **变异**：删上限断言 ⇒ 判据红（panic 文案不含「Buffer 尺寸过大」，退出码 101 ✓）；
   `doc_width` 的 `saturating_mul` 改回 `*` ⇒ 调试构建溢出 panic ⇒ 判据红（退出码 101 ✓）。
   ③ **滤镜参数无界** ✗：`box_blur` 的 radius／passes、`gaussian_blur` 的 σ、`motion_blur` 的 distance
   全来自 JSON ⇒ 一次调用可让单线程算几小时／容量溢出。
   **修法**：`MAX_BLUR_RADIUS=4096`、`MAX_BLUR_PASSES=4`、`MAX_BLUR_SIGMA=256`、`MAX_MOTION_DISTANCE=512`；
   **实现函数**里夹（覆盖所有调用点 ✓）⇒ `FilterKind::padding` 用**同一组常量**申报邻域
   （否则分带少看邻域 ⇒ 接缝 ✗），加法全改 `saturating_add`。
   **判据**：`oversized_filter_params_are_clamped_to_the_declared_caps`（超限与显式上限**逐像素相同**）、
   `oversized_params_padding_matches_the_caps`、`blur_param_clamps_are_the_declared_constants`。
   **变异**：删 `padding` 里的 `.min(MAX_BLUR_RADIUS)` ⇒ padding 判据红（退出码 101 ✓）；
   `.min(MAX_BLUR_RADIUS)` 改 `.min(MAX_BLUR_RADIUS/2)` ⇒ 常量判据红（已记在测试注释里）。
   ④ **试画表无界** ✗：`ATTEMPTS` 每次 `try_stroke` 存整张 RGBA ⇒ 无界内存；
   原注释声称「超 100 条由 list_attempts 负责淘汰」✗，而 `list_attempts` **只读不写** ⇒ 从未淘汰过 ✗；
   `/tmp/yanshi-attempts/<doc>` 与 `log.jsonl` 也无界。
   **修法**：每文档 `MAX_ATTEMPTS_PER_DOC = 100`，按 `created` 序号淘汰最旧（**只淘汰同文档** ✓），
   **随响应报告** `evicted_attempts`／`evicted_count`（不静默 ✓）；被淘汰者的 `.rgba`／`.json` 一并删；
   `log.jsonl` 在**写路径**截到最后 400 行；启动时（首次访问，`Once`）删 > 7 天的目录
   （不删光 ⇒ 保住「跨重启仍可查」的承诺 ✓）；目录根只留 `attempt_root()` 一处。
   **判据**：`attempt_eviction_removes_the_oldest_of_the_same_document_only`（含恒等式
   「淘汰数 ＋ 留存数 ＝ 原有数」与边界「恰好等于上限不淘汰」）、`attempt_log_keeps_only_the_newest_lines`、
   `attempt_cleanup_removes_only_directories_older_than_the_retention`。
   **变异（四条，全部退出码 101 ✓）**：淘汰短路 ⇒ 淘汰判据红；日志保留最旧行 ⇒ 截断判据红；
   过期条件 `>` 翻成 `<` ⇒ 清理判据红（把新目录删了、旧目录留着）；
   ⑤ **证明缓存无界** ✗：`EXPORT_PROOF_CACHE` 是 `HashSet`，键含笔刷内容哈希 ⇒ 可持续撑大。   **修法**：`MAX_EXPORT_PROOF_ENTRIES = 4096`，满了**整体清空**（缓存只省时间，不影响结果 ✓）。
   `EXPORT_RESULT_CACHE` 本来就是「clear＋insert」单条（值可能是大图 ⇒ 更该只留一份 ✓），补注释写明上限。
   **判据**：`export_proof_cache_is_bounded`（插入上限 +10 个唯一键 ⇒ 长度仍 ≤ 上限）。
   **变异**：删掉 `guard.clear()` 分支 ⇒ 长度 4106 > 4096 ⇒ 判据红（退出码 101 ✓）。
   **收尾**：所有变异改回后，`cargo test -p yanshi-server --lib 'tools_tests::attempt_'`（3 passed）、
   `export_proof_cache_is_bounded`（1 passed）、yanshi-render 的
   `revert_chain`（2）／`oversized`（4）／`doc_extent`（1）／`blur_param_clamps`（1）全绿（本机实测 ✓）；
   全量 clippy／test 仍交给 CI ✓。

75. **审计 Batch 3（便宜的安全硬化：M-1／M-5／L-1／L-5／L-7）**（2026-10-28 ✓）：
   ① **L-1 令牌熵源** ✗：`CapabilityToken` 用 `RandomState` 哈希计数器拼串 ⇒ 安全性押在 SipHash 上。
   **修法**：`/dev/urandom` 读 32 字节 ⇒ 64 位小写十六进制；熵源不可用时 **`eprintln!` 留痕**再退回旧方案
   （不静默 ✓）。**判据**：`os_entropy_really_reads_the_kernel_pool`（两次读取不同、非同全 0）。
   **变异**：`fill_os_entropy` 改 `out.fill(0); true` ⇒ 判据红（退出码 101 ✓）。
   ② **M-1 SSRF** ✗：URL 取图可打 `127.0.0.1`／`169.254.169.254`／私网。
   **修法**：`fetch_ip_is_forbidden`（loopback／private／link-local／CGNAT／IPv6 ULA 等）＋
   `url_host_port`（userinfo、`[IPv6]`、缺省端口）＋ `ensure_public_fetch_target` **逐跳**调用；
   拒绝文案含「内网地址」；显式 `YANSHI_ALLOW_PRIVATE_URL_FETCH=1` 可放行（本机调试用）。
   **判据**：`ssrf_blocklist_covers_private_loopback_and_link_local`、
   `url_host_port_parses_userinfo_ipv6_and_default_ports`、
   `fetch_url_refuses_link_local_metadata_endpoint_before_connecting`（连接**之前**就拒）。
   **变异（两条）**：`fetch_ip_is_forbidden` 直接 `return false` ⇒ 黑名单判据红（101 ✓）；
   删 `ensure_public_fetch_target(&current)?` ⇒ 元数据判据红（101 ✓，10 s 连接超时后失败）。
   ③ **M-5 安全响应头** ✗：无 `X-Frame-Options`／CSP frame-ancestors／Referrer-Policy。
   **修法**：`Response::write_inner` 对**所有**响应加 `nosniff`／`no-referrer`／`DENY`／
   `frame-ancestors 'none'`（只限 frame 指令 ⇒ 不碰内联脚本 ✓）；`web/_headers` 同步（Cloudflare 路径）。
   **判据**：`response_serialization_has_required_headers` 断言四条头。
   **变异**：删 `X-Frame-Options` 那一行 ⇒ 判据红（101 ✓）。
   ④ **L-5 头部总量／条数无界** ✗：单行 16 KiB 有帽，但行数无限 ⇒ 内存无界。
   **修法**：`MAX_HEADER_TOTAL_BYTES = 64 KiB`、`MAX_HEADER_COUNT = 100`（超限即 400）。
   **判据**：`header_total_bytes_are_capped`、`header_count_is_capped`（都含「恰好上限仍通过」边界 ✓）。
   **变异**：两个判据分别短路成 `false && …` ⇒ 各自红（101 ✓）。
   ⑤ **L-7 `medium_asset` 缺反斜杠检查** ✗：与 `texture_asset` 不一致。
   **修法**：加 `file.contains('\\')`。**判据**：`medium_asset_rejects_separators_and_parent_refs`
   （`..`／`/`／`\`／错扩展名 ⇒ 400；合法名缺文件 ⇒ 404 ✓）。
   **变异**：删反斜杠子句 ⇒ 判据红（101 ✓）。
   **收尾**：变异全部改回；`cargo test -p yanshi-http --lib`（55 passed）、
   `-p yanshi-server --lib token`（10）、`file_source`（11）本机绿；`cargo fmt --check` 0；
   全量 clippy／test 交给 CI。**未做**（需更大设计）：H-2 令牌吊销/TTL、M-2 恒时比较、
   M-3 未认证文档列表、M-6 图层数上限、L-3 Host 反射、L-4 blob 作用域、L-6 CI 权限、L-8 SHA-1。

76. **审计 H-2：令牌吊销（可跨重启）**（2026-10-28 ✓）：
   ① **问题**：`TokenAuthority::revoke` 全仓无调用；`meta.json` 的 `tokens` 只增不减，
   重开文档会把历史令牌全 `restore` ⇒ 一次泄露 = 永久有效（审计 H-2）。
   ② **修法**：新增工具 `revoke_token`（**仅 owner**，`mutating: true`）：
   `Workspace::revoke_token` **同时**删内存（`TokenAuthority::revoke`）与磁盘
   （`FileStore::forget_token` ⇒ `meta.json.tokens` 过滤后回写）⇒ 重启不复活；
   返回 `{revoked, doc_id, remaining_tokens}`；**找不到如实报 `reference_not_found`** ✗（不静默成功）；
   重复吊销返回 `revoked=false` 语义由 `Workspace` 的 `(bool, usize)` 表达。
   工具进 `Profile::Core` ⇒ `docs/tools.md` 计数改为 **126 core / 207 total**（`tool_inventory` 校验 ✓）。
   ③ **判据**：`forget_token_removes_only_that_token_from_disk`（只删目标、重复删除报 false、
   不存在文档不新建 meta）、`revoked_tokens_stop_authorizing_and_do_not_resurrect_after_restart`
   （同进程立刻失效 ＋ 重启后 `authorize` 仍失败）、
   `revoke_token_is_owner_only_and_reports_unknown_tokens`（editor ⇒ permission_denied、
   owner ⇒ 成功、再次 ⇒ reference_not_found、形态不对 ⇒ invalid_argument）。
   ④ **变异（五条，全部退出码 101 ✓）**：`forget_token` 的 `retain` 置空 ⇒ 判据红；
   删 `persist.forget_token` ⇒ 重启判据红；删 owner 检查 ⇒ editor 判据红；
   `!removed` 改 `false` ⇒ 重复吊销判据红；内存 `revoke` 改恒 `true` ⇒ 剩余数判据红。
   **收尾**：变异全部改回；三条新判据与 `tool_inventory` 本机绿；`cargo fmt --check` 0。
   **未做（需更大设计）**：令牌 TTL、`meta.json` 存哈希而非明文 —— 会在下一批单独评估。

77. **审计 M-4：install.sh 供应链硬化**（2026-10-28 ✓）：
   ① `YANSHI_REPO` 只许 `owner/name`（拒 `a/b/c`、`../evil`、`owner/..`、`./x`）✓；
   ② 版本号只许 `[0-9A-Za-z.]`（它进路径与 wrapper）✓；
   ③ 所有 curl 加 `--proto '=https' --proto-redir '=https'`，wget 加 `--https-only` ✓；
   ④ tar 解包加 `--no-same-owner`，解包后检查符号链接：绝对路径或含 `..` 一律拒绝（tar-slip）✓；
   ⑤ wrapper 不再用双引号拼路径，改为**单引号 + `'\''` 转义**（`$PREFIX` 可含 `'`/`$`/空格）✓；
   ⑥ README 写明信任边界：`SHA256SUMS` 与包同源 ⇒ 防截断/镜像错配，不防上游被控 ✓。
   **验证（实测）**：`sh -n` 0；`a/b/c`／`../evil`／`owner/..`／`./x` 四个恶意 `YANSHI_REPO`
   都在联网前 `exit 1` 且文案指出形状错；wrapper 转义产物通过 `sh -n`；
   `cargo test -p yanshi-server --test shell_scripts --test script_portability` 全绿。
   **未做**：端到端安装需真实 release（本批用直接执行守卫验证）。

78. **审计 M-6：单文档图层数上限**（2026-10-28 ✓）：
   ① **问题**：`create_layer` 无上限 ⇒ **已认证**调用方一条循环即可把图层数推到任意大，
   而存活图层会进入渲染遍历、快照与 `list_layers` ⇒ 内存与每帧成本无界 ✗。
   ② **修法**：`MAX_LAYERS_PER_DOC = 256`，在 `write_create_layer` 的 `ctx.commit(CreateLayer)` 前
   数**存活**图层（墓碑不计 ✓，与 `if_exists="reuse"` 的存活判定同一口径 ✓）；
   到顶返回 `precondition_failed`，错误里带上限与出路（**不静默丢弃** ✓）。
   `if_exists="reuse"` 命中存活图层时**先返回** ✓ ⇒ 不新建、不占额度 ✓。
   ③ **判据**：`create_layer_stops_at_the_per_document_cap` —— 新建文档自带一层 ⇒
   可建数 = 上限 − 初始数（**从状态读出，不写死** ✓）；建满后第 257 个被拒；
   到顶时 `reuse` 仍成功；删掉一层（留墓碑）后可再建一个、再建又到顶。
   ④ **变异（一条，退出码 101 ✓）**：`if false && alive_layers >= MAX_LAYERS_PER_DOC`
   ⇒ 第 257 个被接受（`ok: true`）⇒ 判据红；已改回并重跑绿。
   **未做**：上限按文档全局，不含图层组嵌套深度（另有一个父链成环判据在 Batch 2 已加）。

79. **CI 红（`886d4f52`）复盘：SSRF 挡住本机夹具 ＋ 工具数漂移**（2026-10-28 ✓）：
   ① **`test (stable)`／`test (beta)` 同一个原因**：`tests/file_transfer.rs` 的 HTTP 夹具全是
   127.0.0.1 回环服务器 ✓ ⇒ 审计 M-1 的 SSRF 默认封私网把它当成了攻击 ✗
   （`put_blob_three_channels_land_on_the_same_hash` 与 `fetch_follows_redirects_and_classifies_failures`）
   ⇒ 修法：两条测试开头显式设 `YANSHI_ALLOW_PRIVATE_URL_FETCH=1` ✓（文档给的本机调试旁路 ✓；
   本测试二进制里**没有**「必须拒绝私网」的判据 ✓ —— 那些在 lib 单测里，进程独立 ✓）。
   ② **criteria 分片 4/5/6 是同一类：H-2 加工具后，写死的数过期** ✗：
   · `tool-surface-coverage`：tools.rs 两处「核心层 125 个」＋ `docs/guide.md`「206/125」＋
     `docs/guide.zh-CN.md`「206/125」⇒ 全部改成 **126 / 207** ✓。
     **⚠ tools.rs 的「核心层 … N 个」必须保留数字** ✓：判据 ④ **找不到就红** ✓——
     我一开始改成「以 docs/tools.md 为准」，反而触发了「检查消失」✗ ⇒ 已改回。
   · `tool-examples-doc-check`：重跑生成器 `node scripts/tool-examples-doc.mjs <base>` ✓
     ⇒ `docs/design/tool-examples.md` 补上 `revoke_token` 小节、计数 207 ✓。
   · `tool-example-acceptance`：它缺省发 **editor** 令牌 ⇒ `revoke_token` 示例被记「被拒」✗
     ⇒ 新增 `OWNER_ONLY_TOOLS`（**带理由** ✓）⇒ 这类工具用 `?role=owner` 建文档 ✓；
     并把「没有这个令牌」归入「需要前置状态」✓（示例里是形状合法的**假令牌** ✓）；
     顺手把**抄了第二份**的 `needsPriorState` 正则改回**用同一个常量** ✓（同一事实只写一处）。
   ③ **本机验证**：`tool-surface-coverage` ✓（2 处源码声明与 2/2 文档数均一致）、
   `tool-example-acceptance` ✓（140 跑通｜0 被拒｜67 需前置；`revoke_token` 落 ⊙）、
   `tool-examples-doc-check` ✓、`cargo test -p yanshi-server --test file_transfer` 10/10 ✓。

80. **审计 M-2：令牌查表改常数时间**（2026-10-28 ✓）：
   ① **问题（两半）**：`YANSHI_API_KEY` 的比较**已在 Batch 1 换成常数时间** ✓
   （`server.rs` 的 `secrets_equal`，`0bd7c06f`）⇒ 剩下的是 `TokenAuthority` 的表：
   `BTreeMap<String, Principal>` 用**令牌原文**当键 ⇒ `Ord` 在**第一个不同字节**就返回 ✗
   ⇒ 单次查表耗时随「与真实键共享的前缀长度」变化 ⇒ 逐字节可测的旁路 ✓；
   而且明文凭证在服务端内存里多躺一份 ✓。
   ② **修法**：键改成 `SHA-256(token)` 的 64 位小写十六进制（`token_digest`）⇒
   `issue`／`revoke`／`restore`／`principal_of`／`authorize` **五处**统一走它 ✓；
   摘要定长 32 字节 ⇒ 一次比较 ✓，像空间仍是 2^256 ✓（令牌本身是 OS 熵 ⇒ L-1 ✓）。
   `sha2` 是工作区既有依赖（`yanshi-core` 在用）⇒ `Cargo.lock` 只给 `yanshi-server` 加一行 ✓，
   不新增供应链 crate ✓。
   ③ **判据**：`token_table_is_keyed_by_digest_not_by_plaintext` —— 表里**不出现令牌原文**、
   键长 64 且全小写十六进制、键 == `token_digest(原文)`；再验**语义不变**：
   `principal_of`／`authorize`／`revoke` 后失效／`restore` 后角色变为 Viewer、
   `restore_digest` **不再二次哈希** ✓。
   ④ **变异（一条，退出码 101 ✓）**：让 `token_digest` 返回令牌原文 ⇒ 第一条断言红
   （「表里不许出现令牌原文」）⇒ 已改回并重跑绿（`cargo test -p yanshi-server --lib token` 15/15 ✓）。
   **未做**：令牌 TTL（行为变更，待定 ✓）。`meta.json` 的明文问题见第 81 条（已做 ✓）。

81. **审计 H-2③：`meta.json` 只存令牌摘要**（2026-10-28 ✓）：
   ① **问题**：令牌以**明文**存在 `<root>/docs/<doc>/meta.json` ⇒ 任何能读 `--root` 的
   本地用户／进程直接拿到全部 capability 凭证 ✓（令牌即权限，不需要再猜 ✓）。
   ② **修法**：`DocumentMeta` 加 `tokens_hashed` 标记；`TokenRecord.token` 新写入一律是
   `SHA-256`（与内存查表键**同一个** `token_digest` ✓，一份实现 ✓）；`record_token`／
   `forget_token` 写之前先 `hash_legacy_tokens` **就地迁移**旧明文 ✓；
   启动恢复按标记分派：摘要 ⇒ `restore_digest`（**不再二次哈希** ✗），旧明文 ⇒ `restore` ✓。
   **⇒ 原 token 重启后照常可用** ✓（查表本来就先哈希 ✓）⇒ 不是行为变更 ✓。
   ③ **判据**：`tokens_are_persisted_as_digests_not_plaintext` —— 新写入的 `meta.json`
   **不含原文**、含摘要；手工写一份**旧格式**（无 `tokens_hashed`、记录是明文）⇒
   读出来是明文、下次写入迁成摘要（旧明文消失）、之后 `forget_token` 仍按原文命中 ✓。
   ④ **变异（两条）**：`record_token` 写回原文 ⇒ 新判据红（打印的 meta 里看得见原文 ✓）；
   恢复分支恒走 `restore`（把摘要再哈希一次）⇒ `file_workspace_survives_restart_with_tokens_and_render_cache`
   红（`PermissionDenied` ✓）。均已改回并重跑绿 ✓。
   **未做**：令牌 TTL（同上 ✓）。

82. **审计 L-3：`Host` 头不再无条件反射**（2026-10-28 ✓）：
   ① **问题**：`request_origin` 把请求的 `Host` 原样拼进 `*_absolute` 形如
   `http://{Host}/api/blob/…&token=…` ⇒ 伪造 Host 就把**带令牌的链接**指向攻击者域名 ✗。
   ② **修法**：只回显[可信主机]——① 本机名／回环 IP ✓；② 与 `--bind` 的具体主机一致 ✓；
   ③ `YANSHI_PUBLIC_ORIGIN` 声明的公网主机 ✓（反代部署的**显式**出口，不用放宽成"什么都信"✗）。
   `0.0.0.0`／`::` 是「监听全部」不是主机名 ⇒ 不参与匹配 ✗；形状校验（`split_host_port`）
   只允许主机名字符／方括号 IPv6／1..=65535 端口 ⇒ `/`、`@`、CRLF、未加方括号的 IPv6
   在拼 URL **之前**就被拒 ✓。不可信 ⇒ 退回 bind（bind 是 0.0.0.0 时给**相对** URL ✓）。
   **回环判定抽成一份** ✓：`origin_is_local`、`bind_is_loopback`、Host 白名单共用
   `is_loopback_host`（顺带修掉 `bind_is_loopback` 的 `starts_with("127.")` ——
   它会把 `127.evil.example` 当回环 ✗，现在按 `Ipv4Addr::is_loopback` 判 ✓）。
   ③ **判据**：`host_reflection_trusts_only_loopback_bind_and_declared_origin` ——
   localhost／大小写／127.x／`[::1]`／bind 一致 ⇒ 信；`evil.example`、`0.0.0.0`、`[::]`、
   带路径、`user@`、CRLF、端口 0／99999、未加方括号 IPv6、空 ⇒ 拒 ✓。
   ④ **变异（一条，退出码 101 ✓）**：`host_is_trusted` 首行恒 `true` ⇒ 判据红
   （`evil.example:8080` 被信）⇒ 已改回并重跑绿（`cargo test -p yanshi-http --lib` 60/60 ✓）。

83. **审计 L-2：`/health` 细节按绑定收敛 ＋ 错误不再带服务器路径**（2026-10-28 ✓）：
   ① **问题（两半）**：`GET /health` 无鉴权返回版本／commit／构建时间／工具清单／
   GPU 探测／缓存计数（侦察信息 ✗）；多处错误把 `--root`／安装目录的**绝对路径**回给调用方 ✗。
   ② **修法（health）**：绑回环 ⇒ 照旧完整 ✓；绑对外 ⇒ 只有「配了 `YANSHI_API_KEY` 且
   请求带对」（`?key=` 或 `X-Yanshi-Key`，与写权限**同一份** `provided_api_key`／
   `expected_api_key`／`secrets_equal` ✓）才完整 ✓；否则返回
   `{"ok":true,"detail":"withheld","note":…,"wasm":…}` —— **说清为什么少报** ✓（不静默 ✗）。
   **`wasm` 必须留** ✓：查看器 `initWasm()` 拿它决定要不要加载内核 ⇒ 拿掉会让对外部署的
   每次打开文档都报「服务端未启用」✗（那是功能回退，不是加固 ✗）。
   ② **修法（路径）**：资产/上传错误的路径一律只报**文件名**（`asset_label` ✓）；
   `file_source` 的「工作区根目录之外」不再回 `root=` 与解析后的绝对路径 ✓（仍说清怎么照做 ✓）。
   ③ **判据**：`health_withholds_reconnaissance_on_a_public_bind`（对外绑 ⇒ 无
   version/commit/built/build/tools/profiles、`detail=withheld`、`wasm` 仍是布尔）、
   `health_detail_switch_depends_on_bind_and_key`（四个组合 ＋ 大小写不等）、
   `asset_errors_report_only_the_file_name`；`cargo test -p yanshi-http --lib` 60/60 ✓、
   `cargo test -p yanshi-server --test assets_import` 8/8 ✓。
   ④ **变异（三条）**：① 细节开关恒 `true` ⇒ 对外绑判据红（响应里看得见 `commit` ✓）；
   ② 恒 `false` ⇒ `health_and_viewer_do_not_require_tokens` 红（回环拿不到 `tools` ✓）；
   ③ `asset_label` 改回 `path.display()` ⇒ 文件名判据红 ✓。均已改回。
   **未做**：工具层（`tools.rs`）错误里的路径**保留** —— 那里大多是调用方自己传的
   `path`（诊断需要原样回显 ✓）；若将来要求"所有面统一脱敏"，应在 `ToolRegistry` 出口
   按 `--root` 前缀统一处理（一次改，面更全），不在每个工具里各改一处 ✗。

84. **审计 L-4（blob 按文档隔离）—— 暂不做，理由入档**（2026-10-28 ✓）：
   `GET /api/blob/{hash}` 不做文档范围校验 ⇒ 持有 A 文档令牌的人**知道 hash** 即可读 B 的
   blob ✓。完整修法需要「按文档记录所有签发过的 blob」（含渲染/导出物，它们**不在**
   `active_blob_manifest()` 里 ✓）＋ 历史闭包 ＋ stash ⇒ 会牵动 `render_region` 的
   `raw_url`、历史回退目标与 stash 三条已验证路径 ✗（高风险）。当前前提是**必须知道 hash**
   （2^256 不可枚举 ✓），且 hash 由同一 CAS 的内容寻址语义共享 ✓ ⇒ 排在「CI 权限硬化（L-6）」
   之后；做之前先用判据钉住「文档 A 的令牌取不到文档 B 的**活动** blob」，再逐步扩到历史闭包。

85. **CI 红复盘（`a0861908`）：临时端口被当成"对外"**（2026-10-28 ✓）：
   ① **红的两处**：`test (stable)`／`test (beta)` 16 条 `transport` 集成测试全部
   `precondition_failed`（「服务端绑在对外地址上」）＋ `criteria shard 4/6` 的
   `tool-import-project`（`begin` 失败）；本地 60 条 lib 测试与 25 条 transport **都绿** ✗
   ⇒ **绿在错的输入上**：lib 只测了 `127.0.0.1:8080`，而集成测试/判据用
   **`--bind 127.0.0.1:0`**（临时端口）✓。
   ② **根因**：`bind_is_loopback` 复用了 `split_host_port`，而那里的「端口 0 不合法」
   是 **Host 头**的规则 ✓ ⇒ 监听地址的端口 0 被误判成"对外" ✗。
   ③ **修法**：新增 `bind_host_part`（只取主机部分：方括号 IPv6 ✓、未加方括号的 IPv6 ✓、
   `host:port` 含 `port=0` ✓），`bind_is_loopback` 与 `bind_host` 都走它 ✓；
   `split_host_port` 保持"Host 头不许端口 0"不变 ✓。
   ④ **判据**：纯判据补 `127.0.0.1:0`／`[::1]:0`／`0.0.0.0:0`／`bind_host` 三例；
   本机 `cargo test -p yanshi-http --lib` 60/60 ＋ `--test transport` 25/25 ✓；
   CI 由下一次 run 确认 ✓。
   **教训**：**「同一个字符串形状」不等于「同一套规则」** ✗ —— 监听地址允许端口 0、
   Host 头不允许 ✓；复用校验函数前先问「两边的取值集合一样吗」✓。

86. **审计 L-6／L-8 的处置（用户裁定 2026-10-28）**：
   ① **L-6（CI 最小权限／输入注入／SHA 钉）—— 暂不改工作流** ✓（用户选择）。
   现状（供下次动手时照抄）：`ci.yml`／`heavy.yml`／`pwa.yml` **没有** `permissions:` ⇒
   继承仓库默认；`release.yml` 有 `contents: write`（发布需要 ✓）。
   安全子集（未做）：给 `ci/heavy/pwa` 加 `permissions: contents: read`；
   `release.yml` 的 `${{ github.event.inputs.targets }}`／`dynamic` 先落到 `env:` 再进 bash。
   **为什么这算"安全子集"**：三个工作流只用 checkout/cache/upload-artifact 与
   Cloudflare secret ✓，`contents: read` 足够 ✓；`release.yml` 的注入只有 maintainer
   能触发（审计原话 ✓），走 `env:` 是零行为变化的硬化 ✓。
   第三方 action 的 SHA 钉**未做**：升级维护成本高、钉错直接断 CI ⇒ 与"暂不改"一致 ✓。
   ② **L-8（自研 SHA-1）—— 无需改代码** ✓：握手只要求 SHA-1（RFC 6455 ✓），
   且已有两组标准向量钉住 —— `sha1::tests::rfc3174_vectors`（4 条 RFC 3174 向量 ✓）
   与 `ws::tests` 的 RFC 6455 示例（`dGhlIHNhbXBsZSBub25jZQ==` ⇒
   `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=` ✓）。若要换成熟 crate，收益是维护面，代价是
   新增依赖；审计本身也只说"保持测试向量" ✓ ⇒ 现状已满足。

87. **审计 P1-3 已修：WebSocket 慢客户端（无界队列 ＋ 无写超时）（2026-10-28）**：
   ① **每订阅者队列设限** ✓（`crates/yanshi-server/src/broadcast.rs`）：
   数据流上限 `QUEUE_DATA_LIMIT = 256`、硬上限 `QUEUE_HARD_LIMIT = 1024`。
   **控制流不丢** ✗：硬上限被原子占满时先**挤掉一条更旧的数据流**（tile／缩略图可重取 ✓）
   再入队；连一条数据流都挤不出（队列全是原子）⇒ `overflowed` 标记 ⇒ 推送线程断开连接 ✓。
   可观测：`BroadcastStats` 新增 `queue_dropped_data`／`queue_dropped_overflow` ✓
   （不许静默丢 ✓）；`take_overflowed(id)` 取一次即清 ✓；`unsubscribe` 同时清标记 ✓。
   ② **socket 写超时** ✓（`server.rs` 握手后 `set_write_timeout(10s)`）：
   对端不读时 `write_frame` 不再无限阻塞 ✓。
   ③ **写锁收敛到单帧** ✓：原来持锁跨整批事件 ⇒ 读循环的 pong／ack 被一起卡住 ✗。
   ④ **推送线程任何退出路径都 `shutdown(Both)`** ✓：原来只置 `push_closed`，
   而**读循环仍阻塞在 `read_message`** ✗ ⇒ 连接与订阅永久挂着 ✗（资源泄漏）。
   ⑤ **测试与变异**（都是**执行过的红** ✓）：
   · 单元 `queue_drops_data_flow_before_control_flow` ⇐ 变异：`is_data_event` 恒 false ⇒ 266≠256 红 ✓
   · 单元 `queue_overflow_flags_the_subscriber` ⇐ 变异：硬上限改 `usize::MAX` ⇒ 1029≠1024 红 ✓
   · 单元 `control_flow_evicts_queued_data_before_overflowing` ⇐ 变异：不挤数据流、直接溢出 ⇒ 红 ✓
   · 单元 `unsubscribe_clears_overflow_flag` ⇐ 变异：删 `overflowed.remove` ⇒ 红 ✓
   · 集成 `websocket_overflow_disconnects_a_stalled_subscriber`（`tests/transport.rs`）：
     在**同一把工作区锁**里发 `QUEUE_HARD_LIMIT+1` 条事件 ⇒ 队列必然溢满（不依赖内核缓冲 ✓）
     ⇒ 断言客户端读到 **EOF** ✓。变异：忽略溢出标记 ⇒ 实测 `Ok(Some(Frame))` 红 ✓；
     删 `shutdown` ⇒ 5.5s 读超时 ⇒ 红 ✓。
   ⑥ **诚实的边界** ✗：写超时**本身**没有确定性判据 ✓（要控制内核发送缓冲大小 ✗，
   会跨机器不稳 ✓）；字节级上限**未做** —— 原因是写超时把堆积窗口钉在 ~10s ✓、
   单个事件受 `MAX_BODY_BYTES`（32 MiB ✓）约束 ✓，故选择更简单的条数上限 ✓。
   客户端被断时会看到**无 close 帧的断开** ✓（它本来就有退避重连 ✓ `viewer-app.js:8307`）；
   **已知残余** ✗：重连只重新 `subscribe` 不重放错过原子 ⇒ 溢出断开后本地折叠状态可能落后 ✓
   （与本条无关，是重连语义的旧缺口 ✓；要补需让重连后拉一次完整文档 ✓）。
   本机：`yanshi-server --lib` 252/252 ✓、`service_flow` 31/31 ✓、
   `yanshi-http --lib` 60/60 ✓、`transport` 26/26 ✓；CI 由下一次 run 确认 ✓。
88. **两族外部补丁的合并取舍（2026-10-28，`~/patch/zaipatch.zip` ＋ `~/patch/yanshi-patches-76154e19_to_c605ea6f.tar.gz`）**：
   ① **族一（zaipatch，14 笔）—— 逐笔 `git apply --check` ＋ 读正文后**：
   **落地 8 笔**（写超时 P1-N1、工具层越界闸 P2-N1、CL/TE 走私 P2-N2、`head_seq` 截断对账 P2-N3、
   P3 组、blob 每文档作用域 P1-5.5、GPU `map_async` 降级 K-P1-3、token 改 `getrandom`）。
   **跳过 5 笔**（**主干已有等价或更强的实现 ⇒ 不重贴 ✗**）：
   · `0005` zlib 输出帽 ⇒ 主干是 `zlib_decompress_capped`＋`MAX_INFLATE_BYTES`（编译期 assert 钉住与像素上限的关系 ✓）；
   · `0006` 令牌熵的**主体** ⇒ 主干已读 OS 熵，本笔只取其**增量**（`getrandom` 取代 `/dev/urandom`、
     熵源不可用**改 panic** ✗→✓ —— 原实现是留痕后退回 `RandomState` ✓，那仍是可预测面 ✗）；
   · `0008` WS 队列帽 ⇒ **第 87 条**的策略更强（数据流先丢、控制流挤旧、全控才断开 ✓）；
   · `0011` SSRF 主体＋安全头 ⇒ 主干默认**拒**私网（该笔缺省**放行** ⇒ 更弱 ✗）、`nosniff`/`Referrer-Policy` 已在 ✓；
   · `0014` 只改 `0008` 那条测试 ⇒ 随之跳过。
   ② **族二（yanshi-patches，14 笔）—— 13 笔台账文档全部照合，1 笔代码**：
   `0006` 把 `fetch_ip_is_forbidden` 的 `is_unspecified()`（只挡 `0.0.0.0` **一个点** ✗）
   换成 `octets[0] == 0`（**0/8 整块** ✓）＋ 判据补 `0.1.2.3`／`0.255.255.255`。
   **修复本体记在提交信息**（台账 §645 只给指针 ⇒ 同一事实只写一处 ✓）。
   ③ **合并外部补丁的门**：本机 `cargo clippy --workspace --all-targets -- -D warnings` ✓（pre-commit 拦到一次真实缺口 ⇒
   测试夹具 `ServerState` 少初始化两个新字段 ⇒ 补齐后绿 ✓）；
   **`cargo test --workspace` 交 CI**（内存规则：可用 2.4 Gi < 3 Gi ⇒ 不在本机跑整 workspace ✓）。
   ④ **已知残余（族二自己登记的，未修 ✓）**：SSRF 检查与 ureq 建连之间**解析两次**
   ⇒ DNS rebinding（TOCTOU）✗；修法要 connect-by-IP＋Host/SNI 改写，ureq 无钩子 ⇒ **无判据不动手** ✓
   （核对命令：`grep -c 'to_socket_addrs' crates/yanshi-server/src/file_source.rs` ⇒ 修好后应为 1 ✓）。

89. **05 复审报告（`~/patch/05-re-audit-v0.3.1.md`）逐条处置（2026-10-28）**：
   ① **已修并已验（每条都有变异红⇒复原绿的记录，细节在各提交信息 ✓）**：
   · **NEW-C1**（`d6525652`）：未授权者不再触发 `load_atoms` 全量重放 ⇒ `authorize` 对远端传输
     先做 **meta.json 浅探**（令牌表整个就在 meta 里 ✓），判 `FileStore.load_atoms_calls` 计数器；
     副作用如实记：**不存在的文档 + 坏令牌 ⇒ 403 而非 404**（不给存在性预言机 ✓）。
   · **NEW-M1**（`ffe95f34`）：`doc_param` 与工具入口在 **HTTP 边界**就验 `validate_doc_id`（白名单＋≤128）✓，
     判据 `transport.rs` 补 7 个坏样本 ⇒ 400；判据跑红过一次（`..%2Fetc` 那组）✓。
   · **NEW-H2**（`1f9c379d`）：`X-Yanshi-Key` 头**优先**于 `?key=` ✓（URL 会进访问日志/历史/Referer ✗）；
     `?key=` 仍兜底但**每次用都留一行日志** ✓；lib 测试钉「头与 query 同时给 ⇒ 取头」✓。
   · **C-1 补全**（`8816b1fc`）：`web/_headers` 的 CSP 从「只剩 frame-ancestors」补成完整白名单 ✓，
     口径逐条对着 `web/index.html` 实测用法收 ✓；守卫 `check-security-headers.mjs`（变异：删 directive ⇒ 红 ✓）。
   · **NEW-M3**（`9851cf88`）：known-red 台账 201 行游离散文全部收进 `# ` 注释 ✓、匹配器改**整字段相等**
     （`awk $1 == n` ✓，原来用未转义正则 ✗）、守卫 `check-known-red-format.mjs` ✓。
   · **L-2／L-3**（`e9aeb0b2`）：`.PHONY` 补 `serve`／`targets`（20 个目标全数在册 ✓）、`clean` 认 `$(OUT)` ✓
     （修前实测 `make -n clean OUT=/tmp/x` 打印 `rm -rf dist` ✗）；守卫 `check-makefile-hygiene.mjs` ✓。
   · **NEW-M2**（`9157b778`）：`browser-*.mjs` **自述口径**（`criterion-target`，56 条：client 47／server 6／
     static 1／self 2 ✓），运行器改 grep ✓、迁移期与旧行为**逐条 0 不一致** ✓；守卫
     `check-browser-criteria-target.mjs` 顺手抓到 `ui-control-heights` 分支里**第 1132 轮的名单是死代码** ✗
     （那段只在 `$name==ui-control-heights.mjs` 时可达 ⇒ browser 名永远匹配不上；实测 `browser-range-mask`
     自己 navigate 干净 URL、从未受害 ✓）——**报告没点到，守卫点到 ✓**。
   ② **确认为假**：**H-4**（`make dev PORT=9000` vs `PORT=9000 make dev`）——GNU make 把命令行变量
   放进 recipe 环境 ✓，两种写法**都能通**（最小 Makefile + printenv 当场实验 ✓）；文档表述已改 ✓（`923e13d8`）。
   ③ **报告说的已修**：**H-5**（pwa.yml 部署目标）—— 2026-10-28 已查实改为 `yanshi-online` ✓（注释在册）。
   ④ **暂不做，理由入档**：
   · **H-1（web/ 与 assets/ 约 19 MB 重复资产）**——去重要把 `web/{textures,samples,fonts}` 移出版本控制 ⇒
     CI criteria 作业缺前置步骤 ⇒ **要改 `.github/workflows/` ⇒ 撞上 L-6「暂不改工作流」裁定 ✗** ✓；
     恢复条件：L-6 解冻后照 §5.6 的 `diff -rq` 对账门一起做 ✓。
   · **NEW-M4（release.yml 加 timeout-minutes）**——同为工作流文件 ⇒ 同上冻结 ✓。
   · **NEW-H1（index.html 1.04 MB）**——瘦身是大手术（面板拆分/构建管线），与缓存策略（`_headers` ✓）
     分开排期 ✓；当前 `/index.html` 已单独 `no-cache` ✓ ⇒ 「每次冷启传 1 MB」是**传输成本**问题 ✗ 不是**陈旧内容**问题 ✓。
   · **NEW-C1 的限流（429）**——浅探之后未授权路径**只剩有界的 meta.json 读** ✗ 重 IO 已闸 ✓ ⇒
     限流降级为「排期后做」✓（做之前先钉判据：同一来源高频 authorize ⇒ 429 且有 `Retry-After` ✓）。
   ⑥ **NEW-L3 点名「逐字未变」的旧项，本轮动了最便宜的一条**：**上轮 L-4／E-12（三条部署路径各用各的
   wrangler）**——`deploy-pwa.sh` 的两处 `npx --yes wrangler`（**绕过 lockfile** ✗ 每次现取最新 ✓）
   换成与 `npm run pwa:deploy` **同一个解析**（`scripts/pwa-wrangler.mjs` ⇒ 本地 `node_modules/.bin/wrangler` ✓
   实测复现版本 4.149.0 ✓）；守卫 `check-deploy-tool-pin.mjs` 扫 **scripts/ 全体活代码**禁 `npx --yes` ✓
   （变异：换回 npx ⇒ `deploy-pwa.sh:142` 红 ✓；守卫**先抓到过自己** ⇒ 匹配串拆两段 ✓「检查对象里
   不得有该检查本身的文字」第 301 轮那条规矩 ✓）。`pwa.yml` 的三处**未动**——L-6 冻结 ✓，解冻后一起做。
   ⑦ **NEW-M5（`docs/` 过程文档噪音）**：四篇**零外部引用**的现场记录收进 `docs/archive/` ✓
   （`history-undo-failure-venue`／`sw-cache-critical-path`／`verification-summary`／`render-box-first-paint`，
   收纳规矩写在 `docs/archive/README.md` ✓——**同一事实只写一处**）。
   **没动的**：`surface-alignment.md`（185 KB，**仍在生长的台账** ⇒ 不是噪音 ✗，且 3 处活引用含判据 ✓）、
   `FOR-APPLE2011.md`／`HANDOFF-CI-621ffc2b.md`（各有 3–4 处活引用 ⇒ 先逐处改引用再谈搬，本轮不做 ✓）。
   搬前搬后各跑一遍 `grep -rl <名字> scripts/ crates/ web/ .github/` ⇒ **外部活引用 0→0** ✓；
   文档侧四个守卫（adjustment-count／agents-selfcheck／ledger-numbering／perf-doc-pointers）**全绿** ✓。
   （`5fc2a5bb` 的提交信息说「§89⑦ 落账」时**还没落**——那次 python 的锚点 assert 挡下了未就绪的追加 ✓，
   补账在下一个提交里 ⇒ 这正是「assert 只证明改了、不证明改对了」的反面用例 ✓。）
   ⑧ **M-1（05 复审：六个资源目录没有缓存规则）**：`web/_headers` 原来只给 5 个内容寻址目录
   （wasm/brushes/mediums/brand/icons ✓）配了 immutable ⇒ textures/samples/fonts/brush-previews/
   showcase/palettes（约 28 MB ✓）全落在 `/*` 的 no-cache 下**每次回源校验** ✗。修法**不是一律 immutable** ✗：
   这些目录**文件名按资源名、不按内容哈希** ✓（`ls web/brush-previews` ⇒ `100__Opaque.myb.png` ✓），
   而原地重写是常态（**c5abb674 一次重写全部 brush-previews 同名文件 ✓**；近 90 天提交数实测
   showcase 5／textures 3／其余各 1 ⇒ 数字随台账更新，命令在 `_headers` 注释里 ✓）⇒
   immutable 会把第 832 轮用户报的「改了没生效」放大成**最长一年看不到** ✗ ⇒ 折中 `max-age=3600` ✓
   （最坏一小时旧资源，代价是每小时一次重复下载）。守卫 `check-cache-headers.mjs` 是**开放集** ✓：
   ① web/ 每个子目录都必须有 `/X/*` 规则（新目录忘配 ⇒ 红 ✓）② 原地重写名单上有 max-age 且**不含** immutable
   ③ 其余目录都有 Cache-Control ⇒ 变异三条全实测红过（删规则／改 immutable／mkdir 新目录 ✓）再回绿 ✓。
   ⑤ **CI 复盘（run `40640839341`，`8816b1fc`，failure）**：
   · `browser-brush-preview` **定性为环境抖动** ✓ —— 上一跑（`522ffcd`）红的那条这一跑**没有复现** ✓
     （同跑 `browser-asset-dock` 当时是 `exit 3` 判据自身求值失败 ✓，不是产品失败 ✓）；
   · **真正的两条红是我留下的** ✗✓：`tool-created-opened` ② 与 `tool-unknown-doc-is-not-created` ①
     还钉在**旧契约**「不存在的文档 ⇒ `reference_not_found`」上 ✗ ⇒ NEW-C1（`d6525652`）改成
     「无权限调用方 ⇒ `permission_denied`（存在性不外泄 ✓）」时**只改了实现没回改这两条判据** ✗
     （正是内核 ②「台账表格变了要回去改」的判据版 ✗）。修法见下一轮：②/① 断言改
     `permission_denied` ＋ 加**恒等式**「不存在的文档＋外来令牌」与「存在的文档＋坏令牌」**同码** ✓
     （不同码就是存在性预言机 ✗）；`tool-delete-document` 的同名断言**不受影响** ✓
     （它走 `DELETE /api/documents/<id>` 路由，不经过 authorize 浅探 ✓，且工具面那条断的是**两面对等** ✓
     —— 逐条核对过，没有第三处 ✓：`grep -l reference_not_found scripts/*.mjs` 剩下的都是**图层/示例**域）。

90. **外部包合并：`yanshi-skill-vector-tracing-v5.10后续包.zip`（2026-10-28，`2506f046`）**：
   ① **内容**：只改 `skills/yanshi-vector-tracing/SKILL.md`（437 ⇒ 1476 行 ✓）——v5.4–v5.10 累计文本一次落盘，
   第 11 轮晋升 5 条 P 档规则进正文，其余进附录 R11；**纯文档，零 Rust 改动** ✓。
   ② **合并方式**：`git am`（保留作者 Muse 的原提交 ✓）；**逐字节核对**：合并后文件 = 包内交付 `SKILL.md` ✓。
   ③ **包内声称的验证，本机全部复现**：`check-skill-tool-names.mjs` exit 0（126 处标识符 ✓）✓、
   `cargo fmt --check` 干净 ✓、版本行 v5.10 无 DRAFT ✓、附录 R11 在 1394 行 ✓。
   ④ **如实记（包自己写的，未粉饰 ✓）**：发包门槛三项**只达成两项**（企鹅 6/10 < 8/10 ✗、且 P1/P4 是真退步）⇒
   **这一版是 Wang 决定发的，不是门槛过了发的** ✓；正文残留 4 处 "draft" 历史措辞（照原样 ✓）；
   P-A 编号在第 8 轮与第 11 轮**重名不同义**（原样保留 ✓，R11 按内容区分 ✓）。
   ⑤ **合并外部补丁的门（内存规则版）**：纯文档包 ⇒ `cargo test --workspace` 无对象可回归 ✓，
   clippy/fmt 由 pre-commit 与包侧声称复现 ✓；`git am` 不过 pre-commit 钩子 ⇒ 合并后**另行复跑**了标识符守卫 ✓。

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
   ⑨ **NEW-H1 的防回归半边（字节预算判据）**：瘦身本身仍排期在后 ✓（大手术），
   但先**止损**：`check-index-html-byte-budget.mjs` 钉棘轮 CEILING=1,040,000
   （2026-10-28 实测 1,038,359 ✓ 已用 99.8% ⇒ 每次跑都会打印逼近警示 ⚠ ✓）。
   CEILING 属「判据自己造的期望值」那一类 ⇒ 可以写死 ✓（判据两分类，第 798 轮那条规矩）；
   变异：给 index.html 追加 12 KB ⇒ 红「超出 10,369 字节」✓ 还原回绿 ✓。
   **红之后的规矩写在判据头里**：抬棘轮必须附「谁涨了多少、为什么」⇒ 让膨胀必须被看见 ✓。
   **自我交代**：本条第一次落账（`16c12eab`）把 ⑨ 插进了 ⑧ 的句子中间 ✗——python 拿
   「⑦ 段尾偏移＋marker 长度」当插入点，锚点 assert 全过而**位置是错的** ⇒ 下一提交接回 ✓。
   教训：**assert 只证明锚点存在，不证明插入位置对**；跨段追加要**当场 sed 回读那几行**。
   **CI 首跑复盘（run `40646976572`，`16c12eab`，failure）**：两条新守卫里 `check-cache-headers` **绿** ✓，
   `check-index-html-byte-budget` **红** ✗——红得有价值：CI 跑判据前 `pwa-sync` 会**重写工作区**的
   index.html（同一提交本机 1,038,359 vs CI 1,044,372 ✗）⇒ 判据读工作区就是**跨环境不同源**
   （第 336 轮三问「它读的两个量是不是同源」的又一次 ✓）。修法 `f0f7e0c6`：改读
   `git show HEAD:web/index.html` ⇒ 同一提交两台机器必然同一个数 ✓；变异改走**临时提交**
   （HEAD blob 1,050,369 红 ✓）`reset --soft`＋`restore` 还原回绿 ✓。
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

91. **外部包合并：`~/patch/yanshi-patch-batch1-20261028-6372dd8c.tar.gz`（27 笔，2026-10-28）**：
   ① **完整性先验**：包自述 base=`76154e19`（我们 HEAD 的祖先）／head 树 `41ddcb8b…` ⇒ 在 scratch
   worktree 上 `git am` 全部 27 笔后**树对象逐字节复现** ✓（包与声明一致）。
   ② **去重**：0001–0014（§643–645 台账＋SSRF `0/8` 修复）**已在主干**（`c18c1d98` 家族、
   `file_source.rs:340` 的 `octets[0]==0`、§645 十三面齐全 ⇒ 逐条内容核对，不是 grep 主题——
   MIME 编码的主题 grep 会全漏 ✓）。所以真正新的是 **0015–0027（13 笔）**：PWA 本地层 13 端点
   ＋ 6 处静默降级修复 ＋ §646–656 台账。
   ③ **L-6 裁定照办**：0015 里往 `.github/workflows/ci.yml` 加「重生成前验资产索引」一步 ✗ ——
   **剥掉那 11 行只合其余**（`git am` 前先删 diff 段 ✓）；**待办登记**：L-6 解冻后把该步补回
   （理由他们写得很实：`pwa-sync-viewer.mjs` 会重生成 `web/assets-index.json` ⇒ 判据在 CI 里
   永远只看得到重生成态 ⇒ 仓库里被污染的 15 张测不出 ✗ 历史上 8↔15 反复过四轮 ✓）。
   ④ **合并中查出并修掉一个我们自己的旧错**：`2ad65743`（zaipatch P2-N3 截断对账）**只改了
   `crates/yanshi-http/assets/api-local.js`、漏了 `web/api-local.js`** ✗ ⇒ PWA 副本在超大文档上
   会「静默停在旧状态」——正是 P2-N3 要封的形态，也是 H-1「两份重复资产会漂移」的活案例 ✓。
   本批 0015–0027 对两副本的改动字节相同（各 +808），差集 18 行全部是那段缺失 ⇒ 以 assets 为准
   `cp`+`cmp`+`node --check` 补齐（`3a18015b`）✓。**规矩坐实**：改任一副本必须另一副本同改。
   ⑤ **本机验收**：`tool-pwa-api-behaviour` ✓（锚点/reorder/revert 三组全绿）｜
   `tool-offline-asset-coverage` ✓（他们补的 `setTimeout` 沙箱让这条**复活真验**了 ✓）｜
   `tool-assets-index-matches-tree` ✓（10 条）｜`tool-pwa-assets` ✗＝**既有红**（mediums 产物落后
   源码，第 491 轮登记在册 ✓，非本批引入）。批次**没改任何 `.rs`** ⇒ 合并例外的 clippy/test
   无 Rust 对象，全量套件交 CI（已推）。
   ⑥ **CI 复盘（run `40656113606`，`31035d61`，failure）**：唯一红 `tool-pwa-gc-references` ✗——
   是**新实现对、旧判据输入裸**：0018 让本地层 `import_image` 照服务端 `parse_raster_patch` 的
   P0 **要求尺寸**（无尺寸⇒写一条渲染不出来的原子＝静默降级 ✗），而该判据传的 `{blob_hash,
   layer_id}` 没带 `width/height` ⇒ `missing_bitmap_size` 当场拒 ✓（守卫本身是对的，拒得有理由）。
   修法＝补判据输入（1×1＝4 字节与 blob 恰配），**不动实现**；全仓普查其余带 `import_image` 的
   判据都已带尺寸/region ✓（第 855 轮「改一处 grep 全仓」这条规矩兑现）。他们包自述「CI 从未跑过」
   ⇒ 这条红在预料清单内，属**批次与既有判据的接缝**，不是产品缺陷 ✗⇒✓ 修缝不修闸。

92. **外部包合并：`~/patch/paint-fix-p1-export-receipts后续包.zip`（1 笔，2026-10-28，`81cd13bc`）**：
   ① **基线与接缝**：base 恰为 `3d26ec24`（批次1合并前的主干尖端），改动三文件
   （`render.rs`/`tools_io.rs`/`tests/export_png.rs`）批次1 都没碰 ⇒ `git am --3way` 干净落地。
   ② **内容审查后才合**：`rendered_seq_note` 与既有语义**同向**（`tools_io.rs:649` 早已把
   `raster_pending === (rendered_seq < head_seq)` 定为不变量、`surface-alignment` 也注明
   「别拿它当画布状态」⇒ 只是把话写进回执，不新增事实 ✓）；**落回成功警告降级**不违反
   「失败必须留痕」——落回成功不是失败，且 `YANSHI_TRACE_TILE_STITCH` 仍可查 ✓；
   既有判据 `tool-bitmap-tiles-render` 断的是**像素改动**不是警告文本 ⇒ 不受影响 ✓（先 grep 过）。
   ③ **合并例外的本机项**：`cargo test -p yanshi-render --lib tile_fallback` 2/2 ✓；
   `cargo test -p yanshi-server --test export_png` 10/10（含新测）✓；
   `cargo clippy --workspace --all-targets -D warnings` ✓；**本树复验 M2 变异**：
   删 `rendered_seq_note` ⇒ 回执测红（101）⇒ 还原绿 ✓（不空信包侧声明）。
   `cargo test --workspace` 按内存规则**交 CI**（本机可用 2.4 Gi < 3 Gi 闸门）。
   ④ **接缝线（第 3 件）定性**：两候选（分带边界/落回路）被受控试验排除、导出逐位同哈希——
   包结论「画法产物，无程序可修」**照单收下**（不入补丁、不改 skill；skill 侧归临摹方法轮 ✓）。
   ⑤ `git am` 不过 pre-commit 钩子 ⇒ 合并后**另行复跑**了 fmt/clippy/守卫 ✓（§90⑤ 同规）。

## §93 paint-fix 的 CI 复盘：api-local 撞 id 竞态（2026-10-28，run 40662163923 ⇒ 修复 5d6bbbfa ⇒ run 40667407047）

① **红三处、裁定两条旧账一条新 bug**：Rust 侧全绿（`test (stable)/(beta)`／clippy ⇒ §92③ 交 CI 的
   那步**成立** ✓）。红的是：`browser-cs-render-switch`＝**已登记抖动红**（known-red 原文「3 红 5 绿」）✓；
   `browser-offline-reload`＝**已登记接线红** ✓；`tool-pwa-live-smoke`＝EXIT 124 **打线上站点超时被砍**
   ——本机同判据 12 项全过、站点 curl 0.3–0.6 s ⇒ **外部瞬时**，不是产品 ✓；
   **`tool-pwa-api-behaviour` ⑦l/⑧＝真 bug**（下面②）✗。
② **根因（插桩抓现场，不是猜）**：`p2cNewObjectId` 的计数器 `__p2cCtr` **声明在请求处理块内**
   ⇒ 每个请求从 0 重新计数，且时间戳与计数器**无分隔符拼接** ⇒ **同毫秒两笔生成同一个 object_id**
   （实测红样本：`obj_mvrx4vjm1` ×2、`obj_mvrx8335_1` ×2）⇒ `/api/atoms` 按 `object_id` 建映射时
   **互相覆盖** ⇒ 断言拿到「别人的 group_id」⇒ 抖动红。判据本身没错——**它抓到了产品真缺陷**
   （真实浏览器里同毫秒连点同样会撞）。服务端用 ULID ⇒ 天然无此歧义，本地层照抄语义时自造了坑。
③ **修法**（`5d6bbbfa`）：计数器提升到 `makeLocalApi` 层（与 `__p2ctr` 同一纪律：跨请求存活 ✓）
   ＋ 加 `_` 分隔符；两份副本同步（`cmp` ✓）。**回归**：修前 5 连跑 1 红 → 修后 50 连跑全绿、
   100 连跑全绿（截至 2026-10-28）；grep 全仓无判据依赖旧 id 形状（`browser-effect-card-truth` 的 `obj_\w+` 兼容 ✓）。
   **两面**：收益＝同毫秒多笔不再互相覆盖；代价＝id 多一下划线（本来就是 opaque，无契约）。
④ **教训（内核①的又一次）**：「本地绿、CI 红」**先怀疑环境**是错的默认——本次两步就翻案：
   先连跑 5 次**在本地复现出红**（抖动≠环境）⇒ 插桩抓**具体 id** ⇒ 机理对上了才算定位。
   判据的 `实测 ["LG1","LG2"]` 看起来「值都对」而断言仍红 ⇒ **打印的取样不是断言的全集**
   （第三条子条件 `la1[lS2.object_id]` 压根没打印）——写断言打印时要打印**全部子条件的量** ✓。
⑤ **后效（run 40667407047＝`5d6bbbfa`，completed/success）**：shard 5/6
   `tool-pwa-api-behaviour` ✓（通过 65｜**意外失败 0**｜已知红 2）；shard 3/6 ✓——
   上一 run 超时的 `tool-pwa-live-smoke` 本片自然转过 ✓（外部瞬时成立）。

## §94 paint-fix 期 3（批处理与会话）合并留痕（2026-10-28，`1f66374f`）

① **基线与接缝**：包基线 `aeca3e4c`＝期 2 合入后的尖端；`aeca3e4c..HEAD` 只动了
   `HANDOFF.md`＋两份 `api-local.js`（§93 撞 id 修复）⇒ **与本期 4 个文件零重叠** ✓
   （包说明自己预告了这一点，核对属实）；`git apply --check --3way` 干净 ⇒ `git am --3way`
   直落 `1f66374f`，无需手工并缝。
② **内容审阅**（三件都有分诊根因，不是症状 patch）：P-1＝`serve` 超限行改「排空＋JSON-RPC
   错误回执（-32600／id null／点名 65536 与拆批）」，另设 `MAX_DISCARD=1 MiB`：**行尾永不来**
   的病态输入仍 `return Err` ⇒ 内存闸门没有被放松，只是不再让一条超长行杀全进程 ✓；
   P-10＝`write_batch` 落笔前校验子调用 `arguments`/`args` 的 `doc_id` 与 `ctx.doc_id` 一致，
   不一致**整批拒绝、0 原子落下**（键口径 `arguments||args` 与执行循环 `tools.rs:20805` 逐字
   一致 ✓；放在 `silent`/变更集之前，无副作用 ✓；**落工具层 ⇒ HTTP 与 MCP 同得** ✓）；
   P-11＝重复 initialize 只在回执加 note、不改行为（不报错的理由成立：MCP 规范允许重连后
   重发 initialize）✓。**未修清单**照单收下：按子调用 doc_id 落笔（跨文档拆变更集，另立项）、
   驱动侧 unknown 文案（须等探针驱动源码）、HTTP 面 `read_line_bounded`（不同构，本来就按连接拒）。
③ **本机复验**（合并例外）：定向测试全过——`line_cap` 4／`stdio` 4／P-11 单测 1／
   `batch_doc_id` 5／邻近 `batch_limits` 3／`batch_message` 1／`batch_stroke_evidence` 4／
   `silent_batch` 2；`cargo clippy --workspace --all-targets -D warnings` ✓；
   **本树复现三条变异**（不空信包侧）：M1 改回 `return Err` ⇒ `line_cap` 2 红 ✓；
   M2 校验永不触发（`&& false`）⇒ `batch_doc_id` 3 红（两条放行用例仍绿，与包证据逐条同形）✓；
   M3 note 永不加 ⇒ P-11 单测红 ✓；还原后 `batch_doc_id` 5/5 复绿、`git status` 干净 ✓。
   `cargo test --workspace` 按内存规则**交 CI**。
④ **回归普查**：`scripts/` 无判据以 `calls` 组批、更无「一批混多 doc_id」的用法 ⇒ P-10 的
   整批拒绝不会打红既有判据 ✓；无 JS 判据依赖「超限行杀进程」旧行为 ✓。
⑤ `git am` 不过 pre-commit 钩子 ⇒ 提交前**另行复跑** fmt/clippy/守卫 ✓（§90⑤ 同规）。
⑥ **后效（run 40674084560＝`a04f181e`，completed/success）**：`test (stable)`／`test (beta)`
   全绿 ⇒ §93③/§94③ 交 CI 的 workspace test **成立**（新 `line_cap` 4／`batch_doc_id` 5 在其内）；
   clippy/rustfmt/六片 criteria 全绿 ⇒ P-10 整批拒绝没有打红既有判据（④ 的普查被 CI 印证）✓。

## §95 批次 2（本地层锁闸＋预设四端点）合并留痕（2026-10-28，`3ccde4be`）

① **基线与接缝**：包基线 `6372dd8c`＝批次 1 的 head —— 该哈希**不在本仓历史**（批次 1 是以
   自有提交合入的），但补丁各文件的**前像 blob 在本树都在** ⇒ `git am --3way` 落在 `a04f181e`
   之上直接成 `3ccde4be`，无需手工并缝。**重叠面核对**：本批动两份 `api-local.js`，与 §93
   撞 id 修复同文件不同区段 ⇒ 合并后当场验证修复存活（`+ "_"` ×2、`let __p2cCtr = 0;` ×1 计数对）
   ＋ `cmp web/ crates/yanshi-http/assets/` **逐字节同一** ✓。
② **内容审阅**（四件，全部无 Rust）：A＝锁校验补进**提交层**——`p2LockGuard` 镜像
   `crates/yanshi-core/src/log.rs:786 validate_lock`（detail 文案逐字照抄 log.rs:807–844），
   闸挂在 `putAtom`（40+ 写入口的必经之路，单点），`set_property{locked|visible}` 永远放行
   （否则锁上即死）、墓碑层仍算在状态（`fold.rs:1315`）✓；**顺带修真坑**：`local()` 原
   `try { return dispatch(req) } catch` **抓不到异步拒绝** ⇒ 抽 `localDispatch` 改
   `return await`（包称实测 `update_adjustment` 曾直接穿出——与 §93 同一族：异常路径没被
   自己的兜底接住）。B＝`list_layers` 13 字段／`objects` 计数／`z_index` 折叠、
   `list_objects` 补 `enabled/locked` 对齐服务端 ✓。C＝预设四端点（22 套内联 92959 B，
   生成器 `--write/--check`＋逐字节判据看守；带 `render` 的 `preview_preset` **如实 501**，
   不假装有单层裸像素通道 ✓——静默降级 5 形态的反面教材 ✓）。D＝新判据
   `tool-pwa-local-write-guards`：**同一批写请求分别打真服务端与本地**，比解析后 JSON
   （整数严格、小数 1e-12 相对误差）＋**错误文案逐字**；三处服务端缺陷**记为分歧断在分歧本身、
   本地不复刻**（死父层 200-no-op vs 本地 404／changeset 半写 `service.rs:2437`／复制丢
   `type` `tools.rs:3921`）✓。**未修清单**照单收下：三处分歧的根治属服务端后续，不在本批。
③ **本机复验**（合并例外；README 第 1 条「CI 从未跑过这笔」照单）：`cargo clippy --workspace
   --all-targets -D warnings` ✓（本笔无 Rust，仍按例外跑）；判据全绿——`tool-pwa-local-write-guards`
   0／`tool-pwa-preset-server-parity` 0（22 套／展开三档／错误文案逐字／诚实 501；黑白插值一条
   条件性跳过）／`tool-pwa-preset-library-parity` ✓／`tool-pwa-api-behaviour` **10 连跑全绿**
   （§93 修复在本批之下仍立）／`tool-cs-local-copies`／`tool-web-assets-parity`／
   `cache-headers`／`byte-budget`／`ledger-numbering` ✓／生成器 `--check` ✓。
   **本树复验变异 ⑨**（删锁闸 ⇒ 判据退出码 1，失败集中在 §C「服务端 403｜本地 200」类 ⇒
   断的正是锁路径 ✓；还原复绿）——十变异信包侧十条记录 + 自验一条，其余九条不代跑 ✓。
   踩坑记一笔：复验最初用本机 8080，`/health` 502 —— 那**不是本仓服务**（nohup 起的因
   「Address already in use」已死）⇒ 改**专端口 `127.0.0.1:18090` 起专用服务**再测 ✓（内核①：
   先确认测量对象是自己的）。`cargo test --workspace` 按内存规则**交 CI**。
   包自曝的一点也收下：变异 ⑩ 首跑退出码 3，暴露**判据自身**缺口（`callLocal` 不接异常 ⇒
   产品抛错被当成判据坏了）⇒ 已修成记 0 状态失败响应——「测量工具要先被测量」又一次 ✓。
④ **已知红（本批带入、批次 1 已登记）**：本机 `tool-pwa-assets` 报「介质产物 mtime 落后于
   Cargo.toml」——内容未改、属本机时间戳现象，CI 全新 checkout 应不出现 ⇒ **不动判据，等 CI** ✓。
⑤ `git am` 不过 pre-commit 钩子 ⇒ 本留痕提交**另行复跑** fmt/clippy/台账编号/守卫 ✓（§90⑤ 同规）。
⑥ **后效（run 40675598611＝`c93bf89b`，completed/failure ⇒ 意外失败 1，补丁本体无过）**：
   除 `criteria shard 5/6` 外全部 job 绿（test stable/beta／clippy／rustfmt／其余五片）；
   批次 2 的四条判据在 CI 全绿（`tool-pwa-local-write-guards`／`preset-server-parity`／
   `preset-library-parity`／`api-behaviour`——README「CI 从未跑过这笔」就此清账 ✓）。
   唯一红＝`tool-criteria-coverage` 点名 **`gen-pwa-preset-library.mjs` 生成器漏登记**
   （**与 `pwa-stamp.mjs` 第 881 轮同根**✗：外部包合并没进清单 ⇒ **覆盖判据立刻红并点名** ✓
   ——**它存在的意义又被实测一次**）⇒ 修法照 881 轮先例：加进 `NOT_CRITERIA` 写清理由
   （**判据入口早已接线**✓：`tool-pwa-preset-library-parity.mjs:85` 自己调 `--check` 并断退出码
   ⇒ **再枚举它只会多一条重复检查／恒 0 的假判据** ✗）；本树跑该判据 **生成器 17⇒18、EXIT 0** ✓。
   两片已知红（`browser-offline-brush`／`browser-ui-check`）与三条 exit 3 跳过均与登记一致 ⇒
   **产品侧没有新红** ✓。**修法复验（run 40679121210＝`25e91bcc`，completed/success）**：
   全部 job 绿——`tool-criteria-coverage` 随登记转绿，批次 2 至此**在 CI 上完整成立** ✓。

## §96 paint-fix 期 2 后续包（export-perf）合并留痕（2026-10-28，`d094ca5a`）

① **基线与接缝**：包基线 `aeca3e4c`（期 2 合入后尖端）；包侧前向重放验证跑在 `25e91bcc` 上
   （`verify/重放验证-新main.txt` PASS，漂移＝预期 15 文件）⇒ 而 `25e91bcc..b39d4c2b` 只动
   `HANDOFF.md` 3 行 ⇒ **包验证的正是本树合并前的上一提交** ✓。`aeca3e4c..b39d4c2b` 与本包
   四个文件的交集只有 `tools.rs` 一处（期 3 的 P-10 在 `write_batch` ≈20805 行，本包 hunks 在
   5928／15588／17051／17272）⇒ 距离足够，`git am --3way` 五片**全部干净落地**
   （`2546feb2..d094ca5a`，无冲突、无手工并缝）✓。
② **内容审阅**（五片，全是根因修不是症状 patch）：
   **1/5 X-14 配方记全**——`write_brush_stroke` 的 `source` 补记 `brush_params`（落笔合并后的值）、
   `times`（各点第 4 元素 `time_ms`，没有记 `null`）、`color_mixing`；新增共享
   `replay_brush_paint(ctx, source)`：**export 重放与 `update_stroke` 改色重跑用同一份实现**
   （本项目「两份实现迟早漂移」已有多次前科 ✓）；`restyle_baked_brush_stroke` 由「重建一张丢字段的
   `source`」改为「原 `source` clone + `core` 五字段覆盖」⇒ 新 `source` 即合并后的配方 ✓；
   老笔触没记三项 ⇒ `None`/`&[]`/`Rgb` ⇒ 老行为逐字节不变 ✓。**2/5 P-2 性能**——`paint_brush`
   读背景不再 `document_state(ctx)?`（整表克隆，实测 ≈75 ms/次重放），改
   `document().state().background` 直接借出 ✓。**3/5、5/5 测试夹具**——按真实原子结构读
   `source`（顶层或 supersede 的 `data.source`）；各测试用不同 `doc_id`（证明缓存进程内、
   键含 doc_id+head_seq ⇒ 同图会命中前一条的证明，断言就测不到重放本身 ✗）。
   **4/5 回执恒等式订正**——导出认 supersede 原子 `data` 下的配方（`payload_brush_source`）；
   恒等式从「`omitted + kept_mismatch + kept_no_recipe + kept_missing == blob_count`」订正为
   **`kept_mismatch + kept_no_recipe == blob_count`**（`blob_count` 只数**进包的 blob**，
   `omitted` 的不进包 ⇒ 原来那个式子把两类混数，本树对着 `archive.rs` 计数路径核过 ✓）。
   **未修清单**照单收下：重放并行化**不做**（冷导出已 6–13 s < 60 s 预算，收益不抵复杂度）✓。
③ **本机复验**（合并例外的内存边界）：`cargo check -p yanshi-server --tests` **exit 0** ✓；
   `free -h` 可用 **2.4 Gi < 3 Gi** ⇒ 定向 test（`export_brush_proof` 5 条）与
   `cargo test --workspace` 一律**交 CI**（§95③ 同口径，不自称本机绿）。包侧变异 M1–M5
   （不记 `brush_params`／重放不取 `times`／restyle 丢字段／重放 `None`／导出只认顶层 `source`）
   在盒上 `evidence/mutate.log` 全红——**本树不代跑**（跑一次＝整 crate debug 编译，超内存闸门）✗
   ⇒ 以 CI `test (stable)/(beta)` 里 `export_brush_proof` 5 条为端到端成立依据 ✓。
④ **回归普查**：`grep` 全仓判据**无**一条断 `source` 的**键集合**（深比较 0 处）⇒ 新增三字段不打红；
   `tool-color-space-p2-9` 断的是 `source.color_space === "srgb"/"linear"` 单字段，
   `merged_source` 的覆盖规则（core 给空间记新的、没给沿用原值）与原逻辑逐条一致 ✓；
   `tool-archive-bloat`／`tool-cold-open-budget`／`tool-export-idempotent` 断的是**相对量与恒等式**
   （`blobsB ≤ blobsA×1.5`、原因之和==`kept_no_recipe`），P2 不改变 blob 计数口径 ✓。
⑤ `git am` 不过 pre-commit 钩子 ⇒ 本留痕提交**另行复跑** fmt/clippy/台账编号/守卫 ✓（§90⑤ 同规）。
⑥ **后效（run 40685192585＝`74501886`，completed/success）**：`test (stable)/(beta)`、clippy、
   rustfmt、判据六片全绿。并取 job 日志**实证了③交 CI 那一条的落地**：
   `export_brush_proof` **5/5 ok**（`stroke_with_brush_params_exports_as_omitted`／
   `stroke_with_point_times_exports_as_omitted`／`stroke_with_brush_params_and_times_exports_as_omitted`／
   `legacy_stroke_without_new_fields_still_exports`／`update_stroke_keeps_the_proof`），
   整个 test (stable) 作业 **324 个 `test result: ok`、0 FAILED** ⇒ 配方记全⇒导出 `omitted`
   的机制**在 CI 上成立** ✓。

## §97 paint-fix 期 4 后续包（brushes）合并留痕（2026-10-28，`e721e7de`）

① **基线与接缝**：包基线同为 `aeca3e4c`，包侧前向验证跑在 `25e91bcc`（`verify/前向验证-当前main.txt`
   PASS）⇒ 但本树合完 P2 后 main 已是 `74501886` ⇒ **P4 叠在 P2 之上**顺序应用。
   两包同改 `tools.rs`/`tools_brush.rs` 而**行段互斥**（P2 hunks 5928／15588／17051／17272、
   tools_brush 160／982／1844；P4 hunks 14046／16550–16700／16949–16970、tools_brush 1505–1565）✓；
   `git am --3way` 两片**全部干净**（`3fd25dbc`、`171a1f8b`，无冲突）✓。
   小噪音记一笔：包提交信息里「3011363 起 latent」应为「第 1363 轮」——文字漂移，不碍事。
② **内容审阅**（四件根因 + 一件跟进）：
   **P-4（成本与可见性）**——`smudge_dab` 拖曳长度 ≤0 早返（`oil-01-paint` 的 `smudge_length` 是 0 ⇒
   此前每步都在为一次**数学上恒等的空操作**付全部逐像素 `powf` 成本 ✓）；掩膜遍 `skip_smudge`
   （掩膜只记 dab 落点，涂抹只在落点盘内搬颜色、不扩盘 ⇒ 掩膜遍涂抹是纯成本；正确性靠
   读回用 `deposited || moved`——主遍涂抹改过的像素由 `moved`（±3 容差）独立守住 ✓）；
   `srgb_to_linear` 走 **fix15 精确查表**（32641 项、表项由同一函数逐项生成 ⇒ **逐位相同**，
   D0 基线不受影响，证据是 `fix15_decode_table_is_bitwise_exact` 全定义域断言 ✓）；
   `paint_brush` 拒绝预测 >30 s 的涂抹笔（`invalid_argument` 写明路径／半径／预算与三条改法——
   **不静默跑 7 分钟** ✓，符合「静默降级是唯一对手」）。
   **P-6**——watercolor-02-paint `smudge` 0.95→0.3 加 `brush_radius` 曲线、`smudge_length` 0.1→0.03
   （大面积洗白是**笔刷数据**问题，不是引擎问题 ✓）。
   **P-7**——`MAX_DAB` 64→512：六插件 + 宿主 `MEDIUMS` + schema 文案三处齐改 ✓；
   宿主测试 `all_mediums_allow_a_512_dab` **量实测落墨带宽**（420 请求 ⇒ 300..=480）而**不**断
   `paint_stroke` 回的 bbox——包自己写明理由：回的 bbox 是按请求值**规划**的区域，只断它会恒绿
   （变异「插件改回 64」不红）⇒ 这正是四层齐的「输入能触发」层 ✓。
   **P-17**——classic-knife `opaque_multiply` 压力峰值 1.0→0.99，避开 `opaque_linearize` 在 1.0 的
   奇点（压力峰变成一道暗竖棱）。
   **2/2（s36 跟进）**——净室侧涂抹量与拖曳长度改读 `radius_evaluated_value`（只算 `brush_radius`
   映射的求值后值）⇒ **预算闸门与报错数字和实际跑的拖曳一致** ✓；`radius_evaluated_value` 定义在
   `tools.rs:16544`（私有），`tools_brush.rs` 经 `use super::*` 可见（父子模块可见性 ✓，编译实证）。
③ **本机复验**（合并例外的内存边界，§96③ 同口径）：`cargo check -p yanshi-server --tests`、
   `-p yanshi-medium-host --tests`、`-p yanshi-render --tests`、六插件 `cargo check` **全部 exit 0** ✓；
   pre-commit（fmt ＋ **workspace clippy `-D warnings`**）✓。可用内存 **2.4 Gi < 3 Gi** ⇒
   定向 test（`medium_stroke` 420 落墨、宿主 512 带宽、预算模型、fix15 逐位）与
   `cargo test --workspace` **交 CI** ✗ 不自称本机绿。包侧变异（改回 64 ⇒ 带宽断言红等）在盒上记录、
   本树不代跑；本地二进制早于本提交 ⇒ 本机跑判据**只验脚本形状不取产品数字**（④ 有交代）✓。
④ **回归普查**——**查出一条必假红并已修**（包侧没点名，是本普查抓的 ✗）：
   `tool-medium-stroke-size-clamp.mjs` 写死 `CAP = 64` ⇒ P-7 后请求 192 **不再夹紧** ⇒ 判据必红。
   按「判据里的数字分两类」改成**探测上限**（请求 4096 ⇒ 回的 `size_used` 即当前上限），
   原三条断言**换形保义**，另加④「上限之半不夹紧且回显」防「全都夹紧」的假阳性，
   补 `uncaughtException ⇒ exit(3)`（第 1084 轮规）。**用 P4 前的 release 二进制实跑**：
   探测=64、五项全绿、服务不在时 EXIT 3 ✓——测的是脚本逻辑与响应形状（`size_used`/
   `size_clamped`/`dirty_bbox` 字段名），不是产品数字（二进制陈旧，按新鲜度规则只作此用）✓。
   其余核对：`medium-abi-check` 读各插件 `yanshi_max_dab()` ⇒ 天然自适应 ✓；classic-knife 在
   `brush_stroke.rs`/`preferences.rs` 只当「有东西可抹」的底层，断存在性／裁剪不断定值像素 ⇒
   1.0→0.99 不打红 ✓；**无判据点名 watercolor-02** ✓；涂抹预算只可能拦 `smudge>0` 且拖曳>0 的笔，
   判据里最大几何是 `tool-smudge-drags-color`（size≈H/8、路径数百 px ⇒ 预测 ~1 s ≪ 30 s）✓；
   CI 在测试与判据前**重建介质 wasm 并 `mediums-sync`** ⇒ 仓库内旧产物的陈旧问题不进 CI ✓。
⑤ `git am` 不过 pre-commit 钩子 ⇒ 本留痕提交**另行复跑** fmt/clippy/台账编号/守卫 ✓（§90⑤ 同规）。
⑥ **后效（两条 run，红→修→绿）**：
   · **run 40688036338**（`98ecde5e`）：除 criteria shard 1/6 外全绿；该片 2 条意外红。
     **①`check-smudge-mix-uses-linear` ⑤ 定性为判据侧假红**——它用文本模式 `/srgb_to_linear\s*\(/`
     钉「涂抹走 sRGB 解码」，而 P-4 把逐位等价的 LUT 查表（`decode_fix15_with_table`）接进
     `smudge_dab`，语义在、字面没了 ⇒ **接线判据的文本模式必须跟上被接线的那次重构**（内核①：
     「看起来对」的文本断言不是证据）。判据改为认三种等价形态（`srgb_to_linear(`／
     `decode_fix15_with_table(`／`srgb_decode_fix15_table(`），**变异实证**：把查表调用改坏成
     常量 ⇒ ⑤ 红、EXIT=1；`git checkout --` 还原 ⇒ EXIT=0 ✓（提交 `47706592`）。
     **②`browser-brush-preview`（2B_pencil）定性为外部瞬时**——三条证据：新 release 二进制
     curl 该笔预览 **ok=true、PNG 8449 B、http=200** ✓；本机按 CI 同等编排
     （`run-criteria.sh`＋fresh debug 二进制＋真 headless chromium，SHARD 技巧单跑）**通过** ✓；
     HANDOFF 第 1103 轮有同判据「环境抖动」先例（第 10 轮修过 `__deflate` 死锁后它本就在抖动名单附近）。
     不当产品问题登记，靠复测定性。
   · **run 40692645078**（`47706592`）：**completed/success**，判据六片全绿 ⇒ ① 的修复在 CI 上
     成立、② 复测通过（未复现）⇒ 外部瞬时定性成立 ✓。P4 收账完毕。
   · 留痕时订正一处插入错误：§96⑥（P2 后效）曾被追加到文件尾＝§97 之后（第 254 轮「插入位置」
     坑的文档版）⇒ 本次搬回 §96 末尾，§97⑥ 重写为本段 ✓。
## §98 paint-fix 期 7 后续包（misc：写锁/回执/去重/过滤/alpha/看门狗）合并留痕（2026-10-28，`74ea27ac`）

① **基线与接缝**：包基线 `c6b1926`（**实测成立**：`git log` 里 §97/P4 代码都在它祖先链上 ⇒
   「已含期 2 与期 4」不是口说 ✓）。合入时 main＝`767903b9`，漂移 `c6b1926..767903b9` 只有
   `scripts/check-smudge-mix-uses-linear.mjs` 与 `HANDOFF.md` 两个文件 ⇒ **与 P7 的 13 个文件零交集** ✓
   ⇒ `git am --3way` 一笔干净落地（`74ea27ac`，13 文件 +1020/−64）。包侧 `verify/重放验证.txt`：
   基线上 am 成功、重放 tip `254d47e` 与 `ebd1626` 树一致 ⇒ VERIFY PASS ✓。
② **内容审读**（逐项，含四层齐核对）：
   · **P-13 写锁**：`FileStore::open` 建 `<root>/.yanshi-write.lock`（`create_new` ＋ pid）；
     跨进程活着⇒`Conflict` 点名 pid、死了⇒删锁自愈、同进程⇒`HELD_WRITE_LOCKS`（`Weak` 注册表）共享 ✓；
     `Server::new`（MCP）与 HTTP `serve` 遇 `Conflict` **不回退内存、直接启动失败** ✓——
     修的是「第二进程静默回退 ⇒ 写全丢」这个比双写更坏的形态（内核：静默降级）。
     判法在 Linux 用 `/proc/<pid>`，非 Linux fail-closed（宁可报错等人工，不冒双写险）✓。
   · **P-14**：`export_timelapse` 回执补 `scale_ms`/`write_ms`＝**null**（MCP 路无此两段，不编数字冒充 ✓），
     五段账齐；测试**先断键存在再断 null**（缺键时 `result[field]` 也返回 Null ⇒ 单断 `is_null` 是安慰剂 ✓）。
   · **P-16**：`dabs_per_second=0` 的警告去重，键＝（会话，笔刷），内存态、换会话重报 ✓。
   · **P-18**：`list_assets` 按 `asset_layout` 扩展名过滤（一处改，brush/texture/palette 三类同受益 ✓）；
     测试数盘上 `.myb` 与回执条数**相等**（下界不写死——判据里的数字分两类 ✓）。
   · **P-19**：`color.a` 乘进整笔 `Opaque`（口径与颜色解析一致：对象式 0..255、数组量级切换、
     `linear` 直通；hex 的 `a` 仍只警告不生效——**两口径不混** ✓）；`a=255`/无 `a` ⇒ 什么都不做
     （逐位相同 ✓）；`a=0` ⇒ 零墨规矩 `precondition_failed`，文案点名「opacity 是 0」不再误导加长路径 ✓；
     `color_to.a` 仍忽略＋警告保留 ✓。`ink_pixels` 旧判据（钉「被忽略」的）按其自述**同步改写**
     （原文写着「一旦改成乘进 opacity 本判据会红」⇒ 这正是那次红该被消费的地方 ✓）。
   · **P-20**：MCP `serve` 起始拉看门线程，200 ms 比 `getppid`，父变则一行 stderr＋`exit(0)` ✓；
     `parent_death.rs` 用 `bash -c … < /dev/stdin` 造「无 EOF」现场（说明里写清了 sh/bash 把后台任务
     stdin 指 `/dev/null` 的坑 ✓——判据前提不成立≠产品失败的同族陷阱）。
③ **本机复验**（合并例外，§96③/§97③ 同口径）：`cargo check -p yanshi-server -p yanshi-mcp -p yanshi-http
   --tests` **exit 0** ✓；pre-commit（fmt ＋ workspace clippy `-D warnings` ＋ 四护栏）✓。
   **护栏抓到两处包侧没做的**（本普查＋守卫 ✗ 包上 8 项变异全过却没拦住这两条 ⇒ 守卫比盒上的眼严 ✓）：
   · `check-tests-clean-temp` 红：`parent_death.rs` 用 `temp_dir()` 但清理器没按谓词绑定 ⇒ 补
     绑定式 `Cleanup`（断言提前失败也要删，`/tmp` 是 1.9 GB tmpfs ✓）。
   · `tool-brush-speed.mjs` **必假红**：它给 `color` 顺带 `a: 40`（旧语义「a 被忽略」下无害）⇒
     P-19 后墨整体压到 0.157 倍，它断的 `avg_brightness` 差（门槛 0.05）会被压小。去掉 `a`
     （判据自己造的输入，测的是时间间距⇒墨量，与 alpha 无关）；**本机实测差 0.369／门槛 0.05** ✓。
   定向 `cargo test --workspace` **交 CI**（内存 2.4 Gi < 3 Gi ⇒ §95③/§96③ 措辞）✗ 不自称本机绿。
   **新 release 二进制冒烟**（P7 后重建，新鲜度 ✓）：P-13 第二进程同 root **拒启 exit=1、stderr 点名写锁** ✓；
   P-16 首笔有警告、同会话第二笔不再重复 ✓；P-18 brush 列表 **199 条全 .myb**＝盘上数 ✓；
   P-19 `a=255`≡无 `a`（`ink_mass` 550462 恒等 ✓）、`a=110` 真变淡（230552 ✓）、`a=0`
   `precondition_failed`＋文案点名 opacity ✓、`color_to.a` 警告保留 ✓；P-14 HTTP 回执
   `scale_ms`/`write_ms` 键在且如实 null（frames=1 ✓，path 须落在 root 沙箱内——第一格我用 `/tmp` 绝对路径
   被沙箱拒是**我的调用错**不是产品 ✗）✓；`tool-time-ms` 本地（新起服务）④⑤⑥ 全绿 ✓。
④ **回归普查**（六面各自的既有断言）：
   · P-13：判据起的自备服务全走 `mktemp` 独立根（`with-temp-server.sh` ✓）；`tool-cold-open-foreign`
     的「SIGKILL 后第二个进程开同根」正是**死 pid 自愈**路径 ✓；`check-k01-both-hosts` 是静态判据不碰服务 ✓。
   · P-14：无任何 `*.mjs` 引用 `apply_ms/scale_ms/write_ms` ⇒ 新增 null 字段不打红既有断言 ✓。
   · P-16：唯一断「不吃时间必须报警」的是 `tool-time-ms` ④；CI 单服务上它是 `100%_Opaque`＋time 的
     **第一笔**（全库 grep 只它一个）⇒ 首报归它 ✓。⚠ 语义变化如实记：**同一长期运行的服务里第二次
     探针不会再有 warning**——本机重复跑请用 `run-criteria-local.sh`（每次新起服务 ✓）；
     `check-deposit-mask-time-base` 只静态读 `brush_deposit_mask` 函数体，P7 没碰那个函数 ✓。
   · P-18：`tool-offline-asset-coverage` 的「199 支」与过滤后的真 `.myb` 数**正好对齐**（原来 201 是含
     两份说明文件的虚数 ✓）；无判据断 201 ✓。
   · P-19：全库找 `a<255` 的判据用例：`tool-brush-speed`（已修 ✓）、`tool-gpu-pure-parity`（CPU/GPU
     **同一条 paint_brush** ⇒ delta=0 恒等不受影响 ✓）、`tool-macos-gpu-benefit`/`tool-gpu-backend-report`
     的 a:210/220 走 **`fill`**（不碰笔刷 opacity ✓）；`tool-vermeer-study` 的 `a:180` 只断 `ok===true` ✓；
     `tool-hex-alpha-warning` 断的 hex 路 P-19 明言不动 ✓；UI 侧颜色恒 `a:255`/hex ⇒ 无漂移面 ✓。
   · P-20：只影响 `yanshi-mcp serve`；`mcp-*` 判据的父进程是活着的 node，退出走 EOF/kill 正常路径 ✓。
⑤ `git am` 不过 pre-commit 钩子 ⇒ 本留痕提交**另行复跑** fmt/clippy/台账编号/守卫 ✓（§90⑤ 同规）。

⑥ **后效（run `40697212382` 复判，completed/failure）**——红共 5 面，逐面归因与处置：
   · **test (stable)/(beta) ×2：`textures.rs` 的 `the_list_reports_name_size_and_usability`** ⇒
     P-18「该不该列由扩展名定」与**包侧没回去改的邻居测试**冲突（包侧在 `assets_import.rs`
     新增同口径测试＋变异，却留着旧钉「NOTICE.md 要留下／canvas.jpg usable:false」的邻居 ✗ ⇒
     他们「86 过」只跑了**定向**——§95 例外那句「外部补丁的测试覆盖≠本仓的测试覆盖」再次实证 ✓，
     而抓到它的正是「`cargo test --workspace` 交 CI」这条规矩本身 ✓）。
     **裁定：产品保留 P-18，测试改写为新语义**，依据三条：① 同口径已在 `assets_import.rs` 钉死（带变异）✓；
     ② 许可凭据在**盘上的 NOTICE.md**（`fetch-textures` 照写 ✓），不靠工具列表露出 ✓；
     ③「usable:false 说清」收窄到**本类内**——扩展名对而内容不行（`grey.png`）仍列仍标 ✓。
     夹具照写 `NOTICE.md`/`canvas.jpg` 当**负对照**（过滤被删 ⇒ 名字相等断言当场红 ✓）。
     **变异**：把过滤条件恒假 ⇒ `textures.rs` 与 `assets_import.rs` 两条同红（实测），恢复后 14 测试全绿 ✓。
   · **criteria shard 1：`tool-pressure-curve-echo` ③a/③b** ⇒ 它钉的是旧语义「`color.a` 恒被忽略」，
     P-19 正是那次红的**消费**（包侧同步改写了 `ink_px` 那条旧判据、漏了这条；见下方 ④ 订正）。
     改写为新语义：a=110 **静默**（对象式＋数组式）＋ a=0 `precondition_failed` ＋ **独立文档相对墨量臂**
     （实测 0 ⇒ 8562 ⇒ 10197 ✓）。**变异（静默忽略：`color_alpha_factor` 直接 `return None`）**：
     ③a/③b **仍绿**（只删话不改行为的形态）⇒ 而 ③d/③e0/③e1/③e2 **四条全红** ✓——效果臂挡的正是它 ✓。
     附带 `pixels()` 墨门槛 600→762：半透明笔芯在 600 下数成 0 ⇒ 比较前提不成立（本机实测逼出来的 ✓）。
   · **criteria shard 3：`mcp-watch-external`** ⇒ **编排撞 P-13**：`kill()` 只发信号不等待 ⇒
     负对照第二台在旧进程**还活着**时启动 ⇒ P-13 正确拒绝 ⇒ 判据误读成「产品红」✓（正是写锁要防的
     双写形态，锁没错 ✗ 判据排队错）。修法：`killAndWait` 等 `exit`（残留锁由下一台死 pid 自愈 ✓）
     ＋ 把 P-13 冲突单列为 exit(3)「判据前提不成立」。**当场踩到的新坑**：启动 banner 的
     `profiles=["core","conflict",…]` 里有 "conflict" 一词 ⇒ 冲突正则**不许裸写** `/conflict/`
     （本机首跑即被正常启动误判 ✓，改锚定「启动失败：conflict／已被进程…占用／写锁文件名」）。
     本机重跑双臂绿 ✓。
   · **criteria shard 4：`tool-schema-params-census` ①** ⇒ P-19 给 brush_stroke 文案加的
     `precondition_failed`（**错误码**、`ErrorCode` 枚举里真有 ✓）被当「无处可落的参数名」。
     按 census 自规「只放引擎真有的、不放通配」补 `errorCode` 分类（单名 ✓）。本机全绿
     （268 名恒等式：已分类 268＋未分类 0 ✓）。
   · **跳过面无遮蔽**：shard1 的 8 个 ⊘ 与 P4 run `40692645078` 的 ⊘ **集合逐一相同**（diff 实测 ✓）
     ⇒ 没有新判据被 P7 打成「不可判」藏红 ✓。
   · **④ 订正**（内核②「变了回去改」的文档版）：④ 的 P-19 六面普查只扫了「判据造 `a<255` 输入」的
     **数值面**，没扫「判据断旧文案『被忽略』」的**措辞面** ⇒ `tool-pressure-curve-echo` 从网眼漏过。
     教训一般化：**语义反转 ⇒ 旧语义的全部断言面都要枚举**（grep 旧文案＋旧字段＋旧数值），缺一面必红。
   · 复判提交后重推 ⇒ 等 run 收敛；**P7 收账以新一轮 CI 为准**（不自称已绿 ✓）。

## §99 PWA 本地内核批次 3–6（detach/transform/analyze/prefs 镜像）合并留痕（2026-10-29）

① **基线与接缝**：`~/patch/web_patch.tar.gz` ＝ **裸 diff 格式** ✗（**无 From/Subject ⇒ 不是
   mbox ⇒ `git am` 不适用，只能 `git apply` ✓**）；四片 `0d24a23b→94a2768a→08e9e488→886b3a50→
   a1ff6891` 名字自述 hash 链，**首尾相接 ✓**。四个基线 blob 在本仓 `git cat-file -e` **全部不存在** ✗
   （**∴ 上游是另一份 clone ⇒ 包侧无法在我仓重放验证 ⇒ 只能以「对当前 main 逐片直接 apply」代 ✓**）。
   合入基线 main＝`66f89293`（**§98/P7 收账那一轮 run 40704687440 还在跑 ⇒ 本留痕与合入在
   try-web-batch3456 上完成，push 等组空 ✓**）。try 分支上四片按序 `git apply` **全部 hunk 命中** ✓
   ⇒ 8 文件 +4299/−260。**铁律核对**：`web/api-local.js` ≡ `crates/yanshi-http/assets/api-local.js`
   **逐字节相同 ✓**（**每次改动后 `cmp` 重验 ✓，含我的二期修 ✓**）；`node --check` 两份均过 ✓。
   包内 `._*` AppleDouble 垃圾已剔除（**不是我造 ✗，是 tar 带的 ✓**）。
② **内容审读**（逐项，含四层齐核对；**二期复核抓到两真一分叉** ✓）：
   · **真分裂 1（已修 ✓）detach 副本 data**：本地读裸净荷 `mv.data` ✗ 而服务端复制的是折叠后
     `master.data.clone()`（`tools.rs:6325` ✓）⇒ **master 被 `set_property` 过就分裂** ✓ ——
     正是 `p2EffectiveData` 头注点名的「凡按 `data` 解析语义的地方都必须经这里」。修 ⇒ 用 `meff`
     （**`type` 口径同步改 `meff.type ?? "shape"` ✓ 与 `master_data.get("type")` 逐字 ✓**）；
     **判据补 §E1**（**先断言服务端 master 折叠含 override ⇒ 权威前提 ✓**，变异＝改回 `mv.data`
     ⇒ 实测「本地副本缺 override」红 ✓）。**此前 §A 的输入从没 set_property 过 master** ⇒
     折叠面恒绿 ✗（**四层齐第②层缺 ✓ 的又一个实例**）。
   · **真分裂 2（已修 ✓）撞 id 静默降级形态③**：服务端 `log.rs:100` 对 `create_object` 是
     `MustNotExist` ✓（**判 `state.objects.contains_key` ⇒ 墓碑也算占位 ✓**）＋层 `MustExist` ✓，
     顺序在锁之前 ✓；本地只有 `p2LockGuard`（只管锁）⇒ **显式 `object_id` 撞活对象照写 ✓、被折叠层
     丢弃（`fold.rs:551`）⇒ 回执 200 而状态里没副本** ✗。修 ⇒ `p2RefGuard` 新增 `create_object`／
     `import_image` 拒生支路 ＋ `detach` 建副本一步先过 guard；**判据补 §E2**，变异＝删该支路 ⇒
     实测本地 200／服务端 409、文案 undefined、I7 被吞，三条全红 ✓。
     **∴ 同族既有端点（`create_instance`／`create_group` 等）的同类缺口不在本批 diff 面 ⇒ 记
     known-gap 给上游 ✗**（**普查不顺手改旧面 ✓**）。
   · **真分叉 3（已修 ✓）报错文案**：本地「instance_id 必填（字符串）」✗ 服务端
     `require_str`⇒`missing`⇒「缺少必填参数 instance_id」（`tools.rs:1524` ✓）⇒ **5 处**
     （create_instance／detach／link_to_master／update_override／update_sync_policy）**全改逐字 ✓**
     （**`assert count==5` 换完再 assert ✓**）。判据 §C 的逐字比此前**没有**「缺参」这条输入 ⇒ 恒不触发 ✗
     ——**本条属第②层缺口的措辞面**（与 §98⑥「语义反转要枚举全部断言面」同族 ✓）。
   · **实现侧其余改动（核对 ✓）**：`detach` 两步同 changeset ✓、tombstone 步 `p2RefGuard` ✓（半成品
     同形，包 §D 已断）；新错误支路文案逐字 ✓；`docs/design/brush-capability-gap.md` 表格同步 ✓
     （**表变了回去改 ✓ 本批自带 ✓**）。
   · **已核对、未修（记 known-gap ⇒ 上游）**：
     ① `reference.blob_hash` 大写：本地正则**只收 64 位小写**（`api-local.js:3851` ✓ 实测读码）⇒
       与「本地单独接受大写」**不符** ✗（**审查 agent 说反了 ✓ 两边同拒**，只是分叉在文案与形态）；
     ② prefs 里参考 blob 已缺：本地归「参考图不是能解码的 PNG」✓ 服务端 ⇒ 404 —— **不是静默成功** ✓
       （三态都如实 `ok:false` ✓），但**原因分类分裂** ✗ ⇒ 记；
     ③ batch4 各 null/缺省分叉：texture jitter/mode 的 null 路径**本地更严**、`closed:null`／
       `fill_rule:null`／`snapshot:null` **本地更宽松** ⇒ 与服务端字面不同 ✗，判据输入不覆盖 ⇒ 记；
     ④ `powf`/`cbrt` 双舍入 ~1 ulp ΔE 风险：判据输入是字节域（0/255）⇒ 无现行红面 ⇒ 记；
     ⑤ 未知参数报错**顺序**（本地插入序 vs 服务端 BTreeMap 序）⇒ 单条报错面一致，仅多错并存时分叉 ⇒ 记。
   · **虚警两条（核对后排除 ✓）**：`p2cKernelFor` 在 `withKernel` 之外的「重入风险」——它只做
     `__loaders.wasm()` 加载与 `new WasmKernel` 构造，**不碰任何借用** ⇒ 非重入 ✓；
     「reverted===2 写死数字」——2 是 detach 自己两步 ⇒ **判据造的输入** ✓（第 798 轮表格第一类），可写死。
③ **本机复验**（合并例外口径 ✓）：
   · 新判据三份：`tool-pwa-instance-detach` **绿 ✓**（含 §E 两条新臂 ＋ 两次变异各自能红 ✓）；
     `tool-pwa-transform-object` **绿 ✓**；`tool-pwa-pixel-stats` **exit(3) ⊗**（**内核产物 2026-10-15
     比 `crates/yanshi-{wasm,render,core}` 源码旧 ⇒ 新鲜度守卫按设计生效 ✓＝「判据前提不成立 ≠ 产品失败」**
     ——CI 重建 wasm 后才有真判）。
   · `tool-pwa-*` 全量 14 份：**13 ✓／1 ⊗**（pixel-stats）。⚠ 其中 `api-behaviour`、`assets` 各出现过
     一次红：前者**重跑即绿**（瞬时，未复现 ⇒ 如实记 ✗ 不追成缺陷）；后者是**介质产物 mtime** ——
     源码与 HEAD 无 diff（`git diff HEAD -- crates/yanshi-medium-oil` 空 ✓）而 git checkout 把
     `lib.rs` 时间戳刷成今天 ⇒ **陈旧是 checkout 造成的 ✗ 不是真落后 ✓**（产物已不入库 `1000937f` ✓）
     ⇒ `touch assets/mediums/*.wasm` 后绿 ✓。
   · **`cargo test --workspace` 交 CI**（内存可用 2.1 Gi < 3 Gi ⇒ §95③/§96③ 同措辞 ✗ 不自称本机绿）。
     **workspace clippy 未本机跑**：本批**零 `.rs` 改动**（staged 清单实测：唯一 Rust 侧文件是内嵌资源
     `assets/api-local.js` ✓）⇒ 编译面与 HEAD 完全相同，而 HEAD 已在 run 40704687440 上验过；
     本机跑的 `cargo fmt --check` ✓ 与 ledger 编号守卫 ✓ 都过。**⇒ CI 是最终判 ✓**。
④ **回归普查**（六面）：
   · Rust：零 `.rs` 改动 ⇒ 单测／Rust 判据无触碰面 ✓。
   · api-local 改动面＝detach 路径／`p2RefGuard` 新支路（**只认 create_object|import_image** ⇒ 既有
     调用方 kind 不含这两 ⇒ 零影响 ✓）／5 处文案 ⇒ 风险面全在 `tool-pwa-*`，**已全量跑 ✓（③）**。
   · 双份拷贝：`cmp` 逐字节 ✓（①）。
   · docs 表格：本批自带同步 ✓（②）。
   · 判据侧：§E 以**服务端为权威**断 ✓（两边同错不能同时绿 ✓）；§E2 的存活集合断言**各比各的 id**
     （**E1S/E1L 故意不同 ⇒ 不做 whole-list 比 ✓ 同源核对 ✓**）。
   · known-red：未加条目 ⇒ `scripts/criteria-known-red.txt` 不动 ✓（**pixel-stats 的 ⊗ 只在无
     新鲜 wasm 的本机 ⇒ 不进名单；CI 有重建面 ✓**）。
⑤ **落账与接缝预告**：本包是 `web_patch` 链的前四片；**batch7（`a1ff6891→dc4898c5`）已在
   `~/patch` 排队（任务 #47）⇒ 其基线正是本包的终点 ✓**。`git apply` 不过 pre-commit ⇒ 留痕另行
   复跑 fmt／台账编号／守卫 ✓（§90⑤ 同规）；push 等 run 40704687440 收敛（§98③ 同规）。
⑦ **CI 后效（收账 ✓）**：run `40722469155`（HEAD＝`282b0cfe`）**completed/success** ⇒
   §99 各面成立：`tool-pwa-instance-detach`（含 §E1/§E2 新臂）在 CI 绿 ✓、
   `tool-pwa-transform-object` 绿 ✓、**`tool-pwa-pixel-stats` 的 ⊘ 如预期被 CI 的新鲜 wasm 重建消费 ⇒
   真判且绿 ✓**（本机 exit(3) 不是躲过去，是**换了有前提的判** ✓）；`tool-pwa-*` 全族无回归 ✓。
   known-gap ②①–⑤ 与 §99② 的批 3–6 分裂面**不在本轮红面里**（判据输入不覆盖 ⇒ 绿不绿都不证明它们 ✓
   ——**这正是把它们记进台账而不是忘掉的意义 ✓**）。

## §100 paint-fix 期 5 后续包（P-8 smart_select：错标＋链式生长）合并留痕（2026-10-29）

① **基线与接缝**：包基线 `25e91bcc`（**实测为本仓 HEAD 祖先 ✓**；上游声明的 tip `dd2642d1` 与
   重放 tip `710d2e12` 不在本仓 ✗ ⇒ 同 §99①：**包侧树等式只能信其自述，我侧以「apply 干净落地」代 ✓**）。
   基线序核对：`25e91bcc` 是 P6 基线 `c6b19269` 的**祖先** ✓（相隔 12 笔）⇒ **合并顺序 P5→P6** ✓
   （任务队列 #45→#46 恰与其一致）。`git am --3way` 于 main＝`282b0cfe`（§99 批 3–6 已入）一笔落地
   （`301123df`，3 文件 +424/−10 ＝ **与包自述逐字相同 ✓**）；补丁 sha256
   `c2fc68bb…548105` **与说明书逐字对上 ✓**。漂移面交集只有 `tools_registry.rs` ⇒ 审读见 ②。
② **内容审读**（四层齐口径逐项 ✓）：
   · **P-8 错标修**：`write_smart_select` 的 `(_, de_max, …)` 实取 de_in ⇒ 改回第一位 ✓
     （**回执 `effective_delta_e` 从 0.1165→真值 0.1456@t=0.25** ⇒ **语义反转** ⇒ 按 §98⑥ 教训
     枚举旧语义全部断言面：**全库 grep `effective_delta_e` 判据 0 处** ✓、新增 `chain_step_delta_e`
     无既有引用面 ✓、注册表摘要 1 行与批 3–6 的 smart 摘要行相隔 ~486 行 ✓ 无撞车）。
   · **P-8 断崖修（链式生长）**：核像素**双条件**（距种子 ≤ΔE_max ＋ 距生长父 ≤ΔE_step＝0.5×ΔE_max，
     常量 `WAND_CHAIN_STEP_RATIO` 一处定义 ✓）；覆盖率环仍纯球 ✓；**非连通（contiguous=false）
     无父像素 ⇒ 行为不变 ✓**（有专测 `noncontiguous_ignores_chain` ✓）；**游程首像素父色取
     `lab_at(x.clamp(xl,xr), y)` 对角核像素** ✓——包自述第一版在此漏沿边环列洪水（与旧二进制逐位相同
     ⇒ 等于没修），**这是包侧自己发现并补的 ✓**。回执新增 `chain_step_delta_e` ✓（信号可见，
     不静默 ✓）；**不加**「阈值球覆盖色域比例」的理由成立 ✓（链式后球不是漫溢机制，报它会误导）。
   · **诚实面核对**：「兔图断崖修不掉＝图像性质」的论证带三组实测距离分布 ✓；**渲染漂移 253 vs
     8,915,103（15–24 连选之间）** 是新发现的**未归因缺陷** ✗——包只绕开（每种子新进程）没有修 ⇒
     **我侧记为待分诊项**（与 §99 known-gap 同列，交 05 报告下一轮核对）。
   · **判据侧**：包不自带 `scripts/*.mjs` ⇒ 无判据改动面 ✓；`reverted` 类写死数字问题不存在。
③ **本机复验**（合并例外 ✓）：`smart_select_p8` 集成 **2/2 ✓**；`--lib smart` **23/23 ✓**（含包自述
   六条新单测的全部链式面：`chain_stops_at_step_edge`／`chain_fills_smooth_gradient`／
   `chain_run_start_parent_must_be_core`／`noncontiguous_ignores_chain`，`wand_thresholds` 标定条
   在 smart 过滤器内 ✓）；回归 `smart_layer_prefail` **10/10 ✓**；`cargo clippy -p yanshi-server
   --all-targets -D warnings` **exit 0 ✓**。**定向 `--workspace` 全量 test 交 CI**（可用内存 2.1 Gi
   < 3 Gi ⇒ §95③ 同措辞，不自称本机绿 ✗）。**变异两面（本机、断言粒度 ✓）**：
   M1 错标改回 de_in ⇒ 集成 2 条全红 ✓；M6 对角父 clamp 改回本行同列 ⇒ **只有
   `chain_run_start_parent_must_be_core` 红**（其余 3 条绿 ⇒ 变异打在该断言自己的路径上 ✓）；
   恢复后 `git diff` 干净 ✓、判据面重跑绿 ✓。
④ **回归普查**：`tool-smart-select-block` ✓／`tool-smart-select-add-merges` ✓（纯色块与 add 两路
   在链式下不变——**纯色相邻步 ΔE＝0 ⇒ 链式恒真，判据输入造不出分裂 ✓**）；
   `magic_wand_scanline` 全部 Rust 调用点在 `tools_smart` 测试内 ✓（5416 是唯一生产调用 ⇒ 面即 smart_select ✓）；
   wasm／HTTP 无该函数副本 ✓（本地内核 `api-local` 不实现 smart_select ⇒ 无双侧口径面 ✓）。
⑤ `git am` 不过 pre-commit ⇒ fmt／clippy／台账守卫另行复跑 ✓（§90⑤ 同规）；push 等 §99 那轮
   run 收敛（同组不双派 ✓）。

## §101 paint-fix 期 6 后续包（P-12 取证回执／P-15 分布／X-5 live total）合并留痕（2026-10-29）

① **基线与接缝**：包基线 `c6b19269` **在本仓存在 ✓**（P7 的基线）且为 HEAD 祖先 ✓；
   上游声明的本期 tip `5f06875`／重放 tip `4b524e89` 不在本仓 ✗（同 §99①/§100① 的处置：
   包侧树等式信自述，我侧以 apply 干净落地代 ✓）。**说明书未写补丁 sha256** ✗（P5 写了 ⇒ 口径不齐，
   如实记；本包 sha256 `5c73d48e…6966e0` 留此）。**基线序核对决定了合并顺序 P5→P6**（§100① ✓）：
   P6 自述「已 rebase 到 c6b1926、含期 5」⇒ **队列里的 #45→#46 恰与祖先序一致 ✓**。
   **落枝方式**：§99 那轮 CI（40722469155）在跑 ⇒ 不双派 ⇒ **P5、P6 叠在 `try-p5-selection` 上
   合为一个候选**，随轮 push（§897「攒几处一起提交」口径 ✓）。`git am --3way` 一笔落地
   （7 文件 +1444/−13 ＝ **与自述逐字相同 ✓**）；自述的 rebase 冲突点只在 `tools_tests.rs` 文件尾
   ⇒ 我侧落点直接干净 ✓（测试模块在尾部追加 ⇒ §「测试模块留文件末尾」规矩未被破坏 ✓）。
② **内容审读**：
   · **P-12（draw_*/fill/erase 五项取证）**：提交前按几何算脏区预渲染 ＋ 提交后用 `result.dirty_bbox`
     验算，**超出即放弃取证（字段在位＝null ＋ `evidence_note`）✓**（形态⑤的对偶：「不算」要明说 ✗）。
     **三处如实披露核对 ✓**：① T-E 验收第 3 条**自认不成立**（静默 batch 每笔加 2 次渲染违反
       silent 约定、是 P-21 级成本 ⇒ null＋note 是「明说不提供」✓ 不是静默缺失 ✓）；
     ② **erase 例外口径**（tile 法恒 0 ⇒ 改前后区域比较；`painted==changed_pixels`、`ink_mass` 恒 0）✓；
     ③ **代价写数**（交互每笔 ＋2 区域渲染 ＋1 状态克隆；静默 0）✓。
     `write_fill_region` 的内部 `write_draw` 调用点同样带取证 ✓（一处修覆盖调用面 ✓）。
   · **P-15（分布）**：`max` 口径**不动** ✓（只加 256 桶直方图，同一遍遍历 ⇒ 零额外遍历 ✓）；
     分位数规则**三处共用一份** ✓（一个事实一处 ✓）；**实测值与分诊不一致处不冒充**
     （淡色笔 max==2 vs 分诊报 1 ⇒ 几何不同未复现，断言以本轮实测为准 ✓——数字带来源 ✓）。
   · **X-5（live total）**：按**最近动作**判撤销（reapply 回来算 live）✓；**`list_atoms` 不顺手改** ✓
     （两边口径本来就不同 ⇒ 照字面，见 §「取值集合」教训 ✓）；恒等式 `total + reverted_excluded
     == 全部匹配数` ✓（比单点断言强 ✓）；**既有行为**「`list_atoms` 缺省视图看不见 draw 笔」
     （draw 净荷无 `source` ⇒ total==0）**如实进测试** ✓ 不改。
③ **本机复验**（合并例外 ✓）：新判 **14/14 绿**（`draw_stroke_evidence` 6／`brush_delta_distribution` 4／
   `stroke_log_live_total` 4）＋ lib 单元 `delta_distribution_quantiles_follow_the_documented_rule` ✓；
   既有相邻 Rust 面 **19/19 绿**（`stroke_log_group_filter`／`stroke_log_message`／`batch_message`／
   `ink_pixels`／`read_cursor_resume`／`list_atoms_truncation`）；`cargo clippy -p yanshi-server
   --all-targets -D warnings` exit 0 ✓。**`--workspace` 全量 test 交 CI**（可用 2.1 Gi < 3 Gi ⇒
   §95③ 同措辞 ✗ 不自称本机绿）。判据面：`tool-batch-stroke-evidence` ✓／`tool-brush-landed` ✓／
   `tool-stroke-log{,-color-shape,-vector}` ✓。**⚠ `tool-stroke-log-color-legacy` 的调用契约是
   「argv[2]＝二进制路径」而非 URL** ⇒ 经 `run-criteria-local.sh`（传 URL）必 exit(3) ⇒
   直接 `node … target/release/yanshi-serve` 跑 **绿 ✓**（**exit(3) 是脚本用法不是产品 ✗ 记免得下轮误判**）。
④ **变异本机三面重放**（断言粒度 ✓，恢复后 `git status` 干净、重跑绿 ✓）：
   M1 两个比较子恒返 None ⇒ **恰 4 红 2 绿**（null-期望两条本该绿 ✓ 与包侧预期逐字一致）；
   M2 分位数恒返 255 ⇒ 比包侧**更狠**：lib 单元 ＋ `pale_wash`／`medium`／`soft_dark` **3 条集成红**
   （包只报 2 红 ⇒ 他们的变异是「恒返 max」在我这输入上与 255 等效但面更小；如实记，不算分歧 ✗）；
   M3 撤销过滤失效 ⇒ `after_undo` ＋ `list_atoms 交叉核对` 2 红 2 绿 ✓ 逐字一致。
⑤ **回归普查**（语义反转面枚举，§98⑥ 教训 ✓）：
   · 新增字段面：**全库 grep 无判据钉「draw 回执字段全集」** ⇒ 加键不打红任何 shape 断言 ✓；
   · `paint_delta`/`max_channel_delta` 口径不动 ⇒ gpu-*／color-space-p2-9 等既有断言面不漂移 ✓
     （gpu-* 属本地特殊构建面 ⇒ 交 CI ✓）；
   · X-5 的 `total` 语义反转：唯一可能红的是「撤笔后仍断 total 含撤」的判据 ⇒
     **全库 grep 只有 tool-stroke-log ④（造 2 笔不撤 ⇒ 2 是判据造的输入 ✓）**，四条 stroke-log 全绿 ✓；
   · P-12 的渲染副作用：**渲染路径未改**（只读比较 ⇒ 像素输出恒等 ✓，`tool-pwa-pixel-stats` 面不受涉）；
   · 双份 api-local／docs 表格：本包**不碰** ✓；known-red 名单：不动 ✓。
⑥ `git am` 不过 pre-commit ⇒ fmt／clippy／台账守卫另行复跑 ✓（§90⑤ 同规）；push 等
   40722469155 收敛后与 §100 一并出去（同组不双派 ✓）。
⑦ **CI 后效（收账 ✓）**：run `40726198780`（HEAD＝`9c481d63`，P5＋P6＋§100/§101/§99⑦ 台账）**completed/success** ⇒
   两面同场收账：**§100** 的 `smart_select_p8`／链式面在 CI 的 test(stable/beta) 绿 ✓、
   `tool-smart-select*` 两片在 criteria 分片绿 ✓；**§101** 的 P-12/P-15/X-5 新判 14 条＋相邻 19 条
   在 CI test 绿 ✓、`tool-batch-stroke-evidence`／`tool-stroke-log*` 族分片绿 ✓；
   `tool-pwa-*` 全族（含本两期不碰的双份 api-local）无回归 ✓。**§100② 的渲染漂移待分诊项**
   不在 CI 红面里（判据输入不覆盖 ⇒ 绿不证明它存在或不存在 ✓，仍留台账）。

## §102 paint-fix 期 8 后续包（成本墙第 1 期：背景借用／警告滚动上限／每原子预算校准）合并留痕（2026-10-29）

① **基线与接缝**：包基线 `c6b19269` **在本仓存在 ✓ 且为 HEAD 祖先 ✓**（说明书 §基线写的正是
   §96⑥ 那笔台账提交 ✓）；上游声明 tip `faea27d3`／重放 tip `4c5e6e93` 不在本仓 ✗（同 §99① 处置：
   包侧树等式信自述＋`verify/重放验证.txt` 的 TREE OK，我侧以 apply 干净落地代 ✓）。
   zip sha256 `d08e5b5a…41aeb2d1` 留此。**`git am --3way` 于 HEAD＝`9c481d63` 一笔落地**
   （`78dc50b6`，7 文件 +186/−13 ＝ **与自述逐字相同 ✓**）——基线在 HEAD 上游 21 笔，
   中间合过的 P5/P6/detach 全部**不碰这 7 个文件** ⇒ 零冲突 ✓。
   **证据自洽核对 ✓**：`evidence/` 两枚 mincheck PNG 的 sha256 **当场算得逐字相同**
   （`a4a2b818…74697b7`，与说明书「像素逐位不变」节对上 ✓——这是「基线 vs 修复版」的
   包内侧量，**我侧等价物＝本机 15 条 tool-pwa 全绿＋ink_pixels 5/5 ✓**）。
② **内容审读**（三件事，四层齐口径 ✓）：
   · **快赢一（背景借用）**：`write_brush_stroke` 的 `document_state(ctx)` 克隆**只取一个布尔** ⇒
     改借 `ctx.workspace.document(&ctx.doc_id)` 直读 ✓；**「文档不在 ⇒ false」与旧出错分支同例** ✓
     （语义不动，只动成本）；regime 三分支的既有注释面完整保留 ✓。
   · **快赢二（警告上限 128）**：`MAX_FOLD_WARNINGS` **一处定义** ✓（fold.rs，lib.rs 再导出 ✓）；
     截断在 `state_at` 三个累积点＋`ladder_fold`＋`chain_warnings`＋`warnings_to_json` **全路径一致施加** ✓；
     **包侧自修一处旁路**：`state_at` 尾巴分支原把**未截断**的 `tail_warnings` 存进缓存、命中即绕过上限 ⇒
     现存截断后累积值 ✓（**形态④「非法路径静默放过」的同类：上限存在但有旁路＝没有**）；
     与期 7 的 P-16（同笔刷只报一次）**层面不同不冲突**，说明书写明 ✓。
   · **X-1（估算 300 KB→100 KB）**：根因是 **double-count**（旧口径含渲染缓存、新代码已另行实测叠加）✓；
     **只改估算口径、预算闸不动**（缺省 2,048 MB／超限仍拒 ✓）；校准带实测（4,002 原子冷重开 275 MB ⇒
     0.069 MB/原子，100 KB 留 45% 余量 ✓——**数字带来源与日期 ✓**）；
     **语义反转面枚举（他们做了＋我复核 ✓）**：blob GC 报错文案与注释里的「300 KB」同步改 ✓
     （`grep 300 KB` 于 service.rs 只剩历史叙述 ✓）；**我侧全库 grep `300 \* 1_024|BYTES_PER_ATOM`
     于 scripts/docs ⇒ 零命中** ⇒ 没有判据钉旧口径 ✓（反转不打红既有断言）。
   · **50% 判据不凑数 ✓**：相对本基线 33.1% 未达、相对旧基线 25e91bc 叠加态 50.4% 达标——
     **两种口径分开写、原因写明（P2 已去掉一处克隆）** ✓（内核①：不拿叠加成绩冒充单项成绩）。
   · **只定位不修的两处如实标注 ✓**：dirty_ms 的 `dependents_of` 全表扫（O(对象数)）与 raster 的
     base 段冷热波动——**未解两点也写进说明**（每对象 2.8 µs 高于理论 10 倍／重开首笔 0.5–2.2 ms）✓，
     进第 2 期 ⇒ **不进本期断言面 ✓**。
③ **本机复验**（合并例外 ✓）：`cargo clippy -p yanshi-core -p yanshi-server --all-targets -D warnings`
   exit 0 ✓；`cargo test -p yanshi-core --lib seq` **23/23 ✓**、`--test doc_budget` **6/6 ✓**、
   `--test ink_pixels` **5/5 ✓**（与自述 15/6/5 的差异＝seq 过滤在我尖端上多收 8 条既有测试 ✓）。
   **`--workspace` 全量 test 交 CI**（可用 2.4 Gi < 3 Gi ⇒ 不自称本机绿 ✗）。
   **变异三面本机重放（断言粒度 ✓，恢复后绿、`git status` 干净 ✓）**：
   M1 `background_from_state` 恒 false ⇒ ink_pixels **1 红 4 绿** ✓；
   M2 `truncate_warnings` 空操作 ⇒ `fold_warnings_are_capped_to_the_most_recent` **红** ✓；
   M3 `BYTES_PER_ATOM` 改回 300 KB ⇒ doc_budget **1 红 5 绿** ✓（三条全红 ⇒ 全绿）。
④ **回归普查**：`tool-open-fast` ✓（预算口径改动最近的判据——它断的是**结构**不是毫秒 ✓）；
   `tool-stroke-latency-4k` ✓（背景借用打在笔写路径上）；`tool-snapshot-roundtrip` ✓
   （restore_checkpoint 走 declare_head，先于 §103 落地验证服务端面不受涉）；
   `check-timelapse-shrink` ✓。**`tool-break-even` exit(3)＝本机无 GPU 适配器**（软件光栅 ⇒
   按纪律判不了 ✓ 不是产品红，CI 有真适配器面 ⇒ 交它 ✓）。known-red 名单不动 ✓。
⑤ **落账与接缝预告**：本包与 §103 的 batch7 **文件面零交集**（Rust vs JS/docs）⇒ 叠成同一候选出；
   `git am` 不过 pre-commit ⇒ fmt／clippy 已复跑 ✓。
⑥ **CI 后效（run 40730524806＝`d8e66638`）**：criteria 六片／clippy／fmt／wasm smoke／GPU 全绿 ✓，
   但 **`test (stable)` 与 `test (beta)` 双双红在 `blob_gc_budget.rs:71`** ✗——X-1 把口径
   300 KB→100 KB 后，该测试写死的「造 4 个原子越过 1 MB 预算」**不再触发闸** ⇒ 断言拿不到拒绝。
   **我侧审读漏在哪**：②里枚举旧语义断言面用的是 `grep 300 \* 1_024|BYTES_PER_ATOM`——
   这只抓「把口径数字写下来的地方」✗，抓不到「**输入靠旧口径才成立**」的断言 ✗
   （四层齐的第 2 层＝输入触发，包侧自测与本侧 grep 双双漏过 ⇒ **CI 是最后防线**的老配方 ✓）；
   而 §102③ 本机只跑了自述点名的 doc_budget／seq／ink_pixels **三个过滤器**，没跑
   `--test blob_gc_budget` ⇒ 「合并例外要 workspace 全量 test」这一步当时交了 CI 而 CI 抓到 ✓。
   **修复（`24a19b33`）**：`BYTES_PER_ATOM` 提为 `pub`（带文档），测试按
   `budget/per_atom + 1` **现算**造原子数 ⇒ 口径再校准也不二次失效（内核②写命令不写数 ✓）；
   本机双向验：100 KB 绿 ✓、**300 KB 变异下照样绿**（口径无关性 ✓ 这才是这条修法的变异证明）、
   恢复后绿；doc_budget 6/6 同跑 ✓；pre-commit 的 **workspace clippy** exit 0 ✓。
   `cargo test --workspace` 全量仍交 CI（可用 2.4 Gi < 3 Gi ⇒ 不自称本机绿 ✗）。

## §103 PWA 本地内核批次 7（restore_checkpoint 落地＋state@seq 求值窗口）合并留痕（2026-10-29）

① **基线与接缝**：**裸 diff**（`a1ff6891→dc4898c5`，两个提交都不在本仓 ✗ ⇒ 同 §99①：只能以
   我侧 apply 干净落地代）。补丁 sha256 `8b08b4e2…360253b` 留此。
   **无包内说明文件**（不像 §99 各批带重放验证 ⇒ 本轮断言与变异面**全靠我侧自建**，见 ③④ ✓）。
   `git apply --check` 干净 ✓（4 文件：双份 api-local／docs 台账＋94 行／新判据 631 行）；
   **与我侧 §99/§100/§101 刚改的热区有重叠风险**（detach 文案、list_objects 折叠）⇒ 逐 hunk 审读见 ②。
② **内容审读**（口径以服务端为权威 ✓）：
   · **`p2StateWindow`**：照 `seq.rs::state_at` 的窗口公式 `(0, 链底 base] ∪ 每个 (H.seq, n]` ✓，
     递归 declare_head（atom/checkpoint 两种 base）✓、checkpoint 锚点取 `payload.anchor_seq ?? atom.seq`
     与服务端 `checkpoint_anchor_seq` 同口径 ✓；**无跳变 ⇒ 原样返回同一数组**（下游零成本 ⇒ 不误伤
     既有判据面 ✓，这是本轮全绿的结构性原因）；WeakMap memo 按数组身份 ✓。
   · **restore_checkpoint 501 ⇒ 落地**：写 `declare_head{base:{type:"checkpoint"}, reason}` ✓
     （服务端 `write_restore_checkpoint` 同形 tools.rs:21384 ✓）；缺检查点 ⇒ **404 reference_not_found
     ＋文案逐字「检查点 X 不存在」** ✓（服务端 `log.rs:986` 同码同字 ✓）；**旧 501 的理由（本地折叠器
     从不执行跳变）被窗口层推翻** ⇒ 注释里写明推翻理由 ✓（不是悄悄删 501 ✓）。
   · **形态③补洞（本轮最大面）**：此前本地**回执说恢复了、派生视图还是旧状态**——`p2Neutralized`／
     `p2ObjectView`／`p2PresetLayers`／`p1LayerOrder`／`p2cState` 全量日志口径 ⇒ 现在统一过窗口 ✓
     （**一个事实一处口径**：窗口只在 `p2StateWindow` 定义 ✓）。
   · **参数校验补齐**：`restore_checkpoint`/`checkpoint` 的缺参／类型错／未知参数走
     `p2PresetSchemaCheck` ⇒ 与服务端 `validate_args` 逐字（判据 §F 十条钉住 ✓）。
   · **既有 501 名单同步改**：头部注释加「后续更新」行**点名哪条已落地**（不抹旧文 ⇒ 可追溯 ✓）；
     **判据按 `path === "` 机械计数的分支总数口径不变**（真实实现＋如实 501 ⇒ 总数恒等 ✓）。
③ **本机复验**：`node --check` 双份 ✓、**两份 `cmp` 逐字节一致 ✓**（铁律）；新判据
   `tool-pwa-checkpoint-restore` **358 条断言全绿 ✓**（前提：真 wasm 内核 ⇒ 本机 `web/wasm` 过期
   ⇒ 判据正确 `exit(3)` ⇒ **重建内核＋`pwa-sync-wasm` 后才真判 ✓**——exit(3) 期间没拿旧内核冒充测过）。
   **全族回归：`tool-pwa-*` 15/15 绿 ✓**（api-behaviour／detach／preset-parity／pixel-stats 全在内 ⇒
   窗口层对无跳变文档的恒等性成立 ✓）。docs 台账 94 行随包入（§六百六十二/六百六十三 两轮记录 ✓）。
④ **变异本机重放**（判据自述 ①–⑪，抽两条断言粒度 ✓）：**⑧ 窗口空操作**（`p2StateWindow` 恒返全量）
   ⇒ §C/§E3 多条红（「恢复后 checkpoint 应归零／S4 消失」被窗口擦掉所致）✓；
   恢复后 `cmp` 一致、判据重跑绿 ✓。**①②③⑤⑥⑦⑨⑩ 由判据自述＋§103③ 的 358 条覆盖陈述，
   未逐条重放**（如实记：包侧台账「六百六十三」称逐条实测过，我侧只重放了最核心的两条 ⇒
   其余若 CI 绿也只是「没红」不是「验过能红」✗）。
⑤ **落账**：`git apply` 不过 pre-commit ⇒ fmt／workspace clippy／台账守卫已复跑 ✓（§90⑤ 同规）。
   与 §102 叠为同一候选（`try-p8-costwall`：`78dc50b6`＋`0a8f386e`）push（同组不双派 ✓）。

## §104 NEW-H1 瘦身第一步（双语词表挪成外链 /i18n.js，index.html −70 KB）留痕（2026-10-29）

① **范围与动机**：05 复审 NEW-H1（连续两轮未修的确认缺陷）点名 `web/index.html` 1.04 MB 且无界膨胀；
   棘轮判据 `check-index-html-byte-budget`（#37 时先行就位）只拦增长不减肥 ⇒ 本笔做**第一步**：
   把 `viewer-app.js` 里三张双语表（`I18N_EN_TEXT`/`I18N_EN_ATTR`/`I18N_EN_ATTR_PATTERNS`，
   910 行 / 76,958 B，**边界以当场 sed 核读定**：13285–14194，运行时消费方 14195 起 ✓）挪成
   独立资产 `crates/yanshi-http/assets/i18n.js`。i18n+CSS+懒加载+minify 的**后续步**不在本笔 ✗。
② **机制与口径**（为什么运行时不变）：`<script src="/i18n.js">` 是**传统脚本、同步、先于**内联主脚本 ✓
   ⇒ 顶层 `const` 进入**共享全局词法环境** ⇒ 主脚本引用逐字不变 ✓（**不是** module、**不是** window 挂载 ✓）；
   路由 `/i18n.js` 由**同一二进制** `include_str!` 发（对齐 `/viewer-app.js` 的形态 ✓）⇒ 单二进制部署
   自包含性不变。**守卫口径随之改**：`page_is_self_contained` 从「禁一切 `<script src=`」改为
   「只许 `/i18n.js`、禁 http(s) 主机」＋新守卫
   `the_i18n_dictionary_is_an_external_asset_loaded_before_the_main_script`（表在资产／不在主脚本／
   引用先于主脚本 三面 ✓ 同一事实只写一处 ✓）。接线五处一个口径：SW SHELL、`_headers` no-cache、
   `pwa-sync-viewer` 的 stampFiles＋FILES、`tool-web-assets-parity` 清单、`ui-i18n-coverage` 读表源。
③ **本机复验**：`cargo test -p yanshi-http --lib viewer` **7/7 ✓**（含新守卫）；
   真起服务 `curl /i18n.js` ⇒ **200 且与资产 `cmp` 逐字节一致 ✓**、页面含引用 ✓；
   判据 `ui-i18n-coverage`（315＋151 条模板文案对 427＋197 词条全过 ✓）／`tool-pwa-stamp-shape` 16/16 ✓／
   `tool-web-assets-parity`（i18n.js 行 ✓）／`check-service-worker-roles` ✓ 全绿；
   `--export-viewer-html` 再生 `web/index.html`＝**968,345 B**（−69,984 ✓），棘轮 CEILING 同步收
   1,040,000 ⇒ 970,000（**瘦身必须被看见 ✓**）。**workspace 全量 test 交 CI**（可用 2.4 Gi ✗ 不自称本机绿）。
④ **变异两面（断言粒度，恢复后重跑绿＋`cmp` 干净 ✓）**：M-a 删页面 `<script src="/i18n.js">` 引用 ⇒
   新守卫与 self-contained 守卫**两条全红** ✓；M-b 把资产里 `const I18N_EN_TEXT` 改名 ⇒
   `ui-i18n-coverage` **exit(2)**「找不到两张表」✓（判据失效时报错不假绿 ✓）。
⑤ **意外收获（同一 CI run 的红归因给 §102⑥）**：派发本笔前先看账 ⇒ run 40730524806 的
   `test (stable)/(beta)` 双红是 **P8 口径校准的漏网断言**（与 i18n 无关 ✓），修复＝`24a19b33`
   （造原子数按 `BYTES_PER_ATOM` 现算；300 KB 变异下测试仍绿＝口径无关性 ✓）。
   本笔与修复叠成同一候选 push（`24a19b33`＋`93b14798`＋§48 `9e763525` 未推部分，同组不双派 ✓）；
   `git am` 不适用（我侧手写）⇒ pre-commit fmt／workspace clippy 已随两笔提交复跑 ✓。

## §105 NEW-H1 瘦身第二步（样式表挪成外链 /viewer.css，index.html −37.5 KB）留痕（2026-10-29）

① **范围与动机**：续 §104 的第一步，把 `page_template()` 里 `include_str!("../assets/viewer.css")`
   的内联 `<style>` 块（37,557 B）挪成**已存在**的同源路由 `/viewer.css`（server.rs 早已发，
   离线判据 `tool-offline-asset-coverage`／`browser-sw-cache-not-on-critical-path` 本来就按
   「可 fetch 的外壳资产」对待它 ⇒ 外链化与既有口径一致，不是新机制）。JS 三段仍内联 ✗（无模块加载器，
   加载顺序即依赖顺序）。
② **机制与口径**：与 §104② 同一条口径（同一二进制 `include_str!` 发 ⇒ 单二进制自包含不变；
   守卫只禁外部主机）。PAGE_HEAD 尾部 `<style>` 起点改 `<link rel="stylesheet" href="/viewer.css" />`，
   PAGE_TAIL_A 起点去掉 `</style>`。**旧守卫 `the_stylesheet_is_a_separate_packable_asset` 反转重写**：
   从前断「页面**含**内联 CSS 且与资产一致」，现在断「`stylesheet()` 与资产逐字节相等 ✓ 页面含 link ✓
   页面**不再有** `<style>`／`</style>` ✓」——同一事实仍只写一处（资产），页面只留引用。
   `ui-i18n-coverage` 同步删掉 parts 拼接里的 CSS 体：CSS 注释会漏进可见文本候选 ⇒ **假红**（四层齐第 2 层：
   输入不再触发断言的反面——输入多喂了不该喂的东西）。接线面零新增：SW SHELL／`_headers`／
   pwa-sync stampFiles＋FILES／parity 清单**第一步之前就已含 viewer.css** ✓（这正说明外链形态早被承认）。
③ **本机复验**：`cargo test -p yanshi-http --lib viewer` **7/7 ✓**；`--export-viewer-html` 再生
   `web/index.html`＝**930,816 B**（−37,529 ✓），棘轮 CEILING 970,000 ⇒ 931,000（对本提交落地前的
   旧 HEAD 会红＝预期，落地后同源 ✓）；判据 `ui-i18n-coverage`（315＋151 全过 ✓）／
   `tool-pwa-stamp-shape` 16/16 ✓／`tool-web-assets-parity` ✓／`check-service-worker-roles` ✓；
   真起服务 `curl /viewer.css` ⇒ **37,557 B 与资产 `cmp` 逐字节一致 ✓**、页面 `<style>` 0 次／link 1 次 ✓。
   **workspace 全量 test 交 CI**（可用 2.4 Gi ✗ 不自称本机绿）。
④ **变异两面（断言粒度，恢复后 7/7 绿＋重建二进制保新鲜 ✓）**：M-a 删 PAGE_HEAD 的 link ⇒
   `the_stylesheet_is_a_separate_packable_asset` 红（viewer.rs:1390 ✓）；M-b 往 PAGE_HEAD 塞回 `<style>` ⇒
   **同一条测试的另一面**红（viewer.rs:1394「页面不应再有内联 `<style>` 块」✓）——两条断言各红各的，
   不是同一条兜all（变异粒度=断言 ✓）。
⑤ **顺带**：`web/assets-index.json` 又带进与本笔无关的既有资产漂移 ⇒ `git checkout --` 还原，
   保持提交聚焦（与 §104 同规；H-1 去重仍在 #30 等裁定）。本笔与 #47 收账（run 40735508212）**同组排队**：
   该 run 收敛前不 push，收敛后与本账一起推（同组不双派 ✓）。

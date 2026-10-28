# 第八轮：Service Worker 的缓存记账被放在了响应路径上

基线：`bae5f7ef`（第八轮开工时 `1231047b`，中途上游前进 2 个提交：
`e544fa3a skill(vector-tracing): v5 …` 与 `bae5f7ef docs: record the vector-tracing v5 merge …`；
两者只动 `HANDOFF.md` 与 `skills/yanshi-vector-tracing/SKILL.md`，**产品代码零改动**：

```
git diff --stat 1231047b..bae5f7ef
 HANDOFF.md                            | 28 +++--
 skills/yanshi-vector-tracing/SKILL.md | 57 ++++++++--
```

⇒ 所以"修复前红 ⇒ 修复后绿"这两跑之间，产品侧的差别**只有下面这一处修复**，不是换了基线。

---

## 1. 结论（一句话）

`crates/yanshi-http/assets/service-worker.js` 的 `fetch` 处理器里，**"开缓存 / 写缓存"被放在了
"已经拿到网络响应"与"把响应交给页面"中间**。只要 Cache Storage 的 `open()` 挂起（配额满、
目录不可用——正是第七轮 ㊵ 那个现场），**导航就停在半路、页面上一句责任都没有**。
本轮把它挪出响应路径，并立了一条能把它钉住的判据（修复前红、修复后绿，见 §3）。

## 2. 怎么发现的：先证伪旧假设，再定位真的一行

第七轮我把"SW 写缓存失败会不会让导航挂住"记成**未证明的假设**，当时只看了 `cache.put`
那一句。本轮先用探针把三种失败各打一遍（sabotage 打在 SW 自己的全局上，
每次先**读回**确认打上了、每次 `finally` 还原、还原后**再测一次导航**）：

| 打的补丁 | 导航 | 判定 |
|---|---|---|
| `Cache.prototype.put` 永不 settle | **122 ms commit**，界面在 | 第七轮那版"写 tee 反压"假设 **被证伪** |
| `CacheStorage.prototype.open` 永不 settle | **21002 ms 内没有任何 commit**，`readyState=null` | **真缺陷就在这一句** |
| `open` 直接抛 `QuotaExceededError` | 147 ms commit，界面在（外层 `catch` 走 `caches.match` 兜住） | **抛错不致命、挂起才致命** |

原始输出：`probe-sw-critical2.log`（探针本体 `/tmp/yanshi-web-r8/probe-sw-critical2.mjs`）。

> 探针第一版（`probe-sw-critical.mjs`）自己的读数有错，已作废并写进它的头部注释：
> CDP 的 `Runtime.evaluate` 是**双层 result**，取成 `(r.result||{}).value` 永远得到 `undefined`；
> 且它在 `about:blank` 的后台标签上问 `getRegistration()`。这两条都修在 v2 里。
> **仪器读数错了 ⇒ 结论不能算数**，这一点在 §6 的"假红"事件里又重演了一次。

## 3. 修复

`crates/yanshi-http/assets/service-worker.js` 三处：

1. **主分支（网络优先 / 导航与静态资产）**：`fetch` 到手后**立刻 `return response`**；
   写入改成 `event.waitUntil(swTeeIntoCache(key, response.clone()))`。
   - `clone()` 在关键路径上（同步、不碰磁盘）；`open`/`put` 全在响应之后。
   - 用 `waitUntil` 而不是旧的"不 await 的发射后不管"：旧写法在 SW 被回收时会**静默丢写**。
   - 存键规则不变（导航按 `/`，其余按原请求）。
2. **blob 那一支（cache-first）**：读本来就在回答之前，挪不走 ⇒ 改成**有界等待**
   （`CACHE_READ_DEADLINE_MS = 1500`）：期限内没答案就放弃缓存、直接走网络；
   写入同样挪到响应之后。挂起的后果从"每张 `<img src=/api/blob/…>` 永远空白"
   变成"最多多等 1.5 s，仍能出图"。
3. **写失败现场**：`self.__swWriteFailures` ＋ `console.warn`（只在真失败时出声；
   与 install 期的 `__swPrecacheFailures` 分开，避免污染"预缓存失败 = 0"那类断言）。

## 4. 判据：`scripts/browser-sw-cache-not-on-critical-path.mjs`

只问产品行为，不问"我有没有写新 API"。断言：

| # | 断言 |
|---|---|
| ⓪ | 对照导航拿到的是**真外壳**（有 `#effectsList`、有 title） |
| ① | `open` 挂起时导航仍然 commit |
| ② | 挂起时 commit 的是真外壳（不是错误页／空白页） |
| ③ | `open` 挂起时一次 blob 请求仍在有界时间内返回 |
| ④ | `put` 挂起时导航照样 commit |
| ⑤ | 不 sabotage 时，用过的资产**真的进了缓存**（防止修法变成"干脆不缓存"） |
| ⑥ | 没有静默的写失败（`__swWriteFailures` 为空） |

**反假绿**（本判据最容易骗自己的地方）：sabotage 只活在**那个 worker 实例**里；
worker 若中途被回收，`open` 就悄悄变回原生，导航当然 commit ⇒ 那是仪器坏了不是产品好。
所以**打完读回一次、测完再读回一次**，任何一次不是被换的实现就退 3（无法判断）；
还原后再测一次导航，不 commit 同样退 3。

### 红 → 绿（同一份判据）

| 跑 | 构建 | 结果 |
|---|---|---|
| 修复前 `judge-sw-red-before-fix.log` | `1231047b` 的 SW | **① ② ③ 红**，④⑤⑥ 绿，**exit 1**；`open` 挂起时 12005 ms 内无 commit |
| 修复后 `judge-sw-green-after-fix.log` | `bae5f7ef` ＋ 本修复 | **全绿，exit 0**；`open` 挂起时导航 **118 ms** commit；blob **1513 ms** 返回（＝ 1.5 s 有界等待 ＋ 网络） |

⇒ 同一判据、同一环境：**修复前红 3 条、修复后全绿**；且 ④⑤⑥ 在两跑里都绿
（说明这条判据不是"一律红"）。

### 真实截图（`shots/`）

- `sw-fixed-normal.png`：正常载入，界面完整。
- `sw-fixed-under-open-hang.png`：**把 `open` 换成永不 settle 之后**（读回确认生效）再开新标签 ⇒
  界面**与正常时一样完整**。这正是修复的可视证据：修复前同一条件下标签停在半路（空白）。

两张都逐像素看过：画布、右侧 Layers / Actions / Adjustments、底部状态栏都在。

## 5. 回归（本轮实际跑过的）

| 判据 | 结果 |
|---|---|
| `check-service-worker-roles.mjs` | ✓（先假红，见 §6-d；补齐介质产物后转绿） |
| `tool-web-assets-parity.mjs` | ✓ 五个可同步文件逐字节一致（SW 23685 B） |
| `tool-pwa-assets.mjs` | ✓（补齐介质后） |
| `tool-pwa-stamp-shape.mjs` | ✓ 16/16，三处产物同戳 `yanshi-shell-8130423177401` |
| `browser-offline-shell.mjs` | ✓ 断网后页面仍能打开并渲染（`render=client` ＋ 静态 PWA） |
| `browser-offline-reload.mjs` | ✗ —— **已在 `criteria-known-red.txt:140` 登记**（"在线那一笔没渲染出来"），与本改动无关 |
| `browser-pwa-local.mjs` | 所有断言 ✓，随后被它自己的 90 s 守卫判"无法判断"退 3（见 §6-e） |

## 6. 仪器踩的坑（都写下来，免得下一轮再踩）

**a. 第一次"修复后仍红"是假红：SW 脚本缓存把旧 worker 钉住了。**
重建并重启服务端后，判据依旧 ①②③ 红。诊断办法是在 SW 全局里问代码标记：

```
typeof swTeeIntoCache   ⇒ undefined      ← 跑的是旧脚本
CACHE                   ⇒ yanshi-shell-1231047b-dirty-83c306e1   ← 旧构建的戳
```

而 `curl /service-worker.js` 拿到的是新脚本（新戳 `bae5f7ef-…`）。清注册、删缓存、
`ServiceWorker.stopAllWorkers/unregister` 都不行；最后把 profile 的
`Default/Service Worker` 目录整体挪开再重启浏览器才刷新（备份在
`/tmp/cdp-9490-sw-backup-112254`）。之后读回变成
`function | yanshi-shell-bae5f7ef-dirty-578f69e3` ⇒ 判据转绿。
**教训：判据红的时候先问"我测到的是哪一版代码"，别先怀疑修复。**

**b. CDP 的 `Runtime.evaluate` 是双层 result。** 取错层 ⇒ 永远 `undefined` ⇒
"sabotage 没生效"和"产品没修好"会混在一起（§2 的探针 v1 就是这么被自己骗的）。

**c. `render=client` 不能省。** 静态 PWA 那族判据要在 URL 上加它（`run-criteria.sh:378` 与 `:401` 两处都加）；
漏掉时页面走服务端渲染、内核不加载，离线判据会报"断网后本地内核没有就绪"——
那是**我传参不全**，不是产品红。

**d. 介质 wasm 缺失会让 `pwa-sync-viewer.mjs` 中途退出 ⇒ 打戳那一步不执行。**
表现是 `check-service-worker-roles` 说"静态托管那份还有占位符"、
`tool-pwa-assets` 说"缺少介质插件产物"。本轮把 6 个介质插件在本地编出来
（`cargo build --release --target wasm32-unknown-unknown -j 2 -p yanshi-medium-{oil,watercolor,marker,pencil,pixel,example}`
＋ `node scripts/mediums-sync.mjs`）之后，同步 exit 0、打戳执行、两条判据转绿。
⇒ 第七轮文档里"本机没有介质 wasm ⇒ 无法证明"的限制，本轮**在本地解除了**
（`assets/mediums/*.wasm` 是构建产物，`.gitignore` 里，不进补丁）。

**e. `browser-pwa-local.mjs` 的 90 s 守卫在本机偏紧。** 它先把所有断言打完并打印通过，
随后守卫到点退 3。这是"无法判断"（不是产品红），但会让 CI 分片看起来红。
**未改**，登记在此。

## 7. 复验命令

```bash
# 服务端（已含修复）
cargo build --release -p yanshi-http -j 2
./target/release/yanshi-serve --bind 127.0.0.1:15900 --root /tmp/yanshi-root &
# 新判据（需要 CDP 浏览器）
CDP_PORT=9490 node scripts/browser-sw-cache-not-on-critical-path.mjs \
  "http://127.0.0.1:15900/?doc=default" "http://127.0.0.1:15900" t 9490
# 静态一侧
node scripts/pwa-sync-viewer.mjs && node scripts/check-service-worker-roles.mjs
node scripts/tool-web-assets-parity.mjs && node scripts/tool-pwa-stamp-shape.mjs
```

## 8. 补丁面

- 改：`crates/yanshi-http/assets/service-worker.js`
- 新增：`scripts/browser-sw-cache-not-on-critical-path.mjs`
- `web/service-worker.js` 与 `web/index.html` 是**同步产物**（`web/service-worker.js` 在 `.gitignore` 里），
  不进补丁；`node scripts/pwa-sync-viewer.mjs` 会重新生成。

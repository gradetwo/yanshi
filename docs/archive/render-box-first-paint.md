# 第九轮：勾选框的初值比页面晚到约 150 ms —— 以及本地专用部署里被整只抹掉的勾选框

基线：`bae5f7ef`（与第八轮同一基线；本轮产品侧只改
`crates/yanshi-http/src/viewer.rs` 与 `crates/yanshi-http/assets/viewer-app.js`，
红跑与绿跑之间没有别的产品改动）。

> 本文里的毫秒数是**一次性实测快照**；跨轮次的权威数字只写在 `docs/performance-notes.md`。

---

## 1. 结论（一句话）

「用服务端渲染」那个勾选框的初值，**原来要等启动尾部的一串网络请求跑完才被写上**
（`setupFileMenu()` 在 `setupAssetPanels()` → `setupObjectPanel()` → `warmKernel()` 之后）；
于是**默认（服务端）模式下首帧画的是空框**，而旁边的字写着"默认开启" —— 屏幕自相矛盾约
**150 ms**（实测 52 ms 首绘、84 ms DCL 时还是空框、228 ms 才勾上；慢网下窗口更长）。
本轮把初值放进**模板里紧挨着勾选框的一段内联脚本**，解析到那一行时就定下；
并立了一条六档判据把它钉住（修复前 6 条红、修复后全绿，见 §4）。

修这条时用**真 PWA 截图**复核，又抓出同族的第二个缺陷：本地专用部署
（`__pwaLocalOnly`）里，说明文字用 `label.textContent = …` 追加 ⇒ **把 `<input>` 节点一起抹掉**，
界面上只剩一句话、勾选框消失。修法是把说明写进模板里一个隐藏的 `<span>`，运行时只负责显示。

## 2. 怎么发现的：先把"什么时候写初值"量出来

探头（`probe-render-box.mjs`，原始读数 `probe-box-fresh-default.json`）在文档启动时
用 `Page.addScriptToEvaluateOnNewDocument` 装一个记录器，逐帧读 `#useServerRender`：

| 时刻 | `readyState` | `checked` | 说明 |
|---|---|---|---|
| 52 ms（first-paint / FCP） | loading | —（框已渲染） | **首帧就是空框** |
| 84 ms（DOMContentLoaded） | interactive | `false` | 主脚本已解析，但 `setupFileMenu` 还没跑到 |
| 123 ms（tick 0） | complete | `false` | 启动尾部的网络还在飞 |
| 228 ms（tick 1） | complete | `true` | 初值这时才写上 |

红跑（`judge-box-red-before-fix.log`）更直接：缺省 / 存量 `"1"` / `?render=server`
三档在 DCL 与 DCL 后第一帧都是 `false`，翻面记录是
`{"from":false,"to":true,"t":102,"why":"raf3"}`。

方向也要说清：`?render=client` 本来就从 HTML 解析起就是空框（模板没有 `checked` 属性），
所以它不是"说谎"，只是"默认开"的那几档在说谎。**修法不能靠给模板加一个写死的
`checked` 属性** —— 那会把客户端模式的首帧也说成服务端，变成反向说谎。

## 3. 修复

### 3.1 模板里定初值（`viewer.rs`）

在 `<label>` 之后、`<button id="zoomFit">` 之前放一段**内联脚本**，判定顺序与
`viewer-app.js` 的 `serverRenderPreferred()` **逐档一致**：

| 顺序 | 条件 | 初值 |
|---|---|---|
| 1 | `window.__pwaLocalOnly === true`（静态部署注入的哨兵） | `false` |
| 2 | `?render=client` | `false` |
| 3 | `?render=server` | `true` |
| 4 | `localStorage["yanshi.serverRender"]`：缺省 | `true` |
| 5 | 同上：`"1"` / 其他 | `true` / `false` |
| — | 任何一步抛错（URL / 存储不可用） | 按缺省 `true` |

`localKernelUnusable`（本会话内核折了 ⇒ 热回退服务端）**故意不在这段里**：
它只可能在会话中途变真，模板解析时无从得知；它仍由 `setupFileMenu()` 里那次
"二次对账"（`useServerRenderBox.checked = serverRenderPreferred()`）负责。
判据的 ③ 就是盯这次对账**不许把首帧说过的真话翻掉**（六档 `flips` 均为空）。

### 3.2 本地专用部署的说明不再抹掉节点（`viewer-app.js`）

`label.textContent = “…”` 会把 label 里原有的 `<input>` 当子节点一起替换掉。
现在模板里带一个 `<span id="serverRenderLocalOnlyHint" hidden>（本部署无服务端，不可用）</span>`，
运行时只把它 `hidden = false`；`<input>` 原样保留（`disabled = true`、`checked = false` 不变）。
中英文各一条词条（`ui-i18n-coverage` 静态可查、`browser-i18n` 真浏览器可验）：

- 模板文本：`（本部署无服务端，不可用）` ⇒ `" (no server in this deployment; unavailable)"`
  （前导空格是必要的：前一句以 `)` 收尾，否则两个右括号会粘在一起）；
- label 的 `title=` 仍是"本部署没有服务端 ⇒ 只能用本地内核；此开关不可用"（不进可见文本）。

## 4. 判据：`scripts/browser-render-box-truth.mjs`

只问页面行为。六档、每档一个独立文档启动，用 `Page.addScriptToEvaluateOnNewDocument`
在**文档开始前**种好存储 / 哨兵，记录器逐帧读勾选框：

| 档 | 场景 | 期望初值 |
|---|---|---|
| 1 | 缺省（无参数、无偏好） | `true` |
| 2 | 存量偏好 `"1"` | `true` |
| 3 | `?render=server` 压过存量 `"0"` | `true` |
| 4 | `?render=client` 压过存量 `"1"` | `false` |
| 5 | 存量偏好 `"0"` | `false` |
| 6 | 本地专用 `__pwaLocalOnly`（存量 `"1"`） | `false`，且**启动后节点仍在、禁用、说明可见** |

断言：⓪ 前提（记录器在、DCL 时勾选框存在且可见、`setupFileMenu` 真的跑到）；
① DCL 时 `checked` 等于期望；② DCL 后第一帧仍等于期望；③ 启动后终态
（本地专用那档额外要求节点仍在 / `disabled` / 说明可见）；④ 说明文字可见；
⑤ 每档零 JS 异常（本地专用模拟那档只记不判 —— 服务端页面上它不是真 PWA）。
收尾读回并还原 `localStorage["yanshi.serverRender"]`（同源共享，不还原会污染下一条判据）。

### 红 → 绿（同一份判据）

| 跑 | 构建 | 结果 |
|---|---|---|
| 修复前 | 第九轮开工时的二进制 | **6 红**（①②在档 1/2/3 全红）→ `judge-box-red-before-fix.log` |
| 修复 #1 后 | 重建、重启 15900 | 5 绿 → `judge-box-green-after-fix.log` |
| 修复 #2 前（判据加了档 6） | 同一构建 | **2 红**（③ 节点消失、④ 无说明）→ `judge-box-red-localonly.log` |
| 修复 #2 后 | 重建、重启 15900 | **全绿，六档零翻面** → `judge-box-green-six.log` |

绿跑读数：六档 DCL 时勾选框已经是期望值（DCL 50–64 ms），终态与期望一致，
`wasmState` 与 `stats` 也分得清两条路（服务端档 `wasm:false / serverCalls:14`，
客户端档 `wasm:true / serverCalls:0`），收尾还原偏好、页面零 JS 异常。

## 5. 真截图（4× CPU 降速）

`shot-render-box.mjs` 在 4× CPU 降速下抓"启动还没完成"的那一帧（`fileMenuReady=false`）：

- `box-default-early.png`：t≈398 ms、`fileMenuReady=false`，**框已勾上** —— 首帧不再等启动尾部；
- `box-client-early.png`：t≈423 ms、`fileMenuReady=false`，**框是空的** —— 客户端模式没有被
  写死的 `checked` 属性反着说谎；
- `box-default-final.png` / `box-client-final.png`：两种模式跑完后的整页。

PWA（静态 `http.server` 起 `web/`，`shot-pwa-box.mjs` / `shot-pwa-en.mjs`）：

- `pwa-local-box.png`：勾选框**还在**（灰色禁用），label 是
  「用服务端渲染（默认开启；取消勾选 ⇒ 改用内核离线渲染）（本部署无服务端，不可用）」；
- `pwa-local-box-en.png`：英文模式下两句都翻译到位（"…instead) (no server in this deployment; unavailable)"）；
- `pwa-local-final.png`：整页。

DCL 读数：`{checked:false, disabled:true, label: 上述整句, fileMenuReady:true}`；
英文模式：`hintText=" (no server in this deployment; unavailable)"`、`hintVisible:true`。

## 6. 回归（修复后重跑）

| 判据 | 结果 |
|---|---|
| `cargo test --release -p yanshi-http viewer:: -j 2` | 5 passed / 0 failed（内联脚本没有破坏"资产分块内联、不得 `<script src=`"那组测试） |
| `scripts/ui-i18n-coverage.mjs` | 文本 315 条 / 词条 427；属性 151 / 197；全有英文 |
| `scripts/check-viewer-unique-ids.mjs` | 359 个 id 全唯一；脚本引用的 303 个 id 全在 |
| `scripts/browser-i18n.mjs`（真浏览器） | 英文整页 distinct 中文 = 0；26 条 `data-i18n=off` 数据串豁免 |
| `scripts/tool-web-assets-parity.mjs` | 源头 vs web 逐字节一致（仅缓存戳不同） |
| `scripts/tool-pwa-assets.mjs` | wasm/胶水已入预缓存；manifest / 骨架壳 / 本地接管齐 |
| `scripts/tool-pwa-stamp-shape.mjs` | 16 通过 / 0 失败（戳仍是 13 位纯数字） |
| `node scripts/pwa-sync-viewer.mjs` | 6 个文件 + 静态页 → `web/`，戳 `4473831787443` |

## 7. 边界与遗留

- **两处判定会漂移**：模板的 5 档与 `serverRenderPreferred()` 的 5 档必须同改。
  注释里写明了"改一档必须两处同改"，判据逐档同时盯首帧与终态 —— 只改一处会当场红。
- **首帧之后**仍可能合法地翻面：内核装载失败时 `localKernelUnusableNow()` 会把框勾回服务端
  （这是第六轮的修复，不能去掉）。判据因此只要求"首帧不说谎 + 启动后的对账不翻",
  不禁止会话中途因真实失败而回退。
- `web/index.html`、`web/assets-index.json` 是 `pwa-sync-viewer` 的产物；打包前已还原到
  HEAD（生成物留在 `/tmp/yanshi-web-r9/generated-mirror/`），补丁里不带它们。

# 第八轮（续）：历史卡与撤销/重做只念半个失败

基线：`bae5f7ef`（与第八轮 SW 那篇同一基线；本轮产品侧只改
`crates/yanshi-http/assets/viewer-app.js` 三处 ⇒ "红→绿"两跑之间没有别的产品改动）。

---

## 1. 结论（一句话）

历史卡（`#history`）在 `get_log` 失败时写的是 `读取失败：` + `error_code || "unknown"`，
撤销/重做失败写的是 `撤销失败：` + `error_code || "unknown"` —— **同一份回执在效果卡上
已经会说「哪条路（服务端/本地内核）+ 哪个码」，这三格却还在念裸码或 `unknown`**。
本轮让它们走同一对帮手 `refusalVenue()` / `failureCode()`，并立了一条判据把它们钉住。

## 2. 缺陷形状（修复前实测）

判据跑在**未修的构建**上（日志 `judge-history-red-before-fix.log`），6 条红：

```
✗ ① 历史卡                                  ["读取失败：intercepted_read_failure"]
✗ ② 裸码那一族                              ["读取失败：intercepted_read_failure"]
✗ ③ 撤销失败把回执里的码丢了                ["撤销失败：unknown"]
✗ ③ 全日志仍有「撤销失败：unknown」         ["撤销失败：unknown 错误 intercepted_undo_failure错误 …"]
✗ ⑤ 重做失败把回执里的码丢了                ["重做失败：unknown"]
✗ ⑦ 英文整句无 venue                        ["Read failed: intercepted_read_failure"]
```

第 ③ 条的原始日志本身就是证据：**同一行里**页面通用失败分支写着
`错误 intercepted_undo_failure`（`callTool` 会读 `error` ✓），紧接着卡上那句却是
`撤销失败：unknown` ⇒ **码已经到手，是那一格自己没读**。这不是"服务端没说"，
是"界面把答案丢了"。

三处代码：

| 位置（`viewer-app.js`，修复后行号） | 修复前 | 修复后 |
|---|---|---|
| `:5045` 历史卡（原 `:5041`） | `"读取失败：" + (value.error_code \|\| "unknown")` | `"读取失败（" + refusalVenue() + "）：" + failureCode(value)` |
| `:4456` 撤销 | `((value && value.error_code) \|\| "unknown")` | `failureCode(value)` |
| `:4482` 重做 | 同上 | `failureCode(value)` |

- `refusalVenue()`（`:4711`）读的是 `serverRenderPreferred()` ⇒ 说的是**这一笔实际走过的那条路**，
  不是写死"服务端"（第七轮效果卡 ⑩b 的同一条口径 ✓）。
- `failureCode()`（`:4719`）两个格子都读：服务端 `error_code` ✓ 本地那一臂 `error` ✓。
- **不新增翻译**：历史卡那句与效果卡逐字同形，`i18nText` 的 `m40`
  （`/^读取失败（(服务端|本地内核)）：(\S+)$/`，`:14086`；英文映射 `:14225`）原样命中。
  `m19`（裸句 `读取失败：<code>`）**保留不动**——它是一条纯翻译词条，不在本次缺陷面上。

## 3. 判据：`scripts/browser-history-failure-venue.mjs`

**注入的是真失败回执**，不是文案抽查：

- 在**任何页面脚本之前**包一层 `window.fetch`（`Page.addScriptToEvaluateOnNewDocument`，
  与 `browser-layer-lock` 同一条手法 —— 第 6 轮实测 `Fetch.enable` 在本环境一条都拦不到）；
- 只拦三个工具：`get_log`／`undo_last`／`redo_last`，其余原样转发；
- 撤销/重做回执故意把码放在 **`error`**（本地那一臂的格子名）⇒ 只读 `error_code` 的写法当场红；
- 桩的命中次数当**前提**：一次没接到 ⇒ `exit(3)`（判据自身失效，不冒充产品绿）；
- 跨重载的失败开关记在 `localStorage` ⇒ 英文那一臂重新导航后仍失败，恢复那一臂改成 `"0"` 即通过。

钉住的八件事：⓪ 前提（桩命中、历史卡在「历史」tab 里**可见**、撤销按钮**可用**）；
① 卡上逐字带 venue + 具体码；② 不带 `unknown`、不是旧裸码句式；
③ 撤销失败念出 `error` 里的码；④ **同一颗按钮不注入时真的能撤**（页面说「已撤销…」且判据自己问
`get_undo_status` 数字 1→0 —— 证明 ③ 测的是真路径）；⑤ 重做同理；
⑥ 撤掉注入 ⇒「重新读取」后列表真有行、失败那句退场；⑦ 英文整句逐字 + 零汉字；⑧ 全程 0 条 JS 异常。

夹具自建文档（一层 + 一块填充 ⇒ 服务端报可撤销 1 笔），不碰别的文档。

## 4. 红 → 绿

| 断言 | 修复前 | 修复后 | 原始读数 |
|---|---|---|---|
| ① 历史卡整句 | **红** | 绿 | `读取失败（服务端）：intercepted_read_failure` |
| ② 无 `unknown`/旧句式 | **红** | 绿 | 同上 |
| ③ 撤销失败的码 | **红**（`unknown`） | 绿 | `撤销失败：intercepted_undo_failure` |
| ④ 真撤销 | 绿 | 绿 | 页面「已撤销…」；服务端 remaining 1→0 |
| ⑤ 重做失败的码 | **红**（`unknown`） | 绿 | `重做失败：intercepted_redo_failure` |
| ⑥ 恢复 | 绿 | 绿 | 列表 4 行、失败句退场 |
| ⑦ 英文整句 | **红** | 绿 | `Read failed (the server): intercepted_read_failure` |
| ⑧ JS 异常 | 绿 | 绿 | 0 条 |

日志：`judge-history-red-before-fix.log`（6 红）／`judge-history-green-after-fix.log`（全绿，exit 0）。

## 5. 截图（真实界面，均已逐张看过）

`/tmp/yanshi-web-r8/shots/`（截图脚本 `/tmp/yanshi-web-r8/shot-history.mjs`，与判据同一注入）：

- `history-failure-zh.png`（110,156 B）：整页 1400×900，历史 tab 打开，卡上
  **读取失败（服务端）：intercepted_read_failure**，下方日志仍是 `错误 intercepted_read_failure`；
  画布正常（900×600 文档里那块蓝方块在），底栏「可撤销 1 笔」。
- `history-failure-zh-card.png`（54,596 B）：卡片 2× 放大，那句话可读。
- `history-failure-en-card.png`（56,504 B）：英文模式 **Read failed (the server): intercepted_read_failure**，
  整卡零汉字。
- `history-recovered-en-card.png`（77,508 B）：恢复后三行原子（`draw_shape`／`create_layer`／
  `create_document`），失败句已退场。

## 6. 回归

| 判据 | 结果 |
|---|---|
| `browser-history-failure-venue`（新） | ✓ exit 0 |
| `browser-i18n`（render=client，CI 形状） | ✓ exit 0（切英后整页 0 条中文） |
| `browser-effect-card-truth` | ✓ exit 0（⑩c/⑩d 同族两格照旧绿） |

静态 PWA 镜像已重生成（`node scripts/pwa-sync-viewer.mjs` ⇒ exit 0，三处 shell 戳一致
`yanshi-shell-8130423177401`），`web/viewer-app.js` 里两处卡句 + 一处撤销句都是新写法。

## 7. 仪器的坑（写下来省下一轮）

1. **撤销按钮必须是"真的可用"**：夹具落一笔（`fill_region`）⇒ 服务端 `get_undo_status` 报 1 笔；
   拿不到就 `exit(3)`。不这样，测的是一颗禁用按钮的点击（什么也不发生）。
2. **`#history` 默认不在屏幕上**：卡片在「历史」tab 里，默认显示的是「绘制」tab ⇒
   不点 tab 就读 `offsetParent` 会得到"不可见"。判据先点 `button[data-tab="history"]` 再断言可见。
3. **失败开关必须跨重载**：英文那一臂是新一轮导航；开关只放 `window` 上会"恢复成功"⇒ 英文那格变假绿。
   放 `localStorage` 才两臂都失败。
4. **恢复那一臂要真撤掉注入**：不撤就只测了"失败说得好不好"，没测"失败会不会退场"。
5. **`#log` 不在 i18n 范围**（诊断区）⇒ 撤销/重做那两句用中文断言即可，不必再给英文映射。

## 8. 复现

```bash
cargo build --release -p yanshi-http -j 2
./target/release/yanshi-serve --bind 127.0.0.1:15900 --root /tmp/yanshi-root &
CDP_PORT=9490 node scripts/browser-history-failure-venue.mjs "http://127.0.0.1:15900/" 9490
```

## 9. 补丁面

- `crates/yanshi-http/assets/viewer-app.js`：三处（历史卡一句、撤销一句、重做一句）。
- `scripts/browser-history-failure-venue.mjs`：新判据（`run-criteria.sh` 自动按 `scripts/browser-*.mjs` 枚举）。
- `docs/history-undo-failure-venue.md`：本文。

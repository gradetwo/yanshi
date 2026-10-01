//! 最小 Web 查看器（设计文档 6.2 / 7 章 / 10 章 / 12.8）。
//!
//! 单页 HTML + 内联 JS，无任何前端依赖：
//!
//! - 通过 `POST /api/documents` 打开/新建文档，把 capability token 放进 URL；
//! - 通过 WebSocket 订阅：控制流（全部原子元数据）实时进入日志面板，
//!   数据流（tile/缩略图）按视口驱动重渲染；
//! - 画笔/矩形/椭圆直接调用工具层，服务端返回 10.1 响应与预览地址；
//! - 撤销/重做走 `revert` / `reapply`，时间旅行走 `revert_to`。
//!
//! 该页面是 Phase 1 的「最小 Web 查看器」交付物；完整编辑器属后续阶段。

/// 查看器页面（HTML + CSS + JS）。
pub const PAGE: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>偃师 Yanshi 查看器</title>
<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
<link rel="icon" type="image/png" sizes="32x32" href="/favicon.png" />
<link rel="apple-touch-icon" href="/brand/png/favicon-180.png" />
<style>
  /* **配色令牌集中在这里** ✓ —— 之前只有一个 `--line` ✓，其余颜色散落硬编码 ✗
     （`#171a1f`、`#1b1f26`、`#222`… ✓），改主题要满文件找 ✓。
     现在收敛成一套语义令牌 ✓：底色 / 面板 / 面板浮层 / 分隔线 / 正文 / 次要文字 / 强调色 ✓。
     对比度按 WCAG 正文标准取 ≥4.5:1 ✓（检查脚本会**实测**这一条 ✓，见 browser-ui-check ✓）。 */
  :root {
    color-scheme: dark;
    --bg: #0f1115;          /* 页面底色 */
    --surface: #171a1f;     /* 顶栏 / 工具条 / 状态栏 */
    --surface-2: #1b1f26;   /* 卡片 / 弹出面板 */
    --line: #2b313b;        /* 分隔线 */
    --text: #e7eaef;        /* 正文（对 --bg 约 14.6:1 ✓） */
    --muted: #a3adbb;       /* 次要文字（对 --bg 约 7.8:1 ✓） */
    --accent: #3d6bb3;      /* 强调 / 选中 / 焦点环 */
    --accent-soft: #2b4a7d; /* 选中态的柔化底色 */
  }
  * { box-sizing: border-box; }
  body { margin: 0; background: var(--bg); color: var(--text);
         font: 13px/1.5 system-ui, "Noto Sans CJK SC", sans-serif; }
  /* 键盘可达性 ✓：所有可聚焦控件都有**可见焦点环** ✓（纯键盘用户与快捷键提示配套 ✓）。 */
  :focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; border-radius: 4px; }
  /* 可点区域下限 ✓：界面控件不低于 26px 高 ✓（密集的深色界面里最影响手感 ✓）。
     **收窄到具体控件** ✓ —— 第一版写成裸 `button, select, input` ✗，实测两个介质用例的画布
     同时变空 ✗（可见它影响了画布/舞台的重绘路径 ✓）；现在只作用于界面控件 ✓，
     与检查里量高的那组选择器完全一致 ✓。 */
  header button, #tools button, aside button, .options select, .options input,
  #quickPanel button, .statusbar button { min-height: 26px; }
  /* 滚动条跟随主题 ✓（否则深色界面里出现一条亮色滚动条，很扎眼 ✓）。 */
  * { scrollbar-color: var(--line) transparent; scrollbar-width: thin; }
  ::-webkit-scrollbar { width: 10px; height: 10px; }
  ::-webkit-scrollbar-thumb { background: var(--line); border-radius: 5px; }
  ::-webkit-scrollbar-track { background: transparent; }
  header { display: flex; gap: 8px; align-items: center; padding: 8px 12px; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
  header h1 { font-size: 15px; margin: 0 12px 0 0; display: flex; align-items: center; gap: 6px; }
  .brand-mark { width: 22px; height: 22px; border-radius: 5px; }
  /* justify-items: start 让舞台收缩到 canvas 自身尺寸：否则栅格会把 .stage 拉到整列宽，
     右侧露出一块灰色死区，点击落在 .stage 上而不是 canvas 上（用户报告的「右边一块没法用」）。 */
  /* 第一列用 minmax(0,1fr)：`1fr` 的最小尺寸是 auto ⇒ 左列被内容撑开后整个页面横向溢出，
     画布被挤到屏幕外（用户截图里的「排版都出去了」）。
     `body { overflow-x: hidden }` 只是掩盖症状，真正要允许列收缩。 */
  main { display: grid; grid-template-columns: 56px minmax(0, 1fr) 320px; gap: 12px; padding: 12px; align-items: start; max-width: 100vw;
          /* 兜底：实测 main 自身的 scrollWidth 会达到 2007（其子元素的 rect 都在 1265 内，
             溢出源未定位到具体节点），于是整个页面可横向滚动、画布被推出屏幕。
             所有可见元素都在边界内，因此 clip 不影响显示，只阻止页面被撑宽。 */
          overflow-x: clip; }
  /* 只让舞台按内容收缩（否则右侧留出灰色死区、点击落在 stage 上）；右侧面板保持 320px 列宽，
     不能一起收缩，否则工具按钮会溢出窗口。 */
  /* **舞台铺满可用区、画布在其中居中** ✓ —— 用户反馈："画布固定在左上角很难受，尤其缩放时" ✓。
     此前的 `justify-self: start` 是为了消除"右侧灰色死区" ✗（点击落在 stage 上而不是画布上 ✓）；
     但代价是画布贴左上角 ✓。现在的做法两头兼顾 ✓：
       * 舞台铺满 ✓（视觉上画布周围就是**工作区** ✓，与成熟绘画软件一致 ✓）；
       * 画布**居中** ✓（缩放时视觉重心稳定 ✓）；
       * 画布外的区域**不响应绘制** ✓ —— 事件监听挂在 `#board` 上 ✓，
         所以点在空白区根本不会进入绘制分支 ✓（检查里有专门的断言 ✓）。 */
  .stage { justify-self: stretch; display: flex; align-items: center; justify-content: center;
           min-height: 240px; background: var(--surface); }
  .stage { position: relative; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; background: #f5f5f5; }
  /* 单一几何：内容画布 #board 决定尺寸（文档分辨率位图 + 固有宽高比）；
     #overlay 只画拖动中的笔迹预览，位置与尺寸由 JS 同步为 board 的显示矩形。
     两层分离的原因：此前预览与内容共用一个画布，重绘预览时会把内容一起清空，
     提交后画布变空白（刷新才恢复）。 */
  #board { display: block; width: auto; height: auto; max-width: 100%; max-height: calc(100vh - 96px); touch-action: none; cursor: crosshair; background: #fff; image-rendering: pixelated; }
  #overlay { position: absolute; left: 0; top: 0; pointer-events: none; image-rendering: pixelated; }
  /* minmax(0,1fr)：否则网格列按 max-content 撑开，卡片里的按钮行会溢出到视口外
     （实测 29 个按钮里 14 个跑到屏幕外，"导出/＋图层"因此看起来不存在）。 */
  .options { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; padding: 8px 12px;
             background: var(--surface); border-bottom: 1px solid var(--line); }
  .options .tool-name { font-weight: 600; min-width: 4em; }
  .options label { display: flex; gap: 4px; align-items: center; min-width: 0; }
  .options input[type="range"] { width: 120px; }
  /* 工具条：可纵向滚动 ✓ —— 截图里"填充图层"曾被窗口底部截断 ✗（窄条 + 20 个工具必然超出）。 */
  /* **左列工具两列排布** ✓（用户要求 ✓）：工具多了之后单列会把 rail 拉得很长，
     两列更接近常见图像软件的工具栏 ✓；用栅格而不是 flex-wrap ✓ —— 栅格保证**列对齐** ✓，
     换行时不会出现"某一行只有一个按钮、宽度还不同"的参差 ✓。 */
  #tools { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 4px; padding: 6px;
           min-width: 0; background: var(--surface); border-right: 1px solid var(--line);
           align-content: start; overflow-y: auto; max-height: calc(100vh - 150px); }
  #tools button { padding: 7px 0; width: 100%; min-width: 0; display: flex; justify-content: center;
                  align-items: center; }
  /* **面板开关** ✓（用户要求：左右都要能隐藏，并能进全屏画布 ✓）。
     用 `body` 上的三个类表达状态 ✓（`hide-rail` / `hide-dockers` / `zen` ✓）——
     三列的栅格由这四个选择器穷尽覆盖 ✓（都不隐藏 / 只藏左 / 只藏右 / 都藏 ✓），
     比在 JS 里拼 grid-template-columns 更可控 ✓（也不会与样式表两处打架 ✓）。 */
  body { --rail-w: 108px; }
  body.hide-rail main { grid-template-columns: minmax(0, 1fr) 320px; }
  body.hide-dockers main { grid-template-columns: var(--rail-w) minmax(0, 1fr); }
  body.hide-rail.hide-dockers main { grid-template-columns: minmax(0, 1fr); }
  body.hide-rail #tools, body.hide-dockers aside { display: none; }
  /* **全屏画布模式** ✓：藏掉头部与选项条 ✓，画布占满窗口 ✓（不留内边距 ✓）。 */
  body.zen header, body.zen .options { display: none; }
  body.zen main { grid-template-columns: minmax(0, 1fr); padding: 0; gap: 0; }
  body.zen #tools, body.zen aside { display: none; }
  body.zen .stage { border: 0; border-radius: 0; }
  /* 全屏模式下的**退出把手** ✓ —— 没有它就只能靠快捷键 ✓，
     而"进了全屏不知道怎么出来"是最典型的抱怨 ✓。默认隐藏 ✓，只在 zen 下出现 ✓。 */
  #zenExit { position: fixed; top: 10px; right: 10px; z-index: 40; display: none;
             background: rgba(20, 20, 22, .62); color: #fff; border: 1px solid rgba(255, 255, 255, .28);
             border-radius: 999px; padding: 6px 12px; font-size: 12px; cursor: pointer;
             backdrop-filter: blur(4px); }
  body.zen #zenExit { display: inline-flex; align-items: center; gap: 6px; }
  /* **图层面板** ✓：顶部条 + 行列表 ✓。行里三个可点区域（眼睛 / 锁 / 名字 ✓），
     名字区最大以便点选 ✓；按钮用最小尺寸以免抢走注意力 ✓。 */
  .layers { display: flex; flex-direction: column; gap: 4px; margin-top: 6px; }
  .layers-head { display: flex; align-items: center; justify-content: space-between; font-size: 12px;
                 opacity: .85; }
  .layers-actions { display: inline-flex; gap: 2px; }
  .layers-actions button { padding: 1px 6px; font-size: 12px; line-height: 1.4; }
  .layer-list { display: flex; flex-direction: column; gap: 2px; max-height: 220px; overflow-y: auto;
                border: 1px solid var(--line); border-radius: 6px; padding: 3px; background: var(--bg, #fff); }
  .layer-row { display: flex; align-items: center; gap: 4px; padding: 3px 4px; border-radius: 4px;
               font-size: 12px; cursor: pointer; }
  .layer-row:hover { background: rgba(128, 128, 128, .12); }
  .layer-row.selected { background: var(--accent, #2b6cb0); color: #fff; }
  .layer-row .layer-name { flex: 1 1 auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .layer-row .layer-flag { padding: 0 4px; font-size: 12px; line-height: 1.5; background: transparent;
                           border: 1px solid transparent; border-radius: 3px; cursor: pointer; }
  .layer-row .layer-flag:hover { border-color: currentColor; }
  .layer-row .layer-flag.off { opacity: .35; }
  .panel-toggles { display: inline-flex; gap: 4px; margin-left: auto; }
  .panel-toggles button { padding: 4px 8px; font-size: 12px; }
  .panel-toggles button[aria-pressed="true"] { background: var(--accent, #2b6cb0); color: #fff;
                                               border-color: transparent; }
  #tools svg { width: 22px; height: 22px; fill: none; stroke: currentColor; stroke-width: 1.6;
               stroke-linecap: round; stroke-linejoin: round; }
  #tools button[aria-pressed="true"] { background: var(--accent-soft); border-color: var(--accent); }
  .statusbar { display: flex; gap: 16px; align-items: center; padding: 6px 12px; font-size: 12px;
               background: var(--surface); border-top: 1px solid var(--line); }
  .statusbar .spacer { flex: 1 1 auto; }
  /* 光标处快捷面板 ✓（`position: fixed` ✓ ⇒ 坐标即光标位置 ✓，不受画布滚动影响 ✓）。 */
  #quickPanel { position: fixed; z-index: 40; min-width: 196px; max-width: 260px; padding: 8px 10px;
                background: var(--surface-2); border: 1px solid var(--line); border-radius: 8px;
                box-shadow: 0 8px 24px rgba(0, 0, 0, .45); font-size: 12px; }
  #quickPanel[hidden] { display: none; }
  #quickPanel .qp-title { display: flex; justify-content: space-between; align-items: baseline;
                          margin-bottom: 6px; opacity: .9; }
  #quickPanel .qp-hint { opacity: .55; font-size: 11px; }
  #quickPanel .qp-section { display: flex; flex-wrap: wrap; gap: 4px; margin: 4px 0; }
  #quickPanel .qp-section:empty { display: none; }
  #quickPanel button { min-width: 30px; padding: 4px 7px; font-size: 11px; }
  #quickPanel button[aria-pressed="true"] { background: var(--accent-soft); border-color: var(--accent); }
  #quickPanel .qp-swatch { width: 22px; height: 22px; min-width: 0; padding: 0; border-radius: 4px;
                           border: 1px solid var(--line); }
  #quickPanel .qp-swatch[aria-pressed="true"] { outline: 2px solid #6ea8fe; outline-offset: 1px; }
  #quickPanel .qp-actions { border-top: 1px solid var(--line); padding-top: 6px; }
  /* 可折叠 Dockers ✓（借鉴成熟绘画软件的面板折叠 ✓）：点标题折叠/展开 ✓，状态持久化 ✓。 */
  aside .card > h2 { cursor: pointer; user-select: none; display: flex; align-items: center; gap: 6px; }
  aside .card > h2::before { content: "▾"; font-size: 10px; opacity: .7; transition: transform .1s; }
  aside .card.collapsed > h2::before { transform: rotate(-90deg); }
  aside .card.collapsed > *:not(h2) { display: none !important; }
  aside .card.collapsed { padding-bottom: 8px; }
  aside { display: grid; gap: 12px; grid-template-columns: minmax(0, 1fr); min-width: 0; }
  aside .card { min-width: 0; }
  /* 面板内的可伸缩元素：下拉的选项名可能很长（图层 id）。光限制 select 不够 ——
     包裹它的 <label> 的 min-content 仍然等于最长选项 ⇒ label 会撑破 320px 列。
     因此 label 必须是可收缩的 flex 容器（本轮实测：超宽元素就是 label[1270..1675] ✓）。 */
  aside label { display: flex; align-items: center; gap: 4px; min-width: 0; max-width: 100%; }
  aside select, aside input, aside button { max-width: 100%; min-width: 0; }
  aside label select, aside label input { flex: 1 1 auto; min-width: 0; }
  #history .row { min-width: 0; }
  /* 原生 <dialog> 在 top layer，宽度按内容撑开 ⇒ 会撑大 documentElement.scrollWidth
     （实测：body/main/aside 都在视口内，scrollWidth 却多出 742px ✗）。必须显式限宽。 */
  dialog { max-width: min(760px, 92vw); border: 1px solid var(--line); border-radius: 8px; }
  dialog #docList { max-width: 100%; }
  #history .kind, #history .actor { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  #log { overflow-x: hidden; }
  #history { max-height: 220px; overflow: auto; font-family: ui-monospace, monospace; font-size: 11px; }
  #history .row { display: flex; gap: 6px; align-items: center; padding: 1px 0; }
  #history .row button { padding: 0 5px; font-size: 11px; }
  #history .seq { opacity: .6; min-width: 34px; }
  #history .kind { min-width: 86px; }
  #history .actor { opacity: .75; }
  .card { border: 1px solid var(--line); border-radius: 6px; padding: 8px 10px; }
  .card h2 { font-size: 12px; margin: 0 0 6px; text-transform: uppercase; letter-spacing: .06em; opacity: .7; }
  button { font: inherit; padding: 4px 9px; border-radius: 5px; border: 1px solid var(--line); background: transparent; cursor: pointer; }
  button[aria-pressed="true"] { background: #4a7dff22; border-color: #4a7dff; }
  input, select { font: inherit; padding: 3px 6px; border-radius: 5px; border: 1px solid var(--line); background: transparent; }
  #thumb { display: block; border: 1px solid var(--line); border-radius: 5px; background: #fff; width: 128px; height: 128px; object-fit: contain; }
  #log { max-height: 240px; overflow: auto; font-family: ui-monospace, monospace; font-size: 11px; }
  #log div { white-space: nowrap; }
  .status { display: flex; gap: 10px; flex-wrap: wrap; font-family: ui-monospace, monospace; font-size: 11px; opacity: .85; }
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: #c33; vertical-align: middle; }
  .dot.on { background: #2a2; }
</style>
</head>
<body>
<header>
  <h1><img class="brand-mark" src="/brand/svg/icon-light.svg" alt="" />偃师 Yanshi</h1>
  <span id="identity"></span>
  <button id="newDoc">新建</button>
  <button id="openDoc">打开…</button>
  <span class="status">
    <span><span class="dot" id="conn"></span> <span id="connText">未连接</span></span>
    <span>head <b id="head">0</b></span>
    <span>rendered <b id="rendered">0</b></span>
    <span>dirty <b id="dirty">0</b></span>
  </span>
  <!-- **面板开关** ✓（用户要求：左侧工具栏与右侧各窗口都要能隐藏 ✓，并能进全屏画布 ✓）。
       三个开关都用 `aria-pressed` 表达状态 ✓ ⇒ 屏幕阅读器与检查脚本都能读到"现在是开还是关" ✓，
       不必去猜 CSS 类 ✓。 -->
  <span class="panel-toggles">
    <button id="toggleRail" type="button" aria-pressed="false" title="隐藏 / 显示左侧工具栏（快捷键 [ ）">◧ 工具栏</button>
    <button id="toggleDockers" type="button" aria-pressed="false" title="隐藏 / 显示右侧面板（快捷键 ] ）">◨ 面板</button>
    <button id="toggleZen" type="button" aria-pressed="false" title="全屏画布（快捷键 Tab，Esc 退出）">⛶ 全屏</button>
  </span>
</header>
<button id="zenExit" type="button" title="退出全屏画布（Esc）">⛶ 退出全屏（Esc）</button>
<dialog id="newDialog">
  <h2 style="margin-top:0">新建文档</h2>
  <p style="opacity:.75;font-size:12px;margin:4px 0">
    文档以 id 作为名字（也是主键）。换个名字即可并存多份作品；重名会提示。
  </p>
  <label style="display:flex;gap:8px;align-items:center">
    名称
    <input id="newName" type="text" placeholder="例如 我的第一幅画" style="flex:1 1 auto" />
  </label>
  <div id="newHint" style="font-size:12px;opacity:.75;min-height:16px;margin:6px 0"></div>
  <div style="display:flex;gap:8px;justify-content:flex-end">
    <button id="newCancel">取消</button>
    <button id="newCreate">创建</button>
  </div>
</dialog>
<dialog id="openDialog">
  <h2 style="margin-top:0">打开文档</h2>
  <!-- 示例作品 ✓：每种介质/功能一份 ✓（画它们的过程本身就是验收 ✓，见 docs/samples.md ✓）。
       入口放在文档列表**之前** ✓ —— 这样"打开示例看看"是第一步 ✓，而不是在一堆自己的文档里翻 ✗。 -->
  <h3 style="margin:12px 0 4px">示例作品</h3>
  <p style="opacity:.75;font-size:12px;margin:0 0 6px">
    用不同介质画出来的样例，可以直接打开查看、继续画或拿来练手。
  </p>
  <div id="sampleList" style="display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:8px"></div>
  <h3 style="margin:16px 0 4px">我的文档</h3>
  <div id="docList" style="display:grid;grid-template-columns:repeat(auto-fill,minmax(120px,1fr));gap:8px;max-height:50vh;overflow:auto"></div>
  <hr />
  <h2>导入本地图片</h2>
  <p style="opacity:.75;font-size:12px;margin:4px 0">
    支持浏览器能解码的任何格式（PNG/JPEG/WebP）。图片在新图层上按原始像素导入。
  </p>
  <input id="importFile" type="file" accept="image/*" />
  <hr />
  <h2>另存为副本</h2>
  <p style="opacity:.75;font-size:12px;margin:4px 0">
    以**新 id**保存一份完整副本（原文档保留，可逆）。文档以 id 为主键，因此这里填的是新文档的 id。
  </p>
  <div style="display:flex;gap:8px;align-items:center">
    <input id="copyName" type="text" placeholder="新文档 id" style="flex:1 1 auto" />
    <button id="copyDoc">另存为…</button>
  </div>
  <div style="display:flex;gap:8px;justify-content:flex-end;margin-top:12px">
    <button id="openClose">关闭</button>
  </div>
</dialog>
  <div class="options" id="options">
    <span class="tool-name" id="toolName">画笔</span>
  <label>粗细 <input id="size" type="range" min="1" max="64" value="6" /></label>
  <label>颜色 <input id="color" type="color" value="#c81e3c" /></label>
  <!-- "强度"这个名字在**插件介质**下是**误导**的 ✗：该值直接喂给插件的 `wetness` ✓，
       于是"越强"= **越湿** = **越淡** ✓（子 agent 实测：85 → alpha 0.238 ✓、55 → 0.482 ✓）。
       这里让标签**随介质改名** ✓（插件介质 ⇒ "湿度" ✓；内置笔刷 ⇒ "强度" ✓），
       这是最小且诚实的修法 ✓（改语义会牵动插件 ABI ✓，改名不动任何渲染 ✓）。 -->
  <label><span id="strengthLabel">强度</span> <input id="strength" type="range" min="1" max="100" value="40" /></label>
  <label>羽化 <input id="feather" type="number" min="0" max="256" value="8" style="width:64px" /></label>
  <label>字号 <input id="textSize" type="number" min="7" max="128" value="21" style="width:64px" /></label>
    <label>工作区 <select id="workspace" title="布局预设（绘画 / 修图 / 校对）">
      <option value="paint">绘画</option>
      <option value="retouch">修图</option>
      <option value="review">校对</option>
    </select></label>
    <label>介质 <select id="medium">
      <option value="example">示范点（v1）</option>
      <option value="oil">油画（v2）</option>
      <option value="watercolor">水彩（v2）</option>
      <option value="marker">马克笔（v2）</option>
      <option value="pencil">铅笔（v2）</option>
      <option value="pixel">像素（v2）</option>
    </select></label>
  </div>

<main>
  <nav id="tools" aria-label="工具"><!-- 由 TOOL_DEFS 在加载时填充 ✓ --></nav>
  <div class="stage">
    <canvas id="board"></canvas>
    <canvas id="overlay"></canvas>
    <!-- 光标处快捷面板 ✓（借鉴 Krita 的 Pop-up Palette ✓）：介质 / 颜色 / 笔尖 + 我们的快捷动作 ✓。
         内容由脚本按同一份定义生成 ✓（与工具条、工作区一致：一份定义、多个入口 ✓）。 -->
    <div id="quickPanel" hidden>
      <div class="qp-title">快捷面板 <span class="qp-hint">Esc 关闭</span></div>
      <div class="qp-section" id="qpMediums"></div>
      <div class="qp-section" id="qpColors"></div>
      <div class="qp-section" id="qpSizes"></div>
      <div class="qp-section qp-actions">
        <button id="qpUndo" type="button">撤销</button>
        <button id="qpRedo" type="button">重做</button>
        <button id="qpClearSelection" type="button">清除选区</button>
        <button id="qpExport" type="button">导出 PNG</button>
      </div>
    </div>
  </div>
  <aside>
    <div class="card">
      <h2>操作</h2>
      <div class="toolbar">
  <!-- 初始即禁用 ✓：子 agent 报"没有撤销栈时按钮仍可点" ✗（点了只会打印一句提示 ✓，
       看起来像坏了 ✓）。真实状态由 `updateUndoStatus()` 同步 ✓。 -->
  <button data-tool="undo" disabled>撤销</button>
  <button data-tool="redo" disabled>重做</button>
  <button data-tool="refresh">刷新</button>
  <button data-tool="check">一致性自检</button>
  <button id="addLayer">＋ 图层</button>
  <button id="exportPng">导出 PNG</button>
  <button id="zoomFit">适配</button>
  <button id="zoomActual">1:1</button>
      </div>
        <!-- **图层面板** ✓（用户点名：图层列表、上下移动、锁定、显示/隐藏、复制 等常见功能 ✓）。
             结构上刻意让 `#layer` 这个 `<select>` **继续存在但隐藏** ✓ ——
             它是 `state.layerId` 的既有真相来源 ✓，全查看器还有很多地方在读它 ✓
             ⇒ 面板只是它的**可视化** ✓，两边永远同步 ✓（两处各自维护选择 = 一定会漂移 ✗）。 -->
        <div class="layers">
          <div class="layers-head">
            <span>图层</span>
            <span class="layers-actions">
              <button id="layerAdd" type="button" title="新建图层">＋</button>
              <button id="layerDuplicate" type="button" title="复制图层（含对象）">⧉</button>
              <button id="layerDelete" type="button" title="删除图层（可撤销）">🗑</button>
              <button id="layerUp" type="button" title="上移一层">↑</button>
              <button id="layerDown" type="button" title="下移一层">↓</button>
            </span>
          </div>
          <div id="layerList" class="layer-list" role="listbox" aria-label="图层列表"></div>
          <label style="display:none">图层 <select id="layer"></select></label>
        </div>
    </div>
    <div class="card">
      <h2>WASM 计算内核</h2>
      <div class="status">
        <span>本地乐观渲染 <b id="wasmState">检测中…</b></span>
        <span>首笔 <b id="firstStroke">—</b></span>
        <span>首帧 <b id="firstPaint">—</b></span>
        <span>内核预热 <b id="kernelWarm">—</b></span>
        <span>bit-exact <b id="bitExact">—</b></span>
      </div>
    </div>
    <div class="card">
      <h2>调整 / 滤镜</h2>
      <div style="display:flex; gap:6px; margin-bottom:6px; flex-wrap:wrap">
        <select id="effectKind">
          <option value="adjustment">调整</option>
          <option value="filter">滤镜</option>
        </select>
        <select id="effectName"></select>
        <button id="effectApply">应用</button>
      </div>
      <div style="display:flex; gap:6px; align-items:center; margin-bottom:6px">
        <input id="effectParams" value="{}" style="flex:1; font-family:ui-monospace,monospace" />
      </div>
      <div id="effectsList" style="font-family:ui-monospace,monospace;font-size:11px;max-height:120px;overflow:auto"></div>
    </div>
    <div class="card">
      <h2>历史（原子日志）</h2>
      <div style="display:flex; gap:6px; margin-bottom:6px; flex-wrap:wrap">
        <select id="historyKind"><option value="">全部类型</option></select>
        <select id="historyActor"><option value="">全部操作者</option></select>
        <button id="historyReload">重新载入</button>
      </div>
      <div id="history"></div>
    </div>
    <div class="card">
      <h2>缩略图</h2>
      <img id="thumb" alt="缩略图" />
    </div>
    <div class="card">
      <h2>原子日志（控制流）</h2>
      <div id="log"></div>
    </div>
    <div class="card">
      <h2>最近一次响应</h2>
      <div id="last" style="font-family:ui-monospace,monospace;font-size:11px"></div>
    </div>
    <div class="card">
      <h2>反馈</h2>
      <div class="status" style="flex-direction:column; align-items:flex-start; gap:6px">
        <span>问题反馈、协作沟通、缺陷上报：</span>
        <a id="contact" href="mailto:yanshi@wangda.today?subject=%5BYanshi%5D%20"
           style="color:#3f7fd4; font-family:ui-monospace,monospace; font-size:12px">yanshi@wangda.today</a>
        <span style="opacity:.7">安全漏洞请勿开公开 issue，直接发邮件。</span>
      </div>
    </div>
  </aside>
</main>
<footer class="statusbar">
    <span>缩放 <b id="zoom">100%</b></span>
    <span id="undoDepth">撤销 0 / 重做 0</span>
  <span id="selectionHint">无选区</span>
  <span class="spacer"></span>
  <span>画布 <b id="canvasSize">—</b></span>
</footer>
<script>
// 服务端会把 yanshi://blob/<hash> 改写成 /api/blob/<hash>?doc=..&token=..
// 这里保留一个显式助手，便于直接用 CAS 哈希取回 PNG。
const blobUrl = (hash) => api("/api/blob/" + hash);
// 供 CDP / 自动化验收读取的统计（Phase 2 出口条件：bit-exact 与首笔 < 16ms）。
// **`needsServerPixels` 必须在任何启动路径之前声明** ✓ —— 查看器启动时会在
// `loadKernel()` 里调用 `detectHeavyContent()` ✓；若声明太靠后（或漏写 ✗），
// 启动读到它就会抛错 ⇒ **整段脚本中断** ✓，现象是"工具条还在、但后续一切都没接线" ✓
//（本会话真实踩到：我上一版只写了注释、**忘了写声明** ✗，而探针把任何异常都标成 TDZ ✗，误导了排查 ✓）。
// 含义 ✓：只在**打开含重内容的文档**或**刚发生 heavy 原子**时为真 ✓ ——
// 此时服务端像素才是权威（内核表示不了 heavy 内容 ✓）；用户一开始画就清掉 ✓（乐观笔迹归内核 ✓）。
let needsServerPixels = false;

window.yanshiStats = {
  wasm: false, kernelHead: 0, serverHead: 0,
  // 当前打开的文档与令牌 ✓ —— 自动化验收需要知道"查看器此刻在编辑哪一个文档" ✓
  //（本会话就栽过：检查脚本凭早先的 doc/token 去查对象 ✓，而页面早已切到另一个文档 ✗）。
  docId: null, token: null,
  firstStrokeMs: null, firstPaintMs: null, kernelWarmMs: null,
  lastApplyMs: null, lastRenderMs: null, lastPutMs: null, lastArea: 0, applies: 0,
  bitExact: null, resyncs: 0,
};

const params = new URLSearchParams(location.search);
window.yanshiStats.docId = params.get("doc");
window.yanshiStats.token = params.get("token");
// `?debug=1` 时暴露工具调用入口：自动化验收需要**读服务端的真实响应**（例如 list_objects 的 bbox），
// 而不是靠画布像素反推。此前我在检查脚本里写了一个并不存在的 `window.yanshiCallTool` ✗，
// 于是整段用例静默返回空对象 ✓ —— 现在把它真正接上。
const DEBUG = params.get("debug") === "1";
const state = {
  docId: params.get("doc") || "default",
  token: params.get("token") || "",
  tool: "brush",
  layerId: null,
  // 撤销/重做双栈：存的是**原始原子 id**。
  // 语义（fold.rs：`revert(revert(x)) ≡ reapply(x)`）⇒ 撤销 = revert(原始)，
  // 重做 = reapply(原始)；因此重做栈里必须放原始 id，而不是 revert 原子自身的 id。
  // 撤销栈条目：{kind:"atom", id} → `revert(id)`；{kind:"head", id} → `revert_to(id)`。
  // 后者用于「回到此处」：撤销一次跳转 = 再跳回跳转前的那个原子
  //（`revert(declare_head)` 恢复不了，实测撤销后画面不变）。
  undoStack: [],
  redoStack: [],
  socket: null,
  docSize: { w: 1024, h: 1024 },
  viewport: { x: 0, y: 0, w: 1024, h: 1024 },
  // 显示缩放（1 = 整幅适配容器）。视口是**文档坐标**子矩形，内核按 1:1 渲染它，
  // CSS 把它放大到容器尺寸 —— 与设计的 viewport/tile 数据流一致（6.6/6.7）。
  zoom: 1,
  // 仿制图章 / 修复画笔的源点（文档坐标）：Alt+点击设置（与常见图像编辑器一致）。
  sourcePoint: null,
  // 移动工具选中的对象（含 bbox，用于命中测试与显示选中框）。
  selectedObject: null,
  // 当前选区（用于"清除选区"与覆盖层显示）。
  selectionId: null,
  selectionShape: null,
  dragging: null,
  points: [],
  wasm: null,
  kernel: null,
  localSeq: 0,
  pending: null,
};

const $ = (id) => document.getElementById(id);
const api = (path) => path + (path.includes("?") ? "&" : "?") + "doc=" + state.docId + "&token=" + state.token;
const board = $("board");
const overlay = $("overlay");
const ctx = board.getContext("2d");
const octx = overlay.getContext("2d");
// 服务端渲染结果用离屏图像承载，**画进内容画布**（不再用覆盖 <img>，避免出现
// 「看到的像素来自被拉伸的 img、点击落在下面的 canvas」这种几何不一致）。
const preview = new Image();

/// 内容层尺寸变化时同步覆盖层的显示矩形（画布按 CSS 缩放，覆盖层必须精确对齐它）。
function syncOverlayGeometry() {
  const stage = board.parentElement;
  const rect = board.getBoundingClientRect();
  const stageRect = stage.getBoundingClientRect();
  overlay.style.left = Math.round(rect.left - stageRect.left) + "px";
  overlay.style.top = Math.round(rect.top - stageRect.top) + "px";
  overlay.style.width = Math.round(rect.width) + "px";
  overlay.style.height = Math.round(rect.height) + "px";
}

/// 设定**视口**尺寸（内容画布与覆盖层同尺寸、同坐标系），并清空两层。
///
/// 画布内部分辨率 = 视口的文档像素数（内核 1:1 渲染），CSS 显示尺寸由 `applyDisplaySize` 决定：
/// 缩放后画布像素变少、显示尺寸不变，于是看得更细（`image-rendering: pixelated` 保持清晰）。
function sizeBoards(width, height) {
  board.width = width;
  board.height = height;
  overlay.width = width;
  overlay.height = height;
  // 用**文档背景色**铺底而不是留透明：切换文档/等待内核期间画布不会出现透明空洞
  // （此前表现为「操作后画布空白」，且在冷启动的临时实例上间歇复现）。
  const background = state.backgroundCss || "#ffffff";
  ctx.fillStyle = background;
  ctx.fillRect(0, 0, width, height);
  octx.clearRect(0, 0, width, height);
  state.viewport.w = width;
  state.viewport.h = height;
  applyDisplaySize();
  syncOverlayGeometry();
}

/// 画布/覆盖层的 CSS 尺寸 = 视口 × 显示缩放（上限为可用区域，避免溢出）。
function applyDisplaySize() {
  const available = availableArea();
  const scale = state.displayScale || 1;
  const width = Math.max(32, Math.min(Math.round(state.viewport.w * scale), available.w));
  const height = Math.max(32, Math.min(Math.round(state.viewport.h * scale), available.h));
  board.style.width = width + "px";
  board.style.height = height + "px";
  overlay.style.width = width + "px";
  overlay.style.height = height + "px";
}

/// 舞台可用区域（主栅格第一列减去右侧面板、间隙与内边距）。
function availableArea() {
  const main = document.querySelector("main");
  const aside = document.querySelector("aside");
  const styles = main ? getComputedStyle(main) : null;
  const gap = styles ? parseFloat(styles.columnGap || "12") : 12;
  const padding = styles ? parseFloat(styles.paddingLeft || "12") * 2 : 24;
  const asideWidth = aside ? aside.getBoundingClientRect().width : 320;
  const width = (main ? main.clientWidth : window.innerWidth) - asideWidth - gap - padding;
  const height = Math.max(240, window.innerHeight - 96);
  return { w: Math.max(160, Math.floor(width)), h: Math.floor(height) };
}

/// 按当前缩放与文档尺寸重新计算视口（以 `center` 为中心，缺省用视口中心）。
function clampViewport(center) {
  const { w: docW, h: docH } = state.docSize;
  const available = availableArea();
  // 显示缩放：整幅适配容器的比例 × 用户缩放。
  const fit = Math.min(available.w / docW, available.h / docH);
  state.displayScale = fit * state.zoom;
  const viewW = Math.min(docW, Math.max(32, Math.floor(available.w / state.displayScale)));
  const viewH = Math.min(docH, Math.max(32, Math.floor(available.h / state.displayScale)));
  const focus = center || {
    x: state.viewport.x + state.viewport.w / 2,
    y: state.viewport.y + state.viewport.h / 2,
  };
  const x = Math.max(0, Math.min(docW - viewW, Math.round(focus.x - viewW / 2)));
  const y = Math.max(0, Math.min(docH - viewH, Math.round(focus.y - viewH / 2)));
  state.viewport = { x, y, w: viewW, h: viewH };
}

/// 把某个**文档点**钉在指定的**画布像素位置**上 ✓（缩放锚点用 ✓）。
///
/// 为什么不能只用 `clampViewport(focus)` ✗：它把 `focus` 放到**视口中心** ✓，
/// 而从光标缩放要求"**光标下的内容不动**" ✓ —— 两者只有在光标恰好位于视口中心时才一致 ✗。
/// 实测差异 45.9px ✓（检查里量到的漂移 ✓）。这里先按常规算出缩放与视口尺寸 ✓，
/// 再把视口平移，使该文档点落在指定画布像素处 ✓。
function clampViewportAt(docPoint, pixel) {
  clampViewport(docPoint);
  if (!pixel) return;
  const { w: docW, h: docH } = state.docSize;
  const maxX = Math.max(0, docW - state.viewport.w);
  const maxY = Math.max(0, docH - state.viewport.h);
  // **单位换算** ✓：`pixel` 是**客户端像素** ✓，而视口是**文档像素** ✗ ——
  // 第一版忘了除以 `displayScale` ✓，于是锚点漂移 43px ✗（正是这个量级 ✓）。
  // **直接按锚点解方程** ✓，而不是"先居中再平移" ✗ ——
  // 后者要经过两次取整与两次夹取 ✓，实测漂移 21.75→27.56px ✗（越修越偏 ✓，说明推理链太长 ✓）。
  //
  // 目标只有一个 ✓：**文档点 `docPoint` 恰好落在客户端像素 `pixel` 处** ✓，即
  //   docPoint = viewport + pixel × perPixel  ⇒  viewport = docPoint − pixel × perPixel ✓
  // 其中 `perPixel` 是**实测**比例（`viewport.w / rect.width` ✓）—— 不能用 `displayScale` ✗，
  // 因为画布的 CSS 尺寸会被取整并受可用区上限约束 ✓（实测 4.402 vs 4.147 ✓）。
  // **先刷新画布显示尺寸，再读矩形** ✓ —— `clampViewport` 只算视口与 `displayScale` ✓，
  // 真正改画布 CSS 尺寸的是 `applyDisplaySize()` ✓（它由 `sizeBoards` 在 `renderViewport` 里调 ✓，
  // 也就是**在我读矩形之后** ✗）⇒ 直接读会拿到旧尺寸 ✓，比例随之算错 ✓（残余 26px ✓ 正是这里 ✓）。
  applyDisplaySize();
  const rect = board.getBoundingClientRect();
  const perPixelX = state.viewport.w / Math.max(1, rect.width);
  const perPixelY = state.viewport.h / Math.max(1, rect.height);
  state.viewport.x = Math.max(0, Math.min(maxX, Math.round(docPoint.x - pixel.x * perPixelX)));
  state.viewport.y = Math.max(0, Math.min(maxY, Math.round(docPoint.y - pixel.y * perPixelY)));
}

/// 文档坐标 → 画布坐标。
function toCanvas(point) {
  return { x: point.x - state.viewport.x, y: point.y - state.viewport.y };
}

/// 重新渲染当前视口（缩放/平移后调用）。
function renderViewport() {
  if (!kernelReady()) {
    // **没有内核 ⇒ 画布只能来自服务端** ✓ —— 这里原来**直接 return** ✗
    // ⇒ 每一次落笔之后画布都不更新 ✓（用户实测：普通笔刷"画了却看不见" ✓，
    //  而笔画**其实已经提交**了 ✓ —— 历史里有 ✓、服务端渲染也有 ✓，只是画布不动 ✗）。
    // 介质笔之所以看起来正常 ✓，是因为介质那条路会**自己**触发服务端补画 ✓
    //（提交后立刻按 region 补一次 ✓）⇒ 于是"介质能画、普通笔不能" ✓ 这个奇怪现象由此而来 ✓。
    needsServerPixels = true;
    queueServerBlit();
    return;
  }
  const zoomLabel = $("zoom");
  if (zoomLabel) zoomLabel.textContent = Math.round((state.displayScale || 1) * 100) + "%";
  const { x, y, w, h } = state.viewport;
  sizeBoards(w, h);
  state.kernel.set_viewport(x, y, w, h);
  drawKernelRegion(x, y, w, h);
  subscribeViewport();
  redraw();
}

/// 文档背景（原子里的 `{r,g,b,a}`）转 CSS 颜色；缺省白色。
function backgroundToCss(background) {
  if (!background || typeof background !== "object") return "#ffffff";
  const channel = (value) => Math.max(0, Math.min(255, Math.round(Number(value) || 0)));
  return "rgb(" + channel(background.r) + "," + channel(background.g) + "," + channel(background.b) + ")";
}

function log(line, cls) {
  const el = document.createElement("div");
  el.textContent = line;
  if (cls) el.style.color = cls;
  $("log").prepend(el);
  while ($("log").childElementCount > 200) $("log").lastChild.remove();
}

function setStatus(patch) {
  if (patch.head !== undefined) $("head").textContent = patch.head;
  if (patch.rendered !== undefined) $("rendered").textContent = patch.rendered;
  if (patch.dirty !== undefined) $("dirty").textContent = patch.dirty;
}

async function callTool(name, args, options = {}) {
  const response = await fetch(api("/api/tools/" + name), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  });
  const value = await response.json();
  $("last").textContent = JSON.stringify(value).slice(0, 600);
  if (value.ok) {
    if (value.head !== undefined) setStatus({ head: value.head, dirty: (value.dirty_tiles || 0) });
    // 入栈与"是否刷新"无关：`revert` / `reapply` 自身不入栈（它们由撤销/重做逻辑显式管理栈）。
    const trackable = name !== "revert" && name !== "reapply" ? value.atom_id : null;
    afterMutation(trackable, {
      skipRefresh: options.refresh === false,
      // **把服务端给的脏区带下去** ✓ —— 设计的两层渲染就是按脏区推进的 ✓，
      // 而此前这里把它丢掉了 ✗ ⇒ 无内核的机器每笔都要整视口补画 ✓。
      dirtyBox: value.dirty_bbox || null,
    });
  } else {
    log("错误 " + value.error_code + "：" + ((value.context && value.context.detail) || ""), "#c33");
  }
  return value;
}

let thumbTimer = null;
let thumbInFlight = false;
// 去抖 + 合并：WS 缩略图事件可能密集到达，逐个 fetch 会把连接打满（曾出现成片 Failed to fetch）。
function scheduleThumbRefresh() {
  if (thumbTimer) return;
  thumbTimer = setTimeout(() => {
    thumbTimer = null;
    refreshThumb();
  }, 400);
}

async function refreshThumb() {
  if (thumbInFlight) return;
  thumbInFlight = true;
  try {
  const value = await fetch(api("/api/tools/get_document"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: "{}",
  }).then((r) => r.json());
  if (value.thumb_url) $("thumb").src = value.thumb_url + "&t=" + Date.now();
  if (value.head_seq !== undefined) setStatus({ head: value.head_seq, rendered: value.rendered_seq });
  if (value.width && value.height) state.docSize = { w: value.width, h: value.height };
  if (value.background) state.backgroundCss = backgroundToCss(value.background);
  } finally { thumbInFlight = false; }
}

// —— WASM 计算内核（本地乐观渲染，13.3） ——

const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
function ulid() {
  let time = Date.now();
  let out = "";
  for (let index = 9; index >= 0; index--) {
    out = CROCKFORD[time % 32] + out;
    time = Math.floor(time / 32);
  }
  for (let index = 0; index < 16; index++) out += CROCKFORD[Math.floor(Math.random() * 32)];
  return out;
}

async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function kernelReady() {
  return !!(state.kernel && state.wasm);
}

function setWasmState(text, color) {
  const element = $("wasmState");
  element.textContent = text;
  element.style.color = color || "";
}

// 页面依赖的内核方法清单。**必须与 crates/yanshi-wasm/src/lib.rs 的导出逐一对应**：
// 曾经查看器调用了一个从未实现的 `render_region_direct_rgba`，浏览器抛
// "not a function" 被事件处理器吞掉，表现为「拖动无反馈、操作后画布空白」。
// 这里在启动时显式校验：版本不匹配时给出**可操作**的提示，而不是静默失效。
const REQUIRED_KERNEL_METHODS = [
  "render_region_rgba",
  "render_region_direct_rgba",
  "apply_atom_json",
  "extend_preview_stroke",
  "commit_preview",
  "set_viewport",
  "head_seq",
];

function verifyKernelSurface(kernel) {
  const missing = REQUIRED_KERNEL_METHODS.filter((name) => typeof kernel[name] !== "function");
  if (missing.length === 0) return true;
  const message = "WASM 内核与页面版本不一致，缺少方法：" + missing.join(", ") +
    "。请强制刷新（Ctrl+Shift+R / Cmd+Shift+R）。";
  log(message, "#c33");
  setWasmState("版本不匹配", "#c33");
  setStatus({ kernelError: message });
  window.yanshiStats.kernelSurfaceError = message;
  return false;
}

async function initWasm() {
  try {
    const health = await (await fetch("/health")).json();
    if (!health.wasm) throw new Error("服务端未启用（--no-wasm 或产物缺失）");
    // 动态 import：不需要打包器，直接吃 wasm-bindgen --target web 的输出。
    const module = await import("/wasm/yanshi_wasm.js");
    await module.default();
    state.wasm = module;
    window.yanshiStats.wasm = true;
    setWasmState("已加载", "#2a2");
    log("WASM 计算内核已加载：" + module.WasmKernel.name);
  } catch (error) {
    setWasmState("不可用", "#c33");
    log("WASM 内核不可用，退化为服务端渲染：" + error.message, "#c33");
    // **没有内核 ⇒ 画布的唯一来源就是服务端** ✓ —— 这一条是用户实测逼出来的 ✓：
    // 此前只有 `drawKernelRegion()` 会触发服务端补画 ✓，而它**只在有内核时才被调用** ✗
    // ⇒ 没装 wasm-bindgen 的机器（`make run` 的常见情形 ✓）打开任何**含 heavy 内容**的文档
    // （示例的画都是 `import_image` ✓）**一律空白** ✗ —— 用户看到的正是这个 ✓。
    // 现在：一旦内核不可用 ✓ ⇒ 永久标记 `needsServerPixels` ✓（缩放、重绘、脏区都跟着补画 ✓）。
    needsServerPixels = true;
    queueServerBlit();
  }
}

/// **打开文档时判断"内核表示得了吗"** ✓ —— 四位子 agent 独立复现的**阻断性 bug** ✓：
/// 文档里只要有 heavy 内容（介质笔画 = `import_image`/`raster_patch` ✓、液化 ✓），
/// 客户端内核折叠它只会得到 **空白补丁** ✗ ⇒ **首屏画布全白** ✗，
/// 而服务端渲染、缩略图、导出**全都正确** ✓（"重新打开示例是白板" ✓，违反设计 14.5 ✓）。
async function detectHeavyContent() {
  try {
    const listed = await callTool("list_objects", {}, { refresh: false });
    const objects = (listed && listed.objects) || [];
    // **让"介质"选择器反映文档** ✓ —— 子 agent 报：重载之后它总是回落到 `example` ✗，
    // 于是界面上显示的不是"这份画是用什么画的" ✓，而是"上一次点了什么" ✓。
    // 取**最后一个**带介质的对象 ✓（即最近一笔 ✓）；按插件 **id** 反查选择器的 key ✓。
    const withMedium = objects.filter((object) => object.medium && object.medium.id);
    const latest = withMedium[withMedium.length - 1];
    if (latest) {
      const key = Object.keys(MEDIUMS).find((name) => MEDIUMS[name].id === latest.medium.id);
      if (key && $("medium") && $("medium").value !== key) {
        $("medium").value = key;
        $("medium").dispatchEvent(new Event("change", { bubbles: true }));
        log("这份文档使用介质「" + latest.medium.id + " v" + latest.medium.version + "」");
      }
    }
    if (objects.some((object) => object.medium || object.type === "raster_patch" ||
                                 object.type === "retouch")) {
      needsServerPixels = true;
      queueServerBlit();
    }
  } catch (error) { /* 扫描失败不阻塞加载 ✓ */ }
}

async function loadKernel(since = 0) {
  if (!state.wasm) return false;
  const { w, h } = state.docSize;
  const atoms = await fetch(api("/api/atoms") + "&since=" + since).then((r) => r.json());
  if (!atoms.ok) {
    log("读取原子失败：" + JSON.stringify(atoms).slice(0, 160), "#c33");
    return false;
  }
  if (!state.kernel || since === 0) {
    state.kernel = new state.wasm.WasmKernel(state.docId, 256, w, h, 64 * 1024 * 1024);
    // 诊断句柄：仅在 `?debug=1` 时挂到 window 上，供 scripts/browser-kernel-perf.mjs
    // 直接测量内核区域渲染成本（默认不暴露，避免把内部对象变成事实上的公开 API）。
    if (new URLSearchParams(location.search).has("debug")) window.yanshiKernel = state.kernel;
    verifyKernelSurface(state.kernel);
    window.yanshiKernelReady = true;
    const loaded = JSON.parse(state.kernel.load_atoms_json(JSON.stringify(atoms.atoms)));
    // 装载完成后立刻重绘：此前只有"内核就绪"的状态变化，没有触发重绘 ✗ ——
    // 打开已有作品时画面会是白布，直到用户落笔（用户报告的现象）。
    if (loaded.ok) {
      state.docSize = { w, h };
      clampViewport();
      renderViewport();
    }
    if (!loaded.ok) {
      log("内核装载失败：" + JSON.stringify(loaded).slice(0, 160), "#c33");
      state.kernel = null;
      return false;
    }
  } else {
    let applied = 0;
    for (const atom of atoms.atoms) {
      const response = JSON.parse(state.kernel.apply_atom_json(JSON.stringify(atom)));
      if (!response.ok) {
        // **失败必须回收内核** ✓ —— 否则它停在"应用了一半"的状态 ✓，
        // 而 `state.localSeq` 没更新 ✓ ⇒ 下一次续传会把已应用的原子**再应用一遍** ✗
        //（设计上原子是幂等的吗？**不是所有都幂等** ✗，例如 `draw_stroke` ✓）。
        // 规则 ✓：**失败的续传绝不能把客户端留在半途** ✓ —— 回收 ⇒ 下次从 0 重建 ✓。
        log("内核增量折叠失败：" + JSON.stringify(response).slice(0, 160), "#c33");
        state.kernel = null;
        window.yanshiStats.resyncFallbacks = (window.yanshiStats.resyncFallbacks || 0) + 1;
        return false;
      }
      applied += 1;
    }
    window.yanshiStats.incrementalResyncs = (window.yanshiStats.incrementalResyncs || 0) + 1;
    window.yanshiStats.lastResyncAtoms = applied;
  }
  state.localSeq = atoms.head_seq;
  window.yanshiStats.kernelHead = atoms.head_seq;
  window.yanshiStats.serverHead = atoms.head_seq;
  if (typeof refreshContactLink === "function") refreshContactLink();
  state.kernel.set_viewport(0, 0, w, h);
  // 内核就绪 ⇒ 判断这份文档内核表示得了吗 ✓（否则首屏是白板 ✓）。
  void detectHeavyContent();
  // **打开文档时核对已有选区** ✓ —— 子 agent 实测：文档里遗留一个选区时，
  // 之后画的**一切**都被裁掉（画布看似全白 ✗），而状态栏还写着"无选区" ✗。
  // 选区是设计内的能力 ✓（约束之后的绘制 ✓），不该禁止 ✗，但**必须让人看见** ✓。
  void refreshSelectionHint();
  return true;
}

/// 渲染文档坐标区域 `(x,y,w,h)` 并画进画布（画布坐标 = 文档坐标 − 视口原点）。
function drawKernelRegion(x, y, w, h) {
  // 含 heavy 内容的文档里 ✓，内核那份像素是**空白**的 ✓ ⇒ 内核落笔之后用**服务端像素**补画 ✓
  //（整块绘制、脏区绘制、缩放重绘都走这里 ✓，WS 的 tiles 事件也不例外 ✓）。
  // **按"内核刚刚画成空白的那块区域"补画** ✓ —— 用函数自己的 `(x,y,w,h)` ✓：
  // 这就是脏区协议最直接的用法 ✓（此前一律整视口 ✗）。
  if (needsServerPixels) queueServerBlit([x, y, w, h]);
  // 裁剪到视口：视口外像素不渲染也不上传（与设计的数据流过滤一致）。
  const vx = state.viewport.x;
  const vy = state.viewport.y;
  const x0 = Math.max(x, vx);
  const y0 = Math.max(y, vy);
  const x1 = Math.min(x + w, vx + board.width);
  const y1 = Math.min(y + h, vy + board.height);
  if (x1 <= x0 || y1 <= y0) return;
  const cw = Math.round(x1 - x0);
  const ch = Math.round(y1 - y0);
  const started = performance.now();
  const rgba = state.kernel.render_region_rgba(x0, y0, cw, ch);
  if (!rgba || rgba.length < cw * ch * 4) {
    // **内核给不出这一块像素 ⇒ 用服务端像素补画** ✓ —— 这就是"重新打开含介质的文档是白板"的根因 ✓：
    // heavy 原子（`import_image` = 每一笔介质 ✓、液化…）客户端内核折叠不出来 ✓，
    // 实测 `kernel.render_region_rgba(300,400,60,60)` 返回 **len 0** ✓（三位子 agent 独立复现 ✓），
    // 而这里原先**直接 return** ✗ ⇒ 画布留白 ✓，而服务端渲染/缩略图/导出**全都正确** ✓。
    //
    // 判据刻意选得**精确且廉价** ✓：不是"这份文档曾经有过重内容" ✗（我上一轮那样做，
    // 标记太黏 ⇒ 之后的本地乐观笔迹会被服务端像素覆盖 ✗），而是"
    // **此刻这一块内核确实给不出像素**" ✓ —— 轻量文档永远给得出 ✓ ⇒ 不会误伤乐观渲染 ✓。
    // 补画的范围就是**这一块** ✓（`(x0,y0,cw,ch)` 是裁剪后的 ✓）；此前一律整视口 ✗。
    queueServerBlit([x0, y0, cw, ch]);
    return;
  }
  const renderedAt = performance.now();
  // putImageData 不做 CSS 缩放：画布内部分辨率与视口文档像素一一对应。
  ctx.putImageData(
    new ImageData(new Uint8ClampedArray(rgba), cw, ch),
    Math.round(x0 - vx),
    Math.round(y0 - vy)
  );
  const elapsed = performance.now() - started;
  window.yanshiStats.lastRenderMs = renderedAt - started;
  window.yanshiStats.lastPutMs = performance.now() - renderedAt;
  window.yanshiStats.lastArea = w * h;
  window.yanshiStats.lastApplyMs = elapsed;
  if (window.yanshiStats.firstStrokeMs === null && state.dragging) {
    window.yanshiStats.firstStrokeMs = elapsed;
    $("firstStroke").textContent = elapsed.toFixed(2) + "ms";
  }
}

// 拖动中的笔迹重绘：区域通常只有几十像素见方，直接渲染比「按 tile 组合」便宜得多
// （后者哪怕 1px 变化也要重算整块 256² tile）。两者数值逐位一致。
// 把绘制异常变成可见信息（日志 + 状态栏 + window.yanshiStats），只报一次以免刷屏。
function reportPaintError(where, error) {
  const message = where + "失败：" + (error && error.message ? error.message : String(error));
  if (window.yanshiStats.lastPaintError === message) return;
  window.yanshiStats.lastPaintError = message;
  window.yanshiStats.paintErrors = (window.yanshiStats.paintErrors || 0) + 1;
  log(message, "#c33");
  setStatus({ paintError: message });
}

function drawKernelBoxDirect(bbox) {
  if (window.yanshiStats.tracePaints) log("direct bbox=" + JSON.stringify(bbox) + " board=" + board.width + "x" + board.height);
  if (!bbox) return;
  const started = performance.now();
  const vx = state.viewport.x;
  const vy = state.viewport.y;
  const x0 = Math.max(Math.floor(bbox[0]), vx);
  const y0 = Math.max(Math.floor(bbox[1]), vy);
  const x1 = Math.min(Math.ceil(bbox[0] + bbox[2]), vx + board.width);
  const y1 = Math.min(Math.ceil(bbox[1] + bbox[3]), vy + board.height);
  const w = Math.round(x1 - x0);
  const h = Math.round(y1 - y0);
  if (x1 <= x0 || y1 <= y0 || w <= 0 || h <= 0) return;
  const rgba = state.kernel.render_region_direct_rgba(x0, y0, w, h);
  if (window.yanshiStats.tracePaints) log("direct 渲染 " + x0 + "," + y0 + " " + w + "x" + h + " len=" + (rgba ? rgba.length : "null") + " 期望=" + (w * h * 4));
  if (!rgba || rgba.length < w * h * 4) return;
  const renderedAt = performance.now();
  ctx.putImageData(
    new ImageData(new Uint8ClampedArray(rgba), w, h),
    Math.round(x0 - vx),
    Math.round(y0 - vy)
  );
  const elapsed = performance.now() - started;
  window.yanshiStats.lastDirectMs = renderedAt - started;
  window.yanshiStats.lastDirectArea = w * h;
  return elapsed;
}

/// **用服务端像素补画** ✓ —— heavy 原子（`import_image`/液化…）的像素在服务端 ✓，
/// 客户端 WASM 内核**表示不了**它们 ✗（它只折叠轻量原子 ✓）。
/// 因此"重载内核"救不了空白画布 ✗：必须按设计 14.5「打开即图片」的服务端铺底路径 ✓，
/// 把该区域的**服务端像素**直接贴到内容画布上 ✓。
///
/// 坐标系与 `drawKernelBoxDirect` 完全一致 ✓（文档坐标 − 视口 = 画布坐标 ✓），
/// 这样内核与服务端两条路径画出来的东西不会错位 ✓。
async function blitServerBox(bbox) {
  if (!bbox) return 0;
  const vx = state.viewport.x;
  const vy = state.viewport.y;
  const x0 = Math.max(Math.floor(bbox[0]), vx);
  const y0 = Math.max(Math.floor(bbox[1]), vy);
  const x1 = Math.min(Math.ceil(bbox[0] + bbox[2]), vx + board.width);
  const y1 = Math.min(Math.ceil(bbox[1] + bbox[3]), vy + board.height);
  const w = Math.round(x1 - x0);
  const h = Math.round(y1 - y0);
  if (w <= 0 || h <= 0) return 0;
  const value = await callTool(
    "render_region",
    { region: { x: x0, y: y0, w, h }, raw: true },
    { refresh: false },
  );
  const url = value.raw_url || value.thumb_url;
  if (!url) return 0;
  // `raw_url` 给的是**原始 RGBA** ✓（不是 PNG ✗）⇒ 直接构造 ImageData ✓。
  const bytes = new Uint8ClampedArray(await fetch(api(url)).then((r) => r.arrayBuffer()));
  if (bytes.length < w * h * 4) return 0;
  ctx.putImageData(new ImageData(bytes, w, h), Math.round(x0 - vx), Math.round(y0 - vy));
  window.yanshiStats.serverBlits = (window.yanshiStats.serverBlits || 0) + 1;
  window.yanshiStats.lastServerBlitArea = w * h;
  return w * h;
}

/// 补画排队 ✓：**忙的时候记账，而不是丢弃** ✗ ——
/// 若写成"有请求在飞就 return" ✓，期间发生的重绘（WS 的 tiles 事件很频繁 ✓）就永远不会再补 ✗
/// ⇒ 画布停在内核那张空白图上 ✓（实测 `serverBlits` 有值而画面全白 ✓）。
let serverBlitBusy = false;
let serverBlitPending = false;
/// **待补画的脏区**（文档坐标 `[x,y,w,h]` ✓）。
///
/// **为什么要有它** ✓：提交响应里**本来就返回** `dirty_bbox` / `dirty_tiles` ✓（设计的两层渲染正是
/// 按脏区推进 ✓），内核那条路也**已经**在用 `drawKernelBoxDirect(response.dirty_bbox)` ✓ ——
/// 只有"服务端补画"这条路一直在**整视口**重画 ✗（实测全幅渲染 251ms ✓）。
/// 无内核的机器每落一笔都要补画 ✓ ⇒ 那是实打实的一笔开销 ✗。
/// 现在：有脏区就只补脏区 ✓，多次排队则**取并集** ✓（少发请求、也少画 ✓）。
let serverBlitBox = null;
/// 需要**整视口**补画 ✓（不知道脏区时用它 ✓ —— 例如"打开文档后第一次铺底" ✓）。
let serverBlitWhole = false;
const unionBox = (left, right) => {
  if (!left) return right.slice();
  if (!right) return left.slice();
  const x0 = Math.min(left[0], right[0]);
  const y0 = Math.min(left[1], right[1]);
  const x1 = Math.max(left[0] + left[2], right[0] + right[2]);
  const y1 = Math.max(left[1] + left[3], right[1] + right[3]);
  return [x0, y0, x1 - x0, y1 - y0];
};
function queueServerBlit(bbox = null) {
  // **整视口请求会"顶掉"脏区** ✓：宁可多画一点 ✓，也不能因为只画了脏区而留下空白 ✗。
  if (bbox) serverBlitBox = unionBox(serverBlitBox, bbox);
  else serverBlitWhole = true;
  if (serverBlitBusy) { serverBlitPending = true; return; }
  serverBlitBusy = true;
  void blitServerViewport().finally(() => {
    serverBlitBusy = false;
    if (serverBlitPending) { serverBlitPending = false; queueServerBlit(); }
  });
}

/// 补画**当前视口** ✓（不知道原子的脏区时用它 ✓，一次请求即可 ✓）。
async function blitServerViewport() {
  // **有脏区就只补脏区** ✓（取并集之后一次画完 ✓）；否则整视口 ✓。
  if (!serverBlitWhole && serverBlitBox) {
    const box = serverBlitBox;
    serverBlitBox = null;
    // 等布局稳定那套逻辑对脏区同样需要 ✓ ⇒ 直接复用 `blitServerBox`（它自己会裁剪到视口 ✓）。
    return blitServerBox(box);
  }
  serverBlitBox = null;
  serverBlitWhole = false;
  // **等下一帧再画** ✓ —— 本会话实测：补画确实执行了 ✓（`serverBlits` 计数增加 ✓、面积 262144 ✓），
  // 但最终画布仍是空白 ✗。原因是**布局变化会重设画布尺寸** ✓（`sizeBoards` 改 `board.width` ⇒ 清空 ✓），
  // 而它可能发生在补画**之后** ✓ ⇒ 补画被清掉 ✓。
  // 因此让补画落在**下一帧**（布局稳定之后 ✓）—— 这是"最后画的人赢"那条经验的延续 ✓。
  // **等两帧** ✓ —— 一帧只覆盖"本轮布局" ✓；居中之后布局可能再变一次 ✓
  //（`sizeBoards` 会按新尺寸清空画布 ✓），于是补画又被清掉 ✗。两帧覆盖连续两次布局收敛 ✓。
  // **与超时赛跑** ✓：后台标签页里 `requestAnimationFrame` 不回调 ✓（连 `setTimeout` 也被节流 ✓）
  // ⇒ 只靠 rAF 会让补画挂住 ✓（四位子 agent 独立遇到 ✓，他们手工用 `Page.bringToFront` 绕过 ✓）。
  const settle = () => new Promise((resolve) => {
    let done = false;
    const finish = () => { if (!done) { done = true; resolve(); } };
    try { requestAnimationFrame(finish); } catch (_) { /* 无 rAF 时靠超时 ✓ */ }
    setTimeout(finish, 50);
  });
  await settle();
  await settle();
  const { x, y, w, h } = state.viewport;
  return blitServerBox([x, y, w, h]);
}

function drawKernelBox(bbox) {
  if (!bbox) return;
  const x = Math.max(0, Math.floor(bbox[0]));
  const y = Math.max(0, Math.floor(bbox[1]));
  const w = Math.max(1, Math.ceil(bbox[2]));
  const h = Math.max(1, Math.ceil(bbox[3]));
  // 裁剪交给 drawKernelRegion（按视口裁剪，而不是按画布像素数）。
  drawKernelRegion(x, y, w, h);
}

function drawKernelDirty(report) {
  drawKernelBox(report && report.dirty_bbox);
}

// 拖动中的笔迹：**增量盖章**（只处理新增笔段），并只重绘该段区域。
async function updatePreviewOverlay(pending) {
  const started = performance.now();
  // 任何绘制异常都要**显式可见**：此前 TypeError 被事件处理器吞掉，
  // 现象只是「画布空白」，排查代价很高。
  try {
    return await updatePreviewOverlayInner(pending, started);
  } catch (error) {
    reportPaintError("覆盖层绘制", error);
    return undefined;
  }
}

async function updatePreviewOverlayInner(pending, started) {
  const response = JSON.parse(state.kernel.extend_preview_stroke(JSON.stringify(previewObject(pending))));
  if (!response.ok) { log("覆盖层应用失败：" + JSON.stringify(response).slice(0, 160), "#c33"); return; }
  window.yanshiStats.previewApplies = (window.yanshiStats.previewApplies || 0) + 1;
  if (window.yanshiStats.tracePaints) log("盖章返回 dirty_bbox=" + JSON.stringify(response.dirty_bbox) + " keys=" + Object.keys(response).join(","));
  drawKernelBoxDirect(response.dirty_bbox);
  const elapsed = performance.now() - started;
  window.yanshiStats.lastOverlayMs = elapsed;
  window.yanshiStats.overlayApplies = (window.yanshiStats.overlayApplies || 0) + 1;
  if (typeof window.yanshiStats.firstStrokeMs !== "number") {
    window.yanshiStats.firstStrokeMs = elapsed;
    $("firstStroke").textContent = elapsed.toFixed(2) + "ms";
  }
}

// 覆盖层对象（最小字段：layer_id/type/data；内核会补 z 序与可见性）。
function previewObject(pending) {
  const color = colorCss();
  const size = Number($("size").value);
  const points = state.points.map((p) => [p.x, p.y]);
  // **新笔迹启用渲染时平滑** ✓（设计 11.1 的"矢量"路径 ✓）：日志里存的仍是**原始采样点** ✓，
  // 平滑只发生在渲染时 ✓ ⇒ 放大时线条不再是折线 ✓、无损 ✓、以后想换插值也不用改历史 ✓。
  if (pending.tool === "rect" || pending.tool === "ellipse") {
    const [a, b] = state.points;
    return {
      layer_id: pending.layerId,
      type: "shape",
      data: {
        geometry: {
          kind: pending.tool,
          bbox: {
            x: Math.min(a.x, b.x), y: Math.min(a.y, b.y),
            w: Math.max(1, Math.abs(b.x - a.x)), h: Math.max(1, Math.abs(b.y - a.y)),
          },
        },
        color,
      },
    };
  }
  if (pending.tool === "erase") {
    return { layer_id: pending.layerId, type: "stroke", data: { points, size: size * 1.5, color: { r: 255, g: 255, b: 255, a: 255 } } };
  }
  return {
    layer_id: pending.layerId,
    type: "stroke",
    // `smooth: true` ✓ ⇒ **渲染时**做 Catmull-Rom 平滑 ✓（日志里仍是原始采样点 ✓）——
    // 这就是设计 11.1 里"矢量"那一类介质的落点 ✓：几何存日志 ✓、按视图重栅格化 ✓。
    data: { points, size, color, hardness: 0.7, smooth: true },
  };
}

// 把客户端构造的原子立刻应用到本地内核（乐观渲染），返回是否成功。
function applyLocal(atom) {
  const response = JSON.parse(state.kernel.apply_atom_json(JSON.stringify(atom)));
  window.yanshiStats.applies += 1;
  if (response.ok) {
    drawKernelDirty(response.report);
    state.localSeq = response.report.head;
    window.yanshiStats.kernelHead = response.report.head;
    return true;
  }
  if (response.error_code === "out_of_order") return false;
  log("本地应用被拒：" + JSON.stringify(response).slice(0, 160), "#c33");
  return false;
}

// 服务端校正：seq 预测错了（别人插了原子）或提交被拒时，全量重建本地状态。
/// 与内核重新对齐 ✓。**优先增量续传** ✓，失败才退回**整条重放** ✓。
///
/// 子 agent 报的 F5 ✓：每笔介质都触发一次全量重同步 ✗ ⇒ 提交耗时随文档增长
/// （~1.1s → ~4–5s ✓）。根因是这里**写死 `loadKernel(0)`** ✗ —— 而 `loadKernel`
/// 本来就支持 `since > 0` 的增量应用 ✓（只是没人这样调它 ✓）。
///
/// **规则（上一轮已写进文档 ✓，这里落实）** ✓：续传失败**必须**退回全量 ✓，
/// 绝不能静默停在半途 ✓ —— 半途的内核状态既缺原子 ✓ 又可能被下一次续传重复应用 ✗。
async function resync() {
  window.yanshiStats.resyncs += 1;
  const resumeFrom = state.localSeq || 0;
  // 先试增量 ✓（`localSeq` 是**最后一个成功应用**的序号 ✓ ⇒ 不重复、不遗漏 ✓）。
  if (resumeFrom > 0 && (await loadKernel(resumeFrom))) {
    await refreshPreview(true);
    return;
  }
  // 增量不可用（或从 0 开始 ✓）⇒ 整条重放 ✓（内核已被上次失败回收 ✓ 或被重建 ✓）。
  if (await loadKernel(0)) {
    await refreshPreview(true);
  }
}

/// **提交后的收尾动作，集中在这里**（缩略图、历史列表、撤销/重做栈）。
///
/// 曾经这些动作散落在 `callTool` 与 `submitAtom` 两条路径里 ✗，结果是"笔迹路径漏刷历史/漏入栈"
/// 这类疏漏出现了两次 ✓。现在两条路径都只调这一个函数 ✓；将来再加收尾动作也只改这里。
function afterMutation(atomId, options = {}) {
  // **撤销栈必须无条件维护**：此前这段被放在 `if (options.refresh !== false)` 里 ✗，
  // 于是所有传 `refresh: false` 的调用（填充、效果面板、修图、液化…）都不入栈 ✓ ——
  // 表现为「填充后点撤销没用」（撤销撤掉的是上一笔，填充仍覆盖整幅）。
  if (atomId) {
    state.undoStack.push({ kind: "atom", id: atomId });
    state.redoStack.length = 0;
    updateUndoStatus();
  }
  if (options.skipRefresh) return;
  // **无内核时，任何提交之后都要从服务端补画** ✓ —— 这是"一笔一画"能在新机器上看见的关键 ✓。
  // 放在这里而不是各条落笔路径里 ✓：它是**所有提交的收口** ✓（普通笔 / 形状 / 填充 / 效果 / 图层… ✓），
  // 一处修好，全部受益 ✓（本项目反复吃过"只修一条路径"的亏 ✓）。
  if (!state.wasm) {
    needsServerPixels = true;
    // **有脏区就只补脏区** ✓（提交响应本来就给了 ✓，此前丢掉不用 ✗、一律整视口 ✓）。
    queueServerBlit(options.dirtyBox || null);
  }
  scheduleThumbRefresh();
  void refreshHistory();
}

async function submitAtom(atom) {
  const response = await fetch(api("/api/atoms"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(atom),
  }).then((r) => r.json());
  $("last").textContent = JSON.stringify(response).slice(0, 600);
  window.yanshiStats.serverHead = response.head ?? window.yanshiStats.serverHead;
  setStatus({ head: response.head, dirty: (response.dirty_tiles || []).length });
  if (!response.ok) {
    log("提交被拒（回滚本地乐观渲染）：" + response.error_code, "#c33");
    await resync();
    return null;
  }
  if (response.seq !== undefined && response.seq > state.localSeq + 1) {
    // 服务端把原子排在了本地预测之后（有并发原子），补齐缺口。
    await resync();
  }
  // 笔迹走 `/api/atoms`（不经 callTool），但收尾动作与其它提交**完全一致** ✓。
  afterMutation(response.atom_id, { dirtyBox: response.dirty_bbox || null });
  return response;
}

async function checkBitExact() {
  if (!kernelReady()) { log("没有 WASM 内核，无法自检", "#c33"); return; }
  // 以**服务端返回的尺寸**为准：`state.docSize` 可能因刷新时序而滞后，
  // 那样会出现「尺寸不一致」的误报（实测遇到）。
  const { w, h } = state.docSize;
  // 取服务端**原始像素**（而不是哈希）：哈希相等无法说明差多少，
  // 而跨「客户端预览 / 服务端权威」路径的比较在设计上属 D1（允许 ±1 LSB）。
  const server = await callTool("render_region", {
    region: { x: 0, y: 0, w, h },
    raw: true,
  }, { refresh: false });
  if (!server.raw_url) { log("服务端未返回原始像素，无法自检", "#c33"); return; }
  const response = await fetch(api(server.raw_url.replace("yanshi://blob/", "/api/blob/")));
  const serverPixels = new Uint8Array(await response.arrayBuffer());
  // 用服务端实际渲染的尺寸请求本地像素，避免双方尺寸口径不同。
  const width = server.width || w;
  const height = server.height || h;
  const localPixels = state.kernel.render_region_rgba(0, 0, width, height);
  if (localPixels.length === 0) {
    // 内核没产出像素（未就绪 / 文档尺寸不符 / 内核处于错误状态）：明确报出来，
    // 不要伪装成「尺寸不一致」，否则会误导排查方向。
    window.yanshiStats.bitExact = false;
    $("bitExact").textContent = "内核无输出";
    log(
      `自检失败：本地内核未产出像素（请求 ${width}×${height}）；` +
      `内核 HEAD ${window.yanshiStats.kernelHead}，文档尺寸 ${state.docSize.w}×${state.docSize.h}`,
      "#c33"
    );
    return;
  }
  if (localPixels.length !== serverPixels.length) {
    window.yanshiStats.bitExact = false;
    $("bitExact").textContent = "尺寸不一致";
    log(`自检失败：本地 ${localPixels.length} 字节 vs 服务端 ${serverPixels.length} 字节`, "#c33");
    return;
  }
  let diffPixels = 0;
  let maxDelta = 0;
  for (let index = 0; index < serverPixels.length; index += 4) {
    let pixelDiffers = false;
    for (let channel = 0; channel < 4; channel++) {
      const delta = Math.abs(serverPixels[index + channel] - localPixels[index + channel]);
      if (delta > 0) { pixelDiffers = true; }
      if (delta > maxDelta) { maxDelta = delta; }
    }
    if (pixelDiffers) diffPixels += 1;
  }
  const total = serverPixels.length / 4;
  const ratio = total > 0 ? diffPixels / total : 0;
  // D0（同一路径）应逐位相同；跨路径按 D1 允许 ±1 LSB，且只允许极少数像素踩到舍入边界。
  //
  // 判据由设计决策更新：设计 6.1 原本把**滤镜**列在 D0，实测其成本（方框模糊 O(radius)/像素）是
  // 全项目最大的性能瓶颈，经设计方批准，**模糊族滤镜**放宽到 D1（±1 LSB）。因此跨路径比较不再
  // 要求逐字节相同，而是：**最大通道差 ≤1 LSB** 且 **差异像素占比极少**。
  // 像素数阈值改为**与画布成比例**（0.01%，下限 64）而不是写死 16 —— 写死的绝对值在大画布上过严、
  // 在小画布上过松；比例判据对 1024² 允许约 105 个像素，仍能抓住"大面积 ±1 漂移"这类真实缺陷。
  // 上限取画布的 0.05%（下限 64 像素）。依据实测（1024²）：
  //   * 含大量模糊族滤镜的文档：186 像素（0.018%）
  //   * 含单个 clarity/dehaze 的文档：18 像素（0.0017%）
  //   * **不含模糊族**的文档（形状 + 笔触 + 曝光 + 色彩平衡）：**0 像素（逐位相同）**
  // 即偏差严格限制在模糊族；0.05% 相对实测最差值留约 2.8× 余量，
  // 而"大面积 ±1 漂移"或任何 >1 LSB 的差异仍会被判不通过。
  const allowedDiffPixels = Math.max(64, Math.floor(total * 0.0005));
  const pass = maxDelta <= 1 && diffPixels <= allowedDiffPixels;
  window.yanshiStats.bitExact = pass;
  window.yanshiStats.diffPixels = diffPixels;
  window.yanshiStats.maxChannelDelta = maxDelta;
  window.yanshiStats.localHash = null;
  window.yanshiStats.serverHash = server.blob_hash || null;
  window.yanshiStats.allowedDiffPixels = allowedDiffPixels;
  $("bitExact").textContent = pass
    ? (diffPixels === 0 ? "逐位相同" : `±1 LSB × ${diffPixels}`)
    : `差异 ${diffPixels} 像素 / 最大 ${maxDelta}（上限 ${allowedDiffPixels}）`;
  log(
    `自检：差异像素 ${diffPixels}/${total}（${(ratio * 100).toFixed(4)}%），最大通道差 ${maxDelta}，` +
    `允许上限 ${allowedDiffPixels}；判定 ${pass ? "通过（D1 允许 ±1 LSB）" : "不通过"}`,
    pass ? "#2a7" : "#c33"
  );
  refreshThumb();
}

/// 新建文档：**先让用户输入名字** ✓（用户要求），再按该 id 创建并切换。
///
/// 文档以 `doc_id` 作为名字与主键 ✓；`POST /api/documents` 是"打开或创建"语义 ✓，
/// 因此这里**先查列表**：重名时提示换名字，而不是悄悄打开已有文档 ✓。
function newDocument() {
  const dialog = $("newDialog");
  const name = $("newName");
  name.value = "yanshi-" + Date.now().toString(36);
  $("newHint").textContent = "";
  if (typeof dialog.showModal === "function") dialog.showModal();
  else dialog.setAttribute("open", "");
  name.focus();
  name.select();
}

async function createNamedDocument() {
  const name = ($("newName").value || "").trim();
  const hint = $("newHint");
  if (!name) {
    hint.textContent = "请输入名称";
    return;
  }
  if (!/^[A-Za-z0-9._-]+$/.test(name)) {
    hint.textContent = "名称只能用字母、数字、点、下划线与连字符（它会进入 URL 与文件路径）";
    return;
  }
  try {
    const listed = await fetch("/api/documents").then((response) => response.json());
    if ((listed.documents || []).some((info) => info.doc_id === name)) {
      hint.textContent = "已存在同名文档，请换一个名字，或用「打开…」";
      return;
    }
  } catch (_) {
    // 列不出来就交给服务端判断（打开或创建），不阻塞创建 ✓。
  }
  const dialog = $("newDialog");
  if (typeof dialog.close === "function") dialog.close();
  else dialog.removeAttribute("open");
  log("新建文档：" + name);
  await switchDocument(name);
}

// **无条件**提供工具入口 ✓ —— 此前它挂在 `if (DEBUG)` 下 ✗，于是四份不同的自动化脚本
// 都撞上过 `window.yanshiCallTool is not a function` ✓（子 agent 直接把它列为"与环境说明不符" ✗，
// 我自己的检查脚本里也踩过一次 ✓）。它只是 HTTP 工具 API 的一层薄包装 ✓，
// 而应用本来就把同一套工具暴露成 API ✓ ⇒ 没有理由只在 `?debug=1` 时存在 ✓。
window.yanshiCallTool = (tool, args) => callTool(tool, args || {}, { refresh: false });

/// 打开对话框：列出**服务器上的文档**（`GET /api/documents`，设计第 611 行提到文档列表用
/// `doc_thumb` 缩略图），点击即切换；下方提供**本地图片导入**。
/// **示例作品** ✓：id + 一句话说明 ✓。它们由"用应用自己画一遍"产生 ✓（见 docs/samples.md ✓）——
/// 这不只是好看 ✓：画的过程会**暴露真实问题** ✓（本会话就是用这种方式发现了若干 bug ✓）。
const SAMPLES = [
  { id: "sample-oil", label: "油画 · 风景", hint: "油画介质：鬃毛、载墨、湿画法混色" },
  // **由仓库里的生成器画出来的** ✓（`scripts/make-samples.mjs` ✓，真介质、确定性、可复现 ✓）
  // —— 这是"示例里能看到真实创作"的第一步 ✓，也是任何人 `node scripts/make-samples.mjs` 都能重画的 ✓。
  { id: "sample-lake", label: "油画 · 湖畔写生", hint: "生成器作品：分层铺色、低阳、远岸与倒影" },
  { id: "sample-watercolor", label: "水彩 · 山与湖", hint: "水彩介质：渗开边界、边缘沉积、留白" },
  { id: "sample-brush", label: "笔刷 · 草木", hint: "曲线/动力学/纹理/湿笔（appearance）" },
  { id: "sample-reference", label: "功能清单 · 海报", hint: "文本（含中文）、图形、选区、蒙版、移动、导出" },
];

function renderSamples() {
  const box = $("sampleList");
  if (!box) return;
  box.innerHTML = "";
  for (const sample of SAMPLES) {
    const card = document.createElement("button");
    card.type = "button";
    card.style.cssText = "display:flex;flex-direction:column;gap:2px;padding:8px;text-align:left";
    const title = document.createElement("span");
    title.style.cssText = "font-size:12px;font-weight:600";
    title.textContent = sample.label;
    const hint = document.createElement("span");
    hint.style.cssText = "font-size:11px;opacity:.7";
    hint.textContent = sample.hint;
    card.append(title, hint);
    card.addEventListener("click", async () => {
      closeOpenDialog();
      await switchDocument(sample.id);
      // **打开示例后按需把画面搬进来** ✓（空文档才做 ✓；已有内容则什么都不动 ✓）——
      // 换一台机器时，示例文档原本**不存在** ✓ ⇒ `switchDocument` 只建了个空文档 ✓
      // ⇒ 这里补上画面 ✓（否则用户看到的就是空白 ✓，用户实测过 ✓）。
      if (await seedSampleIfEmpty(sample.id)) {
        await resync();
        await refreshPreview();
        await refreshLayers();
      }
    });
    box.appendChild(card);
  }
}

async function showOpenDialog() {
  const dialog = $("openDialog");
  const list = $("docList");
  renderSamples();
  list.textContent = "载入中…";
  if (typeof dialog.showModal === "function") dialog.showModal();
  else dialog.setAttribute("open", "");
  try {
    const value = await fetch("/api/documents").then((response) => response.json());
    const documents = value.documents || [];
    list.innerHTML = "";
    if (documents.length === 0) list.textContent = "（服务器上还没有文档）";
    for (const info of documents) {
      const card = document.createElement("button");
      card.style.cssText = "display:flex;flex-direction:column;gap:4px;padding:6px;text-align:left";
      const thumb = document.createElement("img");
      thumb.style.cssText = "width:100%;height:72px;object-fit:contain;background:#fff;border:1px solid var(--line);border-radius:4px";
      if (info.thumb_url) thumb.src = info.thumb_url;
      const label = document.createElement("span");
      label.style.fontSize = "12px";
      label.textContent = info.doc_id + (info.head_seq !== undefined ? " · head " + info.head_seq : "");
      card.append(thumb, label);
      card.addEventListener("click", async () => {
        closeOpenDialog();
        await switchDocument(info.doc_id);
      });
      list.appendChild(card);
    }
  } catch (error) {
    list.textContent = "读取文档列表失败：" + error.message;
  }
}

function closeOpenDialog() {
  const dialog = $("openDialog");
  if (typeof dialog.close === "function") dialog.close();
  else dialog.removeAttribute("open");
}

/// `#rrggbb` → `[r, g, b]`（0..1）✓。
function hexToUnit(hex) {
  const value = String(hex || "#000000").replace("#", "");
  const int = Number.parseInt(value.length === 3 ? value.replace(/(.)/g, "$1$1") : value, 16);
  return [((int >> 16) & 255) / 255, ((int >> 8) & 255) / 255, (int & 255) / 255];
}

/// 介质插件的宿主侧加载器 ✓（设计 11.1 的 **WASM 插件**；宿主 = 浏览器 ✓）。
///
/// 加载时**强制三条边界** ✓：
/// ① `imports` 必须为空 ✓ —— 插件拿不到任何宿主能力，因此**无网络、无时钟** ✓（比声明更硬 ✓）；
/// ② ABI 版本必须匹配 ✓；③ `id + version` 随对象记录 ✓（由调用方写进 `data.medium` ✓）。
const MEDIUMS = {
  // 示范介质：仓库里的零依赖插件，产物提交在 assets/mediums/ ✓。
  // `version` 与插件自报的 ABI 版本核对 ✓ —— 两个版本并存正是设计
  // "插件 id + version 随原子记录、升级不改写历史"要支持的 ✓。
  example: { id: "example-dab", version: 1, url: "/mediums/example-dab.wasm" },
  // 油画（ABI v2）：宿主注入笔尖色、目标色、载墨与湿度 ✓。
  oil: { id: "oil", version: 2, url: "/mediums/oil.wasm" },
  // 水彩（ABI v2）：渗开的不规则边界 + 边缘沉积 + 半透明纸感 ✓。
  watercolor: { id: "watercolor", version: 2, url: "/mediums/watercolor.wasm" },
  // 马克笔（ABI v2）：平头笔尖 + 叠色变深 + 轻微洇边 ✓。
  marker: { id: "marker", version: 2, url: "/mediums/marker.wasm" },
  // 铅笔（ABI v2）：软圆尖 + 压力驱动深浅 + 石墨颗粒、几乎不混色 ✓。
  pencil: { id: "pencil", version: 2, url: "/mediums/pencil.wasm" },
  // 像素（ABI v2）：硬边方形笔尖、**完全不抗锯齿**、颜色精确 ✓。
  pixel: { id: "pixel", version: 2, url: "/mediums/pixel.wasm" },
};

async function loadMedium(name) {
  const spec = MEDIUMS[name];
  if (!spec) throw new Error("未知介质 " + name);
  if (spec.instance) return spec;
  const bytes = await fetch(spec.url).then((response) => response.arrayBuffer());
  const module = await WebAssembly.compile(bytes);
  const imports = WebAssembly.Module.imports(module);
  if (imports.length !== 0) {
    throw new Error("插件 import 了宿主函数，违反插件边界：" + imports.map((i) => i.name).join(", "));
  }
  const instance = await WebAssembly.instantiate(module, {});
  // 预检：插件必须导出线性内存 ✓（否则宿主无法读取它产出的像素 ✗ ——
  // 这正是上一轮"点击后毫无日志"的可疑点之一 ✓，现在把它变成**可观测**的检查 ✓）。
  if (!(instance.exports.memory instanceof WebAssembly.Memory)) {
    throw new Error(
      "插件没有导出 memory，宿主无法读取像素（导出：" +
        Object.keys(instance.exports).join(", ") + "）");
  }
  const abi = instance.exports.yanshi_abi_version();
  if (abi !== spec.version) {
    throw new Error("插件 ABI 版本 " + abi + " 与登记版本 " + spec.version + " 不一致");
  }
  spec.instance = instance;
  spec.maxDab = instance.exports.yanshi_max_dab();
  window.yanshiStats.medium = {
    id: spec.id, version: spec.version, abi, maxDab: spec.maxDab, status: "ready",
  };
  return spec;
}

/// **整笔**用插件介质铺开 ✓ —— 沿路径逐点落笔 ✓，最后合成**一个**对象 ✓。
///
/// 为什么整笔合成一个对象（而不是每点一个对象 ✓）：
/// * 日志干净 ✓（一笔 = 一个原子 ✓）、**一次撤销** ✓；
/// * 载墨/混色是**沿笔迹**演化的 ✓（同一条笔触内的状态 ✓），拆成多个对象反而无法表达 ✓。
///
/// 载墨耗尽 ✓：每点的 `load` 随**累计路径长度**下降 ✓（点按几乎不耗 ✓、长拖会枯笔 ✓）。
/// 混色 ✓：每个点取**此刻画布上笔尖处**的颜色作为目标色 ✓（与油画湿画法的直觉一致 ✓）。
/// 介质笔尖尺寸（文档像素）✓：读**粗细滑杆** ✓，并夹到插件的 `maxDab` ✓。
///
/// `maxDab` 是插件**自报**的上限 ✓（设计 11.1 的配额之一 ✓）⇒ 宿主只做**上界**约束 ✓，
/// 不该像此前那样把它当**固定值**用 ✗（那是"滑杆无效"的根因 ✓）。
function mediumTipSize(spec) {
  const requested = Number($("size").value);
  const wanted = Number.isFinite(requested) && requested > 0 ? requested : spec.maxDab;
  return Math.max(1, Math.min(wanted, spec.maxDab));
}

/// 让"强度/湿度"标签**随介质说真话** ✓（见 HTML 里的说明 ✓）。
function syncStrengthLabel() {
  const label = $("strengthLabel");
  const select = $("medium");
  if (!label || !select) return;
  // **按介质决定标签，而不是"插件即湿度"** ✗ —— 加入铅笔后这一点变得明显起来 ✓：
  // 铅笔是**干**介质 ✓，它忽略湿度 ✓、用的是**压力** ✓ ⇒ 在它上面写"湿度"同样是误导 ✓。
  const wet = new Set(["oil", "watercolor", "marker"]);
  const isWetMedium = wet.has(select.value);
  label.textContent = isWetMedium ? "湿度" : "强度";
  label.title = isWetMedium
    ? "湿介质按湿度调色：数值越大越湿、颜色越淡"
    : select.value === "pencil"
      ? "铅笔按压力上墨：数值越大越深、石墨越实"
      : "落笔强度";
}

// **逐笔选项** ✓（`options` ✓）：画谱回放需要"每一笔各有颜色/湿度/粗细" ✓，
// 而拖动只有**一套**界面控件 ✓。**默认值仍取自界面** ✓ ⇒ 所有既有调用行为**逐字节不变** ✓。
//
// **这一条我漏过一次** ✗：我只把画谱播放器搬到了干净工作树 ✓，忘了这半 ✗
// ⇒ 生成出来的画**全用界面上的红色、笔尖还是细的** ✓（服务端渲染一看就露馅 ✓）。
// **教训** ✓：跨工作树搬改动时，要按"这次一共改了几处"逐条核对 ✓，不能凭印象 ✓。
/// **批处理会话** ✓ —— 非空时 `mediumStroke` 不再逐笔提交 ✓。
///
/// **为什么** ✓（实测驱动 ✓）：一次 `import_image` 提交固定约 **280 ms** ✗（**与区域面积无关** ✓，
/// 40×40 与 400×200 一样贵 ✓），因为提交要**等渲染 job** 跑完 ✓。
/// 一笔一付 ⇒ 全尺寸 5000 笔 ≈ 23 分钟 ✗。而**一批只付一次** ⇒ 5000 笔约 170 批 ≈ 48 秒 ✓✓。
///
/// **为什么不只是"少提交"** ✓：介质是**湿画法** ✓ —— 每一笔取色时读的是**画布当前像素** ✓，
/// 所以批处理必须让"上一笔的颜料"**立刻出现在画布上** ✓（本地画回 ✓，不等服务端 ✓），
/// 否则同一批里后面的笔会照着**旧画面**调色 ⇒ **画出来的东西就变了** ✗。
/// 盖章循环本身**一字未改** ✓ ⇒ 像素与逐笔提交**逐字一致** ✓（这条有专门的对照测试 ✓）。
let mediumBatchSession = null;

async function mediumStroke(name, points, options = {}) {
  if (!points || points.length === 0) return;
  const spec = await loadMedium(name);
  // **笔尖尺寸来自"粗细"滑杆** ✓，只受插件**自报**的 `maxDab` 上限约束 ✓ ——
  // 此前写死 `Math.min(48, spec.maxDab)` ✗ ⇒ 滑杆只影响拖动抽稀 ✓，笔尖宽度恒为 48px ✓
  //（子 agent 实测：#size 12/24/32/40/48 画出的色带宽度都是 54~56px ✓）。
  // 粗细 ✓：画谱可逐笔指定 ✓，仍受插件自报 `maxDab` 约束 ✓。
  const size = options.size
    ? Math.max(1, Math.min(spec.maxDab, Number(options.size)))
    : mediumTipSize(spec);
  const plugin = spec.instance.exports;
  // 同上：整笔重采样也用 1/8 ✓（与拖动抽稀保持一致 ✓）。
  const spacing = Math.max(1, size / 8);
  const tip = options.color ? hexToUnit(options.color) : hexToUnit($("color").value);
  const wetness = options.wetness !== undefined
    ? Math.max(0, Math.min(1, Number(options.wetness)))
    : (Number($("strength").value) || 40) / 100;

  // 把路径按间距重采样 ✓（拖动事件本身不均匀 ✓）。
  const stamps = [points[0]];
  for (let i = 1; i < points.length; i++) {
    const from = points[i - 1];
    const to = points[i];
    const distance = Math.hypot(to.x - from.x, to.y - from.y);
    const steps = Math.max(1, Math.ceil(distance / spacing));
    for (let step = 1; step <= steps; step++) {
      stamps.push({
        x: from.x + ((to.x - from.x) * step) / steps,
        y: from.y + ((to.y - from.y) * step) / steps,
      });
    }
  }

  // 整笔的包围盒（合成一张图 ✓，只上传一次 ✓）。
  const half = size / 2;
  const minX = Math.floor(Math.min(...stamps.map((p) => p.x)) - half);
  const minY = Math.floor(Math.min(...stamps.map((p) => p.y)) - half);
  const maxX = Math.ceil(Math.max(...stamps.map((p) => p.x)) + half);
  const maxY = Math.ceil(Math.max(...stamps.map((p) => p.y)) + half);
  const width = Math.max(1, maxX - minX);
  const height = Math.max(1, maxY - minY);

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const paint = canvas.getContext("2d");
  // 画布上已有像素（供混色取色 ✓）：用内容画布当前内容作为底 ✓。
  const source = board.getContext("2d").getImageData(
    Math.max(0, Math.min(board.width - width, Math.round(minX - state.viewport.x))),
    Math.max(0, Math.min(board.height - height, Math.round(minY - state.viewport.y))),
    Math.min(width, board.width), Math.min(height, board.height),
  );
  const dabCanvas = document.createElement("canvas");
  dabCanvas.width = size;
  dabCanvas.height = size;
  const dabContext = dabCanvas.getContext("2d");
  let total = 0;
  let travelled = 0;
  for (let i = 0; i < stamps.length; i++) {
    const point = stamps[i];
    if (i > 0) {
      travelled += Math.hypot(point.x - stamps[i - 1].x, point.y - stamps[i - 1].y);
    }
    // 载墨：走满约 40 个笔尖直径就基本枯笔 ✓（`paint_load` 的直观类比 ✓）。
    const load = Math.max(0, 1 - travelled / (size * 40));
    if (load <= 0) break;
    // 目标色：从**内容画布**上取笔尖处的颜色 ✓（含已铺下的湿颜料 ✓）。
    const docX = Math.max(0, Math.min(board.width - 1, Math.round(point.x - state.viewport.x)));
    const docY = Math.max(0, Math.min(board.height - 1, Math.round(point.y - state.viewport.y)));
    const dest = board.getContext("2d").getImageData(docX, docY, 1, 1).data;
    if (typeof plugin.yanshi_input_ptr === "function") {
      const floats = plugin.yanshi_input_len() / 4;
      const input = new Float32Array(plugin.memory.buffer, plugin.yanshi_input_ptr(), Math.max(floats, 10));
      input.set([tip[0], tip[1], tip[2], 1, dest[0] / 255, dest[1] / 255, dest[2] / 255, dest[3] / 255, load, wetness], 0);
    }
    const written = plugin.yanshi_dab(Number($("strength").value) || 40, size, 1000);
    if (written === 0) break;
    const pixels = new Uint8ClampedArray(plugin.memory.buffer, plugin.yanshi_dab_ptr(), written);
    dabContext.putImageData(new ImageData(new Uint8ClampedArray(pixels), size, size), 0, 0);
    // 以 source-over 叠加 ✓ ⇒ 同一条笔触内的颜料会累积 ✓（油画堆料 ✓）。
    paint.drawImage(dabCanvas, point.x - half - minX, point.y - half - minY);
    total += 1;
  }
  void source;
  if (total === 0) {
    log("介质整笔：没有落笔（载墨为 0？）");
    return;
  }
  const rgba = new Uint8Array(paint.getImageData(0, 0, width, height).data.buffer);
  if (mediumBatchSession && mediumBatchSession.layerId === state.layerId) {
    // **批处理：把这一笔立刻画回画布** ✓（同一批里下一笔的取色就靠它 ✓），**先不提交** ✓。
    const context = board.getContext("2d");
    context.drawImage(canvas, Math.round(minX - state.viewport.x), Math.round(minY - state.viewport.y));
    const box = { x: minX, y: minY, w: width, h: height };
    const current = mediumBatchSession.box;
    mediumBatchSession.box = current
      ? { x: Math.min(current.x, box.x), y: Math.min(current.y, box.y),
          w: Math.max(current.x + current.w, box.x + box.w) - Math.min(current.x, box.x),
          h: Math.max(current.y + current.h, box.y + box.h) - Math.min(current.y, box.y) }
      : box;
    mediumBatchSession.stamps += total;
    mediumBatchSession.strokes += 1;
    return { batched: true, stamps: total };
  }
  return commitMediumBitmap(rgba, { x: minX, y: minY, w: width, h: height }, spec, total);
}

/// **把一批笔画成一次提交** ✓（见 `mediumBatchSession` 的说明 ✓）。
///
/// 整批画完之后 ✓：从画布上取**并集区域**的像素 ✓、**一次**上传 ✓、**一次** `import_image` ✓
/// ⇒ 提交次数从"每笔一次"降到"每批一次" ✓。
async function runMediumBatch(layerId, batch, options = {}) {
  if (!Array.isArray(batch) || batch.length === 0) return null;
  const session = { layerId, box: null, stamps: 0, strokes: 0 };
  const previous = mediumBatchSession;
  mediumBatchSession = session;
  try {
    for (const stroke of batch) {
      const points = (stroke.points || []).map((point) => ({ x: point.x, y: point.y }));
      if (points.length === 0) continue;
      await mediumStroke(stroke.medium || "oil", points, stroke.options || {});
    }
  } finally {
    mediumBatchSession = previous;
  }
  if (!session.box || session.stamps === 0) return null;
  // **一整块取出来** ✓（本批的每一笔都已经画在画布上了 ✓ ⇒ 这块像素就是最终画面 ✓）。
  const context = board.getContext("2d");
  const x = Math.max(0, Math.round(session.box.x - state.viewport.x));
  const y = Math.max(0, Math.round(session.box.y - state.viewport.y));
  const w = Math.max(1, Math.min(board.width - x, Math.round(session.box.w)));
  const h = Math.max(1, Math.min(board.height - y, Math.round(session.box.h)));
  const rgba = new Uint8Array(context.getImageData(x, y, w, h).data.buffer);
  const spec = options.spec || MEDIUMS[options.medium || "oil"];
  const result = await commitMediumBitmap(rgba, { x, y, w, h }, spec, session.stamps);
  window.yanshiStats.mediumBatch = {
    strokes: session.strokes, stamps: session.stamps, area: w * h,
  };
  return result;
}
window.yanshiMediumBatch = (layerId, batch, options) => runMediumBatch(layerId, batch, options);

/// 用**插件介质**在点击处落一个点 ✓：插件产出 RGBA → 上传 CAS → `import_image` →
/// 再用 `replace_object_data` 把介质描述符钉到对象上 ✓。
///
/// 为什么把插件输出**存成位图**（而不是让内核去调用插件）：内核自己就是 wasm 模块 ✓，
/// 无法实例化别的 wasm 模块 ✗；插件只能在宿主侧跑 ✓。而把输出写进 CAS ⇒
/// 像素**随日志固化** ✓ ⇒ "升级插件不改写旧文档渲染"这条不变量自然成立 ✓（历史可复现 ✓）。
async function mediumDab(name, point) {
  // **整段包住** ✓：此前的写法只把加载放进 try ✓，其余步骤一旦抛错，
  // `void mediumDab(...)` 会把异常变成**未捕获的 Promise 拒绝** ✗ ——
  // 既不进日志也不报错 ✓，于是"点击后毫无信号" ✗（上一轮就卡在这里 ✓）。
  try {
    await mediumDabInner(name, point);
  } catch (error) {
    const message = error && error.message ? error.message : String(error);
    window.yanshiStats.medium = Object.assign({}, window.yanshiStats.medium, {
      status: "error", error: message,
    });
    log("介质落笔失败：" + message, "#c33");
  }
}

/// **画谱回放** ✓ —— 用**真实介质**把一份"画谱"画出来 ✓。
///
/// **为什么要有它** ✓（这一条是用户要求"示例里能看到真实创作"之后补的 ✗）：
/// 介质插件是**浏览器里的 WASM** ✓，产物再作为 `import_image` 上传 ✓ ⇒
/// **服务端脚本根本画不出介质作品** ✗。而示例此前是当年临时造、**没进版本库**的 ✗
/// ⇒ 既不可复现 ✗、也没法评审改动 ✓。所以：把"怎么画"写成**画谱** ✓（进仓库 ✓、可 diff ✓），
/// 由查看器**走既有介质路径**回放 ✓ —— 这就是示例的生成方式 ✓，也是任何人都能复现的 ✓。
///
/// **画谱结构** ✓：
/// ```json
/// { "layers": [ { "id": "sky", "name": "天空", "strokes": [
///     { "medium": "oil", "color": "#8fb6d9", "size": 64, "wetness": 0.7,
///       "points": [[80, 120], [300, 110]] } ] } ] }
/// ```
/// `medium` 缺省时走**内置光栅笔刷** ✓（`appearance` 也能用 ✓）。
///
/// **取舍** ✓：逐笔**串行等待** ✓（介质一笔要跑 WASM + 上传 ✓）⇒ 画谱越大越慢 ✓，
/// 但这是**示例生成**不是交互路径 ✓ ⇒ 选**简单可控** ✓。回放期间会写进度日志 ✓。
async function applyScore(score) {
  const started = Date.now();
  let layersDone = 0;
  let strokesDone = 0;
  for (const layer of score.layers || []) {
    // 图层不存在就建 ✓（画谱可以只声明要用的图层 ✓）。
    const existing = await listLayers();
    if (!existing.some((item) => item.layer_id === layer.id)) {
      const created = await callTool("create_layer", { layer_id: layer.id, name: layer.name || layer.id },
        { refresh: false });
      if (!created.ok) {
        log("画谱：" + (layer.id || "?") + " 建层失败 " + (created.error_code || ""), "#c33");
        continue;
      }
    }
    const select = $("layer");
    if (select) { select.value = layer.id; select.onchange(); }
    state.layerId = layer.id;
    await refreshLayers();
    // **把介质笔画积攒成批** ✓（见 `mediumBatchSession` 的说明 ✓）：
    // 一次提交固定约 280 ms ✓ ⇒ 每 30 笔提交一次 ✓ 而不是每笔一次 ✓（全尺寸约省 20 分钟 ✓）。
    // **顺序必须保住** ✗：遇到普通笔迹（`draw_stroke` ✓）之前**先把积攒的介质冲刷掉** ✓，
    // 否则"先介质后普通"会被改成"先普通后介质" ✓ ⇒ 叠放次序变了、画也就变了 ✗。
    let pendingMedium = [];
    const flushMedium = async () => {
      if (pendingMedium.length === 0) return;
      const batch = pendingMedium;
      pendingMedium = [];
      if (typeof window.yanshiMediumBatch === "function") {
        await window.yanshiMediumBatch(layer.id, batch, {});
        return;
      }
      // 没有批量能力（旧页面 ✓）就逐笔来 ✓ —— 退回原路 ✓，不静默丢笔 ✗。
      for (const item of batch) {
        await mediumStroke(item.medium, item.points, item.options);
      }
    };
    for (const stroke of layer.strokes || []) {
      const points = (stroke.points || []).map((point) =>
        Array.isArray(point) ? { x: point[0], y: point[1] } : { x: point.x, y: point.y });
      if (points.length === 0) continue;
      if (stroke.medium) {
        pendingMedium.push({
          medium: stroke.medium, points,
          options: { color: stroke.color, size: stroke.size, wetness: stroke.wetness },
        });
        if (pendingMedium.length >= 30) await flushMedium();
      } else {
        await flushMedium();
        const drawn = await callTool("draw_stroke", {
          layer_id: layer.id,
          data: Object.assign(
            { points: points.map((point) => [point.x, point.y]) },
            stroke.data || {},
            stroke.color ? { color: hexToUnit(stroke.color) } : {},
          ),
        }, { refresh: false });
        if (!drawn.ok) log("画谱：落笔失败 " + (drawn.error_code || ""), "#c33");
      }
      strokesDone += 1;
      if (strokesDone % 10 === 0) {
        log("画谱进度：" + strokesDone + " 笔");
        await new Promise((resolve) => setTimeout(resolve, 0));
      }
    }
    // 这一层画完 ⇒ 冲刷剩余 ✓（顺序上它就是"最后几笔" ✓）。
    await flushMedium();
    layersDone += 1;
  }
  // 回放完让服务端权威状态接管画布 ✓（介质是 heavy 内容 ✓ ⇒ 必须走这条路 ✓）。
  await resync();
  await refreshPreview();
  const summary = { layers: layersDone, strokes: strokesDone, ms: Date.now() - started };
  log("画谱完成：图层 " + summary.layers + " 个、落笔 " + summary.strokes + " 笔、用时 " +
      Math.round(summary.ms / 1000) + " 秒");
  return summary;
}
window.yanshiApplyScore = applyScore;
window.yanshiListLayers = listLayers;

async function mediumDabInner(name, point) {
  let spec;
  try {
    spec = await loadMedium(name);
  } catch (error) {
    log("介质加载失败：" + (error && error.message ? error.message : error), "#c33");
    return;
  }
  const size = mediumTipSize(spec);
  // 注意命名 ✓：**不要**叫 `api` ✗ —— 查看器自己有一个 `api(path)` 的 URL 助手 ✓，
  // 同名局部变量会把它遮蔽 ✓，于是后面 `fetch(api("/api/blob"))` 会调到一个对象上 ✗
  //（实测报 "api is not a function" ✓ —— 这一条是靠 yanshiStats.medium 的可观测信号才立刻定位的 ✓）。
  const plugin = spec.instance.exports;
  // v2：把上下文写进插件的输入缓冲 ✓ —— 笔尖色、目标处已有色、载墨、湿度 ✓。
  // 目标色取**笔尖处画面的当前颜色** ✓（宿主能读画布 ✓，插件读不到 ✗）。
  if (typeof plugin.yanshi_input_ptr === "function") {
    const floats = plugin.yanshi_input_len() / 4;
    const input = new Float32Array(plugin.memory.buffer, plugin.yanshi_input_ptr(), Math.max(floats, 10));
    const tip = hexToUnit($("color").value);
    const board = document.getElementById("board");
    const rect = board.getBoundingClientRect();
    const scale = board.width / Math.max(1, rect.width);
    const px = Math.min(board.width - 1, Math.max(0, Math.round(point.x * scale)));
    const py = Math.min(board.height - 1, Math.max(0, Math.round(point.y * scale)));
    const dest = board.getContext("2d").getImageData(px, py, 1, 1).data;
    const wetness = (Number($("strength").value) || 40) / 100;
    input.set([tip[0], tip[1], tip[2], 1, dest[0] / 255, dest[1] / 255, dest[2] / 255, dest[3] / 255,
               1.0, wetness], 0);
  }
  const written = plugin.yanshi_dab(Number($("strength").value) || 40, size, 1000);
  const pixels = new Uint8ClampedArray(plugin.memory.buffer, plugin.yanshi_dab_ptr(), written);
  const image = new ImageData(new Uint8ClampedArray(pixels), size, size);
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  canvas.getContext("2d").putImageData(image, 0, 0);
  const rgba = new Uint8Array(canvas.getContext("2d").getImageData(0, 0, size, size).data.buffer);

  return commitMediumBitmap(rgba, { x, y, w: size, h: size }, spec, 1);
}

/// 介质产出的位图入库 ✓：上传 CAS → 建层 → `import_image` → 把 `{id, version}` 钉到对象数据上 ✓。
async function commitMediumBitmap(rgba, region, spec, stamps) {
  const upload = await fetch(api("/api/blob"), {
    method: "POST",
    headers: { "content-type": "image/x-yanshi-raw" },
    body: rgba,
  }).then((response) => response.json()).catch(() => ({ ok: false, error_code: "network" }));
  if (!upload.ok) {
    log("介质上传失败：" + (upload.error_code || "unknown"), "#c33");
    return;
  }
  // **画进"当前选中的图层"** ✓ —— 此前每落一笔都新建 `medium_<ulid>` ✗
  // ⇒ 子 agent 画 37/49 笔就得到 37/49 个图层 ✓，`state.layerId` 还被悄悄改走 ✓，
  // 于是"2–6 个图层"这种正常用法**根本做不到** ✓，而且笔迹散落在几十个层里 ✓。
  // 只在**选中的图层不存在**时兜底新建一个 ✓（例如它已被删除 ✓）。
  let layerId = state.layerId || "layer_paint";
  const existing = await callTool("list_layers", {}, { refresh: false }).catch(() => ({}));
  const known = (existing.layers || []).some((layer) => layer.layer_id === layerId);
  if (!known) {
    const fallback = "medium_" + ulid();
    const created = await callTool("create_layer", { layer_id: fallback, name: spec.id }, { refresh: false });
    if (!created.ok) {
      log("介质落笔失败（新建图层）：" + (created.error_code || "unknown"), "#c33");
      return;
    }
    layerId = fallback;
    state.layerId = fallback;
  }
  const objectId = "dab_" + ulid();
  const bitmap = { blob_hash: upload.blob_hash, size: upload.size, mime_type: "image/x-yanshi-raw" };
  // **一条原子说清一件事** ✓：介质描述符 `{id, version}` 随导入一起记下 ✓（设计 11.1 ✓）。
  //
  // 此前这里是**两次**提交 ✗（导入 ✓ + `replace_object_data` 把描述符钉上去 ✓）✓。
  // 实测每次原子提交都有实打实的成本（无内核实例、端到端 ✓）⇒ 合并成一次 ✓ 直接省下一笔 ✓，
  // 而"描述符随原子记录"这个设计要求**一字不动**地满足 ✓（甚至更直白：就在这条原子上 ✓）。
  const imported = await callTool("import_image", {
    layer_id: layerId, object_id: objectId, bitmap, region,
    medium: { id: spec.id, version: spec.version },
  }, { refresh: false });
  if (!imported.ok) {
    log("介质落笔失败：" + (imported.error_code || "unknown") + " " +
        ((imported.context && imported.context.detail) || ""), "#c33");
    return;
  }
  window.yanshiStats.medium = Object.assign({}, window.yanshiStats.medium, {
    status: "dabbed", size: region.w, objectId, layerId, stamps,
  });
  // **立刻只补"这一笔"的区域** ✓ —— 子 agent 报的 F4：每次介质落笔后约 **1 秒白闪** ✗
  //（60ms 采样 40 帧里有 15 帧纯白 ✓）。白闪的成因是"内核把这一块重绘成空白 ✓，
  // 然后要等一次**全视口**补画（`blitServerViewport` ✓，走 WS 的 heavy 分支 ✓）才恢复" ✓。
  // 这里在**提交成功后立刻**按 `region`（正是这个补丁的范围 ✓）补一次 ✓
  // ⇒ 内容**马上出现** ✓，而且只传这一块的字节 ✓（全量补画仍会随后发生 ✓，作为兜底 ✓）。
  // 这是**加法** ✓：不改变任何既有路径 ✓，只是让画面更早正确 ✓。
  if (region && region.w > 0 && region.h > 0) {
    void blitServerBox([region.x, region.y, region.w, region.h]).then(() => {
      window.yanshiStats.mediumEarlyBlits = (window.yanshiStats.mediumEarlyBlits || 0) + 1;
      window.yanshiStats.lastEarlyBlitArea = region.w * region.h;
    });
  }
  log("已用介质「" + spec.id + " v" + spec.version + "」落笔（" + region.w + "×" + region.h + "，" + stamps + " 个点）");
  await refreshLayers();
  $("layer").value = layerId;
  state.layerId = layerId;
  await refreshEffects();
  await refreshPreview();
}


/// 导入本地图片：浏览器解码 → 原始 RGBA → 上传（`POST /api/blob`）→ `import_image`。
///
/// **示例随应用发布** ✓ —— 任何机器第一次打开示例时，把仓库里带的**画面**导入 ✓。
///
/// **为什么需要它** ✓（用户实测 ✓）：此前示例只存在于**开发机的工作区**里 ✗
/// ⇒ 换一台机器点"示例" ⇒ `switchDocument` 只是**新建了一个空文档** ✗
/// ⇒ 用户看到的是**空白画布** ✓（"示例都是空白的" ✓）。
/// 介质像素是浏览器里跑 WASM 得来的 ✓，没法随仓库以"笔"的形式瞬间重现 ✓
/// ⇒ 因此把**画面**随仓库发（`assets/samples/*.png` ✓），
/// 由查看器用**既有的导入路径**（`import_image` + blob 先行 ✓）搬进来 ✓。
///
/// **绝不覆盖** ✓：文档里已有对象就什么都不做 ✓（用户改过的示例永远是他的 ✓）。
async function seedSampleIfEmpty(docId) {
  if (!SAMPLES.some((sample) => sample.id === docId)) return false;
  const listed = await callTool("list_objects", {}, { refresh: false }).catch(() => ({}));
  if ((listed.objects || []).length > 0) return false;
  let response;
  try {
    response = await fetch("/samples/" + encodeURIComponent(docId) + ".png");
  } catch (_) {
    return false;
  }
  if (!response.ok) return false;
  const bitmap = await createImageBitmap(await response.blob());
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  canvas.getContext("2d").drawImage(bitmap, 0, 0);
  const bytes = new Uint8Array(canvas.getContext("2d").getImageData(0, 0, bitmap.width, bitmap.height).data.buffer);
  const upload = await fetch(api("/api/blob"), {
    method: "POST",
    headers: { "content-type": "image/x-yanshi-raw" },
    body: bytes,
  }).then((value) => value.json()).catch(() => ({}));
  if (!upload.ok) {
    log("示例画面上传失败：" + (upload.error_code || "unknown"), "#c33");
    return false;
  }
  const layerId = "artwork";
  const created = await callTool("create_layer", { layer_id: layerId, name: "作品" }, { refresh: false });
  if (!created.ok && created.error_code !== "already_exists") {
    log("示例画面建层失败：" + (created.error_code || "unknown"), "#c33");
    return false;
  }
  const imported = await callTool("import_image", {
    layer_id: layerId,
    bitmap: { blob_hash: upload.blob_hash, size: upload.size, mime_type: "image/x-yanshi-raw" },
    region: { x: 0, y: 0, w: bitmap.width, h: bitmap.height },
  }, { refresh: false });
  if (!imported.ok) {
    log("示例画面导入失败：" + (imported.error_code || "unknown"), "#c33");
    return false;
  }
  log("示例画面已随应用导入（" + bitmap.width + "×" + bitmap.height + "）");
  return true;
}

/// 走**设计规定的** `import_image`（10.2 导入组）+ 6.3 的「blob 先行」✓。
/// 用原始像素（`image/x-yanshi-raw`）而不是原文件格式：内核的 RasterPatch 读的就是原始像素，
/// 浏览器负责解码（PNG/JPEG/WebP 都能解），服务端因此不需要图像解码器 ✓。
async function importLocalImage(file) {
  const bitmap = await createImageBitmap(file);
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const context = canvas.getContext("2d");
  context.drawImage(bitmap, 0, 0);
  const imageData = context.getImageData(0, 0, bitmap.width, bitmap.height);
  const bytes = new Uint8Array(imageData.data.buffer);

  const upload = await fetch(api("/api/blob"), {
    method: "POST",
    headers: { "content-type": "image/x-yanshi-raw" },
    body: bytes,
  }).then((response) => response.json());
  if (!upload.ok) {
    log("导入失败（上传）：" + (upload.error_code || upload.context?.detail || "unknown"), "#c33");
    return;
  }

  const layerId = "import_" + ulid();
  const created = await callTool("create_layer", { layer_id: layerId, name: file.name || "import" }, { refresh: false });
  if (!created.ok) {
    log("导入失败（新建图层）：" + (created.error_code || "unknown"), "#c33");
    return;
  }
  const imported = await callTool("import_image", {
    layer_id: layerId,
    bitmap: { blob_hash: upload.blob_hash, size: upload.size, mime_type: "image/x-yanshi-raw" },
    region: { x: 0, y: 0, w: bitmap.width, h: bitmap.height },
  }, { refresh: false });
  if (!imported.ok) {
    log("导入失败：" + (imported.error_code || "unknown") + " " +
        ((imported.context && imported.context.detail) || ""), "#c33");
    return;
  }
  log("已导入 " + file.name + "（" + bitmap.width + "×" + bitmap.height + "）到图层 " + layerId);
  await refreshLayers();
  $("layer").value = layerId;
  state.layerId = layerId;
  await refreshEffects();
  await refreshPreview();
}

/// 打开文档：提示输入文档 id（本地工具，缺省空即用当前）。
async function promptDocument() {
  const input = window.prompt("要打开的文档 id（不存在则新建）：", state.docId || "default");
  if (input === null) return;
  const docId = input.trim();
  if (!docId) return;
  await switchDocument(docId);
}

/// 切换文档：关闭旧连接、清空日志与本地状态，再走一遍打开流程。
///
/// 此前「打开 / 新建文档」按钮只是用**当前** doc_id 再调一次 `/api/documents`，
/// 因此点了等于没点（用户报告「点击后没有打开或者创建新的功能」）。
async function switchDocument(docId, token) {
  if (state.socket) {
    const previous = state.socket;
    state.socket = null; // 先置空，onclose 便不会重连
    try { previous.close(); } catch (_) { /* 已关闭 */ }
  }
  window.yanshiKernelReady = false;
  state.docId = docId;
  // 显式传入的令牌优先（「另存为副本」刚创建文档时已经拿到令牌，避免再建一次文档）；
  // 否则交给 ensureDocument 去创建/打开并取回令牌。
  state.token = token || "";
  state.localSeq = 0;
  state.undoStack = [];
  state.redoStack = [];
  updateUndoStatus();
  state.dragging = null;
  state.points = [];
  $("log").innerHTML = "";
  $("last").textContent = "";
  // 画布立刻清空，避免切换期间仍显示上一个文档的内容。
  state.zoom = 1;
  state.displayScale = null;
  state.viewport = { x: 0, y: 0, w: 1024, h: 1024 };
  sizeBoards(1024, 1024);
  if (state.token) {
    await loadDocumentData();
  } else {
    await ensureDocument();
  }
}

async function ensureDocument() {
  const response = await fetch("/api/documents", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: state.docId, width: 1024, height: 1024, actor: "human:web" }),
  });
  const value = await response.json();
  if (!value.ok) { log("打开文档失败：" + JSON.stringify(value), "#c33"); return; }
  state.token = value.token;
  state.docId = value.doc_id;
  await loadDocumentData();
}

/// 拿到令牌之后的统一装载流程（`ensureDocument` 与「另存为副本」共用 ✓）。
async function loadDocumentData() {
  const url = new URL(location.href);
  url.searchParams.set("doc", state.docId);
  url.searchParams.set("token", state.token);
  history.replaceState(null, "", url);
  $("identity").textContent = state.docId + " · " + state.token.slice(0, 8) + "…";
  await refreshLayers();
  await refreshThumb();
  // 6.2「打开即图片」：先用服务端渲染（含 HEAD 渲染缓存）出像素，
  // WASM 内核在后台预热，就绪后再换成客户端渲染——首帧因此不等待内核折叠。
  const bootStarted = performance.now();
  window.yanshiStats.bootAt = bootStarted;
  await refreshPreview();
  const bootFirstPaint = $("firstPaint");
  if (bootFirstPaint) bootFirstPaint.textContent = "…";
  connect();
  refreshContactLink();
  void warmKernel();
}

// 后台预热 WASM 内核：装载原子并切换为客户端渲染；失败则保持服务端渲染。
async function warmKernel() {
  const started = performance.now();
  await initWasm();
  if (!state.wasm) return;
  const ok = await loadKernel(0);
  window.yanshiStats.kernelWarmMs = performance.now() - started;
  const warm = $("kernelWarm");
  if (warm) warm.textContent = window.yanshiStats.kernelWarmMs.toFixed(0) + "ms";
  if (ok && kernelReady()) {
    // 注意：`refreshPreview(true)` 表示「从服务端取像素」，这里要的是内核路径。
    await refreshPreview();
  }
}

/// 撤销一次（可连续）。栈空时给出明确提示，而不是静默无反应。
async function undoOnce() {
  const entry = state.undoStack.pop();
  if (!entry) {
    log("没有可撤销的操作");
    updateUndoStatus();
    return;
  }
  const id = typeof entry === "string" ? entry : entry.id;
  const tool = typeof entry === "string" || entry.kind === "atom" ? "revert" : "revert_to";
  const value = await callTool(tool, { atom_id: id }, { refresh: false });
  if (!value.ok) {
    // 撤销失败：把条目放回去，保持栈与实际状态一致。
    state.undoStack.push(entry);
    log("撤销失败：" + (value.error_code || "unknown") + " " +
        ((value.context && value.context.detail) || ""), "#c33");
  } else {
    state.redoStack.push(entry);
  }
  updateUndoStatus();
  await refreshPreview();
}

/// 重做一次（按原始顺序：最近一次被撤销的最先重做）。
async function redoOnce() {
  const entry = state.redoStack.pop();
  if (!entry) {
    log("没有可重做的操作");
    updateUndoStatus();
    return;
  }
  const id = typeof entry === "string" ? entry : entry.id;
  // 跳转类条目用 revert_to 重做（再跳回那个头部），普通原子用 reapply。
  const tool = typeof entry === "string" || entry.kind === "atom" ? "reapply" : "revert_to";
  const value = await callTool(tool, { atom_id: id }, { refresh: false });
  if (!value.ok) {
    state.redoStack.push(entry);
    log("重做失败：" + (value.error_code || "unknown"), "#c33");
  } else {
    state.undoStack.push(entry);
  }
  updateUndoStatus();
  await refreshPreview();
}

/// 在状态栏显示撤销/重做深度，并同步按钮可用性。
function updateUndoStatus() {
  const label = $("undoDepth");
  if (label) {
    label.textContent = "撤销 " + state.undoStack.length + " / 重做 " + state.redoStack.length;
  }
  const undo = document.querySelector('button[data-tool="undo"]');
  const redo = document.querySelector('button[data-tool="redo"]');
  if (undo) undo.disabled = state.undoStack.length === 0;
  if (redo) redo.disabled = state.redoStack.length === 0;
}

/// 效果目录来自服务端 `/api/effects`（内容就是内核的 `ADJUSTMENT_NAMES` / `FILTER_NAMES`），
/// 因此查看器**不硬编码效果名** ✓，也就不会与内核漂移 ✓。
let effectCatalog = { adjustment: [], filter: [] };

async function loadEffectCatalog() {
  const value = await fetch(api("/api/effects"), {
    method: "GET",
    headers: { "content-type": "application/json" },
  }).then((response) => response.json());
  effectCatalog = { adjustment: value.adjustments || [], filter: value.filters || [] };
  fillEffectNames();
}

/// 按当前「调整 / 滤镜」选择填充效果下拉。
function fillEffectNames() {
  const kind = $("effectKind").value;
  const select = $("effectName");
  const previous = select.value;
  select.innerHTML = "";
  for (const name of effectCatalog[kind] || []) select.appendChild(new Option(name, name));
  if (previous && (effectCatalog[kind] || []).includes(previous)) select.value = previous;
}

/// 应用当前效果到选中图层。
///
/// 参数缺省为 `{}` —— **不复制内核的默认值** ✓：缺参时由内核自己决定默认 ✓，
/// 应用后再用 `list_effects` 读回**实际生效的参数**展示，做到零漂移。
async function applyEffect() {
  const kind = $("effectKind").value;
  const name = $("effectName").value;
  if (!name) return;
  let params = {};
  const text = $("effectParams").value.trim();
  if (text) {
    try {
      params = JSON.parse(text);
    } catch (error) {
      log("参数不是合法 JSON：" + error.message, "#c33");
      return;
    }
  }
  const tool = kind === "adjustment" ? "add_adjustment" : "add_filter";
  const args =
    kind === "adjustment"
      ? { layer_id: state.layerId, adjustment_type: name, params }
      : { layer_id: state.layerId, filter_name: name, params };
  const value = await callTool(tool, args, { refresh: false });
  if (!value.ok) {
    log("应用失败：" + (value.error_code || "unknown") + " " +
        ((value.context && value.context.detail) || ""), "#c33");
    return;
  }
  log("已应用" + (kind === "adjustment" ? "调整" : "滤镜") + " " + name);
  await refreshEffects();
  await refreshPreview();
}

/// 列出当前文档的调整/滤镜对象（含**实际生效的参数**与顺序）。
async function refreshEffects() {
  const value = await callTool("list_effects", {}, { refresh: false });
  const list = $("effectsList");
  if (!value.ok) {
    list.textContent = "读取失败：" + (value.error_code || "unknown");
    return;
  }
  const effects = value.effects || [];
  list.innerHTML = "";
  if (effects.length === 0) {
    list.textContent = "（当前文档没有调整/滤镜）";
    return;
  }
  for (const effect of effects) {
    const row = document.createElement("div");
    // 字段名以 `list_effects` 的响应为准：adjustment_type / filter_name / params / layer_id。
    const name = effect.adjustment_type || effect.filter_name || effect.name || "?";
    row.textContent =
      name + " " + JSON.stringify(effect.params || {}) +
      " @" + (effect.layer_id || "-");
    list.appendChild(row);
  }
}

/// 历史浏览（设计 13.2）：数据源是原子日志，支持按原子步进、按 actor / 类型筛选。
/// 工具层已有 `get_log`（`since_seq` / `limit` / `kind` / `actor`），这里只做界面。
async function refreshHistory() {
  const kind = $("historyKind").value;
  const actor = $("historyActor").value;
  const args = { limit: 200 };
  if (kind) args.kind = kind;
  if (actor) args.actor = actor;
  const value = await callTool("get_log", args, { refresh: false });
  const list = $("history");
  if (!value.ok) {
    list.textContent = "读取失败：" + (value.error_code || "unknown");
    return;
  }
  const atoms = value.atoms || [];
  historyAtoms = atoms;
  list.innerHTML = "";
  for (const atom of atoms) {
    const row = document.createElement("div");
    row.className = "row";
    const seq = document.createElement("span");
    seq.className = "seq";
    seq.textContent = "#" + atom.seq;
    const kindLabel = document.createElement("span");
    kindLabel.className = "kind";
    kindLabel.textContent = atom.kind;
    const actorLabel = document.createElement("span");
    actorLabel.className = "actor";
    actorLabel.textContent = atom.actor;
    const jump = document.createElement("button");
    jump.textContent = "回到此处";
    jump.addEventListener("click", async () => {
      // `revert_to` 通过 declare_head 回到该时刻；它本身也是一个原子，所以可被撤销。
      const result = await callTool("revert_to", { atom_id: atom.atom_id }, { refresh: false });
      if (!result.ok) {
        log("回到此处失败：" + (result.error_code || "unknown"), "#c33");
        return;
      }
      // 「回到此处」= 一条 declare_head 原子。rewind 之前的原子已不可撤销
      // （服务端会正确地拒绝：目标原子位于当前求值起点之前 ✓）。
      // 因此「撤销这次跳转」的实现是**再跳回跳转前的那个原子** ✓：
      // 记住跳转前的头部 id，把它作为撤销条目（kind=head）。
      const headBefore = currentHeadAtomId();
      state.undoStack = headBefore ? [{ kind: "head", id: headBefore }] : [];
      state.redoStack = [];
      updateUndoStatus();
      log("已回到 #" + atom.seq + "（可点撤销回到跳转前）");
      await refreshPreview();
      await refreshHistory();
    });
    row.append(seq, kindLabel, actorLabel, jump);
    list.appendChild(row);
  }
  if (atoms.length === 0) list.textContent = "（没有匹配的原子）";
  fillHistoryFilters(atoms);
}

/// 当前头部原子 id（历史列表里 seq 最大的那条）。用于「撤销一次跳转 = 跳回跳转前的头部」。
function currentHeadAtomId() {
  let best = null;
  for (const atom of historyAtoms) {
    if (!best || (atom.seq || 0) > (best.seq || 0)) best = atom;
  }
  return best ? best.atom_id : null;
}

/// 用当前列表填充筛选下拉（保留已有选项，避免每次重建导致选择丢失）。
let historyAtoms = [];
function fillHistoryFilters(atoms) {
  for (const [id, key] of [["historyKind", "kind"], ["historyActor", "actor"]]) {
    const select = $(id);
    const current = select.value;
    const values = new Set(Array.from(select.options).map((option) => option.value).filter(Boolean));
    for (const atom of atoms) values.add(String(atom[key]));
    const wanted = Array.from(values).sort();
    if (wanted.length + 1 !== select.options.length) {
      select.innerHTML = "";
      select.appendChild(new Option("全部" + (key === "kind" ? "类型" : "操作者"), ""));
      for (const value of wanted) select.appendChild(new Option(value, value));
      select.value = current;
    }
  }
}

async function refreshLayers() {
  const value = await fetch(api("/api/tools/list_layers"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: "{}",
  }).then((r) => r.json());
  const select = $("layer");
  select.innerHTML = "";
  for (const layer of value.layers || []) {
    const option = document.createElement("option");
    option.value = layer.layer_id;
    option.textContent = layer.name + " (#" + layer.layer_id + ")";
    select.appendChild(option);
  }
  if (!value.layers || value.layers.length === 0) {
    const created = await callTool("create_layer", { name: "paint", layer_id: "layer_paint" }, { refresh: false });
    if (created.ok) await refreshLayers();
  }
  // **刷新时保持选中项** ✓ —— 这是检查脚本当场逼出来的 bug ✗：
  // 重建 `<option>` 之后 `select.value` 会落到**第一项** ✓，而下面又把
  // `state.layerId` 赋成 `select.value` ✗ ⇒ 每次刷新（改名、隐藏、排序、复制…）
  // **选中图层都被悄悄换掉** ✓，表现为"点一下眼睛，正在编辑的图层就跳走了" ✗。
  // 做法 ✓：先记住 `state.layerId` ✓，重建后若它仍存在就选回它 ✓（不存在才退回第一项 ✓）。
  const wanted = state.layerId;
  if (wanted && [...select.options].some((option) => option.value === wanted)) {
    select.value = wanted;
  }
  state.layerId = select.value || "layer_paint";
  select.onchange = () => { state.layerId = select.value; renderLayerPanel(value.layers || []); };
  renderLayerPanel(value.layers || []);
  return value.layers || [];
}

// **图层面板渲染** ✓（用户点名的功能 ✓）。
//
// 三条取舍 ✓：
// ① **数据源只有一个** ✓：面板渲染的是 `list_layers` 的结果 ✓，选择写回隐藏的 `#layer` ✓
//    与 `state.layerId` ✓ —— 别处读的仍是同一份 ✓（各维护一份选择必然漂移 ✗）。
// ② **显示顺序与图层序相反** ✓：`list_layers` 是**自下而上** ✓（与渲染顺序一致 ✓），
//    而面板按惯例**最上层在最上面** ✓ ⇒ 渲染时 `slice().reverse()` ✓。
//    上下移动按钮因此也要按"屏幕方向"换算回 z 序 ✓（这里是**最容易搞反**的地方 ✗）。
// ③ **按钮只接线一次** ✓（在 `setupLayerPanel` 里 ✓）：`refreshLayers()` 每次都会重画列表 ✓，
//    若把监听器写在重画里 ✓ ⇒ 点一次会触发多次 ✗（本项目的检查脚本抓到过同类问题 ✓）。
function renderLayerPanel(layers) {
  const list = $("layerList");
  if (!list) return;
  list.innerHTML = "";
  for (const layer of layers.slice().reverse()) {
    const row = document.createElement("div");
    row.className = "layer-row" + (layer.layer_id === state.layerId ? " selected" : "");
    row.dataset.layerId = layer.layer_id;
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(layer.layer_id === state.layerId));
    const eye = document.createElement("button");
    eye.type = "button";
    eye.className = "layer-flag" + (layer.visible ? "" : " off");
    eye.textContent = layer.visible ? "👁" : "🚫";
    eye.title = layer.visible ? "隐藏图层" : "显示图层";
    eye.dataset.action = "visible";
    const lock = document.createElement("button");
    lock.type = "button";
    lock.className = "layer-flag" + (layer.locked ? "" : " off");
    lock.textContent = layer.locked ? "🔒" : "🔓";
    lock.title = layer.locked ? "解锁图层" : "锁定图层（锁定后不能改内容）";
    lock.dataset.action = "locked";
    const name = document.createElement("span");
    name.className = "layer-name";
    name.textContent = layer.name + (layer.medium ? " · " + layer.medium : "");
    name.title = layer.layer_id;
    row.append(eye, lock, name);
    list.appendChild(row);
  }
}

// 面板按钮**只接一次线** ✓（见上面第 ③ 条 ✓）。
async function setupLayerPanel() {
  const list = $("layerList");
  if (!list) return;
  list.addEventListener("click", async (event) => {
    const row = event.target.closest(".layer-row");
    if (!row) return;
    const layerId = row.dataset.layerId;
    const action = event.target.dataset ? event.target.dataset.action : null;
    if (action === "visible" || action === "locked") {
      // **显示/隐藏与锁定走 `update_layer`** ✓（服务端已有该工具 ✓，本轮给它补了**强制** ✓）。
      const layers = await listLayers();
      const layer = layers.find((item) => item.layer_id === layerId);
      if (!layer) return;
      const patch = action === "visible" ? { visible: !layer.visible } : { locked: !layer.locked };
      await callTool("update_layer", { layer_id: layerId, patch });
      await refreshLayers();
      // **画布要走服务端权威路径** ✓：`afterMutation` 只刷缩略图与历史 ✓，不重画画布 ✗，
      // 而"图层可见性"这类属性在 WASM 内核里不一定被实现 ✓ ⇒ 只刷内核会出现
      // "隐藏了但画面还在 / 显示回来画面仍然是空的" ✗（检查脚本实测：10717 → 0 → **0** ✗）。
      await resync();
      return;
    }
    // 点名字/行 = **选中** ✓（写回 `#layer` 与 `state.layerId` ✓ ⇒ 全查看器跟着切换 ✓）。
    const select = $("layer");
    if (select) { select.value = layerId; select.onchange(); }
  });
  // **上下移动** ✓：屏幕向上 = z 序 +1 ✓（`list_layers` 是自下而上 ✓）。
  const move = async (delta) => {
    const layers = await listLayers();
    const order = layers.map((layer) => layer.layer_id); // 自下而上 ✓
    const index = order.indexOf(state.layerId);
    if (index < 0) return;
    const target = index + delta;
    if (target < 0 || target >= order.length) return;
    [order[index], order[target]] = [order[target], order[index]];
    await callTool("reorder_layers", { order });
    await refreshLayers();
    await resync();
  };
  const add = $("layerAdd");
  if (add) add.onclick = async () => {
    const layers = await listLayers();
    await callTool("create_layer", { name: "图层 " + (layers.length + 1) });
    await refreshLayers();
    await resync();
  };
  const duplicate = $("layerDuplicate");
  if (duplicate) duplicate.onclick = async () => {
    const result = await callTool("duplicate_layer", { layer_id: state.layerId });
    await resync();
    if (result && result.ok && result.layer_id) {
      // **复制之后选中副本** ✓ —— 用户复制图层的下一步几乎总是要动它 ✓。
      const select = $("layer");
      await refreshLayers();
      if (select) { select.value = result.layer_id; select.onchange(); }
    } else {
      await refreshLayers();
    }
  };
  const remove = $("layerDelete");
  if (remove) remove.onclick = async () => {
    await callTool("delete_layer", { layer_id: state.layerId });
    await refreshLayers();
    await resync();
  };
  const up = $("layerUp");
  if (up) up.onclick = () => move(1);
  const down = $("layerDown");
  if (down) down.onclick = () => move(-1);
}

/// 读一次图层列表 ✓（面板与移动都用它 ✓，避免各自解析响应 ✗）。
async function listLayers() {
  const value = await fetch(api("/api/tools/list_layers"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: "{}",
  }).then((r) => r.json()).catch(() => ({}));
  return value.layers || [];
}

async function refreshPreview(fromKernel = false) {
  // 渲染**整幅文档**（不是固定的 512×512 区域），否则大画布会被裁掉。
  const { w, h } = state.docSize;
  if (!fromKernel && kernelReady()) {
    // 本地乐观路径：直接由 WASM 内核出像素，不等服务端。
    state.docSize = { w, h };
    clampViewport();
    renderViewport();
    return;
  }
  // 只有在**没有内核**时才用服务端像素兜底（有内核时内核是主画布的唯一权威来源）。
  const value = await callTool("render_region", {
    region: { x: 0, y: 0, w, h },
  }, { refresh: false });
  if (value.thumb_url) {
    preview.style.visibility = "visible";
    preview.onload = () => {
      // 6.2「打开即图片」：服务端铺底像素**解码完成**才算首帧。
      if (window.yanshiStats.firstPaintMs === null && window.yanshiStats.bootAt) {
        window.yanshiStats.firstPaintMs = performance.now() - window.yanshiStats.bootAt;
        const firstPaint = $("firstPaint");
        if (firstPaint) firstPaint.textContent = window.yanshiStats.firstPaintMs.toFixed(0) + "ms";
      }
      // 统一走 sizeBoards：它会按文档背景铺底。直接改 board.width 会把画布清成**透明**
      // （配合图片加载失败/竞态就表现为「画布空白」）。
      state.docSize = { w: preview.naturalWidth, h: preview.naturalHeight };
      state.zoom = 1;
      clampViewport();
      sizeBoards(state.viewport.w, state.viewport.h);
      setStatus({});
      if (state.socket && state.socket.readyState === 1) subscribeViewport();
      redraw();
      // **无内核时把服务端整幅渲染真正画到画布上** ✓ —— `redraw()` 走的是"内核补丁"那条路 ✓，
      // 没有内核时它什么也画不出来 ✗（实测：`preview` 加载成功、画布仍是 0 墨 ✓）。
      // `blitServerViewport()` 是**自洽**的（自己取服务端像素并画进画布 ✓）⇒ 直接用它 ✓。
      if (!state.wasm) {
        needsServerPixels = true;
        queueServerBlit();
      }
    };
    preview.src = value.thumb_url + "&t=" + Date.now();
    setStatus({ rendered: value.head_seq !== undefined ? value.head_seq : undefined, dirty: 0 });
  }
}

// 反馈邮件里预填文档 id 与 HEAD，便于定位问题（不包含任何画布内容）。
function refreshContactLink() {
  const link = $("contact");
  if (!link) return;
  const subject = encodeURIComponent(`[Yanshi] ${state.docId || "document"}`);
  const body = encodeURIComponent(
    `\n\n---\n文档: ${state.docId || "-"}\n本地 HEAD: ${window.yanshiStats.kernelHead}\n` +
    `服务端 HEAD: ${window.yanshiStats.serverHead}\n地址: ${location.href.split("?")[0]}\n`
  );
  link.href = `mailto:yanshi@wangda.today?subject=${subject}&body=${body}`;
}

function connect() {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  const socket = new WebSocket(scheme + "//" + location.host + "/ws?doc=" + state.docId + "&token=" + state.token);
  state.socket = socket;
  socket.onopen = () => {
    $("conn").className = "dot on";
    $("connText").textContent = "已连接";
    subscribeViewport();
  };
  socket.onclose = () => {
    // 只允许**当前**这条连接触发重连：切换文档时我们主动关闭旧连接，
    // 若它的 onclose 也去重连，就会同时存在多条订阅（表现为同一 atom 被处理多次）。
    if (state.socket !== socket) return;
    state.socket = null;
    $("conn").className = "dot";
    $("connText").textContent = "已断开";
    setTimeout(() => { if (state.token && !state.socket) connect(); }, 1500);
  };
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.type === "event" && message.event && message.event.event === "atom") {
      const event = message.event;
      log("atom seq=" + event.seq + " " + event.kind + (event.heavy ? " [heavy]" : ""));
      setStatus({ head: event.seq });
      // 13.3：其他客户端（以及自己）的原子经全局广播到达后增量折叠并重绘。
      //
      // **heavy 原子（`import_image`/`liquify`/`declare_head`…）本地内核应用不了** ✗ ——
      // 它们的像素要由服务端产出 ✓。此前的写法只在 `out_of_order`/`precondition_failed`
      // 两种错误下 `resync()` ✗，其它失败（包括 heavy ✓）**什么都不做** ✓，
      // 于是画布永远拿不到这次改动 ✓ —— 症状正是"缩略图有内容、主画布空白" ✓
      //（用户实测的介质落笔与早先记录的刷新问题都是它 ✓）。
      // 修法：**本地内核应用不了的一律重新同步** ✓（拿服务端像素 ✓），heavy 直接走这条路 ✓。
      // **先让本地内核尝试应用** ✓，失败了再 resync ✓ ——
      // 不能写成"heavy 一律 resync" ✗：跳转（`declare_head` ✓）也是 heavy ✓，
      // 而内核**能**应用它 ✓；一律 resync 会把客户端拉回 head ✗，
      // 表现为"跳转后画面没变" ✓（检查里的跳转用例当场抓到了这个回归 ✓✓）。
      if (kernelReady() && event.atom) {
        const response = JSON.parse(state.kernel.apply_atom_json(JSON.stringify(event.atom)));
        if (response.ok) {
          drawKernelDirty(response.report);
          state.localSeq = response.report.head;
          window.yanshiStats.kernelHead = response.report.head;
          window.yanshiStats.serverHead = event.seq;
          // **heavy 原子应用"成功"不等于像素正确** ✗ —— 实测：内核能折叠 `import_image` ✓，
          // 但它拿不到 blob 的像素 ⇒ 得到一个**空白**的补丁 ✓ 且返回 ok ✓
          //（诊断：`resyncs: 0`、`kernelHead = serverHead` ✓，而画布 0 个有墨像素 ✗）。
          // 所以 heavy（且非跳转 ✓）之后必须用**服务端像素**补画 ✓。
          // 跳转（`declare_head` ✓）不能补画 ✗ —— 服务端此刻的像素是 **head** 的 ✓，
          // 会把客户端从"跳转后的历史时刻"拉回最新 ✓（上一轮就是这么把跳转用例弄红的 ✓）。
          if (event.heavy && event.kind !== "declare_head") {
            // 先重载内核（它会触发一次 renderViewport ✓），**再**补画 ✓ ——
            // 顺序很关键：只补画会被随后的内核重绘覆盖 ✓（实测 serverBlits=2 却仍 0 个有墨像素 ✗）。
            void resync().then(() => blitServerViewport());
          }
        } else {
          // **任何**应用失败都重新同步 ✓（含"内核表示不了 heavy 内容"这种情况 ✓），
          // 绝不静默丢掉这次变更 ✗ —— 此前只在 `out_of_order`/`precondition_failed`
          // 两种错误下才 resync ✗，其它失败什么都不做 ✓，介质落笔因此永远画不出来 ✓。
          // 注意本处理器**不是 async** ✗：只能用 `void resync()` ✓
          //（写 `await` 会让整段页面脚本语法错误 ✓，实测是内核迟迟不就绪、检查全线超时 ✗）。
          window.yanshiStats.resyncs += 1;
          // 重载内核 **之后**再补画服务端像素 ✓ —— 内核表示不了 heavy 内容 ✗，
          // 少了这一步画布就是空白 ✓（实测介质落笔 10 秒内 0 有墨像素 ✗）。
          void resync().then(() => blitServerViewport());
        }
      }
    } else if (message.type === "event" && message.event && message.event.event === "tiles") {
      // 降噪：失效 tile 数只更新状态栏，日志最多每 2 秒一条（此前每个事件都写一行）。
      const count = (message.event.keys || []).length;
      setStatus({ dirty: count });
      const now = performance.now();
      if (now - (window.yanshiStats.lastTileLogMs || 0) > 2000) {
        window.yanshiStats.lastTileLogMs = now;
        window.yanshiStats.tileInvalidations = (window.yanshiStats.tileInvalidations || 0) + count;
        log("tiles " + count + " 个失效（累计 " + window.yanshiStats.tileInvalidations + "）");
      }
    } else if (message.type === "event" && message.event && message.event.event === "thumbnail") {
      scheduleThumbRefresh();
    } else if (message.type === "ack") {
      const result = message.result || {};
      // 注意：ack 里的 preview 是**缩略图级**的服务端预览，不能画进主画布。
      // 内核就绪时主画布的唯一权威来源是内核；把服务端预览覆盖上去会让刚提交的笔迹
      // 「看起来消失」（服务端预览可能早于该原子生成），这与用户报告的
      // 「操作后画布空白、刷新才可见」是同一个根因。
    }
  };
}

function subscribeViewport() {
  if (!state.socket || state.socket.readyState !== 1) return;
  state.socket.send(JSON.stringify({
    type: "subscribe",
    doc_id: state.docId,
    viewport: state.viewport,
    zoom: state.displayScale || 1,
  }));
}

function colorCss() {
  const hex = $("color").value;
  return { r: parseInt(hex.slice(1, 3), 16), g: parseInt(hex.slice(3, 5), 16), b: parseInt(hex.slice(5, 7), 16), a: 255 };
}

// 只重绘**覆盖层**（拖动中的笔迹/选区）。内容层绝不能被清空 —— 此前两者共用一个画布，
// 拖动结束的最后一次重绘会把已提交的内容一起擦掉，表现为「操作后画布空白，刷新才恢复」。
function redraw() {
  // 每次重绘前同步几何：窗口缩放、滚动或布局变化都会让覆盖层偏离内容层。
  syncOverlayGeometry();
  octx.clearRect(0, 0, overlay.width, overlay.height);
  redrawSourceMark();
  drawSelectionBox();
  drawSelectionOutline();
  if (!state.dragging) return;
  octx.strokeStyle = $("color").value;
  octx.lineWidth = Number($("size").value);
  if (state.points.length === 2 && state.tool === "select_rect") {
    const [a, b] = state.points;
    const topLeft = toCanvas({ x: Math.min(a.x, b.x), y: Math.min(a.y, b.y) });
    const bottomRight = toCanvas({ x: Math.max(a.x, b.x), y: Math.max(a.y, b.y) });
    octx.save();
    octx.strokeStyle = "#ffd166";
    octx.setLineDash([6, 4]);
    octx.strokeRect(topLeft.x, topLeft.y, bottomRight.x - topLeft.x, bottomRight.y - topLeft.y);
    octx.restore();
  } else if (state.points.length === 2 && (state.tool === "rect" || MASK_TOOLS.has(state.tool))) {
    const [a, b] = state.points;
    const pa = toCanvas(a);
    const pb = toCanvas(b);
    octx.strokeRect(pa.x, pa.y, pb.x - pa.x, pb.y - pa.y);
  } else if (state.points.length === 2 && (state.tool === "ellipse" || state.tool === "mask_ellipse")) {
    const [a, b] = state.points;
    const pa = toCanvas(a);
    const pb = toCanvas(b);
    octx.beginPath();
    octx.ellipse((pa.x + pb.x) / 2, (pa.y + pb.y) / 2, Math.abs(pb.x - pa.x) / 2, Math.abs(pb.y - pa.y) / 2, 0, 0, Math.PI * 2);
    octx.stroke();
  } else {
    octx.beginPath();
    state.points.forEach((point, index) => {
      const canvasPoint = toCanvas(point);
      return index ? octx.lineTo(canvasPoint.x, canvasPoint.y) : octx.moveTo(canvasPoint.x, canvasPoint.y);
    });
    octx.stroke();
  }
}

function localPoint(event) {
  const rect = board.getBoundingClientRect();
  // 画布内部像素 = 视口文档像素；再加视口原点得到文档坐标。
  return {
    x: state.viewport.x + (event.clientX - rect.left) * board.width / rect.width,
    y: state.viewport.y + (event.clientY - rect.top) * board.height / rect.height,
  };
}

let pendingStroke = null;
let panState = null;
// **"当前该由服务端像素说话吗"** ✓ —— 只在**打开含重内容的文档**或**刚发生 heavy 原子**时为真 ✓；
// 用户一旦开始画（轻量乐观路径 ✓）立刻清掉 ✓：服务端补画只含**已提交**内容 ✓，
// 若一直为真就会把未提交的乐观笔迹覆盖掉 ✗（我上一轮"标记太黏"的失败正是这个 ✓，
// 当时检查立刻报"移动处没有对象""invert 无变化" ✓）。

/// 吸管（设计 13.3 基础工具）：点击画布 → 向内核要该**文档坐标**的 1×1 像素 → 设为当前颜色。
/// 不读画布位图的原因：画布会被 CSS 缩放，而内核渲染是文档坐标 1:1 ✓。
function pickColorAt(event) {
  const point = localPoint(event);
  const x = Math.floor(point.x);
  const y = Math.floor(point.y);
  if (x < 0 || y < 0 || x >= state.docSize.w || y >= state.docSize.h) return;
  // 取 3×3 的中心像素：单点取色容易被抗锯齿边缘影响，也让 1×1 区域渲染的边界情况暴露出来。
  const side = 3;
  const x0 = Math.max(0, Math.min(state.docSize.w - side, x - 1));
  const y0 = Math.max(0, Math.min(state.docSize.h - side, y - 1));
  const rgba = state.kernel.render_region_rgba(x0, y0, side, side);
  if (!rgba || rgba.length < side * side * 4) {
    log("吸管失败：内核没有返回像素（len=" + (rgba ? rgba.length : "null") + "）", "#c33");
    return;
  }
  const center = (1 * side + 1) * 4;
  const hex = "#" + [rgba[center], rgba[center + 1], rgba[center + 2]]
    .map((channel) => channel.toString(16).padStart(2, "0"))
    .join("");
  $("color").value = hex;
  log("吸管取色 " + hex + "（文档坐标 " + x + ", " + y + "）");
}

/// 填充图层：用当前颜色填充整幅区域（`fill` 的语义是**按区域**填充，不是洪水填充）。
async function fillCurrentLayer() {
  const color = colorCss();
  const value = await callTool(
    "fill",
    {
      layer_id: state.layerId,
      data: { color, region: { x: 0, y: 0, w: state.docSize.w, h: state.docSize.h } },
    },
    { refresh: false }
  );
  if (!value.ok) {
    log("填充失败：" + (value.error_code || "unknown") + " " +
        ((value.context && value.context.detail) || ""), "#c33");
    return;
  }
  log("已填充图层 " + state.layerId);
  await refreshPreview();
}

/// 移动工具（设计 13.3 基础工具「移动」）：点击选中光标下最上层的对象，拖动后提交
/// `move_object{object_id, delta:{dx,dy}}`（注意参数名是 **dx/dy**，不是 x/y ✓）。
const MOVE_TOOL = "move_object";
const MOVE_LAYER_TOOL = "move_layer";
// 整层拖动状态（起点 + 目标图层 ✓）。
let layerMoveState = null;
// 介质整笔状态（沿路径累积的点 ✓）。
let mediumStrokeState = null;
let moveState = null;

/// 命中测试：`list_objects` 的 bbox 是 `[x,y,w,h]`（文档坐标）✓。
async function pickObjectAt(point) {
  const value = await callTool("list_objects", { include_hidden: false }, { refresh: false });
  if (!value.ok) return null;
  const candidates = (value.objects || []).filter((object) => {
    const bbox = object.bbox;
    if (!bbox || bbox.length < 4) return false;
    return point.x >= bbox[0] && point.y >= bbox[1] &&
           point.x <= bbox[0] + bbox[2] && point.y <= bbox[1] + bbox[3];
  });
  if (candidates.length === 0) return null;
  // **优先命中"当前选中的图层"** ✓ —— 子 agent 实测的抱怨（G5 ✓）：
  // 在图层面板选中某一层、用"移动"拖动时，命中测试会**命中所有图层** ✗，
  // 于是拖到了另一个图层上的**全幅背景** ✓ 并把背景拖出了画布 ✓（用户完全没打算动它 ✓）。
  //
  // **设计未规定此处 ⇒ 记录选择 ✓**：先只看当前图层 ✓；该图层上没东西时**回退**到其它图层 ✓
  //（而不是干脆不选 ✗ —— 那会让"点一下就选中画面上的东西"这种直觉失效 ✓）。
  // 这与成熟绘画软件一致 ✓，也是**最小惊讶** ✓：用户选中的图层就是他正在处理的那一层 ✓。
  const preferred = candidates.filter((object) => object.layer_id === state.layerId);
  const pool = preferred.length > 0 ? preferred : candidates;
  // 取 z_index 最大者（同 z 取列表中较晚者，即较新对象）。
  pool.sort((a, b) => (a.z_index || 0) - (b.z_index || 0));
  return pool[pool.length - 1];
}

/// 在覆盖层画出当前选区轮廓 ✓（让用户看得见"落笔会被限制在哪里" ✓）。
function drawSelectionOutline() {
  const shape = state.selectionShape;
  if (!shape || shape.kind !== "rect" || !shape.bbox) return;
  const topLeft = toCanvas({ x: shape.bbox.x, y: shape.bbox.y });
  const bottomRight = toCanvas({ x: shape.bbox.x + shape.bbox.w, y: shape.bbox.y + shape.bbox.h });
  octx.save();
  octx.strokeStyle = "#ffd166";
  octx.lineWidth = 1;
  octx.setLineDash([6, 4]);
  octx.strokeRect(
    topLeft.x, topLeft.y,
    bottomRight.x - topLeft.x, bottomRight.y - topLeft.y
  );
  octx.restore();
}

/// 选区工具：拖出一个矩形选区，约束其后的**笔触与擦除/形状/填充**落笔 ✓（路线 A）。
///
/// 语义细节（已记录）：选区**不绑定图层**（作用于全文档 ✓），且只约束
/// **在其创建之后创建的对象** ✓ —— 与"选区影响后续编辑"一致 ✓。
/// 把当前选区状态写进状态栏 ✓ —— 子 agent 报："拖出选区后 `#selectionHint` 仍显示**无选区**" ✗，
/// 更要命的是**遗留选区会静默裁掉一切** ✓（画布看似全白 ✓，而界面上没有任何提示 ✓）。
/// 因此这里由**服务端事实**驱动 ✓（`list_selections` ✓），而不是本地猜测 ✓。
async function refreshSelectionHint() {
  const hint = $("selectionHint");
  if (!hint) return;
  try {
    const listed = await callTool("list_selections", {}, { refresh: false });
    const selections = (listed && listed.selections) || [];
    if (selections.length === 0) {
      hint.textContent = "无选区";
      return;
    }
    const first = selections[0];
    const bbox = first.bbox || first.shape && first.shape.bbox;
    if (Array.isArray(bbox)) {
      hint.textContent = `选区 ${selections.length} 个（${Math.round(bbox[2])}×${Math.round(bbox[3])} @ ${Math.round(bbox[0])},${Math.round(bbox[1])}）｜约束之后的绘制`;
    } else {
      hint.textContent = `选区 ${selections.length} 个｜约束之后的绘制`;
    }
  } catch (error) {
    hint.textContent = "选区状态未知";
  }
}

async function commitSelection() {
  const points = state.points;
  if (points.length < 2) {
    log("选区需要拖出一个区域", "#c33");
    return;
  }
  const [a, b] = points;
  const bbox = {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    w: Math.max(1, Math.abs(b.x - a.x)),
    h: Math.max(1, Math.abs(b.y - a.y)),
  };
  const selectionId = "sel_" + ulid();
  const created = await callTool(
    "create_selection",
    {
      selection_id: selectionId,
      shape: { kind: "rect", bbox },
      feather: Number($("feather").value) || 0,
      invert: false,
      mode: "new",
    },
    { refresh: false }
  );
  if (!created.ok) {
    log("创建选区失败：" + (created.error_code || "unknown") + " " +
        ((created.context && created.context.detail) || ""), "#c33");
    return;
  }
  state.selectionId = selectionId;
  state.selectionShape = { kind: "rect", bbox };
  log("已创建选区 " + selectionId + "（其后的落笔只在选区内生效）");
  await refreshPreview();
}

async function clearSelection() {
  if (!state.selectionId) {
    log("当前没有选区");
    return;
  }
  const removed = await callTool(
    "delete_selection",
    { selection_id: state.selectionId },
    { refresh: false }
  );
  if (!removed.ok) {
    log("清除选区失败：" + (removed.error_code || "unknown"), "#c33");
    return;
  }
  log("已清除选区 " + state.selectionId + "（已画内容按日志重算，保持原样）");
  state.selectionId = null;
  state.selectionShape = null;
  await refreshPreview();
}

/// 文本工具：点击位置 + 输入文字 ⇒ `draw_text`（内核用内置 5×7 ASCII 位图字体 ✓）。
async function commitText(point) {
  // **文案要与实现一致** ✓ —— 此前写着"CJK 为后续项" ✗，而 CJK 早已由内嵌 OFL 图集渲染 ✓
  //（子 agent 用中文标题作画时正是靠它 ✓）。现在如实说明**边界** ✓：图集之外的字形显示为 `?` ✓。
  const text = window.prompt(
    "要输入的文本（内置 ASCII 点阵 + 内嵌中日韩图集；图集之外的字形显示为 ?）", "");
  if (text === null || text === "") return;
  const size = Number($("textSize").value) || 21;
  const drawn = await callTool("draw_text", {
    layer_id: state.layerId,
    data: {
      text,
      font: "builtin",
      size,
      color: colorCss(),
      position: [Math.round(point.x), Math.round(point.y)],
      align: "left",
    },
  }, { refresh: false });
  if (!drawn.ok) {
    log("输入文本失败：" + (drawn.error_code || "unknown") + " " +
        ((drawn.context && drawn.context.detail) || ""), "#c33");
    return;
  }
  log("已输入文本 " + JSON.stringify(text) + "（字号 " + size + "）");
  await refreshPreview();
}

/// 绘制选中框（覆盖层）。
function drawSelectionBox() {
  const object = state.selectedObject;
  if (!object || !object.bbox) return;
  const [x, y, w, h] = object.bbox;
  const topLeft = toCanvas({ x, y });
  octx.save();
  octx.strokeStyle = "#4a7dff";
  octx.lineWidth = 1;
  octx.setLineDash([4, 3]);
  octx.strokeRect(topLeft.x, topLeft.y, w, h);
  octx.restore();
}

/// 蒙版工具（设计 13.3「蒙版编辑」）：拖动出一个形状 → `create_mask` → 用 `set_property`
/// 把 `mask_id` 挂到**当前图层**（工具摘要里写的正是这个工作流 ✓）。
const MASK_TOOLS = new Set(["mask_rect", "mask_ellipse"]);

async function commitMask() {
  const points = state.points;
  if (points.length < 2) {
    log("蒙版需要拖出一个区域", "#c33");
    return;
  }
  const [a, b] = points;
  const bbox = {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    w: Math.max(1, Math.abs(b.x - a.x)),
    h: Math.max(1, Math.abs(b.y - a.y)),
  };
  const kind = state.tool === "mask_ellipse" ? "ellipse" : "rect";
  const maskId = "mask_" + ulid();
  const feather = Number($("feather").value) || 0;
  const created = await callTool(
    "create_mask",
    { mask_id: maskId, shape: { kind, bbox }, feather, invert: false, linked_layer: state.layerId },
    { refresh: false }
  );
  if (!created.ok) {
    log("创建蒙版失败：" + (created.error_code || "unknown") + " " +
        ((created.context && created.context.detail) || ""), "#c33");
    return;
  }
  const attached = await callTool(
    "set_property",
    { layer_id: state.layerId, key: "mask_id", value: maskId },
    { refresh: false }
  );
  if (!attached.ok) {
    log("挂载蒙版失败：" + (attached.error_code || "unknown") + " " +
        ((attached.context && attached.context.detail) || ""), "#c33");
    return;
  }
  log("已为图层 " + state.layerId + " 添加" + (kind === "ellipse" ? "椭圆" : "矩形") +
      "蒙版（羽化 " + feather + "）");
  await refreshPreview();
}

/// 这些工具不走 `draw_stroke` 原子，而是调用同名工具（工具层会构造正确的原子）。
/// 拖动中仍用覆盖层显示笔迹（不给本地乐观像素 —— 内核目前只为 `draw_stroke` 提供增量预览）。
const RETOUCH_TOOLS = new Set([
  "clone_stamp", "heal_stamp", "smudge",
  "liquify_push", "liquify_twirl", "liquify_pinch",
]);

/// Alt+点击设置仿制/修复的源点。
board.addEventListener("pointerdown", (event) => {
  // **用户开始画 ⇒ 内核重新成为权威** ✓（见 needsServerPixels 的说明 ✓）：
  // 只有 pan/eyedropper 这类"不动内容"的工具才不清 ✓。
  if (state.tool && state.tool !== "pan" && state.tool !== "eyedropper" &&
      state.tool !== MOVE_TOOL && state.tool !== MOVE_LAYER_TOOL) {
    needsServerPixels = false;
  }
  if (!event.altKey || !RETOUCH_TOOLS.has(state.tool)) return;
  const point = localPoint(event);
  state.sourcePoint = { x: Math.round(point.x), y: Math.round(point.y) };
  log("已设置源点 (" + state.sourcePoint.x + ", " + state.sourcePoint.y + ")");
  redrawSourceMark();
  event.preventDefault();
  event.stopPropagation();
}, true);

/// 在覆盖层上标出源点（只是提示，不写入任何原子）。
function redrawSourceMark() {
  const point = state.sourcePoint;
  if (!point || !RETOUCH_TOOLS.has(state.tool)) return;
  const canvasPoint = toCanvas(point);
  if (canvasPoint.x < 0 || canvasPoint.y < 0) return;
  octx.save();
  octx.strokeStyle = "#4a7dff";
  octx.lineWidth = 1;
  octx.beginPath();
  octx.arc(canvasPoint.x, canvasPoint.y, 5, 0, Math.PI * 2);
  octx.moveTo(canvasPoint.x - 8, canvasPoint.y);
  octx.lineTo(canvasPoint.x + 8, canvasPoint.y);
  octx.moveTo(canvasPoint.x, canvasPoint.y - 8);
  octx.lineTo(canvasPoint.x, canvasPoint.y + 8);
  octx.stroke();
  octx.restore();
}

/// 提交一次修图/液化操作：把拖动点列交给对应工具（服务端构造原子）。
async function commitRetouch() {
  const points = state.points.map((point) => [Math.round(point.x), Math.round(point.y)]);
  if (points.length === 0) return;
  const size = Number($("size").value);
  const strength = Number($("strength").value) / 100;
  const tool = state.tool;
  let args = { layer_id: state.layerId, points, size };
  if (tool === "clone_stamp" || tool === "heal_stamp") {
    if (!state.sourcePoint) {
      log("请先按住 Alt 点击设置源点（仿制/修复需要源点）", "#c33");
      return;
    }
    args.source_offset = [
      state.sourcePoint.x - points[0][0],
      state.sourcePoint.y - points[0][1],
    ];
  } else if (tool === "liquify_push") {
    // 方向取整条笔迹的首末向量；点数不足时用最后一点相对前一点的走向。
    const first = points[0];
    const last = points[points.length - 1];
    let direction = [last[0] - first[0], last[1] - first[1]];
    if (direction[0] === 0 && direction[1] === 0) direction = [1, 0];
    args.direction = direction;
    args.strength = strength;
  } else if (tool === "smudge") {
    args.smudge_length = Math.max(1, Math.round(size));
  } else {
    args.strength = strength;
  }
  const value = await callTool(tool, args, { refresh: false });
  if (!value.ok) {
    log("操作失败：" + (value.error_code || "unknown") + " " +
        ((value.context && value.context.detail) || ""), "#c33");
    return;
  }
  await refreshPreview();
  redraw();
}

// 滚轮缩放：以光标下的文档点为锚点（图像编辑器的常规行为）。
board.addEventListener("wheel", (event) => {
  if (!kernelReady()) return;
  event.preventDefault();
  const before = localPoint(event);
  const factor = Math.exp(-event.deltaY * 0.0015);
  state.zoom = Math.max(0.1, Math.min(16, state.zoom * factor));
  clampViewport(before);
  renderViewport();
}, { passive: false });

// 中键拖动平移。
board.addEventListener("pointerdown", (event) => {
  // **平移的三种入口统一在这里** ✓：中键 / 手形工具 / 按住空格 ✓。
  // 只保留一条路径的原因：本会话已多次教训"两条相似路径会漂移" ✗
  //（整段/增量盖章、两条介质链路都栽过 ✓）。
  const wantsPan = event.button === 1 || state.tool === "pan" || spaceHeld;
  if (!wantsPan) return;
  event.preventDefault();
  panState = { startX: event.clientX, startY: event.clientY,
               originX: state.viewport.x, originY: state.viewport.y, pointerId: event.pointerId };
  try { board.setPointerCapture(event.pointerId); } catch (_) { /* 合成事件无 pointerId */ }
  updatePanCursor();
});
board.addEventListener("pointermove", (event) => {
  if (!panState) return;
  const scale = (state.displayScale || 1);
  const rect = board.getBoundingClientRect();
  const perPixel = state.viewport.w / Math.max(1, rect.width);
  const center = {
    x: panState.originX - (event.clientX - panState.startX) * perPixel + state.viewport.w / 2,
    y: panState.originY - (event.clientY - panState.startY) * perPixel + state.viewport.h / 2,
  };
  void scale;
  clampViewport(center);
  renderViewport();
});
board.addEventListener("pointerup", (event) => {
  // 不再判断 `button === 1` ✗：手形工具与空格拖动都是**左键** ✓（判错会让平移"卡住" ✓）。
  if (panState && (event.pointerId === undefined || panState.pointerId === event.pointerId ||
                   event.button === 1)) {
    panState = null;
    updatePanCursor();
    // 切换工具不改变内容 ✓，这里不动 needsServerPixels ✓。
  }
});

/// 手形光标 ✓：手形工具或按住空格时显示抓手 ✓。
function updatePanCursor() {
  if (panState) {
    board.style.cursor = "grabbing";
  } else if (state.tool === "pan" || spaceHeld) {
    board.style.cursor = "grab";
  } else if (state.tool === "eyedropper") {
    board.style.cursor = "copy";
  } else if (state.tool === "text") {
    board.style.cursor = "text";
  } else if (state.tool === MOVE_TOOL || state.tool === MOVE_LAYER_TOOL) {
    board.style.cursor = "move";
  } else {
    board.style.cursor = "crosshair";
  }
}

/// **滚轮缩放** ✓：以**光标处**的文档坐标为中心重算视口 ✓ ⇒ 光标下的内容保持不动 ✓
///（这是缩放最自然的手感 ✓；此前的 +/- 按钮只能以视口中心缩放 ✓）。
board.addEventListener("wheel", (event) => {
  if (!kernelReady()) return;
  event.preventDefault();
  const focus = localPoint(event);
  const factor = event.deltaY < 0 ? 1.1 : 1 / 1.1;
  state.zoom = Math.max(0.1, Math.min(16, (state.zoom || 1) * factor));
  // **先定下缩放，再重读画布矩形** ✓ —— 画布是**居中**的 ✓，缩放会改变它的尺寸 ✓
  // ⇒ 左边缘也移动 ✓ ⇒ 锚点像素必须按**新**矩形算 ✓
  //（第一版用旧矩形 ⇒ 漂移 21.75px ✗，正是"半个尺寸差"的量级 ✓）。
  clampViewport(focus);
  const rect = board.getBoundingClientRect();
  // 把光标处的文档点**钉在光标所在的画布像素**上 ✓ ⇒ 该点视觉上不动 ✓。
  clampViewportAt(focus, { x: event.clientX - rect.left, y: event.clientY - rect.top });
  renderViewport();
}, { passive: false });

// **空格临时手形** ✓（成熟软件的通用肌肉记忆 ✓）：按住空格拖动即平移 ✓，松开恢复 ✓。
let spaceHeld = false;
window.addEventListener("keydown", (event) => {
  if (event.code !== "Space") return;
  if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
  if (spaceHeld) return;
  spaceHeld = true;
  event.preventDefault();
  updatePanCursor();
});
window.addEventListener("keyup", (event) => {
  if (event.code !== "Space") return;
  spaceHeld = false;
  updatePanCursor();
});

// 键盘：+ / - 缩放，0 复位到整幅。
window.addEventListener("keydown", (event) => {
  if (!kernelReady() || event.target instanceof HTMLInputElement) return;
  if (event.key === "+" || event.key === "=") state.zoom = Math.min(16, state.zoom * 1.25);
  else if (event.key === "-") state.zoom = Math.max(0.1, state.zoom / 1.25);
  else if (event.key === "0") state.zoom = 1;
  else return;
  event.preventDefault();
  clampViewport();
  renderViewport();
});

board.addEventListener("pointerdown", (event) => {
  if (state.tool === "text") {
    event.preventDefault();
    void commitText(localPoint(event));
    return;
  }
  if (state.tool === "medium_dab") {
    // 介质插件（设计 11.1）：**拖动整笔** ✓ —— 沿路径连续落笔 ✓，
    // 载墨沿笔迹耗尽 ✓、每点取笔尖处画面颜色混色 ✓，最后合成**一个**对象入 CAS 与日志 ✓。
    event.preventDefault();
    const point = localPoint(event);
    mediumStrokeState = { points: [point] };
    state.dragging = event.pointerId;
    state.points = [point];
    try { board.setPointerCapture(event.pointerId); } catch (_) { /* 合成事件无 pointerId */ }
    return;
    return;
  }
  if (state.tool === MOVE_LAYER_TOOL) {
    // 拖动整层 ✓：起点记下即可 ✓，落点时对**该层所有对象**下同一批 move ✓。
    const start = localPoint(event);
    layerMoveState = { start, layerId: state.layerId };
    state.dragging = event.pointerId;
    state.points = [start];
    log("移动图层 " + state.layerId + "（拖动即可整体移动）");
    event.preventDefault();
    return;
  }
  if (state.tool === MOVE_TOOL) {
    const start = localPoint(event);
    void pickObjectAt(start).then((object) => {
      state.selectedObject = object;
      if (!object) {
        log("移动：此处没有对象");
        redraw();
        return;
      }
      moveState = { objectId: object.object_id, start, bbox: object.bbox };
      log("已选中 " + object.object_id + "（拖动即可移动）");
      redraw();
    });
    try { board.setPointerCapture(event.pointerId); } catch (_) { /* 合成事件无 pointerId */ }
    state.dragging = event.pointerId;
    state.points = [start];
    event.preventDefault();
    return;
  }
  if (state.tool === "eyedropper" && kernelReady()) {
    pickColorAt(event);
    event.preventDefault();
    return;
  }
  try { board.setPointerCapture(event.pointerId); } catch (_) { /* 合成事件无 pointerId */ }
  state.dragging = event.pointerId;
  state.points = [localPoint(event)];
  pendingStroke = kernelReady()
    ? { atomId: ulid(), objectId: "obj_" + ulid(), layerId: state.layerId, tool: state.tool, base: 0 }
    : null;
});

board.addEventListener("pointermove", (event) => {
  if (state.dragging !== event.pointerId) return;
  const point = localPoint(event);
  if (state.tool === MOVE_LAYER_TOOL) {
    state.points = layerMoveState ? [layerMoveState.start, point] : [point];
    redraw();
    return;
  }
  if (state.tool === "medium_dab" && mediumStrokeState) {
    // 抽稀：两点间距小于 1/4 笔尖直径就不记 ✓（否则同一位置会叠很多次 ✓，既慢又浓 ✗）。
    const last = mediumStrokeState.points[mediumStrokeState.points.length - 1];
    const gap = Math.hypot(point.x - last.x, point.y - last.y);
    // 抽稀间距取笔尖直径的 **1/8** ✓ —— 原先 1/4 太疏 ✗：
    // 水彩每个点都有自己的边缘沉积 ✓，点距太大就叠成"一串环"而不是一片水痕 ✗
    //（截图核验发现 ✓）。1/8 让相邻点的沉积充分重叠 ✓。
    const spacing = Math.max(1, (Number($("size").value) || 6) / 8);
    if (gap >= spacing) {
      mediumStrokeState.points.push(point);
      state.points = mediumStrokeState.points.slice();
      redraw();
    }
    return;
  }
  if (state.tool === MOVE_TOOL) {
    state.points = moveState ? [moveState.start, point] : [point];
    if (moveState && state.selectedObject) {
      // 覆盖层实时显示"移动后"的位置（不改数据）。
      const dx = point.x - moveState.start.x;
      const dy = point.y - moveState.start.y;
      const [x, y, w, h] = moveState.bbox;
      const topLeft = toCanvas({ x: x + dx, y: y + dy });
      octx.clearRect(0, 0, overlay.width, overlay.height);
      octx.save();
      octx.strokeStyle = "#4a7dff";
      octx.lineWidth = 1;
      octx.setLineDash([4, 3]);
      octx.strokeRect(topLeft.x, topLeft.y, w, h);
      octx.restore();
    }
    return;
  }
  // **"用两个角点定义的"工具都要替换第二个点，而不是一直追加** ✓ ——
  // 它们提交时都取 `points[0], points[1]` ✓：形状 ✓、**选区** ✓、**蒙版** ✓。
  // 此前只有形状这样做 ✗，选区/蒙版一路 `push` ✗ ⇒ 提交拿到的是**前两次移动事件** ✓
  //（子 agent 实测：拖 (64,372)→(432,500) 十步，得到 `{w:36.8,h:12.8}` 而不是 368×128 ✓）。
  const TWO_CORNER_TOOLS = new Set(["rect", "ellipse", "select_rect", "mask_rect", "mask_ellipse"]);
  if (TWO_CORNER_TOOLS.has(state.tool)) state.points = [state.points[0], point];
  else state.points.push(point);
  if (pendingStroke && state.points.length >= 2 && !RETOUCH_TOOLS.has(state.tool)) {
    // 乐观渲染：拖动中只更新**本地覆盖层**（不进原子日志），落笔才提交最终原子。
    updatePreviewOverlay(pendingStroke);
  }
  redraw();
});

board.addEventListener("pointerup", async (event) => {
  if (state.dragging !== event.pointerId) return;
  state.dragging = null;
  if (state.tool === "medium_dab") {
    const pending = mediumStrokeState;
    mediumStrokeState = null;
    state.points = [];
    if (!pending) {
      redraw();
      return;
    }
    try {
      await mediumStroke($("medium").value, pending.points);
    } catch (error) {
      const message = error && error.message ? error.message : String(error);
      window.yanshiStats.medium = Object.assign({}, window.yanshiStats.medium, {
        status: "rejected", error: message,
      });
      log("介质落笔失败（未捕获）：" + message, "#c33");
    }
    redraw();
    return;
  }
  if (state.tool === MOVE_LAYER_TOOL) {
    const end = localPoint(event);
    const pending = layerMoveState;
    layerMoveState = null;
    state.points = [];
    if (!pending) {
      redraw();
      return;
    }
    const dx = Math.round(end.x - pending.start.x);
    const dy = Math.round(end.y - pending.start.y);
    if (dx === 0 && dy === 0) {
      redraw();
      return;
    }
    const listed = await callTool("list_objects", {}, { refresh: false });
    const objects = (listed.objects || []).filter((o) => o.layer_id === pending.layerId);
    if (objects.length === 0) {
      log("移动图层：" + pending.layerId + " 里没有对象");
      redraw();
      return;
    }
    // **同一变更集** ✓ ⇒ 一次撤销 ✓（批量调用 ✓）。
    const batched = await callTool(
      "batch",
      {
        message: "move layer " + pending.layerId,
        calls: objects.map((o) => ({
          tool: "move_object",
          arguments: { object_id: o.object_id, delta: { dx, dy } },
        })),
      },
      { refresh: false },
    );
    if (!batched.ok) {
      log("移动图层失败：" + (batched.error_code || "unknown") + " " +
          ((batched.context && batched.context.detail) || ""), "#c33");
      redraw();
      return;
    }
    log("已移动图层 " + pending.layerId + " 的 " + objects.length + " 个对象（dx=" + dx + ", dy=" + dy + "）");
    await refreshPreview();
    redraw();
    return;
  }
  if (state.tool === MOVE_TOOL) {
    const end = localPoint(event);
    const pending = moveState;
    moveState = null;
    state.points = [];
    if (!pending) {
      redraw();
      return;
    }
    const dx = Math.round(end.x - pending.start.x);
    const dy = Math.round(end.y - pending.start.y);
    if (dx === 0 && dy === 0) {
      redraw();
      return;
    }
    const moved = await callTool(
      "move_object",
      { object_id: pending.objectId, delta: { dx, dy } },
      { refresh: false }
    );
    if (!moved.ok) {
      log("移动失败：" + (moved.error_code || "unknown") + " " +
          ((moved.context && moved.context.detail) || ""), "#c33");
      redraw();
      return;
    }
    log("已移动 " + pending.objectId + "（dx=" + dx + ", dy=" + dy + "）");
    // 更新选中框到新位置。
    state.selectedObject = await pickObjectAt(end);
    await refreshPreview();
    redraw();
    return;
  }
  if (state.tool === "select_rect") {
    state.points.push(localPoint(event));
    await commitSelection();
    state.points = [];
    await refreshSelectionHint();
    redraw();
    return;
  }
  if (MASK_TOOLS.has(state.tool)) {
    state.points.push(localPoint(event));
    await commitMask();
    state.points = [];
    redraw();
    return;
  }
  if (RETOUCH_TOOLS.has(state.tool)) {
    state.points.push(localPoint(event));
    await commitRetouch();
    state.points = [];
    redraw();
    return;
  }
  if (state.tool === "rect" || state.tool === "ellipse") state.points.push(localPoint(event));
  if (pendingStroke && kernelReady()) {
    // 落笔：内核合并本地日志并失效重算受影响 tile，这里**按返回的脏区重绘**。
    // （不要假定覆盖层像素已在 tile 里：覆盖层与提交原子落在不同图层时不成立，
    //   那正是「操作后画布空白、刷新才可见」的原因。）
    const atom = strokeAtom(pendingStroke, true);
    const committed = JSON.parse(state.kernel.commit_preview(JSON.stringify(atom)));
    if (committed.ok) {
      state.localSeq = committed.seq;
      window.yanshiStats.kernelHead = committed.seq;
      if (committed.report) {
        drawKernelDirty(committed.report);
        window.yanshiStats.commits = (window.yanshiStats.commits || 0) + 1;
      }
    } else {
      await resync();
    }
    const response = await submitAtom(atom);
    if (response && response.seq !== undefined && response.seq !== committed.seq) {
      // 服务端把原子排在了别处（并发），以权威日志为准重建。
      await resync();
    }
    pendingStroke = null;
  } else {
    await commitShape();
  }
  state.points = [];
  redraw();
});

// 由当前指针轨迹构造 draw_stroke / draw_shape / erase 原子（客户端 ULID，5.1）。
function strokeAtom(pending, final) {
  const color = colorCss();
  const size = Number($("size").value);
  const points = state.points.map((p) => [Math.round(p.x), Math.round(p.y)]);
  let kind = "draw_stroke";
  let data = { points, size, color, hardness: 0.7, smooth: true };
  if (pending.tool === "rect" || pending.tool === "ellipse") {
    const [a, b] = state.points;
    const bbox = {
      x: Math.round(Math.min(a.x, b.x)), y: Math.round(Math.min(a.y, b.y)),
      w: Math.max(1, Math.round(Math.abs(b.x - a.x))), h: Math.max(1, Math.round(Math.abs(b.y - a.y))),
    };
    kind = "draw_shape";
    data = { geometry: { kind: pending.tool, bbox }, color };
  } else if (pending.tool === "erase") {
    kind = "erase";
    data = { points, size: size * 1.5, color: { r: 0, g: 0, b: 0, a: 0 } };
  }
  return {
    id: pending.atomId,
    kind,
    actor: "human:web",
    session: "session:wasm",
    timestamp: Date.now(),
    payload: { object_id: pending.objectId, layer_id: pending.layerId, data },
  };
}

async function commitShape() {
  const color = colorCss();
  const size = Number($("size").value);
  if (state.tool === "brush") {
    if (state.points.length < 2) return;
    await callTool("draw_stroke", {
      layer_id: state.layerId,
      data: { points: state.points.map((p) => [p.x, p.y]), size, color, hardness: 0.7 },
    });
  } else if (state.tool === "erase") {
    if (state.points.length < 2) return;
    await callTool("erase", {
      layer_id: state.layerId,
      data: { points: state.points.map((p) => [p.x, p.y]), size: size * 1.5, color: { r: 0, g: 0, b: 0, a: 0 } },
    });
  } else {
    const [a, b] = state.points;
    if (!a || !b) return;
    const bbox = {
      x: Math.min(a.x, b.x), y: Math.min(a.y, b.y),
      w: Math.abs(b.x - a.x) || 1, h: Math.abs(b.y - a.y) || 1,
    };
    await callTool("draw_shape", {
      layer_id: state.layerId,
      data: { geometry: { kind: state.tool, bbox }, color },
    });
  }
  await refreshPreview();
}

// 工具条：**数据表驱动** ✓ —— 借鉴成熟绘画软件的做法 ✓（图标 + 快捷键 + 悬停提示 ✓）。
//
// 为什么改成生成而不是写死 HTML ✓：图标、快捷键、工具提示、以后的工作区与右键快捷面板
// 都要读同一份定义 ✓；写死 20 个按钮会让每加一个能力就要改四处 ✓（本会话已经吃过
// "改了结构忘了同步"的亏 ✗）。
const TOOL_ICONS = {
  brush: '<path d="M4 20l3-1 9-9-2-2-9 9z"/><path d="M15 8l3-3 2 2-3 3z"/>',
  rect: '<rect x="4" y="6" width="16" height="12" rx="1"/>',
  ellipse: '<ellipse cx="12" cy="12" rx="8" ry="6"/>',
  erase: '<path d="M8 17l-3-3 8-8 5 5-4 4z"/><path d="M4 20h16"/>',
  clone_stamp: '<path d="M12 3l7 6-3 1-4 9-4-9-3-1z"/>',
  heal_stamp: '<path d="M5 12h14"/><path d="M12 5v14"/>',
  smudge: '<path d="M5 18c6 0 11-4 11-11"/><circle cx="7" cy="18" r="2"/>',
  liquify_push: '<path d="M4 12h11"/><path d="M12 8l4 4-4 4"/>',
  liquify_twirl: '<path d="M12 5a7 7 0 1 1-6 10"/><path d="M6 17l-2-4 4-1"/>',
  liquify_pinch: '<path d="M4 12h5"/><path d="M20 12h-5"/><circle cx="12" cy="12" r="2"/>',
  eyedropper: '<path d="M4 20l2-6 8-8 4 4-8 8z"/>',
  move_object: '<path d="M12 4v16"/><path d="M4 12h16"/><path d="M12 4l-2 3h4z"/><path d="M12 20l-2-3h4z"/>',
  move_layer: '<rect x="4" y="8" width="10" height="10" rx="1"/><path d="M8 5h10a1 1 0 0 1 1 1v10"/><path d="M17 4l3 3-3 3"/>',
  // 手形（平移）✓：画布比窗口大时用它拖动 ✓（也可按住空格临时切换 ✓，与成熟软件一致 ✓）。
  pan: '<path d="M8 12V6.5a1.5 1.5 0 0 1 3 0V11"/><path d="M11 11V5.5a1.5 1.5 0 0 1 3 0V11"/><path d="M14 11V7a1.5 1.5 0 0 1 3 0v7"/><path d="M17 12v-1a1.5 1.5 0 0 1 3 0v4a5 5 0 0 1-5 5h-3a5 5 0 0 1-5-5v-3l-2 2"/>',
  select_rect: '<rect x="4" y="6" width="16" height="12" stroke-dasharray="3 2"/>',
  clearSelection: '<rect x="4" y="6" width="16" height="12" stroke-dasharray="3 2"/><path d="M7 17L17 7"/>',
  text: '<path d="M5 5h14"/><path d="M12 5v14"/><path d="M9 19h6"/>',
  medium_dab: '<path d="M12 3c4 5 6 7.5 6 10a6 6 0 0 1-12 0c0-2.5 2-5 6-10z"/>',
  mask_rect: '<rect x="4" y="6" width="16" height="12"/><path d="M4 12h16"/>',
  mask_ellipse: '<ellipse cx="12" cy="12" rx="8" ry="6"/><path d="M12 6v12"/>',
  fillLayer: '<path d="M5 12l7-7 7 7-7 7z"/><path d="M20 15c1 2 1 3 0 4"/>',
};
// 顺序按用途分组 ✓：绘制 → 修图 → 液化 → 取色/移动 → 选区/蒙版 → 文本/介质 → 填充。
// `keys` 是行业惯例的快捷键 ✓（B 画笔、E 橡皮、M 选区、T 文本、V 移动、I 吸管、G 填充… ✓）。
const TOOL_DEFS = [
  { tool: "brush", label: "画笔", key: "b" },
  { tool: "rect", label: "矩形", key: "u" },
  { tool: "ellipse", label: "椭圆", key: "o" },
  { tool: "erase", label: "橡皮", key: "e" },
  { tool: "clone_stamp", label: "仿制", key: "s" },
  { tool: "heal_stamp", label: "修复", key: "j" },
  { tool: "smudge", label: "涂抹", key: "r" },
  { tool: "liquify_push", label: "液化推", key: "" },
  { tool: "liquify_twirl", label: "液化旋", key: "" },
  { tool: "liquify_pinch", label: "液化缩", key: "" },
  { tool: "eyedropper", label: "吸管", key: "i" },
  { tool: "move_object", label: "移动对象", key: "v" },
  // 移动**整层** ✓ —— 用户诉求：画在一起的东西应该一起走 ✓（设计里"成组"是独立特性 ✓，
  // 图层级移动是它的实用等价 ✓，且实现上是**同一变更集里的 N 个 move 原子** ⇒ 一次撤销 ✓）。
  { tool: "move_layer", label: "移动图层", key: "y" },
  // 平移画布 ✓（H 键 ✓；空格按住时为**临时**手形 ✓）。
  { tool: "pan", label: "平移画布", key: "h" },
  { tool: "select_rect", label: "选区", key: "m" },
  { id: "clearSelection", label: "清除选区", key: "d", icon: "clearSelection" },
  { tool: "text", label: "文本", key: "t" },
  { tool: "medium_dab", label: "介质", key: "" },
  { tool: "mask_rect", label: "矩形蒙版", key: "" },
  { tool: "mask_ellipse", label: "椭圆蒙版", key: "" },
  { id: "fillLayer", label: "填充图层", key: "g", icon: "fillLayer" },
];
function renderToolStrip() {
  const nav = document.getElementById("tools");
  if (!nav) return;
  nav.innerHTML = TOOL_DEFS.map((def) => {
    const key = def.icon || def.tool || def.id;
    const hint = def.key ? def.label + " (" + def.key.toUpperCase() + ")" : def.label;
    const attrs = def.tool ? ` data-tool="${def.tool}"` : ` id="${def.id}"`;
    return `<button${attrs} title="${hint}" aria-label="${hint}" aria-pressed="false">` +
      `<svg viewBox="0 0 24 24" aria-hidden="true">${TOOL_ICONS[key] || ""}</svg></button>`;
  }).join("");
}
renderToolStrip();

// **面板可见性与全屏画布** ✓（用户要求：左侧工具栏与右侧各窗口都能隐藏 ✓，并能进全屏画布 ✓）。
//
// 三条设计取舍 ✓：
// ① **状态放在 `body` 的类上** ✓（`hide-rail` / `hide-dockers` / `zen` ✓）——
//    CSS 里四个选择器穷尽三种组合 ✓ ⇒ JS 只负责"加类 / 去类" ✓，不拼样式 ✓（两处打架是排版 bug 的常见来源 ✓）。
// ② **持久化到 localStorage** ✓（与本文件既有的焦点 / dockers 记忆一致 ✓）——
//    用户把面板收起来是个**意图** ✓，刷新后弹回来会很烦 ✓。
// ③ **全屏是"页内全屏"** ✗ 不调用浏览器 Fullscreen API ✓：页内模式不打断用户的全屏状态 ✓、
//    不需要用户手势 ✓、也不会在检查脚本里造成"到底谁在控制"的歧义 ✓
//    （真需要浏览器全屏时用 F11 ✓，两者互不冲突 ✓）。
const PANEL_STORE = "yanshi.panels";
const panels = { rail: false, dockers: false, zen: false };
function loadPanels() {
  try {
    const saved = JSON.parse(localStorage.getItem(PANEL_STORE) || "{}");
    panels.rail = saved.rail === true;
    panels.dockers = saved.dockers === true;
    panels.zen = saved.zen === true;
  } catch (_) { /* 存储损坏时用默认值 ✓，不影响使用 ✓ */ }
}
function applyPanels() {
  document.body.classList.toggle("hide-rail", panels.rail);
  document.body.classList.toggle("hide-dockers", panels.dockers);
  document.body.classList.toggle("zen", panels.zen);
  const rail = document.getElementById("toggleRail");
  const dockers = document.getElementById("toggleDockers");
  const zen = document.getElementById("toggleZen");
  // **全屏时左右都被藏起来** ✓ ⇒ 两个开关如实显示为"已按下" ✓（而不是显示未按下却看不见面板 ✗）。
  if (rail) rail.setAttribute("aria-pressed", String(panels.zen || panels.rail));
  if (dockers) dockers.setAttribute("aria-pressed", String(panels.zen || panels.dockers));
  if (zen) zen.setAttribute("aria-pressed", String(panels.zen));
  try { localStorage.setItem(PANEL_STORE, JSON.stringify(panels)); } catch (_) {}
  // 画布尺寸变了 ✓ ⇒ 重新适配视口 ✓（否则全屏后画布还按旧宽度居中 ✓，看起来"没生效" ✗）。
  if (typeof clampViewport === "function") clampViewport();
}
function setupPanels() {
  loadPanels();
  const wire = (id, apply) => {
    const button = document.getElementById(id);
    if (button) button.addEventListener("click", () => { apply(); applyPanels(); });
  };
  wire("toggleRail", () => {
    // 在全屏里点"工具栏" ⇒ **退出全屏并只显示工具栏** ✓（比"按了没反应"直观 ✓）。
    if (panels.zen) { panels.zen = false; panels.rail = false; panels.dockers = true; }
    else panels.rail = !panels.rail;
  });
  wire("toggleDockers", () => {
    if (panels.zen) { panels.zen = false; panels.dockers = false; panels.rail = true; }
    else panels.dockers = !panels.dockers;
  });
  wire("toggleZen", () => { panels.zen = !panels.zen; });
  const exit = document.getElementById("zenExit");
  if (exit) exit.addEventListener("click", () => { panels.zen = false; applyPanels(); });
  window.addEventListener("keydown", (event) => {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === "Tab") {
      // **Tab 切换全屏** ✓（与图像软件的直觉一致 ✓）—— 必须 `preventDefault` ✓，
      // 否则浏览器会去移动焦点 ✓（那会让"按了 Tab 界面乱跳" ✗）。
      event.preventDefault();
      panels.zen = !panels.zen;
      applyPanels();
      return;
    }
    if (event.key === "[") { event.preventDefault(); panels.rail = !panels.rail; applyPanels(); return; }
    if (event.key === "]") { event.preventDefault(); panels.dockers = !panels.dockers; applyPanels(); return; }
    if (event.key === "Escape" && panels.zen) {
      // **Esc 退出全屏** ✓ —— 但**不吞掉**其他 Esc 语义 ✓（快捷面板的关闭在自己的处理器里 ✓）。
      panels.zen = false;
      applyPanels();
    }
  });
  applyPanels();
}
setupPanels();

// 快捷键 ✓：与工具提示一致 ✓ —— 输入框里打字时不受影响 ✓。
const TOOL_BY_KEY = new Map(TOOL_DEFS.filter((d) => d.key).map((d) => [d.key, d]));
window.addEventListener("keydown", (event) => {
  if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
  if (event.metaKey || event.ctrlKey || event.altKey) return;
  const def = TOOL_BY_KEY.get(event.key.toLowerCase());
  if (!def) return;
  const selector = def.tool ? `button[data-tool="${def.tool}"]` : `#${def.id}`;
  const button = document.querySelector(selector);
  if (!button) return;
  event.preventDefault();
  button.click();
});

// 可折叠 Dockers + 工作区预设 ✓（界面上借鉴 Krita/Photoshop 的 Workspaces ✓）。
//
// 状态放在 `localStorage` ✓：刷新后布局保持 ✓（这是"工作区"的意义 ✓）。
// 折叠靠 CSS class ✓，不改 DOM 结构 ✓ ⇒ 既有选取器与检查都不受影响 ✓。
const DOCKER_PRESETS = {
  // 绘画：图层与内核常看 ✓，历史/调整/日志收起 ✓。
  paint: { open: ["图层", "WASM 计算内核"], closed: ["历史（原子日志）", "调整 / 滤镜", "缩略图", "原子日志（控制流）", "最近一次响应", "反馈"] },
  // 修图：图层 + 调整展开 ✓。
  retouch: { open: ["图层", "调整 / 滤镜", "缩略图"], closed: ["WASM 计算内核", "历史（原子日志）", "原子日志（控制流）", "最近一次响应", "反馈"] },
  // 校对：历史 + 日志 + 反馈展开 ✓（核对与反馈用 ✓）。
  review: { open: ["历史（原子日志）", "原子日志（控制流）", "反馈", "最近一次响应"], closed: ["调整 / 滤镜", "WASM 计算内核", "缩略图"] },
};
const DOCKER_STORE = "yanshi.dockers";
const WORKSPACE_STORE = "yanshi.workspace";

function dockerCards() {
  return [...document.querySelectorAll("aside .card")];
}

// Docker 标题的**归一化** ✓ —— 预设字符串与 DOM 文本必须逐字一致才能匹配 ✓，
// 而"全角/半角括号、空格、不可见空白"的差异会让匹配**静默失败** ✗：
// 现象正是"工作区切了、但面板没折叠" ✓（截图核验发现"绘画"预设内核卡片仍折叠 ✓）。
// 归一化：去掉所有空白 ✓，并把全角括号与全角斜杠折算成半角 ✓。
function normalizeTitle(text) {
  return String(text || "")
    .replace(/\s+/g, "")
    .replace(/（/g, "(")
    .replace(/）/g, ")")
    .replace(/／/g, "/");
}

function cardTitle(card) {
  const h2 = card.querySelector("h2");
  return normalizeTitle(h2 ? h2.textContent : "");
}

function saveDockers() {
  try {
    localStorage.setItem(DOCKER_STORE, JSON.stringify(
      dockerCards().filter((card) => card.classList.contains("collapsed")).map(cardTitle),
    ));
  } catch (_) { /* 隐私模式下忽略 ✓ */ }
}

function applyCollapsed(titles) {
  const wanted = titles.map(normalizeTitle);
  for (const card of dockerCards()) {
    card.classList.toggle("collapsed", wanted.includes(cardTitle(card)));
  }
}

function applyWorkspace(name) {
  const preset = DOCKER_PRESETS[name];
  if (!preset) return;
  for (const card of dockerCards()) {
    card.classList.remove("collapsed");
  }
  applyCollapsed(preset.closed);
  try { localStorage.setItem(WORKSPACE_STORE, name); } catch (_) { /* 忽略 ✓ */ }
  const select = $("workspace");
  if (select) select.value = name;
  saveDockers();
}

function initDockers() {
  for (const card of dockerCards()) {
    const h2 = card.querySelector("h2");
    if (!h2) continue;
    h2.addEventListener("click", () => {
      card.classList.toggle("collapsed");
      saveDockers();
      // 手动折叠后视为"自定义" ✓：工作区选择器不再声称某个预设 ✓。
      try { localStorage.removeItem(WORKSPACE_STORE); } catch (_) { /* 忽略 ✓ */ }
    });
  }
  let saved = null;
  try { saved = JSON.parse(localStorage.getItem(DOCKER_STORE) || "null"); } catch (_) { saved = null; }
  let workspace = null;
  try { workspace = localStorage.getItem(WORKSPACE_STORE); } catch (_) { workspace = null; }
  if (workspace && DOCKER_PRESETS[workspace]) {
    applyWorkspace(workspace);
  } else if (Array.isArray(saved)) {
    applyCollapsed(saved);
  } else {
    applyWorkspace("paint");
  }
  const select = $("workspace");
  if (select) {
    select.addEventListener("change", () => applyWorkspace(select.value));
  }
}

for (const button of document.querySelectorAll("button[data-tool]")) {
  button.addEventListener("click", async () => {
    const tool = button.dataset.tool;
    if (tool === "undo") {
      await undoOnce();
      return;
    }
    if (tool === "redo") {
      await redoOnce();
      return;
    }
    if (tool === "refresh") { await refreshPreview(); refreshThumb(); return; }
    if (tool === "check") { await checkBitExact(); return; }
    state.tool = tool;
    for (const other of document.querySelectorAll("button[data-tool]")) {
      other.setAttribute("aria-pressed", String(other === button));
    }
    // 工具切换后刷新光标 ✓（否则抓手会留在画笔上 ✓），并收起手形拖动状态 ✓。
    panState = null;
    updatePanCursor();
  });
}

// 光标处快捷面板 ✓（借鉴 Krita Pop-up Palette ✓）：右键在光标处弹出 ✓，
// 内含**介质 / 常用颜色 / 笔尖大小** ✓ 与**我们的快捷动作** ✓（撤销/重做/清除选区/导出 ✓）。
//
// 设计要点 ✓：
// * 面板**只驱动既有控件** ✓（`#medium`/`#color`/`#size` ✓）⇒ 单一真源 ✓，不另存一份状态 ✓；
// * 位置用 `position: fixed` + 光标坐标 ✓，并**夹在视口内** ✓（贴边右键也不会跑出去 ✓）；
// * Esc / 点击别处 / 选中即关 ✓（弹出面板不该留在屏幕上 ✓）。
const QUICK_COLORS = [
  "#111111", "#ffffff", "#c81e3c", "#e08600",
  "#2f9e44", "#2f5fbf", "#7048e8", "#8a5a2b",
];
const QUICK_SIZES = [4, 12, 30, 60];

function quickPanelVisible() {
  const panel = $("quickPanel");
  return panel && !panel.hidden;
}

function closeQuickPanel() {
  const panel = $("quickPanel");
  if (panel) panel.hidden = true;
}

function openQuickPanel(clientX, clientY) {
  const panel = $("quickPanel");
  if (!panel) return;
  const mediums = $("qpMediums");
  const colors = $("qpColors");
  const sizes = $("qpSizes");
  // 介质：直接读 `MEDIUMS` ✓（同一份定义 ✓）。
  mediums.innerHTML = Object.keys(MEDIUMS).map((key) => {
    const spec = MEDIUMS[key];
    const active = $("medium") && $("medium").value === key;
    return `<button type="button" data-qp-medium="${key}" aria-pressed="${active}">${spec.id}</button>`;
  }).join("");
  colors.innerHTML = QUICK_COLORS.map((value) =>
    `<button type="button" class="qp-swatch" data-qp-color="${value}" title="${value}" aria-pressed="${$("color") && $("color").value === value}" style="background:${value}"></button>`,
  ).join("");
  sizes.innerHTML = QUICK_SIZES.map((value) =>
    `<button type="button" data-qp-size="${value}" aria-pressed="${Number($("size") && $("size").value) === value}">${value}</button>`,
  ).join("");
  panel.hidden = false;
  // 夹在视口内 ✓（先显示再量尺寸 ✓）。
  const rect = panel.getBoundingClientRect();
  const left = Math.max(6, Math.min(clientX, window.innerWidth - rect.width - 6));
  const top = Math.max(6, Math.min(clientY, window.innerHeight - rect.height - 6));
  panel.style.left = left + "px";
  panel.style.top = top + "px";
}

function initQuickPanel() {
  const panel = $("quickPanel");
  if (!panel) return;
  board.addEventListener("contextmenu", (event) => {
    // 画布右键是**我们的**快捷面板 ✓ ⇒ 屏蔽浏览器菜单 ✓。
    event.preventDefault();
    openQuickPanel(event.clientX, event.clientY);
  });
  panel.addEventListener("click", (event) => {
    const target = event.target.closest("button");
    if (!target) return;
    const medium = target.getAttribute("data-qp-medium");
    const color = target.getAttribute("data-qp-color");
    const size = target.getAttribute("data-qp-size");
    if (medium) {
      $("medium").value = medium;
      $("medium").dispatchEvent(new Event("change", { bubbles: true }));
      log("快捷面板：介质 " + MEDIUMS[medium].id + " v" + MEDIUMS[medium].version);
    } else if (color) {
      $("color").value = color;
      $("color").dispatchEvent(new Event("change", { bubbles: true }));
      log("快捷面板：颜色 " + color);
    } else if (size) {
      $("size").value = size;
      $("size").dispatchEvent(new Event("input", { bubbles: true }));
      log("快捷面板：笔尖 " + size);
    } else {
      return; // 动作按钮自己处理 ✓
    }
    closeQuickPanel();
  });
  $("qpUndo").addEventListener("click", () => { closeQuickPanel(); void undoOnce(); });
  $("qpRedo").addEventListener("click", () => { closeQuickPanel(); void redoOnce(); });
  $("qpClearSelection").addEventListener("click", () => {
    closeQuickPanel();
    const button = $("clearSelection");
    if (button) button.click();
  });
  $("qpExport").addEventListener("click", () => {
    closeQuickPanel();
    const button = $("exportPng");
    if (button) button.click();
  });
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && quickPanelVisible()) closeQuickPanel();
  });
  document.addEventListener("pointerdown", (event) => {
    if (!quickPanelVisible()) return;
    if (!panel.contains(event.target)) closeQuickPanel();
  });
}

initDockers();
// **图层面板接线** ✓（面板本身由 `refreshLayers()` 渲染 ✓；这里只接**一次**监听器 ✓ ——
// 写在重画里会让点一次触发多次 ✗，本项目抓到过同类问题 ✓）。
void setupLayerPanel();
// 首次同步撤销/重做按钮的可用状态 ✓（HTML 里已先禁用 ✓，这里再按真实栈同步一次 ✓）——
// 子 agent 报："没有撤销栈时按钮仍可点" ✗（点了只打印一句提示 ✓，看起来像坏了 ✓）。
updateUndoStatus();
// 让"强度 / 湿度"标签**随介质说真话** ✓（见 HTML 里的说明 ✓）：
// 该滑杆在插件介质下喂的是 `wetness` ✓ ⇒ 越大越湿、颜色越淡 ✓，
// 继续叫"强度"会让人以为越大越浓 ✗。
{
  const strengthSelect = $("medium");
  if (strengthSelect) {
    strengthSelect.addEventListener("change", syncStrengthLabel);
    syncStrengthLabel();
  }
}
initQuickPanel();

$("addLayer").addEventListener("click", async () => {
  const layerId = "layer_" + ulid();
  const created = await callTool("create_layer", { layer_id: layerId, name: "layer" }, { refresh: false });
  if (!created.ok) {
    log("新建图层失败：" + (created.error_code || "unknown"), "#c33");
    return;
  }
  await refreshLayers();
  const select = $("layer");
  select.value = layerId;
  state.layerId = layerId;
  log("已新建图层 " + layerId);
});

/// 导出整幅 PNG：显式请求整幅区域渲染（设计 A 下整幅 PNG 只在**显式导出**时生成），
/// 再把服务端改写过的可直接 GET 的地址交给浏览器下载。
$("exportPng").addEventListener("click", async () => {
  const { w, h } = state.docSize;
  if (!w || !h) {
    log("导出失败：文档尺寸未知", "#c33");
    return;
  }
  const value = await callTool("render_region", { region: { x: 0, y: 0, w, h } }, { refresh: false });
  if (!value.ok || !value.thumb_url) {
    log("导出失败：" + (value.error_code || "no url"), "#c33");
    return;
  }
  if (value.width !== w || value.height !== h) {
    log("导出警告：返回 " + value.width + "×" + value.height + "，期望 " + w + "×" + h, "#c33");
  }
  const link = document.createElement("a");
  link.href = value.thumb_url;
  link.download = (state.docId || "yanshi") + ".png";
  document.body.appendChild(link);
  link.click();
  link.remove();
  window.yanshiStats.lastExport = { url: value.thumb_url, width: value.width, height: value.height, bytes: value.bytes };
  log("已导出 PNG：" + value.width + "×" + value.height + "（" + (value.bytes || 0) + " 字节）");
});

$("zoomFit").addEventListener("click", () => {
  state.zoom = 1;
  clampViewport();
  renderViewport();
});

$("zoomActual").addEventListener("click", () => {
  // 1:1：显示比例 1 像素文档 = 1 CSS 像素。
  if (!state.docSize) return;
  state.zoom = 1 / Math.max(0.0001, state.displayScale || 1);
  clampViewport();
  renderViewport();
});

$("fillLayer").addEventListener("click", fillCurrentLayer);
$("effectKind").addEventListener("change", fillEffectNames);
$("effectApply").addEventListener("click", applyEffect);
$("historyReload").addEventListener("click", refreshHistory);
$("historyKind").addEventListener("change", refreshHistory);
$("historyActor").addEventListener("change", refreshHistory);

$("newDoc").addEventListener("click", newDocument);
$("newCancel").addEventListener("click", () => {
  const dialog = $("newDialog");
  if (typeof dialog.close === "function") dialog.close();
  else dialog.removeAttribute("open");
});
$("newCreate").addEventListener("click", createNamedDocument);
$("newName").addEventListener("keydown", (event) => {
  if (event.key === "Enter") void createNamedDocument();
});
$("openDoc").addEventListener("click", showOpenDialog);
$("clearSelection").addEventListener("click", async () => {
  await clearSelection();
  // **清除之后同样刷新状态栏** ✓ —— 我第一版只在创建时刷新 ✗，
  // 于是"清除选区"后状态栏仍写着有选区 ✓（检查当场抓到 ✓）。
  await refreshSelectionHint();
});
$("openClose").addEventListener("click", closeOpenDialog);
$("copyDoc").addEventListener("click", async () => {
  const name = ($("copyName").value || "").trim();
  if (!name) {
    log("另存为：请先填新文档 id", "#c33");
    return;
  }
  if (name === state.docId) {
    log("另存为：新 id 不能与当前文档相同", "#c33");
    return;
  }
  // 源文档要用**它自己的令牌**授权（目标文档的令牌管不到源文档）✓。
  const response = await fetch("/api/documents", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: name, copy_from: state.docId, from_token: state.token }),
  }).then((value) => value.json());
  if (!response.ok) {
    log("另存为失败：" + (response.error_code || response.context?.detail || "unknown"), "#c33");
    return;
  }
  log("已另存为副本 " + response.doc_id + "（复制 " + response.copied_atoms + " 个原子，原文档保留）");
  closeOpenDialog();
  await switchDocument(response.doc_id, response.token);
});
$("importFile").addEventListener("change", async (event) => {
  const file = event.target.files && event.target.files[0];
  if (!file) return;
  closeOpenDialog();
  await importLocalImage(file);
  event.target.value = "";
});

(async () => {
  window.addEventListener("resize", () => {
    if (kernelReady() && state.docSize) {
      clampViewport();
      renderViewport();
    } else {
      syncOverlayGeometry();
    }
    if (state.socket) subscribeViewport();
  });
  window.addEventListener("scroll", syncOverlayGeometry, { passive: true });
  if (!state.token) {
    await ensureDocument();
  } else {
    $("identity").textContent = state.docId;
    await refreshLayers();
    await refreshThumb();
    await loadEffectCatalog();
    await refreshEffects();
    await refreshHistory();
    window.yanshiStats.bootAt = performance.now();
    await refreshPreview();
    connect();
    refreshContactLink();
    void warmKernel();
  }
})();
</script>
</body>
</html>
"##;

/// 页面长度（测试与可观测性）。
pub fn page_len() -> usize {
    PAGE.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_is_self_contained_and_uses_the_documented_endpoints() {
        assert!(PAGE.starts_with("<!DOCTYPE html>"));
        assert!(PAGE.trim_end().ends_with("</html>"));
        for needle in [
            "/api/documents",
            "/api/tools/",
            "/api/blob",
            "/ws?doc=",
            "subscribe",
            "render_region",
            "draw_stroke",
            "draw_shape",
            "revert",
            "reapply",
            "token",
            "viewport",
        ] {
            assert!(PAGE.contains(needle), "查看器缺少 {needle}");
        }
        // 无外部依赖（不加载 CDN 脚本或字体）。
        assert!(!PAGE.contains("http://cdn"));
        assert!(!PAGE.contains("https://cdn"));
        assert!(!PAGE.contains("<script src="));
        assert!(page_len() > 4000);
    }

    /// 回归：内联脚本里 `const preview = $("preview")` 与 `async function preview()`
    /// 曾经同名冲突，导致整页 JS 直接 SyntaxError（浏览器里白屏）。
    ///
    /// 只检查**顶层**（花括号深度 0）声明：函数/块内的同名变量是合法的遮蔽。
    #[test]
    fn viewer_script_has_no_duplicate_top_level_declarations() {
        let script = PAGE
            .split_once("<script>")
            .and_then(|(_, rest)| rest.split_once("</script>"))
            .map(|(script, _)| script)
            .expect("页面含内联脚本");

        let mut names: Vec<&str> = Vec::new();
        let mut depth: i32 = 0;
        for raw_line in script.lines() {
            // 去掉行注释后统计花括号深度（跳过字符串字面量里的括号）。
            let line = match raw_line.split_once("//") {
                Some((code, _)) if !code.contains('"') && !code.contains('\'') => code,
                _ => raw_line,
            };
            if depth == 0 {
                let trimmed = line.trim_start();
                for prefix in ["const ", "let ", "var ", "async function ", "function "] {
                    if let Some(rest) = trimmed.strip_prefix(prefix) {
                        let name = rest
                            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                            .next()
                            .unwrap_or("");
                        if !name.is_empty() {
                            names.push(name);
                        }
                        break;
                    }
                }
            }
            let mut in_string: Option<char> = None;
            let mut escaped = false;
            for character in line.chars() {
                match in_string {
                    Some(quote) => {
                        if escaped {
                            escaped = false;
                        } else if character == '\\' {
                            escaped = true;
                        } else if character == quote {
                            in_string = None;
                        }
                    }
                    None => match character {
                        '"' | '\'' => in_string = Some(character),
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    },
                }
            }
        }

        let mut sorted = names.clone();
        sorted.sort_unstable();
        let mut duplicates: Vec<&str> = Vec::new();
        for pair in sorted.windows(2) {
            if pair[0] == pair[1] && !duplicates.contains(&pair[1]) {
                duplicates.push(pair[1]);
            }
        }
        assert!(
            duplicates.is_empty(),
            "查看器脚本顶层存在重复声明（会导致 SyntaxError）：{duplicates:?}"
        );
        // **JS ↔ wasm 接口面一致性**：查看器里调用的每个 `state.kernel.<方法>` 都必须在
        // wasm 绑定里真实存在。实际缺陷：查看器长期调用 `render_region_direct_rgba`，而该方法
        // 从未实现 —— 浏览器抛 "not a function" 被事件处理器吞掉，表现为「拖动无反馈、
        // 操作后画布空白」。这类名字不匹配在 Rust 侧编译期发现不了，必须在这里拦住。
        let wasm_bindings = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../yanshi-wasm/src/lib.rs"),
        )
        .expect("应能读取 wasm 绑定源码");
        let exported: Vec<&str> = wasm_bindings
            .lines()
            .filter_map(|line| line.trim().strip_prefix("pub fn "))
            .filter_map(|rest| rest.split(['(', '<']).next())
            .collect();
        let mut missing: Vec<String> = Vec::new();
        let mut rest = script;
        while let Some(index) = rest.find("state.kernel.") {
            rest = &rest[index + "state.kernel.".len()..];
            let name: String = rest
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .collect();
            // 只检查方法调用（后面紧跟括号），跳过属性读取。
            if rest[name.len()..].starts_with('(')
                && !name.is_empty()
                && !exported.contains(&name.as_str())
                && !missing.contains(&name)
            {
                missing.push(name);
            }
        }
        assert!(
            missing.is_empty(),
            "查看器调用了 wasm 绑定里不存在的方法：{missing:?}（浏览器里会抛 not a function）"
        );
        assert!(script.contains("async function refreshPreview("));
        assert!(names.len() > 15, "顶层声明数量异常：{names:?}");
    }
}

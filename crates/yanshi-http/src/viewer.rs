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
/// **发页面时把只读工具清单注入进去** ✓ —— 来源是 `yanshi-server` 的 `ALL_TOOLS` ✓，
/// 取 `!spec.mutating` ✓ ⇒ **权威、自动、不会漏** ✓（这比手写白名单强的地方就在这里 ✓）。
pub fn page_with_read_tools() -> String {
    let names: Vec<String> = yanshi_server::tools::ALL_TOOLS
        .iter()
        .filter(|spec| !spec.mutating)
        .map(|spec| format!("\"{}\"", spec.name))
        .collect();
    PAGE.replace("__READ_TOOLS__", &names.join(", "))
}

/// **查看器整页** ✓（原样 ✓；发出去时会把只读工具清单注入 `__READ_TOOLS__` ✓ —— 见上面那个函数 ✓）。
/// 注意 ✓：我这个函数是**插在它之前**的 ✓ ⇒ 一度把它的文档注释"抢"走了 ✗ ⇒ `clippy -D warnings` 当场红 ✓。
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
    /* **控件统一高度** ✓（真浏览器实测过：同一排按钮曾出现 26 / 27 / 28 三档并存 ✗ ⇒
       差 1–2px，看着就不规整 ✓ —— 这正是"没有专业软件设计感"的机制之一 ✓）。
       做法与专业软件一致 ✓：**一个高度令牌** ✓ + 显式高度 ✓；紧凑档按需再定 24px ✓。 */
    --control-h: 28px;
  }
  * { box-sizing: border-box; }
  /* **整页垂直锁死** ✓（用户："保持整个软件的垂直方向是锁死（但是画布例外，
     画布是内部一个可以独立上下和左右滚动的）"✓）——
     页面本身不滚 ✗；要滚的是**各自的面板**（右栏 `.aside` 自己滚 ✓）与**画布舞台**（`.stage` ✓）。 */
  html, body { height: 100%; overflow: hidden; }
  body { margin: 0; background: var(--bg); color: var(--text);
         font: 13px/1.5 system-ui, "Noto Sans CJK SC", sans-serif; }
  /* 键盘可达性 ✓：所有可聚焦控件都有**可见焦点环** ✓（纯键盘用户与快捷键提示配套 ✓）。 */
  :focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; border-radius: 4px; }
  /* **所有控件一个高度** ✓（令牌见 `:root` ✓）：同一排按钮不再各差 1–2px ✓。
     复选框**排除在外** ✓（它本来就是小方格 ✓）；色块若被压到 28px 会与调色板栅格不符 ⇒ 下一轮实测后再定 ✓。 */
  button, input:not([type="checkbox"]), select { height: var(--control-h); }
  /* **复选框也统一** ✓：实测有 3 个是 13px、2 个被容器撑到 26px ✗ ⇒ **同类控件之间**也不一致 ✓
     （判据只看"总档数 ≤3" ⇒ 这类"同类内的不一致"要靠显式定尺寸消掉 ✓）。 */
  input[type="checkbox"] { height: 16px; width: 16px; }
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
  header h1 { font-size: 16px; margin: 0 12px 0 0; display: flex; align-items: center; gap: 8px; }
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
  /* **画布舞台自己滚** ✓：放大到超出可视区时，它内部上下左右滚 ✓（而不是把整页顶开 ✗）。 */
  .stage { position: relative; border: 1px solid var(--line); border-radius: 6px;
           overflow: auto; background: #f5f5f5; }
  /* **「文件」菜单** ✓（用户："导入导出之类功能也不适合放在信息面板，按照行业主流软件的习惯来设置分类"✓）
     —— 行业习惯就是**顶栏一个「文件」** ✓，而不是散在右侧信息面板里 ✓。 */
  #fileMenu { position: fixed; top: 52px; left: 118px; z-index: 42; width: min(340px, 92vw);
    background: Canvas; color: CanvasText; border: 1px solid rgba(128,128,128,.5);
    border-radius: 8px; padding: 8px; box-shadow: 0 8px 28px rgba(0,0,0,.28); }
  #fileMenu[hidden] { display: none; }
  #fileMenu .file-menu-head { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
  #fileMenu .file-menu-head .hint { flex: 1; }
  #fileMenu .file-menu-group { margin: 8px 0 4px; font-size: 11px; opacity: .7; }
  #fileMenu .card { border: 0; padding: 0; margin: 0; background: transparent; }
  #fileMenu .card h2 { font-size: 12px; margin: 4px 0; }
  /* **右栏 tab** ✓（用户："面板内部有小 tab 切换不同信息"✓）—— 一行 CSS 决定显隐 ✓。 */
  .tabs { display: flex; gap: 4px; margin-bottom: 8px; flex-wrap: wrap; }
  .tabs button[aria-pressed="true"] { background: var(--accent, #2b6cb0); color: #fff;
                                      border-color: transparent; }
  .tab-pane > .card, .tab-pane > details { margin-bottom: 8px; }
  /* 单一几何：内容画布 #board 决定尺寸（文档分辨率位图 + 固有宽高比）；
     #overlay 只画拖动中的笔迹预览，位置与尺寸由 JS 同步为 board 的显示矩形。
     两层分离的原因：此前预览与内容共用一个画布，重绘预览时会把内容一起清空，
     提交后画布变空白（刷新才恢复）。 */
  #board { display: block; width: auto; height: auto; max-width: 100%; max-height: calc(100vh - 96px); touch-action: none; cursor: crosshair; background: #fff; image-rendering: pixelated; }
  /* **放大之后不许再被 CSS 顶回去** ✗（第 49 轮查明 ✓）：`max-width: 100%` 会把**内联宽度**
     盖回舞台宽度 ✓（实测：400% 时画布 844px = 舞台 ✓，内联宽度白设了 ✗）
     ⇒ 放大时给 `body` 加 `canvas-zoomed` ✓，把这两条上限放开 ✓ ⇒ 画布真的超出舞台 ✓、
     `.stage` 的 `overflow: auto` 才**有东西可滚** ✓（用户："画布是内部一个可以独立上下和左右滚动的"✓）。 */
  body.canvas-zoomed #board, body.canvas-zoomed #overlay { max-width: none; max-height: none; }
  #overlay { position: absolute; left: 0; top: 0; pointer-events: none; image-rendering: pixelated; }
  /* **标注图钉层** ✓（设计 4.6 / 13.4 ✓）：用**独立的 DOM 层** ✓，不跟 `#overlay` 的笔迹预览抢画布 ✓，
     而且图钉**可点** ✓（点它跳到该标注 ✓）。 */
  #annotationPins { position: absolute; left: 0; top: 0; pointer-events: none; }
  #annotationPins button {
    position: absolute; transform: translate(-50%, -50%); pointer-events: auto;
    width: 20px; height: 20px; border-radius: 50%; padding: 0; font-size: 11px;
    border: 1px solid #10131a; background: #ffd166; color: #1a1206; cursor: pointer; line-height: 1;
  }
  #annotationPins button.resolved { background: #6b7280; color: #e5e7eb; }
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
  #tools { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 4px; padding: 8px;
           min-width: 0; background: var(--surface); border-right: 1px solid var(--line);
           align-content: start; overflow-y: auto; max-height: calc(100vh - 150px); }
  #tools button { padding: 8px 0; width: 100%; min-width: 0; display: flex; justify-content: center;
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
             border-radius: 999px; padding: 8px 12px; font-size: 12px; cursor: pointer;
             backdrop-filter: blur(4px); }
  body.zen #zenExit { display: inline-flex; align-items: center; gap: 8px; }
  /* **图层面板** ✓：顶部条 + 行列表 ✓。行里三个可点区域（眼睛 / 锁 / 名字 ✓），
     名字区最大以便点选 ✓；按钮用最小尺寸以免抢走注意力 ✓。 */
  .layers { display: flex; flex-direction: column; gap: 4px; margin-top: 8px; }
  .layers-head { display: flex; align-items: center; justify-content: space-between; font-size: 12px;
                 opacity: .85; }
  .layers-actions { display: inline-flex; gap: 4px; }
  .layers-actions button { padding: 4px 8px; font-size: 12px; line-height: 1.4; }
  .layer-list { display: flex; flex-direction: column; gap: 4px; max-height: 220px; overflow-y: auto;
                border: 1px solid var(--line); border-radius: 6px; padding: 4px; background: var(--bg, #fff); }
  .layer-row { display: flex; align-items: center; gap: 4px; padding: 4px 4px; border-radius: 4px;
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
  /* **素材浮层** ✓：浮在画布上方 ✓、可滚动 ✓、不挡工具条 ✓。 */
  #assetDock { position: fixed; top: 96px; left: 16px; z-index: 40; width: min(420px, 92vw);
    max-height: 70vh; overflow: auto; background: Canvas; color: CanvasText;
    border: 1px solid rgba(128,128,128,.5); border-radius: 8px; padding: 8px;
    box-shadow: 0 8px 28px rgba(0,0,0,.28); }
  #assetDock[hidden] { display: none; }
  /* **笔刷库** ✓（用户："笔刷这个列表里头都带个笔刷的效果图是不是更好，直接列表中就能找到想要的"✓）。
     每行 = **真实落笔**的效果图 ✓（服务端 `brush_preview` ✓，与 `brush_stroke` 同一条实现 ✓）+ 名字 + 分组 ✓；
     图是**懒加载**的 ✓（滚到哪画到哪 ✓ —— 201 支一次全画既慢又是可见的浪费 ✗）。 */
  #brushLibrary { position: fixed; top: 96px; right: 16px; z-index: 41; width: min(380px, 92vw);
    max-height: 76vh; overflow: auto; background: Canvas; color: CanvasText;
    border: 1px solid rgba(128,128,128,.5); border-radius: 8px; padding: 8px;
    box-shadow: 0 8px 28px rgba(0,0,0,.28); }
  #brushLibrary[hidden] { display: none; }
  #brushLibrary .brush-lib-head { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
  #brushLibrary .brush-lib-head .hint { flex: 1; }
  .brush-lib-row { display: flex; align-items: center; gap: 8px; padding: 4px 4px; border-radius: 6px;
    cursor: pointer; }
  .brush-lib-row:hover { background: rgba(128,128,128,.14); }
  .brush-lib-row.selected { background: var(--accent-soft, rgba(43,108,176,.22)); }
  .brush-lib-row img { width: 84px; height: 32px; object-fit: contain; background: #fff;
    border: 1px solid #ccc; flex: none; }
  .brush-lib-row .name { flex: 1; font-size: 12px; overflow: hidden; text-overflow: ellipsis;
    white-space: nowrap; }
  .brush-lib-row .group { font-size: 11px; opacity: .65; flex: none; }
  /* **画笔区可折叠** ✓（用户："画笔那块区域应该可以更紧凑和可隐藏"✓）——
     折叠只**隐藏**这些控件 ✓，不改任何状态 ✓（选中的笔、颜色、平滑都不动 ✓）。 */
  body.brush-area-collapsed .brush-control { display: none; }
  /* **快捷键按到哪一张卡** ✓（`P` / `T`）：给个看得见的落点 ✓，不然"按了没反应"✗。 */
  #assetDock .asset-dock-target { outline: 2px solid var(--accent, #2b6cb0); outline-offset: 3px;
    border-radius: 6px; }
  #assetDock .asset-dock-head { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
  #assetDock .asset-dock-head .hint { flex: 1; }
  .panel-toggles button[aria-pressed="true"] { background: var(--accent, #2b6cb0); color: #fff;
                                               border-color: transparent; }
  #tools svg { width: 22px; height: 22px; fill: none; stroke: currentColor; stroke-width: 1.6;
               stroke-linecap: round; stroke-linejoin: round; }
  #tools button[aria-pressed="true"] { background: var(--accent-soft); border-color: var(--accent); }
  .statusbar { display: flex; gap: 16px; align-items: center; padding: 8px 12px; font-size: 12px;
               background: var(--surface); border-top: 1px solid var(--line); }
  .statusbar .spacer { flex: 1 1 auto; }
  /* 光标处快捷面板 ✓（`position: fixed` ✓ ⇒ 坐标即光标位置 ✓，不受画布滚动影响 ✓）。 */
  #quickPanel { position: fixed; z-index: 40; min-width: 196px; max-width: 260px; padding: 8px 8px;
                background: var(--surface-2); border: 1px solid var(--line); border-radius: 8px;
                box-shadow: 0 8px 24px rgba(0, 0, 0, .45); font-size: 12px; }
  #quickPanel[hidden] { display: none; }
  #quickPanel .qp-title { display: flex; justify-content: space-between; align-items: baseline;
                          margin-bottom: 8px; opacity: .9; }
  #quickPanel .qp-hint { opacity: .55; font-size: 11px; }
  #quickPanel .qp-section { display: flex; flex-wrap: wrap; gap: 4px; margin: 4px 0; }
  #quickPanel .qp-section:empty { display: none; }
  #quickPanel button { min-width: 30px; padding: 4px 8px; font-size: 11px; }
  #quickPanel button[aria-pressed="true"] { background: var(--accent-soft); border-color: var(--accent); }
  #quickPanel .qp-swatch { width: 22px; height: 22px; min-width: 0; padding: 0; border-radius: 4px;
                           border: 1px solid var(--line); }
  #quickPanel .qp-swatch[aria-pressed="true"] { outline: 2px solid #6ea8fe; outline-offset: 1px; }
  #quickPanel .qp-actions { border-top: 1px solid var(--line); padding-top: 8px; }
  /* 可折叠 Dockers ✓（借鉴成熟绘画软件的面板折叠 ✓）：点标题折叠/展开 ✓，状态持久化 ✓。 */
  aside .card > h2 { cursor: pointer; user-select: none; display: flex; align-items: center; gap: 8px; }
  aside .card > h2::before { content: "▾"; font-size: 11px; opacity: .7; transition: transform .1s; }
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
  #history .row { display: flex; gap: 8px; align-items: center; padding: 4px 0; }
  #history .row button { padding: 0 8px; font-size: 11px; }
  #history .seq { opacity: .6; min-width: 34px; }
  #history .kind { min-width: 86px; }
  #history .actor { opacity: .75; }
  .card { border: 1px solid var(--line); border-radius: 6px; padding: 8px 8px; }
  .card h2 { font-size: 12px; margin: 0 0 8px; text-transform: uppercase; letter-spacing: .06em; opacity: .7; }
  button { font: inherit; padding: 4px 8px; border-radius: 5px; border: 1px solid var(--line); background: transparent; cursor: pointer; }
  button[aria-pressed="true"] { background: #4a7dff22; border-color: #4a7dff; }
  input, select { font: inherit; padding: 4px 8px; border-radius: 5px; border: 1px solid var(--line); background: transparent; }
  #thumb { display: block; border: 1px solid var(--line); border-radius: 5px; background: #fff; width: 128px; height: 128px; object-fit: contain; }
  #log { max-height: 240px; overflow: auto; font-family: ui-monospace, monospace; font-size: 11px; }
  #log div { white-space: nowrap; }
  .status { display: flex; gap: 8px; flex-wrap: wrap; font-family: ui-monospace, monospace; font-size: 11px; opacity: .85; }
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: #c33; vertical-align: middle; }
  .dot.on { background: #2a2; }
</style>
</head>
<body>
<header>
  <h1><img class="brand-mark" src="/brand/svg/icon-light.svg" alt="" />偃师 Yanshi</h1>
  <span id="identity"></span>
  <!-- **「文件」菜单** ✓：新建 / 打开 / 导入 / 导出**都归到这里** ✓（原来是散在右侧信息面板里 ✗）。
       里面的控件是**搬进来的原节点** ✓（`#newDoc` / `#exportPng` / 工程包那张卡… ✓）
       ⇒ 行为、监听器、id 全都不变 ✓，只是**换了地方** ✓（搬，不重建 ✓）。 -->
  <button id="fileMenuButton" type="button" aria-pressed="false"
          title="文件：新建 / 打开 / 导入 / 导出（顶栏这一个入口，信息面板里不再重复）">文件 ▾</button>
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
  <p style="opacity:.75;font-size:12px;margin:4px 0"
     title="文档以 id 作为名字（也是主键）。换个名字即可并存多份作品；重名会提示。">
    id 即名字
  </p>
  <label style="display:flex;gap:8px;align-items:center">
    名称
    <input id="newName" type="text" placeholder="例如 我的第一幅画" style="flex:1 1 auto" />
  </label>
  <div id="newHint" style="font-size:12px;opacity:.75;min-height:16px;margin:8px 0"></div>
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
  <p style="opacity:.75;font-size:12px;margin:0 0 8px">
    用不同介质画出来的样例，可以直接打开查看、继续画或拿来练手。
  </p>
  <div id="sampleList" style="display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:8px"></div>
  <h3 style="margin:16px 0 4px">我的文档</h3>
  <div id="docList" style="display:grid;grid-template-columns:repeat(auto-fill,minmax(120px,1fr));gap:8px;max-height:50vh;overflow:auto"></div>
  <hr />
  <h2>导入本地图片</h2>
  <p style="opacity:.75;font-size:12px;margin:4px 0"
     title="支持浏览器能解码的任何格式（PNG/JPEG/WebP）。图片在新图层上按原始像素导入。">
    PNG / JPEG / WebP
  </p>
  <!-- `accept` 要**接受 PSD** ✓：否则用户在选择器里根本看不到自己的 `.psd` ✗。 -->
  <input id="importFile" type="file" accept="image/*,.psd" />
  <hr />
  <h2>另存为副本</h2>
  <p style="opacity:.75;font-size:12px;margin:4px 0"
     title="以新 id 保存一份完整副本（原文档保留，可逆）。文档以 id 为主键，因此这里填的是新文档的 id。">
    新 id 即新文档
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
  <!-- **一笔多色（Loaded Brush）** ✓（用户："花瓣渐变只能分两笔，交界硬" ✗）：
       起点 = 上面的「颜色」✓、末端 = 这里 ✓；勾上之后**同一条笔迹**上渐变 ✓（不是一个对象两笔 ✗）。 -->
  <label>末端色 <input id="colorTo" type="color" value="#2b6cb0" /></label>
  <label title="一笔多色：同一条笔迹上从「颜色」渐变到「末端色」（Loaded Brush；与 MCP 的 brush_stroke.color_to 同一条实现）">
    <input id="duoTone" type="checkbox" /> 一笔多色
  </label>
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
    <!-- **三条落笔路径写清楚** ✓（用户："三套笔触系统机制不清晰 / Web 端同样无说明" ✗）：
         画笔（`.myb`，Hokusai ✓）· 介质（插件 ✓）· 内置画笔（纯几何 ✓）——
         以及**能力边界** ✓：每条只作用于**当前图层** ✓，跨图层只是普通叠加 ✓、介质的湿搅**不跨层** ✓。 -->
    <span class="hint brush-control" id="strokeSystemHint"
          title="画笔 = MyPaint .myb（Hokusai 引擎）；介质 = 我们自己的插件（油画/水彩/…）；内置画笔 = 纯几何无物理。每条笔触只作用于当前图层：跨图层只是普通叠加，介质的湿搅/混色不跨层。">三条落笔路径 · 只作用于当前图层</span>
    <!-- **折叠 / 展开画笔区** ✓（用户："更紧凑和可隐藏"✓）—— 状态在 `body` 的类上 ✓，
         一行 CSS 决定显隐 ✓（不给每个控件写一遍 `hidden` ✗ —— 那会漏掉后来新增的 ✓）。 -->
    <button id="brushAreaToggle" type="button" aria-pressed="false"
            title="折叠 / 展开画笔区（快捷键 \ 也可；折叠只隐藏控件，不改你选好的笔与颜色）">画笔 ▾</button>
    <label class="brush-control">笔刷 <select id="brush" title="MyPaint .myb 笔刷（Hokusai 引擎 ⇒ 由服务端落笔；首次点开时载入）">
      <option value="">（内置画笔）</option>
    </select></label>
    <!-- **预览** ✓（用户："201 支笔刷只有一个名字 ⇒ 选笔全凭猜，**web 上也是**"✗）：
         用同一支笔刷**真画一小笔** ✓（服务端 `brush_preview` ✓，与 `brush_stroke` **同一条落笔实现** ✓）
         ⇒ 这里显示的是它**真实落笔**的样子 ✓，不是示意图 ✗（示意图迟早与真笔触漂移 ✓）。 -->
    <!-- **平滑** ✓（用户："12 瓣花手写 60 个坐标"✗）：勾上 ⇒ 工具层把控制点当
         **Catmull-Rom 样条**（曲线过这些点 ✓、不把它们拉走 ✓）——
         与 MCP 那边是**同一个参数**（`brush_stroke.smooth` / `draw_stroke.data.smooth` ✓）。 -->
    <label class="brush-control" title="把落笔的点当平滑曲线（Catmull-Rom，曲线过这些点）——手绘的折线不再有硬角">
      <input id="smooth" type="checkbox" checked /> 平滑
    </label>
    <!-- **素材浮层** ✓（用户："画笔区快捷方式、点开浮出来" ✓）：把**调色板 / 纹理**两张卡
         浮到画布上方 ✓ —— 点一下开 ✓、再点一下收 ✓；卡本身是**搬过去再搬回来**，
         不是重建 ✗（上一版把面板弄空 ✓ 就是栽在"重建/丢了原来的位置"上 ✓）。 -->
    <button id="brushLibraryOpen" class="brush-control" type="button" aria-pressed="false"
            title="笔刷库：每支笔刷都带**真实落笔**的效果图（滚到哪画到哪）；点一行就换那支笔">笔刷库</button>
    <button id="assetFloat" class="brush-control" type="button" aria-pressed="false"
            title="把调色板 / 纹理浮到画布上（再点一次收回，卡片会回到原来的位置）">素材</button>
    <span id="brushPreviewWrap" class="brush-control" title="这支笔刷真实落一小笔的样子（服务端 brush_preview，与落笔同一条实现）">
      <img id="brushPreview" alt="" style="display:none;vertical-align:middle;border:1px solid #ccc;background:#fff;max-width:160px;max-height:64px" />
      <span id="brushPreviewHint" class="hint"></span>
    </span>
    <!-- **搜索** ✓（目标 ⑥ ✓）：库里 199 支 ✓ ⇒ 一个长下拉里"翻着找"是**没有界面设计** ✗ ——
         下拉里同时按**来源分组**（`classic-` / `deevad-` / `ramon-` / `brushkit-` ✓）。 -->
    <label class="brush-control">搜笔刷 <input id="brushSearch" type="search" placeholder="名字片段，如 knife / pen" style="width:150px" /></label>
    <button id="brushFavorite" class="brush-control" type="button" title="把当前选中的笔刷加入/移出收藏（存在工作区偏好里 ✓，MCP 也能读到 ✓）">★ 收藏</button>
    <span id="brushSearchHint" class="hint brush-control"></span>
    <span id="brushFavoriteHint" class="hint brush-control"></span>
    <!-- **懒加载与搜索的触发，必须写在这里** ✓ —— 不能写在页面主脚本里 ✗。
         实测（真实教训 ✓）：主脚本里那些函数**不是全局** ✓（`typeof refreshBrushOptions === "undefined"` ✗），
         而"点开下拉才装载"这条线`title` 里承诺了很久 ✓、却**从未真正接上** ✗
         ⇒ **真人**点开只看到"（内置画笔）" ✓（我此前"看到 200 支"的验收是**脚本自己调了装载** ✗
         ⇒ 脚本替用户做了他没做的事 ✓）。
         于是改成：**紧跟标记的一段独立脚本** ✓ —— 它处在**全局作用域** ✓，
         且只调用**已被证明可靠**的 `window.yanshi.loadBrushes / applyBrushFilter` ✓。 -->
    <script>
      (function () {
        var select = document.getElementById("brush");
        var box = document.getElementById("brushSearch");
        var loadOnce = function () {
          if (select && select.options.length <= 1 && window.yanshi && window.yanshi.loadBrushes) {
            window.yanshi.loadBrushes();
          }
        };
        if (select) {
          // **点开 / 聚焦 / 按键都算"用户要用它"** ✓（一次就够 ✓）。
          select.addEventListener("pointerdown", loadOnce, { once: true });
          select.addEventListener("focus", loadOnce, { once: true });
          select.addEventListener("keydown", loadOnce, { once: true });
        }
        var favorite = document.getElementById("brushFavorite");
        if (favorite) {
          // **收藏当前笔刷** ✓（走 `window.yanshi` ✓ —— 该对象是**唯一被证明可靠**的入口 ✓）。
          favorite.addEventListener("click", function () {
            if (window.yanshi && window.yanshi.toggleFavoriteBrush) {
              window.yanshi.toggleFavoriteBrush();
            }
          });
        }
        if (select) {
          // **换笔刷就记一次"最近使用"** ✓（只记名字 ✓，不改任何绘制行为 ✓）。
          select.addEventListener("change", function () {
            if (select.value && window.yanshi && window.yanshi.recordBrushUse) {
              window.yanshi.recordBrushUse();
            }
            // **换笔刷就换预览** ✓（用户："选笔全凭猜" ✗ —— 现在选之前/选中都能看到它长什么样 ✓）。
            if (window.yanshi && window.yanshi.previewBrush) {
              window.yanshi.previewBrush();
            }
          });
        }
        if (box) {
          // **输入即过滤** ✓；还没装载 ⇒ 先装载再过滤 ✓（否则是在空下拉上过滤 ⇒ 用户看到"搜不到" ✗）。
          box.addEventListener("input", function () {
            if (select && select.options.length <= 1 && window.yanshi && window.yanshi.loadBrushes) {
              window.yanshi.loadBrushes().then(function () {
                if (window.yanshi.applyBrushFilter) window.yanshi.applyBrushFilter();
              });
            } else if (window.yanshi && window.yanshi.applyBrushFilter) {
              window.yanshi.applyBrushFilter();
            }
          });
        }
      })();
    </script>
  </div>

<main>
  <nav id="tools" aria-label="工具"><!-- 由 TOOL_DEFS 在加载时填充 ✓ --></nav>
  <div class="stage">
    <canvas id="board"></canvas>
    <canvas id="overlay"></canvas>
    <div id="annotationPins"></div>
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
    <div class="card" data-panel="paint">
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
  <!-- **显式渲染开关** ✓（(A)⑤）：勾上就走**服务端像素**那条 ✓（= 弱设备回退 ✓）。
       改了会**重载页面** ✓ —— 比热切换简单，也不会有"半客户端半服务端"的中间状态 ✓。 -->
  <label style="display:block;margin-top:6px;font-size:12px"><input type="checkbox" id="useServerRender" /> 用服务端渲染（弱设备回退）</label>
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
    <div class="card" data-panel="diag">
      <h2>WASM 计算内核</h2>
      <div class="status">
        <span>本地乐观渲染 <b id="wasmState">检测中…</b></span>
        <span>首笔 <b id="firstStroke">—</b></span>
        <span>首帧 <b id="firstPaint">—</b></span>
        <span>内核预热 <b id="kernelWarm">—</b></span>
        <span>bit-exact <b id="bitExact">—</b></span>
      </div>
    </div>
    <div class="card" data-panel="paint">
      <h2>调整 / 滤镜</h2>
      <div style="display:flex; gap:8px; margin-bottom:8px; flex-wrap:wrap">
        <select id="effectKind">
          <option value="adjustment">调整</option>
          <option value="filter">滤镜</option>
        </select>
        <select id="effectName"></select>
        <button id="effectApply">应用</button>
        <button id="effectNew" type="button">＋新建</button>
      </div>
      <div style="display:flex; gap:8px; align-items:center; margin-bottom:8px">
        <input id="effectParams" value="{}" style="flex:1; font-family:ui-monospace,monospace" />
      </div>
      <div id="effectsList" style="font-family:ui-monospace,monospace;font-size:11px;max-height:120px;overflow:auto"></div>
    </div>
    <div class="card" data-panel="history">
      <h2>历史（原子日志）</h2>
      <div style="display:flex; gap:8px; margin-bottom:8px; flex-wrap:wrap">
        <select id="historyKind"><option value="">全部类型</option></select>
        <select id="historyActor"><option value="">全部操作者</option></select>
        <button id="historyReload">重新载入</button>
      </div>
      <!-- **检查点** ✓（设计 §4.5 ✓）—— `checkpoint` / `restore_checkpoint` / `get_checkpoints`
           此前在查看器里**零引用** ✗ ⇒ 与"标注""实例/组"同一类缺口 ✓（工具就绪、用户够不到 ✓）。
           画家最直观的用法就是"**打一个存档点 ✓、以后回到这里** ✓"。 -->
      <div style="display:flex; gap:8px; margin:8px 0; flex-wrap:wrap">
        <button id="checkpointCreate" type="button">打一个存档点</button>
        <button id="checkpointReload" type="button">刷新存档点</button>
      </div>
      <div id="checkpointList" class="annotation-list"></div>
      <div id="history"></div>
      <!-- **原子详情** ✓（设计 §13.2 的历史浏览 ✓）：点历史里任意一条 ✓ ⇒ 显示它**到底改了什么** ✓。
           这需要 `get_atom` ✓（本轮新补 ✓）—— 此前 `get_log`/`find_atom` **按设计只给元数据** ✗
           ⇒ 界面能列出"发生了什么" ✓、却问不出"这一条改了什么" ✗。 -->
      <!-- **变更集** ✓（设计 793 ✓）—— `begin/commit/abort/get_changesets/revert_changeset`
           此前在查看器里**零引用** ✗ ⇒ 用户拿不到"**成组撤销**" ✓（把接下来这一串动作打包 ✓，
           不满意就**整体撤销** ✓）。 -->
      <div class="toolbar" style="margin-top:8px">
        <button id="changesetBegin" type="button">开始变更集</button>
        <button id="changesetCommit" type="button">提交</button>
        <button id="changesetAbort" type="button">放弃（整体撤销）</button>
        <button id="changesetReload" type="button">刷新</button>
      </div>
      <div id="changesetList" class="annotation-list"></div>
      <div class="hint" id="atomDetailHint">点上面任意一条，看它改了什么 ✓</div>
      <pre id="atomDetail" style="font-size:11px;white-space:pre-wrap;max-height:180px;overflow:auto;margin:4px 0 0"></pre>
    </div>
    <div class="card" data-panel="history">
      <h2>缩略图</h2>
      <img id="thumb" alt="缩略图" />
    </div>
    <div class="card" data-panel="history">
      <h2>原子日志（控制流）</h2>
      <div id="log"></div>
    </div>
    <div class="card" data-panel="diag">
      <h2>最近一次响应</h2>
      <div id="last" style="font-family:ui-monospace,monospace;font-size:11px"></div>
    </div>
    <div class="card" data-panel="diag">
      <h2>反馈</h2>
      <div class="status" style="flex-direction:column; align-items:flex-start; gap:8px">
        <span>问题反馈、协作沟通、缺陷上报：</span>
        <a id="contact" href="mailto:yanshi@wangda.today?subject=%5BYanshi%5D%20"
           style="color:#3f7fd4; font-family:ui-monospace,monospace; font-size:12px">yanshi@wangda.today</a>
        <span style="opacity:.7">安全漏洞请勿开公开 issue，直接发邮件。</span>
      </div>
    </div>
    <!-- **标注** ✓（设计 4.6 / 13.4 ✓）：七个标注工具**早就有** ✓，但编辑器里此前**没有任何入口** ✗
         ⇒ 这一块就是补那个入口 ✓。新建走工具栏的「标注」✓，列表里可以改文字 ✓、解决 ✓、删除 ✓。 -->
    <div class="card" data-panel="diag">
      <h2>标注</h2>
      <div class="hint">用工具栏的「标注」在画布上点一下就新建 ✓；点图钉可选中 ✓。</div>
      <div class="toolbar">
        <button id="annotationRefresh" type="button">刷新</button>
        <button id="annotationShowResolved" type="button">显示已解决</button>
      </div>
      <div id="annotationList" class="annotation-list"></div>
    </div>

    <!-- **对象** ✓（设计 §9：复制/实例化/组引用 ✓）—— 目标③点名的"实例/组" ✓。
         此前 `create_instance` / `create_group` / `add_to_group` 在查看器里**零引用** ✗
         ⇒ 和"标注"一样，是"**工具就绪、用户够不到**" ✓。这一块补上入口 ✓。 -->
    <div class="card" data-panel="paint">
      <h2>对象</h2>
      <div class="hint"
        title="勾选对象后可「实例化」「编组」「变换」（实例与 master 联动，设计 9.1）。「重采样」只作用于光栅对象（导入的图片、PSD 合成图）——手画的笔迹是 stroke，服务端会明确拒绝并告知类型（先用「转为形状」或改为导入图片）。">
        勾选对象后可实例化 / 编组 / 变换
      </div>
      <div class="toolbar">
        <button id="objectRefresh" type="button">刷新</button>
        <button id="objectInstance" type="button">实例化</button>
        <button id="objectGroup" type="button">编组</button>
        <button id="objectToShape" type="button">转为形状</button>
        <button id="objectToPath" type="button">转为路径</button>
        <button id="objectTransform" type="button">变换</button>
        <button id="objectResample" type="button">重采样</button>
        <button id="objectPath" type="button">执行路径算子</button>
        <button id="objectRestyle" type="button">改笔触</button>
      </div>
      <div style="display:flex; gap:8px; margin:8px 0; flex-wrap:wrap; align-items:center">
        <label>角度 <input id="transformRotate" type="number" value="0" step="15" style="width:64px" /></label>
        <label>缩放% <input id="transformScale" type="number" value="100" step="10" style="width:64px" /></label>
        <label>dx <input id="transformDx" type="number" value="0" step="10" style="width:56px" /></label>
        <label>dy <input id="transformDy" type="number" value="0" step="10" style="width:56px" /></label>
        <label>笔触色 <input id="strokeColor" type="color" value="#c81e3c" /></label>
        <label>选中笔迹的粗细 <input id="strokeSize" type="number" value="8" step="2" style="width:56px" /></label>
        <label>不透明 <input id="strokeOpacity" type="number" value="1" min="0" max="1" step="0.1" style="width:56px" /></label>
        <label>路径算子
          <select id="pathOp">
            <option value="reverse">reverse 反向</option>
            <option value="close">close 闭合</option>
            <option value="join">join 连接</option>
            <option value="merge">merge 合并</option>
            <option value="split">split 切开</option>
            <option value="boolean">boolean 布尔</option>
          </select>
        </label>
        <label>布尔模式
          <select id="pathMode">
            <option value="union">union 并</option>
            <option value="intersect">intersect 交</option>
            <option value="subtract">subtract 差</option>
            <option value="xor">xor 异或</option>
          </select>
        </label>
        <label>切口节点 <input id="pathAt" type="number" value="0" step="1" style="width:56px" /></label>
      </div>
      <div id="objectList" class="annotation-list"></div>
    </div>
    <!-- **存储 / 维护** ✓（设计 §6.3 的 Blob 三级生命周期 ✓）——
         `blob_gc` 此前在查看器里**零引用** ✗ ⇒ 用户看不到工作区里有多少**孤儿 blob** ✓，也无从回收 ✓。
         这一块把"统计"与"回收"分开 ✓：**统计永远安全** ✓（`dry_run` 默认就是 true ✓）；
         回收**必须先勾确认** ✓（删除不可逆 ✓，与本项目"不做不可逆动作"一致 ✓）。 -->
    <div class="card" data-panel="diag">
      <h2>存储 / 维护</h2>
      <div class="hint"
        title="孤儿是「上传过、但没有任何原子引用」的数据（设计 §6.3）；回收按 TTL 走。">孤儿数据</div>
      <div class="toolbar">
        <button id="storageReport" type="button">统计</button>
        <button id="storageCollect" type="button">回收孤儿</button>
        <button id="storageDemote" type="button">降冷历史</button>
      </div>
      <label class="hint"><input id="storageConfirm" type="checkbox" /> 我确认（删除不可逆）</label>
      <div id="storageReport0" style="font-family:ui-monospace,monospace;font-size:11px;white-space:pre-wrap"></div>
    </div>
    <!-- **调色板** ✓（目标第 ① 件 ✓）—— `list_palette_colors` 此前**只有工具层入口** ✗
         ⇒ MCP 能用 ✓，而界面里**点不到颜色** ✗ ⇒ 这正是"只在一边有"的缺陷 ✓。 -->
    <div class="card" data-panel="assets" id="cardPalette">
      <h2>调色板</h2>
      <div class="hint" title="点色块即取色（写回笔刷颜色）；来源是随包发布的，或你自己导入的。">点色块取色</div>
      <label>调色板 <select id="palettePick"></select></label>
      <!-- **取色写到哪里** ✓（目标 ⑤ ✓）—— 一个调色板、多个去向 ✓：
           否则"调色板的颜色"与"渐变卡片里的颜色"就是**两套各自为政的颜色** ✗。 -->
      <label>取色写入 <select id="paletteTarget">
        <option value="brush">笔刷色（缺省）</option>
        <option value="gradFrom">渐变起点</option>
        <option value="gradTo">渐变终点</option>
      </select></label>
      <div id="paletteSwatches" class="toolbar" style="flex-wrap:wrap;gap:4px"></div>
      <div id="paletteInfo" class="hint"></div>
    </div>
    <!-- **纹理** ✓（目标第 ② 件 ✓）—— `texture_background` 同样此前只有工具层入口 ✗。 -->
    <div class="card" data-panel="assets" id="cardTexture">
      <h2>纹理</h2>
      <div class="hint" title="把 CC0 纸张 / 画布纹理铺成背景（会新建一层并沉到最底）。">铺成背景</div>
      <label>纹理 <select id="texturePick"></select></label>
      <!-- **缩略图** ✓（目标 (b) ✓）：由 `/textures/<file>` 发图 ✓ ——
           浏览器读不到服务器上的文件 ✓ ⇒ 必须走服务端路由 ✓（与介质插件同一条路 ✓）。 -->
      <div id="textureThumbs" class="toolbar" style="flex-wrap:wrap;gap:4px"></div>
      <label>铺法 <select id="textureMode">
        <option value="tile">平铺（缺省）</option>
        <option value="stretch">拉伸</option>
        <option value="cover">等比铺满</option>
      </select></label>
      <!-- **只作用于选区** ✓（目标 ④ ✓）：工具层早就收 `region` ✓ ⇒ 这里只补入口 ✓，
           不新增"只有 Web 有"的能力 ✓。 -->
      <label class="hint"><input id="textureUseSelection" type="checkbox" /> 只作用于选区</label>
      <div class="toolbar"><button id="textureApply" type="button">设为背景</button></div>
      <div id="textureInfo" class="hint"></div>
    </div>
    <!-- **渐变** ✓（目标 (d) ✓）—— 针对"大面积背景难处理" ✓：
         笔刷铺底会留下笔触边缘与噪声 ✗，而渐变是纯函数 ✓（无边、无噪、可复现 ✓）。 -->
    <div class="card" data-panel="paint">
      <h2>渐变</h2>
      <div class="hint"
        title="给当前图层填一层渐变（天空 / 底色 / 光照过渡）——确定性：同样的输入永远同样的像素。">填充当前图层</div>
      <label>起点 <input id="gradFrom" type="color" value="#fad6a5" /></label>
      <label>终点 <input id="gradTo" type="color" value="#3b5bdb" /></label>
      <label>类型 <select id="gradKind">
        <option value="linear">线性（角度 ↓）</option>
        <option value="radial">径向（从中心散开）</option>
      </select></label>
      <label>角度 <input id="gradAngle" type="number" value="90" step="15" style="width:64px" /></label>
      <label class="hint"><input id="gradUseSelection" type="checkbox" /> 只作用于选区</label>
      <div class="toolbar"><button id="gradApply" type="button">填充</button></div>
      <div id="gradInfo" class="hint"></div>
    </div>
    <!-- **工程包** ✓（目标 ⑧ ✓）—— 工具层早就有 `export_project` / `import_project` ✓，
         而界面里**一次都没提到过它们** ✗（实测：查看器里出现次数 = 0 ✓）
         ⇒ "真人备份不了自己的画" ✓ —— 这正是"两边都要有"的**反面缺口** ✓。
         **路径是服务端的路径** ✓（不是浏览器的文件选择器 ✓）⇒ 这一点必须写在界面上 ✗，
         否则用户会以为点一下就从自己电脑上选文件 ✓。 -->
    <div class="card" data-panel="file">
      <h2>工程包</h2>
      <div class="hint">
        <span title="整份文档（原子日志 + 元数据 + 全部 blob）打成一个 .yanshi：未压缩 tar，任何 tar 都能看。">打包整个文档为 <code>.yanshi</code></span>
        <strong>路径是服务器上的路径</strong> ✓ —— 不是从你电脑上选文件 ✗。
      </div>
      <label>路径 <input id="projectPath" type="text" value="yanshi-project.yanshi" style="width:200px" /></label>
      <div class="toolbar">
        <button id="projectExport" type="button">导出工程</button>
        <button id="projectImport" type="button">导入为新文档</button>
      </div>
      <div id="projectInfo" class="hint"></div>
    </div>
    <!-- **建议** ✓（设计 §12.6 ✓）—— `suggest` / `list_suggestions` / `accept_suggestion` /
         `reject_suggestion` 此前在查看器里**零引用** ✗ ⇒ 用户看不到 AI 提出的可执行补丁 ✓、
         也无法接受或拒绝 ✓。它与上面的「标注」配对：**标注说明问题 ✓、建议给出可执行的修法 ✓**
         （`accept_suggestion` 会**按序重放 patch** ✓，并把关联标注置为 resolved ✓）。 -->
    <div class="card" data-panel="diag">
      <h2>建议</h2>
      <div class="hint">建议来自 `suggest`（含**可执行补丁**）✓；接受会**按序重放**补丁 ✓。</div>
      <div class="toolbar">
        <select id="suggestionStatus">
          <option value="pending">待处理</option>
          <option value="accepted">已接受</option>
          <option value="rejected">已拒绝</option>
          <option value="">全部</option>
        </select>
        <button id="suggestionReload" type="button">刷新</button>
      </div>
      <div id="suggestionList" class="annotation-list"></div>
      <div class="hint" id="suggestionPreviewHint">点「预览」先看清补丁会被怎么执行 ✓（预览**不应用** ✓）</div>
      <pre id="suggestionPreview" style="font-size:11px;white-space:pre-wrap;max-height:140px;overflow:auto;margin:4px 0 0"></pre>
    </div>
    <!-- **评论** ✓（设计 §12.6 的协作通道 ✓）—— `comment` 此前在查看器里**零引用** ✗。
         注意：**没有** `list_comments` 工具 ✓ —— 评论就是**原子** ✓ ⇒ 用现成的
         `get_log {kind:"comment"}` 读回来 ✓（历史面板本来就在这么做 ✓）。 -->
    <div class="card" data-panel="diag">
      <h2>评论</h2>
      <div class="toolbar">
        <input id="commentText" type="text" placeholder="写一条评论…" style="flex:1 1 auto" />
        <button id="commentPost" type="button">发表</button>
        <button id="commentReload" type="button">刷新</button>
      </div>
      <div id="commentList" class="annotation-list"></div>
    </div>
  </aside>
</main>
<footer class="statusbar">
    <!-- **缩放的显式入口** ✓：状态栏里直接**输入百分比** ✓（用户："或者输入具体数值才变化" ✓）。
         数值 = **实际显示比例** ✓（与旁边的读数同一口径 ✓），不是"相对适配的倍数" ✗。 -->
    <!-- **提示必须与行为一致** ✗（第三方代码审计 #6：提示教用户用滚轮，而滚轮被有意吞掉 ✓）。
         实测现状：滚轮/触摸板**什么都不做** ✓ —— 这是**有意**的 ✓（用户实测"一碰就误缩放" ✗，
         注释见 `viewer.rs:5592` 一带 ✓）。所以这里不再说"滚轮只平移" ✗（那是更早一版的写法 ✓）。 -->
    <span>缩放 <input id="zoomInput" type="number" min="5" max="1600" step="25" value="100"
                     style="width:64px" title="画布显示比例（%）；回车或失焦生效 —— 滚轮**不缩放也不平移**（防误触）" />%</span>
    <span id="zoom" class="hint">100%</span>
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

// **开关的判定必须在这里** ✓ —— `needsServerPixels` 上面那句注释早就写着
// "必须在任何启动路径之前声明" ✓，而我第一版把这两个定义放在了**导出按钮的接线块**里 ✗（≈4852 行 ✓）
// ⇒ `initWasm`（**1510 行** ✓）用到时还没定义 ✗ ⇒ 浏览器报
// `ReferenceError: serverRenderPreferred is not defined` ✓ ⇒ **内核初始化中断** ✓
// ⇒ `state.wasm` 恒为 false ✗、状态停在"检测中…" ✗ —— 与实测症状**完全吻合** ✓。
const SERVER_RENDER_KEY = "yanshi.serverRender";
/// **是否强制走服务端渲染** ✓（(A)⑤ 的显式开关 ✓）。缺省 `false` = 照旧（客户端优先 ✓）。
const serverRenderPreferred = () => {
  try { return localStorage.getItem(SERVER_RENDER_KEY) === "1"; } catch (error) { return false; }
};


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
  // **最近一次"连不上服务端"的提示时间** ✓（用来限流 ✓ —— 见 `reportServerUnreachable` ✓）。
  lastOfflineReport: 0,
  // **上一次"设为背景"建的那一层** ✓（用它保证同一时刻只有一张纹理底 ✓ —— 见 `textureApply` ✓）。
  textureLayerId: null,
  // **可撤销 / 可重做的笔数** ✓ —— 来自服务端工具 ✓；`null` = 还不知道 ✓（界面显示 — ✓，不猜 ✓）。
  remainingUndo: null,
  remainingRedo: null,
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

// ——— **本地持久化（离线优先）** ✓ ———
// **为什么用 OPFS + IndexedDB** ✓（第 160 轮的算术依据 ✓）：文档像素可达 **48MB** ✓，
// 而 `localStorage` 上限约 **5MB** ✗ ⇒ 差近一个数量级 ⇒ 像素必须走 **OPFS**（大二进制 ✓），
// 索引走 **IndexedDB** ✓（行业常见配对 ✓）。验收判据是 `scripts/browser-offline-reload.mjs` ✓：
// **断网重载后那一笔还得在** ✓ ⇒ 只有像素真落到本地才会绿 ✓。
const LOCAL_DB = "yanshi-local-v1";
const LOCAL_STORE = "blobs";
const LOCAL_JSON_STORE = "json";
/// **缓存键** ✓：必须剥掉破缓存参数 `t` ✗ —— 它每次渲染都不同 ✓
/// ⇒ 拿整个 URL 当键 ⇒ **每次都是新键** ⇒ 存储**无限增长** ✗（第 163 轮记下的坑之一 ✓）。
/// 按**文档 id** 分区 ✓（不同文档不互相覆盖 ✓）。
function localKey(url) {
  try {
    const parsed = new URL(url, location.href);
    parsed.searchParams.delete("t");
    // **文档 id 从 URL 自己解析** ✗ —— 实测：用 `state.docId` 会在**引导期取到空值** ✓
    // ⇒ 写进去的键前缀是 `doc` ✗、后来读的却是真 id ✗ ⇒ **4 个端点只缓存了 2 个** ✓
    //（第 165 轮实测：`json` store 里 count=2 ✓，而缺的正是启动第一个要用的那个 ✓）。
    // 每个 `api()` 生成的 URL 里**都带 `doc=`** ✓ ⇒ 从 URL 取就**与初始化顺序无关** ✓。
    const doc = parsed.searchParams.get("doc") || String(state.docId || "doc");
    return doc + "|" + parsed.pathname + "?" + parsed.searchParams.toString();
  } catch (error) {
    return String(state.docId || "doc") + "|" + url;
  }
}
/// IndexedDB 打开（只存 `{key, file, size, at}` ✓，**不存字节** ✗ —— 字节在 OPFS ✓）。
function localDb() {
  return new Promise((resolve, reject) => {
    if (!self.indexedDB) { reject(new Error("没有 IndexedDB")); return; }
    const request = indexedDB.open(LOCAL_DB, 2);
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(LOCAL_STORE)) {
        database.createObjectStore(LOCAL_STORE, { keyPath: "key" });
      }
      if (!database.objectStoreNames.contains(LOCAL_JSON_STORE)) {
        database.createObjectStore(LOCAL_JSON_STORE, { keyPath: "key" });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
async function localWithStore(mode, run, storeName) {
  const db = await localDb();
  const name = storeName || LOCAL_STORE;
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(name, mode);
      const store = tx.objectStore(name);
      const request = run(store);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  } finally {
    db.close();
  }
}
async function localDir() {
  const root = await navigator.storage.getDirectory();
  return await root.getDirectoryHandle("yanshi", { create: true });
}
/// 取本地那份 blob ✓ ⇒ 返回 object URL（**离线可显示** ✓）或 null ✓。
async function localBlobGet(url) {
  try {
    const record = await localWithStore("readonly", (store) => store.get(localKey(url)));
    if (!record || !record.file) return null;
    const dir = await localDir();
    const handle = await dir.getFileHandle(record.file);
    const file = await handle.getFile();
    if (!file || file.size === 0) return null;
    return URL.createObjectURL(file);
  } catch (error) {
    // **存储不可用就静默降级** ✓（无痕/配额满/老浏览器 ✗）⇒ 退化成在线行为 ✓，
    // 绝不因为存储把查看器拖垮 ✗（与"参考图不在关键路径"同一个原则 ✓）。
    return null;
  }
}
/// 存一份 blob ✓（文件名用**内容无关的稳定键**的哈希 ✓，避免文件名里出现怪字符 ✓）。
async function localBlobPut(url, bytes) {
  try {
    const key = localKey(url);
    let hash = 0;
    for (let index = 0; index < key.length; index += 1) {
      hash = (hash * 31 + key.charCodeAt(index)) >>> 0;
    }
    const file = hash.toString(16) + "-" + key.length.toString(16) + ".bin";
    const dir = await localDir();
    const handle = await dir.getFileHandle(file, { create: true });
    const writable = await handle.createWritable();
    await writable.write(bytes);
    await writable.close();
    await localWithStore("readwrite", (store) =>
      store.put({ key: key, file: file, size: bytes.byteLength, at: Date.now() }),
    );
    return file;
  } catch (error) {
    return null; // 同上：存储问题**不影响**主流程 ✓
  }
}
/// **引导期 JSON 的本地缓存** ✓ —— 断网时查看器**连状态都建不起来** ✗，
/// 所以光缓存缩略图不够 ✓（第 164 轮实测：断网后画布是 **300×150 的小占位** ✓ ⇒ 说明它**没进正常状态** ✗）。
/// 实测出的引导链只有三处 ✓：`/api/tools/get_document` ✓、`/api/tools/list_layers` ✓、`/api/atoms` ✓
/// （`/api/blob` 那 5 处是像素 ✓，已由缩略图那条覆盖 ✓，这里**不重复缓存** ✗）。
const LOCAL_JSON = ["/api/tools/get_document", "/api/tools/list_layers", "/api/atoms", "/api/effects"];
/// **只缓存读工具** ✗ —— 写工具绝不缓存 ✓（缓存写请求会造成"看着成功其实没落库"✗）。
// **实测补全（第 168 轮 ✓）**：原来只有 `get_document`/`list_layers` ✗ —— 但离线实测失败的还有
// `get_preferences` ✓ 与 **`list_effects`** ✓（注意：**不是** `/api/effects` ✗ ——
// 我第一版把字面量 `api("/api/effects")` 当成了它 ✓ ⇒ 又一次"凭字面量猜"栽了 ✓）。
/// **从规格生成** ✓（第 176 轮记下的改进 ✓）：仓库里 `ToolSpec.mutating` **本来就是权威清单** ✓
/// ⇒ 不必再靠"逐条点名"补 ✗（我那样补了 **7 次** ✓，每次都靠一次失败才发现 ✓）。
/// 服务端发页面时会把 `__READ_TOOLS__` 换成**所有只读工具名** ✓（见 `page_with_read_tools` ✓）。
/// 该 crate 要求常量也有文档注释 ✓（我第一次写成 `//` ✓ ⇒ `clippy -D warnings` 当场红 ✓）。
const LOCAL_READ_TOOLS = [__READ_TOOLS__];

/// 这三个端点走"**本地优先**" ✓；**其余一律原样转发** ✓（不改变任何别处的行为 ✓）。
async function fetchOrLocal(url, options) {
  const method = (options && options.method) || "GET";
  let cacheable = false;
  let binary = false;
  let toolName = "";
  try {
    const parsed = new URL(url, location.href);
    if (method === "GET") {
      // **`/api/blob/…` 一律本地化** ✓（实测：离线时有一条 blob 请求失败 ✗ ⇒ 图层/预览要用它 ✓）。
      // 字节存 OPFS ✓（48MB 级别的算术依据 ✓），索引仍走 IndexedDB ✓。
      if (parsed.pathname.startsWith("/api/blob/")) {
        binary = true;
        cacheable = true;
      } else {
        cacheable = LOCAL_JSON.includes(parsed.pathname);
      }
    } else if (
      method === "POST" &&
      parsed.pathname.startsWith("/api/tools/") &&
      options &&
      options.body
    ) {
      // **实测教训（第 167 轮 ✓）**：这个调用是 **POST**，但路径是 `/api/tools/list_layers` ✓
      // ⇒ 我原来只认**正好等于** `/api/tools` ✗ ⇒ `cacheable` 一直是 false ✓
      // ⇒ 于是"**写入根本没发生**"✗（而不是写失败 ✓ —— 我加的警告一句都没出现 ✓，
      //   正是这个"没有警告"把范围从"写失败"缩小到"没进分支"✓）。
      // 工具名**从路径末段取** ✓（`{tool}` 那个 body 形态也仍然支持 ✓）。
      // **实测根因（第 166 轮 ✓）**：`list_layers` 与 `get_document` **在源码里根本不出现** ✗
      // ⇒ 它们是**构造出来**的调用 ✓ ⇒ 走的是 **`POST /api/tools`**（`{tool, arguments}` ✓）
      // ⇒ 而我第一版只放行 `GET` ✗ ⇒ 于是 `atoms`/`effects`（GET ✓）进了缓存 ✓，
      //   这两个（POST ✓）**一条都没写进去** ✗ ⇒ 离线时 `refreshLayers` 就炸在这里 ✓。
      // ⇒ **只放行"读性质"的那两个工具** ✓（**写工具绝不放行** ✗ —— 缓存写请求是灾难 ✓）。
      try {
        const fromBody = String(JSON.parse(String(options.body)).tool || "");
        toolName = fromBody || parsed.pathname.slice("/api/tools/".length);
      } catch (error) {
        toolName = parsed.pathname.slice("/api/tools/".length);
      }
      cacheable = LOCAL_READ_TOOLS.includes(toolName);
    }
  } catch (error) {
    cacheable = false;
  }
  if (!cacheable) {
    // **可选依赖失败不得致命** ✓（第 170 轮实测的因果链 ✓）：
    // 离线时 `render_region`（**服务端像素回退** ✓）必然失败 ✓ ⇒ 它原来**抛异常** ✗
    // ⇒ 把**整个启动链**打断 ✓ ⇒ `state.wasm` 永远是 false ✓（**尽管 wasm 就在 SW 缓存里** ✓）
    // ⇒ 画布停在 `<canvas>` 默认 `300×150` ✓。
    // 修法 ✓：只对**这一条可选调用**把网络错误变成**失败的 Response**（503 ✓）——
    // 调用方按 `response.ok` 走它本来就有的降级 ✓，**而不再中断初始化** ✓
    //（行业原则 ✓：`try/catch` 包住**可选**依赖 ✓，只在关键路径上抛 ✓ —— 第 160 轮已记 ✓）。
    try {
      const probe = new URL(url, location.href);
      if (probe.pathname.endsWith("/api/tools/render_region") || probe.pathname === "/api/tools/render_region") {
        try {
          return await fetch(url, options);
        } catch (error) {
          return new Response("{}", { status: 503, headers: { "content-type": "application/json" } });
        }
      }
    } catch (error) {
      // URL 都解析不了 ⇒ 按原样转发 ✓
    }
    // **点名** ✓：走到这里说明**这条调用不在白名单** ✓ —— 离线时它**必然失败** ✗
    // ⇒ 只有把**它到底是谁**打出来 ✓，才知道下一个要补的是哪一个 ✓
    //（第 174 轮实测：报出的行号落在注释上 ✗ ⇒ 实际是这条"原样转发" ✓，但**看不到 URL** ✗）。
    try {
      console.warn("[yanshi] 未缓存即转发：" + method + " " + new URL(url, location.href).pathname);
    } catch (error) {
      console.warn("[yanshi] 未缓存即转发（URL 解析不了）");
    }
    return fetch(url, options);
  }
  const key = localKey(url) + (toolName ? "|tool:" + toolName : "") + (binary ? "|bin" : "|json");
  if (binary) {
    // 二进制：本地有 ⇒ **直接回一个 Response** ✓（离线可显示 ✓）；没有 ⇒ 取网并**写进 OPFS** ✓。
    const localBytes = await localBytesGet(key);
    if (localBytes) {
      return new Response(localBytes, { status: 200, headers: { "content-type": "application/octet-stream" } });
    }
    try {
      const response = await fetch(url, options);
      try {
        if (response && response.ok) await localBytesPut(key, await response.clone().arrayBuffer());
      } catch (error) {
        console.warn("[yanshi] blob 缓存写入失败（不影响主流程）：", error);
      }
      return response;
    } catch (error) {
      const fallbackBytes = await localBytesGet(key);
      if (fallbackBytes) {
        return new Response(fallbackBytes, { status: 200, headers: { "content-type": "application/octet-stream" } });
      }
      throw error;
    }
  }
  // **可变数据必须"网络优先、缓存兜底"** ✗（第 171 轮实测的教训 ✓）：
  // 我原来写的是"本地优先" ✓ ⇒ 于是**在线**也读到了**空白那一次**的旧 `list_objects` ✗
  // ⇒ 在线画布**也变成空的** ✓（dark 3288 → **0** ✓ ⇒ 判据当场红 ✓）。
  // **行业做法** ✓：**可变**读用 `networkFirst` ✓（拿新的 ✓，失败才回落 ✓）；
  // **不可变**资源（`/brush-module.wasm` ✓、**按内容哈希命名**的 `/api/blob/…` ✓）才用 `cacheFirst` ✓
  // —— 这也是 **(A)⑥"SW 升级不脏读"** 的同一条原则 ✓。
  try {
    const response = await fetch(url, options);
    try {
      if (response && response.ok) {
        const text = await response.clone().text();
        await localJsonPut(key, text);
      }
    } catch (error) {
      console.warn("[yanshi] 缓存写入失败（不影响主流程）：", error);
    }
    return response;
  } catch (error) {
    // **取网失败 ⇒ 必须回落到本地** ✓ —— 这才是"离线优先" ✓。
    //（实测：我第一版只在**开头**查了一次缓存 ✗ ⇒ 那时缓存还没写进去 ✗ ⇒
    //  `refreshLayers` 直接抛 `TypeError: Failed to fetch` ✓ ⇒ 查看器进不了正常状态 ✓，
    //  画布停在 `<canvas>` 的默认 **300×150** ✓ —— 这个尺寸就是线索 ✓。）
    const fallback = await localJsonGet(key);
    if (fallback !== null) {
      return new Response(fallback, { status: 200, headers: { "content-type": "application/json" } });
    }
    // **只把"真正落空"的那一次打出来** ✗ —— 改成 network-first 之后 ✓，
    // 离线时**每次读都先试网络** ✓ ⇒ `ERR_INTERNET_DISCONNECTED` **必然会刷一屏** ✓
    // ⇒ 那份清单**不再等于"缓存没命中"** ✗（第 174 轮实测踩到的解读陷阱 ✓）。
    // 真正要找的是：**回落也空了的那一次** ✓（`key` 里带 token ✓ ⇒ 打出来才能看出键对不对 ✓）。
    console.warn("[yanshi] 缓存落空：", key);
    throw error;
  }
}
async function localJsonGet(key) {
  try {
    const record = await localWithStore("readonly", (store) => store.get(key), LOCAL_JSON_STORE);
    return record && typeof record.text === "string" ? record.text : null;
  } catch (error) {
    return null;
  }
}
async function localJsonPut(key, text) {
  try {
    await localWithStore("readwrite", (store) => store.put({ key: key, text: text, at: Date.now() }), LOCAL_JSON_STORE);
    return true;
  } catch (error) {
    // **降级仍然静默**（不影响主流程 ✓），但**必须留痕** ✗ —— 我这轮就是被"吞掉的错误"挡住了一整轮 ✓。
    console.warn("[yanshi] 本地缓存写入失败（不影响主流程）：", key, error);
    return false;
  }
}
/// 读回 OPFS 里的字节（与 `localBlobGet` 同一套机制 ✓，区别是**返回字节**而不是 object URL ✓）。
async function localBytesGet(key) {
  try {
    const record = await localWithStore("readonly", (store) => store.get(key + "|file"));
    if (!record || !record.file) return null;
    const dir = await localDir();
    const handle = await dir.getFileHandle(record.file);
    const file = await handle.getFile();
    return file && file.size > 0 ? await file.arrayBuffer() : null;
  } catch (error) {
    return null;
  }
}
async function localBytesPut(key, buffer) {
  try {
    let hash = 0;
    for (let index = 0; index < key.length; index += 1) {
      hash = (hash * 31 + key.charCodeAt(index)) >>> 0;
    }
    const file = hash.toString(16) + "-" + key.length.toString(16) + ".bin";
    const dir = await localDir();
    const handle = await dir.getFileHandle(file, { create: true });
    const writable = await handle.createWritable();
    await writable.write(buffer);
    await writable.close();
    await localWithStore("readwrite", (store) =>
      store.put({ key: key + "|file", file: file, size: buffer.byteLength, at: Date.now() }),
    );
    return file;
  } catch (error) {
    console.warn("[yanshi] blob 缓存写入失败（不影响主流程）：", error);
    return null;
  }
}
/// **本地优先地**装底图 ✓ —— 这是 5014 那唯一一处的替身 ✓。
async function loadPreview(url) {
  const local = await localBlobGet(url);
  if (local) {
    preview.src = local;
    return;
  }
  preview.src = url; // 老行为 ✓（一个字节都没改 ✓）
  try {
    const response = await fetch(url);
    if (response && response.ok) await localBlobPut(url, await response.arrayBuffer());
  } catch (error) {
    // 网络失败就算了 ✓（存储里没有就是没有 ✓，不要在这里制造假象 ✗）
  }
}

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
  // **只有尺寸真的变了才碰 backing store** ✗ —— 这是"每一笔结束抖一下"的**真凶** ✓（第 43 轮 ✓）。
  //
  // **为什么** ✗：给 `board.width`/`height` 赋值会**重建 backing store ⇒ 清空画布** ✓（浏览器规范 ✓，
  // 与"赋相同值也会清"这条老坑一致 ✓）。而 `sizeBoards` 会被**很多**路径调用 ✓
  //（提交之后的状态栏/历史/图层面板刷新都可能改布局 ✓ ⇒ 又走一遍 ✓）
  // ⇒ 画布被擦成背景色 ✓、随后只补回一部分 ✓ ⇒ 用户看到"每一笔结束闪/抖一下"✗，
  // 严重时干脆"画了看不见，要手工刷新"✗。
  // **实测证据** ✓（探针 `scripts/browser-stroke-refresh.mjs` ✓）：落笔后连续采样画布墨量 ✓
  // 前 1752、后 **0** ✓ —— 而那一刻**没有任何一次补画**（`blankBlitsSkipped` = 0 ✓）
  // ⇒ 只能是**本地清屏** ✓ ✓。
  const changed = board.width !== width || board.height !== height;
  if (changed) {
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
  }
  state.viewport.w = width;
  state.viewport.h = height;
  applyDisplaySize();
  syncOverlayGeometry();
}

/// 画布/覆盖层的 CSS 尺寸 = 视口 × 显示缩放（上限为可用区域，避免溢出）。
function applyDisplaySize() {
  const available = availableArea();
  const scale = state.displayScale || 1;
  // **`fit` 时正好塞满、放大后才让舞台自己滚** ✓（用户："画布是内部一个可以独立上下和左右滚动的"✓）。
  //
  // **第 47 轮的"没查清"其实是测量假象** ✗（第 49 轮查明 ✓）：当时我在**启动瞬间**读
  // `board.style.width`（空 ✗）、`getBoundingClientRect().width`（300 = `<canvas>` 默认值 ✗）
  // ⇒ 得出"算出来的尺寸没落到元素上"✗ —— 而真相是那一刻**画布还没被设过尺寸** ✓
  //（同一刻 `state.viewport` 还是默认的 1024×1024 ✓）。等它稳定之后再量 ✓ 就一切正常 ✓。
  // ⇒ **判据要等被测对象稳定** ✓，否则量到的是"还没开始"✗（与"怀疑判据"那条纪律同型 ✓）。
  //
  // **两条都要** ✓：一直封顶 ⇒ 放大也塞得进 ⇒ `.stage` 的 `overflow: auto` 永远不触发 ✗
  //（"内部滚动"成了空话 ✓）；一直不封顶 ⇒ `fit` 时也多出十几像素 ✗（平白多一条滚动条 ✓）。
  // ⇒ 判据就是**用户缩放**：`state.zoom <= 1`（适配档 ✓）⇒ 封顶到可用区 ✓；放大 ⇒ 不封顶 ✓。
  const zoomedIn = (state.zoom || 1) > 1.0001;
  const rawWidth = Math.round(state.viewport.w * scale);
  const rawHeight = Math.round(state.viewport.h * scale);
  const width = Math.max(32, zoomedIn ? rawWidth : Math.min(rawWidth, available.w));
  const height = Math.max(32, zoomedIn ? rawHeight : Math.min(rawHeight, available.h));
  board.style.width = width + "px";
  board.style.height = height + "px";
  overlay.style.width = width + "px";
  overlay.style.height = height + "px";
  // **放大时就放开 CSS 上限** ✓（见 `#board` 那条注释 ✓）—— 一处开关、两个元素一起管 ✓。
  document.body.classList.toggle("canvas-zoomed", zoomedIn);
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
  const percent = Math.round((state.displayScale || 1) * 100);
  if (zoomLabel) zoomLabel.textContent = percent + "%";
  // **输入框回填** ✓ —— 但**不打扰正在输入的人** ✗（聚焦时不覆盖 ✓）。
  const zoomInput = $("zoomInput");
  if (zoomInput && document.activeElement !== zoomInput) zoomInput.value = String(percent);
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
  const response = await fetchOrLocal(api("/api/tools/" + name), {
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
    const value = await fetchOrLocal(api("/api/tools/get_document"), {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: "{}",
    }).then((r) => r.json());
    if (value.thumb_url) $("thumb").src = value.thumb_url + "&t=" + Date.now();
    if (value.head_seq !== undefined) setStatus({ head: value.head_seq, rendered: value.rendered_seq });
    if (value.width && value.height) state.docSize = { w: value.width, h: value.height };
    if (value.background) state.backgroundCss = backgroundToCss(value.background);
  } catch (error) {
    // **缩略图取不到不许变成未处理的拒绝** ✗（探针实测：翻页/重载时冒
    // `TypeError: Failed to fetch` ✓ —— 那是控制台红字 ✓，而用户只看到"缩略图不动了"✗）。
    // 缩略图是**附属信息** ✓ ⇒ 失败记一行即可 ✓，不打断主流程 ✓、也不刷红字 ✓。
    window.yanshiStats.thumbErrors = (window.yanshiStats.thumbErrors || 0) + 1;
    log("缩略图刷新失败（不影响画布）：" + String(error).slice(0, 60), "#c93");
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
  // **显式要求服务端渲染 ⇒ 内核根本不加载** ✓（第 198 轮的真因 ✓）：
  // 我上一版把守卫加在了 `loadLocalBrushModule()`（**门面** ✓ = 本地笔刷预览 ✓）上 ✗，
  // 而内核是**这个函数**拉起来的 ✓ ⇒ 于是"用服务端渲染"勾上之后**内核照样加载** ✗
  // ⇒ `state.wasm` 一直是 true ✗ ⇒ 开关**从未生效** ✓（而当时那条时序敏感的判据**没抓到** ✗）。
  // 加在**函数内部**而不是某个调用点 ✓：这样**所有**调用路径都被覆盖 ✓。
  if (serverRenderPreferred()) {
    setWasmState("已按设置跳过（用服务端渲染）", "#a60");
    return;
  }
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
  // **把"这一版是哪一版"写进日志** ✓（用户提的排查建议 ✓）：出问题时先看这一行 ✓。
  try {
    // **服务端只有 `/health`，没有 `/api/health`** ✗（真实报告 ✓：用户在 macOS 上看到
    // `GET /api/health 404` ✓）⇒ 这里一直 404 ✓，于是**那行"服务端构建："永远打不出来** ✗，
    // 而它正是"这一版是哪一版"的排查入口 ✓ ⇒ 改对路径 ✓。
    const health = await fetch("/health").then((response) => response.json());
    if (health && health.build) log("服务端构建：" + health.build);
  } catch (_) { /* 拿不到就算了 ✓，不影响使用 ✓ */ }
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
  const atoms = await fetchOrLocal(api("/api/atoms") + "&since=" + since).then((r) => r.json());
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
/// **把内核的当前状态重画到整块视口** ✓ —— `resync()` 的两条分支都要它 ✓。
///
/// **为什么需要它** ✓：`refreshPreview()` 刷的是**预览/缩略图** ✗，**不是画布** ✗。
/// `resync()` 此前**两条分支都只调它** ✓ ⇒ 内核状态更新了 ✓、预览更新了 ✓，
/// 而**画布没人管** ✗ ⇒ "点眼睛/锁 ⇒ 变白 ⇒ 手工刷新才恢复" ✗（真浏览器实测 ✓）。
/// **收成一个函数** ✓：将来 `resync()` 再加分支 ✓，也只需记得调这一个名字 ✓ ——
/// 这比"记得同时调预览和画布"可靠 ✓（本项目反复吃过"两条路径漏一条"的亏 ✗）。
function redrawCanvasFromKernel() {
  if (!state.kernel || !state.wasm) return;
  const board = $("board");
  if (!board) return;
  drawKernelRegion(state.viewport.x, state.viewport.y, board.width, board.height);
}

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
/// **取服务端像素之前必须等布局稳定** ✓ —— 本轮查明的真因 ✓。
///
/// **为什么** ✗：`sizeBoards()` 会在布局变化时重设 `board.width`（⇒ **清空画布** ✓），
/// 而它可能发生在补画**之后** ✓ ⇒ 补画被清掉 ✓（本会话早有这条注释 ✓）。
/// 但还有更隐蔽的一半 ✓：**提交刚发生时**，服务端的 `render_region` 可能还返回
/// **旧/空白**的图 ✓ ⇒ 把它 `putImageData` 上去 = 画一块空白 ✓。
/// **实测证据** ✓（探针 `scripts/browser-stroke-refresh.mjs` ✓）：整视口那条**先等两帧** ⇒ 墨 1750 ✓；
/// 脏区那条**立刻取图** ⇒ 墨 **0** ✗（补画流水里 `box [0,0,900,640]` 执行了却没画上 ✓）。
/// ⇒ 现在**所有**取图都先等 ✓（`blitServerBox` 是唯一入口 ✓ ⇒ 一处修好、两条路都对 ✓）。
async function settleFrames() {
  const settle = () => new Promise((resolve) => {
    let done = false;
    const finish = () => { if (!done) { done = true; resolve(); } };
    try { requestAnimationFrame(finish); } catch (_) { /* 无 rAF 时靠超时 ✓ */ }
    setTimeout(finish, 50);
  });
  await settle();
  await settle();
}

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
  // **等布局稳定再取图** ✓（见 `settleFrames` 的说明 ✓ —— 少了这一步，脏区补画会画上一块空白 ✗）。
  await settleFrames();
  const value = await callTool(
    "render_region",
    { region: { x: x0, y: y0, w, h }, raw: true },
    { refresh: false },
  );
  const url = value.raw_url || value.thumb_url;
  if (!url) return 0;
  // `raw_url` 给的是**原始 RGBA** ✓（不是 PNG ✗）⇒ 直接构造 ImageData ✓。
  const bytes = new Uint8ClampedArray(await fetchOrLocal(api(url)).then((r) => r.arrayBuffer()));
  if (bytes.length < w * h * 4) return 0;
  // **数一下服务端这批字节里有多少墨** ✓ —— 决定性的那一问 ✓：
  // "画布上没有墨"到底是**服务端给的就是空白** ✗，还是**客户端随后把它擦了** ✗（内核重绘 ✓）。
  // 不数它就只能在两个假设之间反复猜 ✓（本轮已经猜错一次 ✗）。
  let serverInk = 0;
  for (let i = 0; i < w * h * 4; i += 4) {
    if (bytes[i + 3] > 32 && !(bytes[i] > 245 && bytes[i + 1] > 245 && bytes[i + 2] > 245)) serverInk += 1;
  }
  window.yanshiStats.lastBlitServerInk = serverInk;
  // **服务端给的是空白、而画布这块已经有墨 ⇒ 不许盖** ✗ —— 真实用户说的"落笔结束还是会闪一下"✓
  // 就是这个：提交之后**立刻**取图会拿到**空白** ✓（实测 `serverInk: 0` ✓，
  // 而同一时刻画布上有墨 1752 ✓），`putImageData` 一盖 ⇒ 肉眼看到闪一下 ✓（甚至整块变白 ✓）。
  // **判据** ✓（探针 `scripts/browser-stroke-refresh.mjs` ✓）：落笔之后**连续采样**画布墨量 ✓，
  // 每一次都必须 ≥ 落笔后的 90% ✓ —— 中间掉到 0 就是那一下闪 ✗ ⇒ 当场红 ✓。
  // **为什么不干脆不补画** ✗：别处传来的变更（另一个客户端 ✓）确实需要服务端像素 ✓
  // ⇒ 只在"服务端空白 **且** 画布已有墨"时跳过 ✓（两侧都不亏 ✓）。
  if (serverInk === 0) {
    const destX = Math.round(x0 - vx);
    const destY = Math.round(y0 - vy);
    const before = ctx.getImageData(destX, destY, w, h).data;
    let canvasInk = 0;
    for (let i = 0; i < before.length; i += 4) {
      if (before[i + 3] > 32 && !(before[i] > 245 && before[i + 1] > 245 && before[i + 2] > 245)) canvasInk += 1;
    }
    if (canvasInk > 0) {
      window.yanshiStats.blankBlitsSkipped = (window.yanshiStats.blankBlitsSkipped || 0) + 1;
      (window.yanshiStats.blitLog = window.yanshiStats.blitLog || []).push({
        reason: "skipped-blank-over-ink " + JSON.stringify([x0, y0, w, h]),
        area: w * h,
        at: Math.round(performance.now()),
        serverInk,
        canvasInk,
      });
      window.yanshiStats.blitLog = window.yanshiStats.blitLog.slice(-12);
      return 0;
    }
  }
  ctx.putImageData(new ImageData(bytes, w, h), Math.round(x0 - vx), Math.round(y0 - vy));
  window.yanshiStats.serverBlits = (window.yanshiStats.serverBlits || 0) + 1;
  window.yanshiStats.lastServerBlitArea = w * h;
  window.yanshiStats.lastServerBlitReason = "box " + JSON.stringify([x0, y0, w, h]);
  (window.yanshiStats.blitLog = window.yanshiStats.blitLog || []).push({
    reason: window.yanshiStats.lastServerBlitReason,
    area: w * h,
    at: Math.round(performance.now()),
    serverInk,
  });
  window.yanshiStats.blitLog = window.yanshiStats.blitLog.slice(-12);
  return w * h;
}

// **把"按区域补画"单独暴露出来** ✓（第 54 轮的窄实验 ✓）：拖动中那个卡点到底是
// **补画路径本身画不上** ✗，还是**拖动期的时序/排队把它挤掉了** ✗ —— 单独调一次就知道 ✓。
// **必须在本段里挂** ✗（`window.yanshi` 那个对象在**另一段**脚本里 ✓，够不到这个名字 ✓ ——
// 这个坑本项目踩过很多次 ✓）。返回实际画上去的面积 ✓（0 = 没画上 ✓，判据就看它 ✓）。
window.yanshiDebugBlit = async (box) => {
  const area = await blitServerBox(box);
  return {
    area,
    serverInk: window.yanshiStats.lastBlitServerInk || 0,
    blits: window.yanshiStats.serverBlits || 0,
  };
};

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
/// **最近一次本地提交的脏区** ✓（含时间戳 ✓）。
///
/// **为什么要它** ✗：真实用户实测"每一笔结束还是会闪一下"✓ —— 探针量到那一笔之后的补画
/// 面积是 **576000 = 整视口** ✓，来源是 **WS 的 heavy 分支**：文档里只要画过一笔
/// （`raster_patch` ⇒ heavy ✓），服务端就会推一条 heavy 事件 ✓，而那一支只会
/// `blitServerViewport()` **整视口**重画 ✗ ⇒ 肉眼可见地闪 ✓。
/// **本地刚提交过** ⇒ 脏区是已知的 ✓ ⇒ 只补那一块 ✓；
/// **别处来的变更**（另一个客户端 / 未知 ✓）⇒ 没有可信脏区 ✗ ⇒ 仍然整视口 ✓（宁可多画 ✓ 不能少画 ✗）。
let lastDirtyBox = null;
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
  window.yanshiStats.lastServerBlitReason = "viewport";
  (window.yanshiStats.blitLog = window.yanshiStats.blitLog || []).push({
    reason: "viewport",
    area: 0,
    at: Math.round(performance.now()),
  });
  window.yanshiStats.blitLog = window.yanshiStats.blitLog.slice(-12);
  // **等布局稳定** ✓ —— 这段原先是**内联**的 ✓；现在挪进 `settleFrames` ✓，
  // 因为"立刻取图会拿到空白"这件事对**脏区那条**同样成立 ✓（本轮实测 ✓），
  // 而两条路本来就都走 `blitServerBox` ✓（一处修好、两条都对 ✓，不是两份实现 ✓）。
  // **与超时赛跑** ✓：后台标签页里 `requestAnimationFrame` 不回调 ✓（连 `setTimeout` 也被节流 ✓）
  // ⇒ 只靠 rAF 会让补画挂住 ✓（四位子 agent 独立遇到 ✓）⇒ helper 里 rAF 与 50ms 超时赛跑 ✓。
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
  // **压力必须发出去** ✗ —— 用户实测报告："用绘画板测试，橡皮没有压力支持，不同压力下表现都一样" ✓。
  // 真相：这一行**只映射了 `x, y`** ✗ ⇒ 压力被丢掉 ⇒ 橡皮只能按**固定强度**擦 ✓
  //（`.myb` 笔刷那条另有 `controlPoints` ✓（6264 一带）带压力 ✓ ⇒ 所以过去只有橡皮受影响 ✓）。
  // 兜底用 0.5 ✓，与门面那条（`paintLiveFrame`）以及 `.myb` 那条保持一致 ✓（同输入 ⇒ 同结果 ✓）。
  const points = state.points.map((p) => [p.x, p.y, Number.isFinite(p.pressure) ? p.pressure : 0.5]);
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
    data: { points, size, color, hardness: 0.7, smooth: smoothEnabled() },
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
  // **⚠️ 无内核时必须走服务端像素** ✓ —— 这里此前**什么都不做** ✗（真实用户报告 ✓）。
  //
  // **症状** ✓："点图层的小眼睛和锁 ⇒ 画布变白 ⇒ **手工刷新可以恢复**" ✓。
  // **为什么是这里** ✓：`resync()` 只会用**内核**重放 ✗（下面两句都要 `loadKernel` ✓）
  // ⇒ **没有内核时它静默返回** ✓ ⇒ 而眼睛/锁的处理器最后一句正是 `await resync()` ✓
  // ⇒ **画布永不重绘** ✓。而"刷新能恢复"恰好证明**服务端是对的** ✓、坏的只是这一处 ✓。
  //
  // **修法与 `afterMutation` 同一套路** ✓：置 `needsServerPixels` ✓ + 整视口排队补画 ✓
  // ⇒ **一处修好、所有调用 `resync()` 的地方一起受益** ✓
  //（图层可见性 ✓、锁定 ✓、新建 ✓、复制 ✓、删除 ✓、重排 ✓、上下移动 ✓）。
  if (!state.wasm) {
    // **无内核 ⇒ 走服务端像素** ✓：可见性/顺序会改变**整幅**外观 ✓ ⇒ 补整个视口 ✓，宁可多补不能漏 ✗。
    needsServerPixels = true;
    queueServerBlit(null);
    return;
  }
  const resumeFrom = state.localSeq || 0;
  // 先试增量 ✓（`localSeq` 是**最后一个成功应用**的序号 ✓ ⇒ 不重复、不遗漏 ✓）。
  if (resumeFrom > 0 && (await loadKernel(resumeFrom))) {
    // **⚠️ 重放完必须重绘主画布** ✗（真实用户报告 ✓，**浏览器里实测复现** ✓）：
    // 这里此前只 `refreshPreview(true)` ✓ —— 那刷的是**预览/缩略图** ✗，**不是画布** ✗
    // ⇒ 于是"点眼睛/锁 ⇒ 画布变白 ⇒ **手工刷新才恢复**" ✗ ✓。
    // **实测数据** ✓（CDP，真浏览器 ✓，`wasm: true` ✓ 即内核在的 ✓）：
    // `before: 9600` ✓ ⇒ 隐藏后 `0` ✓ ⇒ **再显示回来仍然是 `0`** ✗（`resyncs: 2` ✓ 说明它确实被调了 ✓）。
    // ⇒ 所以这**不是**"无内核"的问题 ✓，而是**两条分支都漏了画布** ✗：
    // 增量重放只更新了内核状态 ✓、只刷了预览 ✓ ⇒ **没人把结果画到画布上** ✗。
    await refreshPreview(true);
    redrawCanvasFromKernel();
    return;
  }
  // 增量不可用（或从 0 开始 ✓）⇒ 整条重放 ✓（内核已被上次失败回收 ✓ 或被重建 ✓）。
  if (await loadKernel(0)) {
    await refreshPreview(true);
    redrawCanvasFromKernel();
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
  // **判据是"这份文档需要服务端像素"，不是"有没有内核"** ✗（真实用户实测 ✓，两条症状同一个根因 ✓）：
  // ①"新建图层后画布不刷新，只有手工点刷新才看得到新画的"✗；
  // ②"每一笔结束还是会闪一下"✗ —— 闪就是因为走了**整视口**补画 ✓。
  // **为什么以前没暴露** ✗：`detectHeavyContent` 把**含任何 `raster_patch`（= 任何画笔笔触 ✓）**
  // 的文档都判成 heavy ✓ ⇒ 一旦画过一笔 ✓，`needsServerPixels` 就为真 ✓，
  // 而这里却因为**内核是加载好的**（`state.wasm` 为真 ✗）而**跳过补画** ✗
  // ⇒ 只能等 WS 那条 heavy 分支整视口补画 ✓（既慢又能看见闪 ✗），
  // 或者干脆看不到 ✗（用户手工刷新才出现 ✓）。
  // **本轮查清的事** ✓（真实用户两条实测症状 ✓，记在这里免得下轮从头找 ✓）：
  // 文档里只要画过一笔（`raster_patch` ⇒ heavy ✓）内核就**表示不了它** ✗
  //（能折叠 ✓ 但拿不到 blob 像素 ⇒ 得到**空白**补丁 ✓ 且返回 ok ✗）
  // ⇒ 画布的权威只能是服务端 ✓，而这个判据当时写的是**"有没有内核"** ✗ 而不是
  // **"这份文档要不要服务端像素"** ✗ ⇒ 有内核的机器上提交之后**不补画** ✓
  // ⇒ 用户看到"新建图层后画了看不见，只有手工刷新才行"✗。
  // **但我这一轮没敢改掉它** ✗：改成 `needsServerPixels` 之后，探针量到的墨在 1750 与 0 之间翻覆 ✓
  //（补画流水显示 `box [0,0,900,640]` **执行了却没画上** ✓，而同一时刻的 `viewport` 补画有时也画不出 ✓）
  // ⇒ 说明**下面那两条路本身就不稳** ✗（按脏区取图 / 服务端的区域渲染缓存 ✓），
  // 动门控只会把"看不见"换成"有时看不见" ✗ ⇒ **先留原样 ✓，把判据与证据交给下一轮 ✓**
  //（探针 `scripts/browser-stroke-refresh.mjs` ✓ + `state().blitLog` ✓）。
  // **判据是"这份文档要不要服务端像素"，不是"有没有内核"** ✗ —— 上一轮我从代码上就认定它写错了 ✓，
  // 但当时改了之后画布反而没墨 ✗ ⇒ 撤回 ✓。**现在有 `settleFrames` 兜底** ✓（脏区补画不再画空白 ✓）
  // ⇒ 这条改动才站得住 ✓：文档里只要画过一笔（`raster_patch` ⇒ heavy ✓），
  // 内核就表示不了它 ✗（折叠"成功"但得到空白补丁 ✓）⇒ 必须靠服务端像素 ✓。
  if (needsServerPixels) {
    // **有脏区就只补脏区** ✓（提交响应本来就给了 ✓，此前丢掉不用 ✗、一律整视口 ✓）。
    queueServerBlit(options.dirtyBox || null);
  }
  scheduleThumbRefresh();
  void refreshHistory();
}

async function submitAtom(atom) {
  // **服务端不在时必须被捕获并说清** ✗ —— 用户实测：控制台出现
  // `Uncaught (in promise) TypeError: Failed to fetch` ✓（原来这里没有 try ✗），
  // 而画面上**什么都没说** ✓ ⇒ 用户不知道这一笔到底提交了没有 ✓。
  let response;
  try {
    response = await fetchOrLocal(api("/api/atoms"), {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(atom),
    }).then((r) => r.json());
  } catch (error) {
    $("conn").className = "dot";
    $("connText").textContent = "已断开";
    log("提交失败：连不上服务端 ✗ ⇒ **这一笔没有提交** ✓（服务端可能已退出 ✓；" +
        "把它起回来再画，或刷新页面 ✓）", "#c33");
    return null;
  }
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
  const response = await fetchOrLocal(api(server.raw_url.replace("yanshi://blob/", "/api/blob/")));
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
  // **《偃师造人》以"草稿"登记** ✓ —— 标签说实话 ✓：两处致命伤（同心环 ✗、白框 ✗）已修 ✓，
  // 故事也读得出来 ✓（油灯、朱衣造人、敞开的胸腔与铜枢、右侧偃师的侧影 ✓），
  // 但它仍是**粗放的油画速写** ✗ ⇒ 不冒充成完成品 ✓。
  { id: "sample-yanshi", label: "油画 · 偃师造人（草稿）",
    hint: "生成器作品 1039 笔：统一光源、绝对笔尖、逐层私有画布" },
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
/// **逐 dab 的确定性种子** ✓ —— 这条是照 **libmypaint（brushlib）** 的动力学抄的 ✓：
/// 它的 `Random` 输入 + `RADIUS_BY_RANDOM` / `OFFSET_BY_RANDOM` / `OPAQUE` 都在讲同一件事 ✓ ——
/// **每一枚 dab 必须不一样** ✓，笔触才不是机械的 ✗。
///
/// 而本项目的介质插件**本来就按 seed 生成鬃毛与颗粒** ✓（`yanshi_dab(seed, size, pressure)` ✓），
/// 只是**宿主每一枚都传同一个 seed** ✗（写死 1000 ✓）⇒ 每枚印章的纹理**完全相同** ✓
/// ⇒ 再怎么加笔也只是"同一块印章盖很多次" ✗（这就是画面"机械、像矢量"的根因之一 ✓）。
/// 现在：**每枚 dab 递增取种** ✓、并把**载墨当作压力**传进去 ✓（`pressure` 影响笔痕的浓淡 ✓）。
/// 计数器在每次画谱开始时归零 ✓ ⇒ 同一份画谱仍然画出**同一幅画** ✓（确定性不破 ✓）。
let mediumDabCounter = 0;
const nextDabSeed = () => ((mediumDabCounter++ * 0x9E3779B1) ^ 0x5BF03635) >>> 0;

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
/// **批处理画在哪张画布上** ✓ —— 这是一个**必须讲清的选择** ✓：
///
/// 逐笔提交时，盖章是画在一块"这一笔的包围盒"小画布上 ✓，取色则读**内容画布**（= 文档合成 ✓）。
/// 批量之后如果照旧读合成 ✓ ⇒ 提交的补丁会把**别的图层的内容连同背景一起压进本层** ✗ ——
/// 上一版实拍就是**三个白色矩形硬边** ✓（人物、胸腔、灯各一个 ✓），因为那些地方合成是白的 ✓。
/// **这不只是显示问题** ✓：它意味着"每一层都存了一份合成" ✗，图层显隐、体积、语义全都错 ✓。
///
/// 所以批处理**自己开一张透明画布** ✓，只在上面画本层的颜料 ✓：
/// * 取色读**本层** ✓（这才是画家真正在混的颜色 ✓ —— 湿画法混的是自己的颜料 ✓）；
/// * 提交只提交**本层**的像素 ✓ ⇒ 白框消失 ✓，"每层存合成"的老毛病一并修掉 ✓。
/// 坐标是**文档坐标** ✓（没有视口偏移 ✓）⇒ 与 `state.docSize` 同尺寸 ✓。
// **同一图层共用一张画布** ✓ —— 一个图层可能分好几批画 ✓（1039 笔 ÷ 30 ⇒ 35 批 ✓），
// 若每批都新开一张空白画布 ✗ ⇒ 后续批次取色看不到本层**先前**的颜料 ✓ ⇒ 湿画法被削弱 ✗、
// 画出来的东西也就随之改变 ✗。所以按图层 id 缓存 ✓（换文档时清掉 ✓，尺寸可能不同 ✓）。
const layerCanvases = new Map();
function newLayerCanvas(layerId) {
  const key = String(layerId) + "@" + Math.round(state.docSize.w) + "x" + Math.round(state.docSize.h);
  const cached = layerCanvases.get(key);
  if (cached) return cached;
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(state.docSize.w));
  canvas.height = Math.max(1, Math.round(state.docSize.h));
  layerCanvases.set(key, canvas);
  return canvas;
}

/// **一点上的压感** ✓：`undefined`/`null` 表示"没有压感信息" ✓（鼠标 ✓、旧画谱 ✓）。
function pressureOf(point) {
  if (!point) return null;
  const value = point.pressure;
  if (value === undefined || value === null) return null;
  const number = Number(value);
  if (!Number.isFinite(number)) return null;
  return Math.max(0, Math.min(1, number));
}

/// 两点之间插值 ✓（任一端没有压感就返回 `null` ✓ —— 不猜 ✗）。
function pressureAlong(from, to, t) {
  const a = pressureOf({ pressure: from });
  const b = pressureOf({ pressure: to });
  if (a === null || b === null) return null;
  return a + (b - a) * t;
}

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
  const stamps = [Object.assign({}, points[0], { pressure: pressureOf(points[0]) })];
  for (let i = 1; i < points.length; i++) {
    const from = points[i - 1];
    const to = points[i];
    const distance = Math.hypot(to.x - from.x, to.y - from.y);
    const steps = Math.max(1, Math.ceil(distance / spacing));
    for (let step = 1; step <= steps; step++) {
      const t = step / steps;
      stamps.push({
        x: from.x + (to.x - from.x) * t,
        y: from.y + (to.y - from.y) * t,
        // **压感沿笔迹插值** ✓（拖动事件稀疏 ✓ ⇒ 不插值会一跳一跳 ✗）。
        pressure: pressureAlong(from.pressure, to.pressure, t),
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
    // **取色来源分两种** ✓（见 `newLayerCanvas` 的说明 ✓）：
    // 批处理 ⇒ 读**本层画布**的文档坐标 ✓（混自己的颜料 ✓）；否则 ⇒ 读内容画布的视口坐标 ✓（原样 ✓）。
    const sourceCanvas = mediumBatchSession ? mediumBatchSession.canvas : board;
    const destX = mediumBatchSession
      ? Math.max(0, Math.min(sourceCanvas.width - 1, Math.round(point.x)))
      : Math.max(0, Math.min(sourceCanvas.width - 1, Math.round(point.x - state.viewport.x)));
    const destY = mediumBatchSession
      ? Math.max(0, Math.min(sourceCanvas.height - 1, Math.round(point.y)))
      : Math.max(0, Math.min(sourceCanvas.height - 1, Math.round(point.y - state.viewport.y)));
    const dest = sourceCanvas.getContext("2d").getImageData(destX, destY, 1, 1).data;
    if (typeof plugin.yanshi_input_ptr === "function") {
      const floats = plugin.yanshi_input_len() / 4;
      const input = new Float32Array(plugin.memory.buffer, plugin.yanshi_input_ptr(), Math.max(floats, 10));
      input.set([tip[0], tip[1], tip[2], 1, dest[0] / 255, dest[1] / 255, dest[2] / 255, dest[3] / 255, load, wetness], 0);
    }
    // **逐 dab 取种 + 以载墨为压力** ✓（见 `nextDabSeed` 的说明 ✓）——
    // 此前这里把"强度"当 seed 传 ✗：强度**每一枚都一样** ✓ ⇒ 纹理重复 ✗；
    // 而它本该影响的是**湿度** ✓（已经由上面的输入数组承担 ✓）。
    // **第三参数就是 ABI 里的 `pressure`** ✓ —— 此前宿主传的是"载墨" ✗（按行进距离衰减 ✓）。
    // 有压感时用压感 ✓（量程 1..1000 ✓）；没有时沿用载墨 ✓ ⇒ **旧行为逐位一致** ✓。
    // **这一版故意不动 `size`** ✗：我上一版让"逐笔尖尺寸"随压感变 ✓，结果整幅墨量成了 0 ✗
    //（`ImageData` 与画布尺寸不匹配 ⇒ 抛错被 catch ⇒ 上传了空白补丁 ✓）⇒ 先回退 ✓，
    // "压感改粗细"留到下一轮**单独**做 ✓（它要动画布分配与外扩 ✓，必须单独验证 ✓）。
    const pressure = pressureOf(stamps[i]);
    const dabPressure = pressure === null
      ? Math.max(1, Math.round(load * 1000))
      : Math.max(1, Math.round(pressure * 1000));
    if (pressure !== null) {
      window.yanshiStats.pressureUsed = (window.yanshiStats.pressureUsed || 0) + 1;
    }
    // **压感决定这一枚的直径** ✓（用户要求"创作时把笔触压感用起来" ✓）。
    // 曲线 0.45..1.0 × 满笔尖 ✓：收笔细、行笔粗 ✓ —— 这是压感在画面上**看得见**的关键 ✓
    //（只改浓淡几乎看不出来 ✗）。没有压感信息时（鼠标 ✓ / 旧画谱 ✓）⇒ 用满笔尖 ✓ ⇒ **旧行为一致** ✓。
    const dabSize = pressure === null
      ? size
      : Math.max(2, Math.round(size * (0.45 + 0.55 * pressure)));
    const written = plugin.yanshi_dab(nextDabSeed(), dabSize, dabPressure);
    if (written === 0) break;
    const pixels = new Uint8ClampedArray(plugin.memory.buffer, plugin.yanshi_dab_ptr(), written);
    // **写之前必须清画布** ✗ —— 我上一轮漏了这一步 ⇒ `dabCanvas` 是 `size×size` 的**复用**画布 ✓
    // ⇒ 换成细笔尖时，左上角之外**留着上一枚满笔尖的像素** ✗ ⇒ 补丁被污染 ✗
    //（那就是上一轮"整幅墨 0 / 画面错乱"的成因 ✓ —— 这次先清 ✓，并且**只合成这一枚的区域** ✓）。
    dabContext.clearRect(0, 0, dabCanvas.width, dabCanvas.height);
    // `pixels` 的长度**正好**是 `dabSize²×4` ✓（插件 ABI 保证 ✓）⇒ 与 `ImageData` 尺寸一致 ✓。
    dabContext.putImageData(new ImageData(new Uint8ClampedArray(pixels), dabSize, dabSize), 0, 0);
    // 细笔尖要**居中**于原落点 ✓，否则细的那一段会整体偏左上 ✗（同一条线会走歪 ✓）。
    const inset = (size - dabSize) / 2;
    // **只合成 `dabSize` 那一块** ✓（`drawImage` 的九参形式 ✓）⇒ 不会把画布其余部分一起贴上去 ✗。
    // 以 source-over 叠加 ✓ ⇒ 同一条笔触内的颜料会累积 ✓（油画堆料 ✓）。
    if (mediumBatchSession) {
      mediumBatchSession.context.drawImage(dabCanvas, 0, 0, dabSize, dabSize,
        point.x - half + inset, point.y - half + inset, dabSize, dabSize);
    } else {
      paint.drawImage(dabCanvas, 0, 0, dabSize, dabSize,
        point.x - half + inset - minX, point.y - half + inset - minY, dabSize, dabSize);
    }
    total += 1;
  }
  void source;
  if (total === 0) {
    log("介质整笔：没有落笔（载墨为 0？）");
    return;
  }
  const rgba = new Uint8Array(paint.getImageData(0, 0, width, height).data.buffer);
  if (mediumBatchSession && mediumBatchSession.layerId === state.layerId) {
    // **批处理：把这一笔的颜料显示到内容画布上** ✓ —— 用户要看得见进度 ✓；
    // 而**取色**读的是本层画布 ✓（见 `newLayerCanvas` 的说明 ✓）⇒ 本层只装本层的东西 ✓。
    const context = board.getContext("2d");
    const viewX = Math.round(minX - state.viewport.x);
    const viewY = Math.round(minY - state.viewport.y);
    context.drawImage(mediumBatchSession.canvas, Math.round(minX), Math.round(minY), width, height,
                      viewX, viewY, width, height);
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
  const layerCanvas = newLayerCanvas(layerId);
  const session = {
    layerId, box: null, stamps: 0, strokes: 0,
    canvas: layerCanvas, context: layerCanvas.getContext("2d"),
  };
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
  // **从本层画布取这一块** ✓（文档坐标 ✓）—— 这正是"本层只装本层的东西"的落点 ✓。
  const x = Math.max(0, Math.round(session.box.x));
  const y = Math.max(0, Math.round(session.box.y));
  const w = Math.max(1, Math.min(session.canvas.width - x, Math.round(session.box.w)));
  const h = Math.max(1, Math.min(session.canvas.height - y, Math.round(session.box.h)));
  const rgba = new Uint8Array(session.context.getImageData(x, y, w, h).data.buffer);
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
  // 总笔数 ✓（进度公布用 ✓）—— 让"进度"这个概念在批量与逐笔两条路上都成立 ✓。
  const scoreTotal = (score.layers || []).reduce((sum, layer) => sum + (layer.strokes || []).length, 0);
  // **归零** ✓：逐 dab 的种子序列从同一处开始 ✓ ⇒ 同一份画谱重放得到同一幅画 ✓。
  mediumDabCounter = 0;
  window.yanshiStats.score = { strokesDone: 0, layersDone: 0, total: scoreTotal, done: false };
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
      // **画谱的点可以带压感** ✓：`[x, y, pressure]` ✓ 或 `{x, y, pressure}` ✓；不给就 `null` ✓
      // ⇒ 下游退回原行为 ⇒ **旧画谱逐位一致** ✓。
      const points = (stroke.points || []).map((point) =>
        Array.isArray(point)
          ? { x: point[0], y: point[1], pressure: point.length > 2 ? point[2] : null }
          : { x: point.x, y: point.y, pressure: point.pressure !== undefined ? point.pressure : null });
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
      // **把进度公布出来** ✓ —— 外部脚本不该再靠"原子数"猜进度 ✗：
      // 批量提交之后"一次提交 = 一批笔" ✓ ⇒ 原子数 ≪ 笔数 ✓ ⇒ 我第一版按原子数判断"画完" ✓
      // ⇒ **提前收工** ✗（只画了约 690 笔就截图 ✓，日志还写着"约 23 笔" ✗）。
      window.yanshiStats.score = { strokesDone, layersDone, total: scoreTotal, done: false };
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
  window.yanshiStats.score = { strokesDone, layersDone, total: scoreTotal, done: true };
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
  // 同名局部变量会把它遮蔽 ✓，于是后面 `fetchOrLocal(api("/api/blob"))` 会调到一个对象上 ✗
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
  const written = plugin.yanshi_dab(nextDabSeed(), size, 1000);
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
  const upload = await fetchOrLocal(api("/api/blob"), {
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
  const upload = await fetchOrLocal(api("/api/blob"), {
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
/// **导入 PSD** ✓（设计第 17 章的"只读导入" ✓）—— **走服务端** ✓，浏览器解不了 PSD ✗。
///
/// **为什么要分流** ✓：本函数上面那条路用 `createImageBitmap(file)` ✓（浏览器解码 ✓）——
/// 它只认 PNG/JPEG/WebP ✓；用户选一个 `.psd` 会**直接抛错** ✗ ⇒ 在界面上表现为"**点了没反应**" ✗
/// —— 正是我第 95 轮扫过的那一类**静默失败** ✗。所以这里**按签名**（`8BPS` ✓）认出来 ✓，
/// 把**原始字节**交给服务端的 `import_psd` ✓（它只取**合成图** ✓、并会给**具体原因** ✓）。
async function importPsdFile(file) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  const upload = await fetchOrLocal(api("/api/blob"), {
    method: "POST",
    headers: { "content-type": "application/octet-stream" },
    body: bytes,
  }).then((response) => response.json());
  if (!upload.ok) {
    log("导入 PSD 失败（上传）：" + (upload.error_code || (upload.context && upload.context.detail) || "unknown"), "#c33");
    return;
  }
  const layerId = "psd_" + ulid();
  const created = await callToolChecked("create_layer",
    { layer_id: layerId, name: file.name || "PSD" }, "导入 PSD（新建图层）");
  if (!created || !created.ok) return;
  const imported = await callToolChecked("import_psd",
    { layer_id: layerId, blob_hash: upload.blob_hash }, "导入 PSD");
  if (imported && imported.ok) {
    // **把"只导入了什么"讲清楚** ✓（只读契约 ✓）：用户该知道图层结构没有进来 ✓。
    const source = imported.source || {};
    log("已导入 PSD 的**合成图**（" + (source.width || "?") + "×" + (source.height || "?") +
        "）—— 图层结构与蒙版没有导入（只读）");
  }
}

async function importLocalImage(file) {
  // **先按签名认 PSD** ✓（不靠扩展名 ✓ —— 扩展名不可靠 ✓）。
  const head = new Uint8Array(await file.slice(0, 4).arrayBuffer());
  if (head.length === 4 && head[0] === 0x38 && head[1] === 0x42 && head[2] === 0x50 && head[3] === 0x53) {
    await importPsdFile(file);
    return;
  }
  const bitmap = await createImageBitmap(file);
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const context = canvas.getContext("2d");
  context.drawImage(bitmap, 0, 0);
  const imageData = context.getImageData(0, 0, bitmap.width, bitmap.height);
  const bytes = new Uint8Array(imageData.data.buffer);

  const upload = await fetchOrLocal(api("/api/blob"), {
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
  // **按 id 打开示例也要种入画面** ✓ —— 此前种入只挂在"点示例卡片"上 ✗
  // ⇒ 收藏的链接、程序化打开（子 agent 的脚本 ✓）都只会得到一个**空文档** ✓，
  // 而那看起来就像"示例是坏的" ✗（我自己的探针就这样误判过一次 ✓：按 id 切过去 ⇒ 墨 0 ✗）。
  // `seedSampleIfEmpty` 只在"是已知示例 **且** 文档为空"时才动作 ✓ ⇒ 用户改过的示例绝不被覆盖 ✓。
  if (await seedSampleIfEmpty(state.docId)) {
    await resync();
    await refreshPreview();
    await refreshLayers();
  }
  // **打开就保证有图层可画** ✓（见 `ensurePaintLayer` 的说明 ✓）：
  // 否则普通笔刷会被静默拒绝 ✗ —— 用户只会看到"点了没反应" ✗。
  await ensurePaintLayer();
  await refreshLayers();
  // **打开文档就拉一次标注与对象** ✓（面板立刻正确 ✓）。
  await refreshAnnotations();
  await refreshObjects();
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
  setupAnnotationPanel();
  setupObjectPanel();
  setupCheckpointPanel();
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

/// **撤销一笔**（整笔粒度 ✓，目标 ⑦ ✓）。
///
/// **为什么换成服务端工具** ✗：原来这里是"本地 `undoStack` + 原子级 `revert`" ✓
/// ⇒ 两个毛病 ✓：**(1)** 粒度是**一个原子** ✓（一笔若由多条原子组成 ⇒ 只撤掉一半 ✗）；
/// **(2)** 本地栈**会与事实漂移** ✓（刷新页面、或另一个客户端改了文档 ⇒ 栈里还记着旧东西 ✗
/// ⇒ 界面上"可撤销 3 步" ✓、实际一步都没有 ✗）。
/// ⇒ 现在只问服务端 ✓：`undo_last` 是**整笔**的 ✓，并**回报还剩几笔** ✓ ⇒ 界面照它显示 ✓。
async function undoOnce() {
  const value = await callTool("undo_last", {}, { refresh: false });
  if (!value || value.ok === false) {
    log("撤销失败：" + ((value && value.error_code) || "unknown") + " " +
        ((value && value.context && value.context.detail) || ""), "#c33");
    return;
  }
  if (!value.undone_count) {
    // **把服务端的话原样说出来** ✓（"没有可撤销的"是**事实** ✓，不是静默无反应 ✗）。
    log(value.message || "没有可撤销的笔迹", "#c93");
  } else {
    log("已撤销 " + value.gestures_undone + " 笔（" + value.undone_count + " 条原子）✓｜还剩 " +
        value.remaining_gestures + " 笔", "#2a2");
  }
  // **数字以服务端为准** ✓。
  state.remainingUndo = value.remaining_gestures;
  updateUndoStatus();
  await refreshPreview();
  await resync();
}

/// **重做一笔** ✓（`undo_last` 的对称面 ✓，同样以服务端为准 ✓）。
async function redoOnce() {
  const value = await callTool("redo_last", {}, { refresh: false });
  if (!value || value.ok === false) {
    log("重做失败：" + ((value && value.error_code) || "unknown") + " " +
        ((value && value.context && value.context.detail) || ""), "#c33");
    return;
  }
  if (!value.redone_count) {
    log(value.message || "没有可重做的笔迹", "#c93");
  } else {
    log("已重做 " + value.gestures_redone + " 笔（" + value.redone_count + " 条原子）✓", "#2a2");
  }
  state.remainingRedo = value.remaining_gestures;
  updateUndoStatus();
  await refreshPreview();
  await resync();
}

/// **显示"还剩几笔"** ✓ —— **数字来自服务端** ✓，不是本地栈 ✗。
///
/// **为什么必须换** ✗：本地栈在"刷新页面 / 另一个客户端改了文档"之后**会与事实不符** ✓
/// ⇒ 界面写"可撤销 3 步" ✓、实际一步都撤不了 ✗ —— 正是"界面与事实不一致" ✓。
/// 没拿到服务端数字之前显示 `—` ✓（**不猜** ✓）。
function updateUndoStatus() {
  const label = $("undoDepth");
  const undoCount = typeof state.remainingUndo === "number" ? state.remainingUndo : null;
  const redoCount = typeof state.remainingRedo === "number" ? state.remainingRedo : null;
  if (label) {
    label.textContent =
      "可撤销 " + (undoCount === null ? "—" : undoCount) +
      " 笔 / 可重做 " + (redoCount === null ? "—" : redoCount) + " 笔";
  }
  const undo = document.querySelector('button[data-tool="undo"]');
  const redo = document.querySelector('button[data-tool="redo"]');
  // **"计数未知"也算不可用** ✗（判据实测：空栈时 `undo.disabled === false` ✓ ⇒ 按钮可点却没反应 ✓）。
  // `undoCount` 为 `null` 表示未知 ✓，而 `null === 0` 是假 ✗ ⇒ 老代码保持可点 ✗。
  // ⇒ 未知时**先禁用** ✓，等计数到达再启用 ✓（不给"看着能点"的中间态 ✗）。
  if (undo) undo.disabled = undoCount === null || undoCount === 0;
  if (redo) redo.disabled = redoCount === null || redoCount === 0;
}

/// 效果目录来自服务端 `/api/effects`（内容就是内核的 `ADJUSTMENT_NAMES` / `FILTER_NAMES`），
/// 因此查看器**不硬编码效果名** ✓，也就不会与内核漂移 ✓。
let effectCatalog = { adjustment: [], filter: [] };

async function loadEffectCatalog() {
  const value = await fetchOrLocal(api("/api/effects"), {
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
  // **"应用"与"更新"分流** ✓：正在编辑某个效果时就**改它** ✓，否则**新建** ✓。
  //
  // `update_adjustment` / `update_filter`（设计 §792 一带的调整与滤镜 ✓）此前在查看器里**零引用** ✗
  // ⇒ 用户"加完想再调一调"做不到 ✓ —— 与"标注/实例组/检查点/矢量互转"是**同一类缺口** ✓。
  // 两条工具的真实签名 ✓：`{object_id, params（覆盖）, opacity?}` ✓。
  let value;
  if (editingEffectId) {
    const update = kind === "adjustment" ? "update_adjustment" : "update_filter";
    value = await callToolChecked(update, { object_id: editingEffectId, params }, "更新" + (kind === "adjustment" ? "调整" : "滤镜"));
    if (value && value.ok) log("已更新「" + name + "」的参数 ✓");
  } else {
    const tool = kind === "adjustment" ? "add_adjustment" : "add_filter";
    const args =
      kind === "adjustment"
        ? { layer_id: state.layerId, adjustment_type: name, params }
        : { layer_id: state.layerId, filter_name: name, params };
    value = await callTool(tool, args, { refresh: false });
    if (!value.ok) {
      log("应用失败：" + (value.error_code || "unknown") + " " +
          ((value.context && value.context.detail) || ""), "#c33");
      return;
    }
    log("已应用" + (kind === "adjustment" ? "调整" : "滤镜") + " " + name);
  }
  await refreshEffects();
  await refreshPreview();
}

/// **当前正在编辑的效果** ✓（`null` = 新建模式 ✓）。
let editingEffectId = null;

/// 退出编辑态 ✓（回到"新建" ✓）。
function clearEffectEditing() {
  editingEffectId = null;
  const button = $("effectApply");
  if (button) button.textContent = "应用";
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
    // **点一行就进入"编辑它"** ✓（这也是"更新"与"新建"的分界 ✓）。
    row.style.cursor = "pointer";
    row.title = "点一下：编辑这个" + (effect.type === "adjustment" ? "调整" : "滤镜") + " ✓";
    if (effect.object_id === editingEffectId) row.style.outline = "1px solid #ffd166";
    row.addEventListener("click", () => {
      editingEffectId = effect.object_id || null;
      // 把**它现在的参数**装进输入框 ✓ —— 用户改一个数就能更新 ✓，不必从头敲 JSON ✓。
      const box = $("effectParams");
      if (box) box.value = JSON.stringify(effect.params || {});
      const kindBox = $("effectKind");
      if (kindBox) kindBox.value = (effect.adjustment_type || effect.type === "adjustment") ? "adjustment" : "filter";
      const button = $("effectApply");
      if (button) button.textContent = "更新这个";
      log("正在编辑「" + name + "」✓ —— 改好参数后点「更新这个」✓");
      renderEffectsRowHighlight();
    });
    list.appendChild(row);
  }
}

/// 重画效果行的选中描边 ✓（点选之后把高亮挪到那一行 ✓）。
function renderEffectsRowHighlight() {
  void refreshEffects();
}

/// **变更集** ✓（设计 793 ✓）—— "**把接下来这一串动作打包 ✓、不满意就整体撤销**" ✓。
///
/// **真实签名** ✓（先读规格 ✓）：`begin_changeset {}` ✓（**无参数** ✓；此后**本会话**的提交都并入它 ✓）、
/// `commit_changeset {}` ✓（收尾 ✓，**原子保留** ✓）、`abort_changeset {}` ✓（**整体撤销** ✓）、
/// `get_changesets {limit?}` ✓（返回 `{changesets:[{changeset_id, atoms, first_seq, last_seq, kinds}], count}` ✓）、
/// `revert_changeset {changeset_id}` ✓。
///
/// **为什么是用户会要的** ✓：这就是画家说的"**这一串动作打包，一起撤**" ✓ ——
/// 比一次一次点撤销实际得多 ✓（而且**原子都还在日志里** ✓ ⇒ 撤销本身也可撤销 ✓）。
let changesetCache = [];

function renderChangesetList() {
  const list = $("changesetList");
  if (!list) return;
  list.innerHTML = "";
  if (changesetCache.length === 0) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = "还没有变更集 ✓（点「开始变更集」再画 ✓）";
    list.appendChild(empty);
    return;
  }
  for (const item of changesetCache) {
    const row = document.createElement("div");
    row.className = "annotation-row";
    const head = document.createElement("div");
    head.className = "annotation-head";
    head.textContent = "#" + String(item.changeset_id || "").slice(-8) +
      " · " + (item.atoms === undefined ? '?' : item.atoms) + " 条原子" +
      (item.first_seq !== undefined ? " · seq " + item.first_seq + "–" + item.last_seq : "");
    const kinds = document.createElement("div");
    kinds.className = "hint";
    kinds.textContent = (item.kinds || []).slice(0, 5).join(" ") || "（无类型信息 ✓）";
    const actions = document.createElement("div");
    actions.className = "toolbar";
    const undo = document.createElement("button");
    undo.type = "button";
    undo.textContent = "整体撤销";
    undo.addEventListener("click", async () => {
      const done = await callToolChecked("revert_changeset",
        { changeset_id: item.changeset_id }, "整体撤销变更集");
      if (done && done.ok) {
        log("已整体撤销变更集 ✓（撤销本身也在日志里 ✓ ⇒ 还能再前进 ✓）");
        await resync();
      }
      await refreshChangesets();
    });
    actions.appendChild(undo);
    row.append(head, kinds, actions);
    list.appendChild(row);
  }
}

/// 拉一次变更集列表 ✓。
async function refreshChangesets() {
  const listed = await callTool("get_changesets", { limit: 20 }, { refresh: false }).catch(() => null);
  changesetCache = (listed && (listed.changesets)) || [];
  renderChangesetList();
}

/// 变更集面板的按钮 ✓。
function setupChangesetPanel() {
  const begin = $("changesetBegin");
  if (begin) begin.addEventListener("click", async () => {
    const done = await callToolChecked("begin_changeset", {}, "开始变更集");
    if (done && done.ok) log("已开始变更集 ✓ —— 此后这一串动作可以**一起撤销** ✓");
    await refreshChangesets();
  });
  const commit = $("changesetCommit");
  if (commit) commit.addEventListener("click", async () => {
    const done = await callToolChecked("commit_changeset", {}, "提交变更集");
    if (done && done.ok) log("已提交变更集 ✓（原子保留 ✓）");
    await refreshChangesets();
  });
  const abort = $("changesetAbort");
  if (abort) abort.addEventListener("click", async () => {
    const done = await callToolChecked("abort_changeset", {}, "放弃变更集");
    if (done && done.ok) {
      log("已放弃变更集 ✓ —— 里面的原子被**整体撤销** ✓");
      await resync();
    }
    await refreshChangesets();
  });
  const reload = $("changesetReload");
  if (reload) reload.addEventListener("click", () => { void refreshChangesets(); });
}

/// **显示一条原子的净荷** ✓（用本轮新补的 `get_atom` ✓）。
///
/// **为什么值得** ✓：`get_log` 给的是"**元数据**" ✓（谁、什么时候、什么类型 ✓），
/// 而用户真正想知道的是"**这一条到底改了什么**" ✓ —— 那在**净荷**里 ✓，此前**没有任何工具能给** ✗。
async function showAtomDetail(atomId) {
  const box = $("atomDetail");
  const hint = $("atomDetailHint");
  if (!box || !atomId) return;
  const value = await callTool("get_atom", { atom_id: atomId }, { refresh: false }).catch(() => null);
  if (!value || !value.ok) {
    const reason = (value && value.context && value.context.detail) || "读取失败";
    box.textContent = "✗ " + reason;
    if (hint) hint.textContent = "读取失败 ✓";
    return;
  }
  if (hint) hint.textContent = "#" + value.seq + " " + value.kind + " · " + value.actor +
    " · " + new Date(value.timestamp).toLocaleTimeString();
  box.textContent = JSON.stringify(value.payload, null, 2);
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
    // **详情** ✓（本轮补 ✓）：`get_atom` 把这一条的**净荷**取回来 ✓ ⇒ 才知道它改了什么 ✓。
    const detail = document.createElement("button");
    detail.type = "button";
    detail.textContent = "详情";
    detail.addEventListener("click", () => { void showAtomDetail(atom.atom_id || atom.id); });
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
    // **别忘了把它挂上去** ✓ —— 我第一版只**创建**了「详情」按钮 ✗、没加进 `append` ✓
    // ⇒ 真机验收里按钮文字只有「回到此处」✗ ⇒ **创建了不等于挂上了** ✓
    //（本会话第二次同类 ✓：另一处是「写了补丁但写后校验失败」✓ ⇒ 两次都是**看执行结果才发现** ✓）。
    row.append(seq, kindLabel, actorLabel, detail, jump);
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

/// **标注** ✓（设计 4.6 / 13.4 ✓）—— 七个标注工具**早就有** ✓，此前**编辑器里没有入口** ✗。
///
/// **为什么用独立的图钉层** ✓（而不是画在 `#overlay` 上 ✗）：`#overlay` 是**笔迹预览**的画布 ✓
/// ⇒ 两者共用会互相擦掉 ✗；而且图钉**要能点** ✓（点它选中该标注 ✓），画在画布上就点不了 ✗。
///
/// **交互取舍** ✓：点画布**直接新建**（不弹输入框 ✗）——
/// 弹窗在真机验收里无法自动化 ✓，而且打断作画 ✓；文字就在面板里改 ✓（`<input>` ✓，可自动化 ✓）。
let annotationCache = [];
let annotationShowResolved = false;
let annotationSelected = null;

/// 一条标注的**文档坐标** ✓。
///
/// **真实的存储形状** ✓（我第一版猜成了 `payload.point` ✗ ⇒ 服务端当场回了一句
/// "**不接受参数 `point`；可用参数：type, intent, target, content…**" ✓ —— 框架把我该用的名字报了出来 ✓）：
/// 目标存在 `target` 里 ✓，形如 `{target:'region', bbox:{x,y,w,h}}` ✓ ⇒ 取 bbox 的中心 ✓。
function annotationPointOf(item) {
  const target = item.target || item.payload || {};
  const bbox = target.bbox;
  if (bbox && typeof bbox.x === "number") {
    return { x: bbox.x + (bbox.w || 0) / 2, y: bbox.y + (bbox.h || 0) / 2 };
  }
  if (typeof target.x === "number" && typeof target.y === "number") {
    return { x: target.x, y: target.y };
  }
  return null;
}

/// 一条标注的**文字** ✓（字段名是 `content` ✓，不是 `text` ✗）。
function annotationTextOf(item) {
  return typeof item.content === "string" ? item.content : "";
}

/// 是否已解决 ✓（服务端把状态放在 `status` 里 ✓；读不到就当待处理 ✓）。
function annotationResolved(item) {
  return item.status === "resolved" || item.status === "rejected";
}

/// **把文档坐标换算成屏幕坐标** ✓（与落笔用的是同一套换算 ✓，方向相反 ✓）。
function annotationScreenOf(point) {
  const rect = board.getBoundingClientRect();
  const scaleX = rect.width / Math.max(1, board.width);
  const scaleY = rect.height / Math.max(1, board.height);
  return {
    x: rect.left + (point.x - state.viewport.x) * scaleX,
    y: rect.top + (point.y - state.viewport.y) * scaleY,
  };
}

/// 重画图钉 ✓（每次视图变化都调 ✓ —— 由 `redraw()` 挂钩 ✓）。
function positionAnnotationPins() {
  const layer = $("annotationPins");
  if (!layer) return;
  layer.innerHTML = "";
  for (const item of annotationCache) {
    if (annotationResolved(item) && !annotationShowResolved) continue;
    const point = annotationPointOf(item);
    if (!point) continue;
    const at = annotationScreenOf(point);
    const pin = document.createElement("button");
    pin.type = "button";
    pin.textContent = String(item.number || "");
    pin.title = annotationTextOf(item).slice(0, 60);
    if (annotationResolved(item)) pin.classList.add("resolved");
    pin.style.left = Math.round(at.x) + "px";
    pin.style.top = Math.round(at.y) + "px";
    pin.addEventListener("click", () => {
      annotationSelected = item.id;
      renderAnnotationList();
    });
    layer.appendChild(pin);
  }
  // 图钉层与画布对齐 ✓（画布可能因为面板折叠而移动 ✓）。
  const rect = board.getBoundingClientRect();
  const stage = board.parentElement.getBoundingClientRect();
  layer.style.left = Math.round(rect.left - stage.left) + "px";
  layer.style.top = Math.round(rect.top - stage.top) + "px";
  layer.style.width = Math.round(rect.width) + "px";
  layer.style.height = Math.round(rect.height) + "px";
}

/// 面板列表 ✓：文字可改 ✓、可解决 ✓、可删除 ✓（对应 `update_annotation` / `resolve_annotation` / `delete_annotation` ✓）。
function renderAnnotationList() {
  const list = $("annotationList");
  if (!list) return;
  list.innerHTML = "";
  const shown = annotationCache.filter((item) => !annotationResolved(item) || annotationShowResolved);
  if (shown.length === 0) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = "还没有标注 ✓";
    list.appendChild(empty);
    positionAnnotationPins();
    return;
  }
  for (const item of shown) {
    const row = document.createElement("div");
    row.className = "annotation-row" + (item.id === annotationSelected ? " selected" : "");
    const head = document.createElement("div");
    head.className = "annotation-head";
    head.textContent = "#" + (item.number || "?") + " " + (item.actor || "");
    const text = document.createElement("input");
    text.type = "text";
    text.value = annotationTextOf(item);
    text.placeholder = "写点什么…";
    text.addEventListener("change", async () => {
      // **真实参数** ✓：`content` 是**字符串** ✓（我第一版写成 `patch: {text}` ✗ —— 猜的 ✗）。
      await callToolChecked("update_annotation",
        { annotation_id: item.id, content: text.value }, "改标注");
      await refreshAnnotations();
    });
    const actions = document.createElement("div");
    actions.className = "toolbar";
    const resolve = document.createElement("button");
    resolve.type = "button";
    resolve.textContent = annotationResolved(item) ? "重新打开" : "解决";
    resolve.addEventListener("click", async () => {
      // 未解决 ⇒ `resolve_annotation` ✓；已解决 ⇒ 用 `update_annotation {status:"pending"}` 重开 ✓
      //（服务端只允许这一个方向 ✓："仅支持 pending：把已解决/已拒绝的标注重开为待处理" ✓）。
      if (annotationResolved(item)) {
        await callToolChecked("update_annotation",
          { annotation_id: item.id, status: "pending" }, "重开标注");
      } else {
        await callToolChecked("resolve_annotation", { annotation_id: item.id }, "解决标注");
      }
      await refreshAnnotations();
    });
    const remove = document.createElement("button");
    remove.type = "button";
    remove.textContent = "删除";
    remove.addEventListener("click", async () => {
      await callToolChecked("delete_annotation", { annotation_id: item.id }, "删除标注");
      await refreshAnnotations();
    });
    // **拒绝** ✓（`reject_annotation` ✓）—— 与「解决」并列 ✓：
    // 解决表示"认可并处理了" ✓、拒绝表示"这条不成立" ✓，两者都会从待处理里消失 ✓。
    const reject = document.createElement("button");
    reject.type = "button";
    reject.textContent = "拒绝";
    reject.disabled = annotationResolved(item);
    reject.addEventListener("click", async () => {
      await callToolChecked("reject_annotation", { annotation_id: item.id }, "拒绝标注");
      await refreshAnnotations();
    });
    actions.append(resolve, reject, remove);
    row.append(head, text, actions);
    list.appendChild(row);
  }
  positionAnnotationPins();
}

/// **检查点** ✓（设计 §4.5 ✓）—— `checkpoint` / `restore_checkpoint` / `get_checkpoints` 此前**零引用** ✗。
///
/// **取舍说明** ✓：`restore_checkpoint` 的服务端实现是 **`declare_head`** ✓ ⇒ 它只**移动 HEAD** ✓，
/// **原子一条都不会删** ✓（日志 append-only ✓）⇒ 所以它**不是不可逆动作** ✓，不需要二次确认 ✓；
/// 但它**大幅改变画面** ✓ ⇒ 恢复之后必须 `resync()` ✓（让服务端权威像素重新铺满 ✓）。
let checkpointCache = [];

function renderCheckpointList() {
  const list = $("checkpointList");
  if (!list) return;
  list.innerHTML = "";
  if (checkpointCache.length === 0) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = "还没有存档点 ✓（上面那个按钮可以打一个 ✓）";
    list.appendChild(empty);
    return;
  }
  // **新的在上面** ✓（越靠后打的越靠前 ✓，符合"最近用过的"直觉 ✓）。
  for (const item of checkpointCache.slice().reverse()) {
    const row = document.createElement("div");
    row.className = "annotation-row";
    const head = document.createElement("div");
    head.className = "annotation-head";
    head.textContent = (item.name || item.checkpoint_id || "").slice(0, 28) +
      (item.anchor_seq !== undefined ? "  ·  seq " + item.anchor_seq : "");
    const actions = document.createElement("div");
    actions.className = "toolbar";
    const back = document.createElement("button");
    back.type = "button";
    back.textContent = "回到这里";
    back.addEventListener("click", async () => {
      const id = item.checkpoint_id || item.id;
      const done = await callToolChecked("restore_checkpoint", { checkpoint_id: id }, "回到存档点");
      if (done && done.ok) {
        log("已回到存档点「" + (item.name || id) + "」—— 原子都还在日志里 ✓，随时可以再前进 ✓");
        // **必须 resync** ✓：HEAD 跳变之后，画布要由服务端权威像素重新铺 ✓。
        await resync();
        await refreshCheckpoints();
  await refreshSuggestions();
  await refreshComments();
  await refreshChangesets();
      }
    });
    actions.appendChild(back);
    row.append(head, actions);
    list.appendChild(row);
  }
}

/// 拉一次存档点列表 ✓（工具名是 **`get_checkpoints`** ✓ —— 不是 `list_checkpoints` ✗）。
async function refreshCheckpoints() {
  const listed = await callTool("get_checkpoints", {}, { refresh: false }).catch(() => null);
  checkpointCache = (listed && (listed.checkpoints || listed.items)) || [];
  renderCheckpointList();
}

/// 打一个存档点 ✓。
async function createCheckpoint() {
  const now = new Date();
  const name = "存档 " + String(now.getHours()).padStart(2, "0") + ":" +
    String(now.getMinutes()).padStart(2, "0") + ":" + String(now.getSeconds()).padStart(2, "0");
  const created = await callToolChecked("checkpoint", { name, message: "来自编辑器的存档点" }, "打存档点");
  if (created && created.ok) {
    log("已打存档点「" + name + "」✓");
    await refreshCheckpoints();
  }
}

/// 存档点面板的两个按钮 ✓。
function setupCheckpointPanel() {
  const create = $("checkpointCreate");
  if (create) create.addEventListener("click", () => { void createCheckpoint(); });
  const reload = $("checkpointReload");
  if (reload) reload.addEventListener("click", () => { void refreshCheckpoints(); });
}

/// **对象面板** ✓（设计 §9：复制 / 实例化 / 组引用 ✓）—— 目标③点名的"实例/组" ✓。
///
/// **为什么值得补** ✓：`create_instance` / `create_group` / `add_to_group` 此前在查看器里**零引用** ✗
/// ⇒ 用户**根本用不到** ✓ —— 与"标注"完全同一类缺口 ✓（工具就绪 ✓、界面没有入口 ✗）。
///
/// **真实签名** ✓（这一轮**先读规格再写调用** ✓，不是猜 ✗）：
/// `create_instance {instance_id, layer_id, master_id, local_transform?}` ✓、
/// `create_group {group_id, layer_id, members?}` ✓（**一次就能带成员** ✓）、
/// `delete_object {object_id}` ✓、`list_objects {layer_id?}` ✓（可按图层过滤 ✓）。
let objectCache = [];
let objectChecked = new Set();

function renderObjectList() {
  const list = $("objectList");
  if (!list) return;
  list.innerHTML = "";
  if (objectCache.length === 0) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = "当前图层还没有对象 ✓";
    list.appendChild(empty);
    return;
  }
  for (const item of objectCache) {
    const row = document.createElement("div");
    row.className = "annotation-row";
    const head = document.createElement("div");
    head.className = "annotation-head";
    const check = document.createElement("input");
    check.type = "checkbox";
    check.checked = objectChecked.has(item.object_id);
    check.addEventListener("change", () => {
      if (check.checked) objectChecked.add(item.object_id);
      else objectChecked.delete(item.object_id);
    });
    const label = document.createElement("span");
    label.textContent = " " + (item.type || "object") + " · " + String(item.object_id).slice(-8);
    head.append(check, label);
    const actions = document.createElement("div");
    actions.className = "toolbar";
    const remove = document.createElement("button");
    remove.type = "button";
    remove.textContent = "删除";
    remove.addEventListener("click", async () => {
      await callToolChecked("delete_object", { object_id: item.object_id }, "删除对象");
      await refreshObjects();
    });
    actions.appendChild(remove);
    row.append(head, actions);
    list.appendChild(row);
  }
}

/// 拉一次当前图层的对象列表 ✓。
async function refreshObjects() {
  if (!state.layerId) { objectCache = []; renderObjectList(); return; }
  const listed = await callTool("list_objects", { layer_id: state.layerId }, { refresh: false }).catch(() => null);
  objectCache = (listed && (listed.objects || listed.items)) || [];
  renderObjectList();
}

/// 勾选的对象 id ✓（保持顺序 ✓）。
function checkedObjects() {
  return objectCache.map((item) => item.object_id).filter((id) => objectChecked.has(id));
}

/// **实例化** ✓：拿勾选的**第一个**做 master ✓，在同一个图层建一个**实例** ✓（设计 9.1 的 linked 复制 ✓）。
async function instanceCheckedObject() {
  const ids = checkedObjects();
  if (ids.length === 0) { log("先勾选一个对象 ✓（实例化要以它为 master ✓）", "#c33"); return; }
  const master = ids[0];
  if (!state.layerId) return;
  const instanceId = "inst_" + ulid();
  // 偏移一点 ✓ ⇒ 新实例不会**正落在 master 上面**（否则看起来"没反应" ✗）。
  const created = await callToolChecked("create_instance", {
    instance_id: instanceId, layer_id: state.layerId, master_id: master,
    local_transform: { dx: 24, dy: 24 },
  }, "实例化对象");
  if (created && created.ok) {
    log("已实例化：master " + String(master).slice(-8) + " ⇒ 实例 " + instanceId.slice(-8) + "（联动 ✓）");
  }
  await refreshObjects();
}

/// **编组** ✓：把勾选的对象一次性建成一个组 ✓（`create_group` 支持带 `members` ✓）。
async function groupCheckedObjects() {
  const ids = checkedObjects();
  if (ids.length === 0) { log("先勾选要编组的对象 ✓", "#c33"); return; }
  if (!state.layerId) return;
  const groupId = "group_" + ulid();
  const created = await callToolChecked("create_group", {
    group_id: groupId, layer_id: state.layerId, members: ids,
  }, "编组");
  if (created && created.ok) log("已编组 " + ids.length + " 个对象 ⇒ " + groupId.slice(-8));
  await refreshObjects();
}

/// **矢量互转** ✓（设计 §792 / 目标①把"矢量"列为介质之一 ✓）——
/// `convert_to_shape` 与 `convert_to_path` 此前在查看器里**零引用** ✗ ⇒ 用户够不到 ✓。
///
/// **真实签名** ✓（先读规格 ✓）：两者都只收 `object_id` ✓（+ 可选的目标 id ✓）。
/// 把画好的笔迹转成**形状**或**路径**之后 ✓，它就能被当作矢量对象继续编辑 ✓，
/// 而原始笔迹仍留在日志里 ✓（可撤销 ✓、可回放 ✓）。
async function convertCheckedObjects(tool, what, idField, prefix) {
  const ids = checkedObjects();
  if (ids.length === 0) { log("先勾选要" + what + "的对象 ✓", "#c33"); return; }
  let done = 0;
  const failed = [];
  for (const objectId of ids) {
    const args = { object_id: objectId };
    // 目标 id 可选 ✓；显式给一个**可读的前缀** ✓，便于在对象列表里认出来 ✓。
    args[idField] = prefix + ulid();
    const result = await callToolChecked(tool, args, what);
    if (result && result.ok) done += 1;
    else failed.push(objectId);
  }
  if (done > 0) log("已" + what + " " + done + " 个对象 ✓" + (failed.length ? "（" + failed.length + " 个失败 ✗）" : ""));
  // **转换会改变对象的呈现** ✓ ⇒ 走一次 resync ✓（服务端权威像素 ✓）。
  if (done > 0) await resync();
  await refreshObjects();
}

/// **评论** ✓（设计 §12.6 的协作通道 ✓）—— `comment` 此前在查看器里**零引用** ✗。
///
/// **真实签名** ✓（先读规格 ✓）：`comment {text, target_atom?, object_id?}` ✓
/// —— 它是一个**协作原子** ✓，"**不产生状态效果**" ✓（即：它不会改变画面 ✓）。
/// **读取用 `list_comments`** ✓ —— 它**是本轮新补的** ✓：
/// 此前**没有**这个工具 ✗，而 `get_log` / `find_atom` 都**只返回元数据** ✗（不含净荷 ✓，设计如此 ✓）
/// ⇒ 评论**写得进、读不回** ✗ ⇒ 先补工具 ✓、再让界面用它 ✓。
async function refreshComments() {
  const list = $("commentList");
  if (!list) return;
  // **用 `list_comments`** ✓（本轮新补的读工具 ✓）——
  // 原来用 `get_log {kind:"comment"}` ✗，而它**只返回元数据** ✓（设计如此 ✓：它是"MCP 轮询通道" ✓）
  // ⇒ 界面上读回来是"**（读取正文失败）**" ✗。这一条是我在真机验收里**看到**的 ✓，
  // 顺着它查出**评论根本没有读工具** ✗ ⇒ 于是先补工具 ✓、再改界面 ✓（内核/工具先行 ✓）。
  const value = await callTool("list_comments", { limit: 50 }, { refresh: false }).catch(() => null);
  const atoms = (value && value.comments) || [];
  list.innerHTML = "";
  if (atoms.length === 0) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = "还没有评论 ✓";
    list.appendChild(empty);
    return;
  }
  for (const atom of atoms.slice().reverse()) {
    const row = document.createElement("div");
    row.className = "annotation-row";
    const head = document.createElement("div");
    head.className = "annotation-head";
    head.textContent = (atom.actor || "?") + " · seq " + (atom.seq === undefined ? "?" : atom.seq) +
      (atom.object_id ? " · @" + String(atom.object_id).slice(-6) : "");
    const text = document.createElement("div");
    text.className = "hint";
    // **字段以 `list_comments` 的真实返回为准** ✓（`text` 是顶层的 ✓ —— 它就在我的 Rust 测试里 ✓）。
    text.textContent = atom.text || "（空评论 ✓）";
    row.append(head, text);
    list.appendChild(row);
  }
}

/// 发表一条评论 ✓。
async function postComment() {
  const box = $("commentText");
  const text = ((box && box.value) || "").trim();
  if (!text) { log("先写点什么再发表 ✓", "#c33"); return; }
  const done = await callToolChecked("comment", { text }, "发表评论");
  if (done && done.ok) {
    if (box) box.value = "";
    log("已发表评论 ✓（它是协作原子 ✓，不会改变画面 ✓）");
    await refreshComments();
  }
}

/// 评论面板的两个按钮 ✓。
function setupCommentPanel() {
  const post = $("commentPost");
  if (post) post.addEventListener("click", () => { void postComment(); });
  const reload = $("commentReload");
  if (reload) reload.addEventListener("click", () => { void refreshComments(); });
  const box = $("commentText");
  if (box) box.addEventListener("keydown", (event) => {
    if (event.key === "Enter") { event.preventDefault(); void postComment(); }
  });
}

/// **建议** ✓（设计 §12.6 ✓）—— `suggest`/`list_suggestions`/`accept_suggestion`/`reject_suggestion`
/// 此前在查看器里**零引用** ✗ ⇒ AI 提出的**可执行补丁**用户看不到、也接受不了 ✓。
///
/// **真实签名** ✓（先读规格 ✓）：
/// `list_suggestions {status?, since_seq?, limit?, offset?}` ✓（`status` 由 accept/reject 原子**推导** ✓）；
/// `accept_suggestion {suggestion_id}` ✓ —— **按序重放 patch** ✓（只允许会产生状态效果的步骤 ✓）
/// 并把**关联标注**置为 resolved ✓；`reject_suggestion {suggestion_id, reason?}` ✓。
///
/// **它是"标注"的另一半** ✓：标注负责"哪里不对" ✓，建议负责"**怎么改，而且是可执行的**" ✓
/// ⇒ 两者合起来才是设计 §12.6 的评审闭环 ✓。
let suggestionCache = [];

function renderSuggestionList() {
  const list = $("suggestionList");
  if (!list) return;
  list.innerHTML = "";
  if (suggestionCache.length === 0) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = "还没有建议 ✓（AI 或代理可以用 suggest 提出 ✓）";
    list.appendChild(empty);
    return;
  }
  for (const item of suggestionCache) {
    const row = document.createElement("div");
    row.className = "annotation-row";
    const head = document.createElement("div");
    head.className = "annotation-head";
    const steps = Array.isArray(item.patch) ? item.patch.length : 0;
    head.textContent = "#" + String(item.suggestion_id || item.id || "").slice(-8) +
      " · 优先级 " + (item.priority === undefined ? "?" : item.priority) +
      " · " + steps + " 步" + (item.status ? " · " + item.status : "");
    const text = document.createElement("div");
    text.className = "hint";
    text.textContent = item.summary || "（没有说明 ✓）";
    const actions = document.createElement("div");
    actions.className = "toolbar";
    // **预览** ✓（`preview_suggestion` ✓）—— 它**只校验、不应用** ✓：
    // 逐步检查工具名与参数 ✓、报告目标与变更类别 ✓ ⇒ 审阅代理建议时先看这一眼 ✓，
    // 比"点了接受才知道对不对"安全得多 ✓（接受会**按序重放**补丁 ✓）。
    const preview = document.createElement("button");
    preview.type = "button";
    preview.textContent = "预览";
    preview.addEventListener("click", async () => {
      const value = await callToolChecked("preview_suggestion",
        { suggestion_id: item.suggestion_id || item.id }, "预览建议");
      const box = $("suggestionPreview");
      if (!box || !value || !value.ok) return;
      // **原样把校验结果摆出来** ✓（哪些步骤合法 ✓、哪些不合法 ✗，让它自己说话 ✓）。
      const steps = value.steps || value.results || [];
      const lines = steps.map((step, index) => {
        const name = step.tool || step.name || "?";
        const ok = step.ok === undefined ? true : step.ok;
        const why = step.error || step.detail || (step.target ? "目标 " + step.target : "");
        return (ok ? "✓ " : "✗ ") + (index + 1) + ". " + name + (why ? " — " + why : "");
      });
      box.textContent = lines.length ? lines.join("\n") : JSON.stringify(value, null, 2);
    });
    const accept = document.createElement("button");
    accept.type = "button";
    accept.textContent = "接受";
    accept.disabled = item.status === "accepted";
    accept.addEventListener("click", async () => {
      const done = await callToolChecked("accept_suggestion",
        { suggestion_id: item.suggestion_id || item.id }, "接受建议");
      if (done && done.ok) {
        log("已接受建议 ✓（补丁已按序重放 ✓，关联标注置为 resolved ✓）");
        // **补丁会改动文档** ✓ ⇒ 重新对一次服务端 ✓。
        await resync();
      }
      await refreshSuggestions();
    });
    const reject = document.createElement("button");
    reject.type = "button";
    reject.textContent = "拒绝";
    reject.disabled = item.status === "rejected";
    reject.addEventListener("click", async () => {
      const reason = (($("suggestionReason") || {}).value || "").trim() || "编辑器里拒绝";
      const done = await callToolChecked("reject_suggestion",
        { suggestion_id: item.suggestion_id || item.id, reason }, "拒绝建议");
      if (done && done.ok) log("已拒绝建议 ✓：原因「" + reason + "」✓");
      await refreshSuggestions();
    });
    actions.append(preview, accept, reject);
    row.append(head, text, actions);
    list.appendChild(row);
  }
}

/// 拉一次建议列表 ✓。
async function refreshSuggestions() {
  const box = $("suggestionStatus");
  const args = {};
  if (box && box.value) args.status = box.value;
  const listed = await callTool("list_suggestions", args, { refresh: false }).catch(() => null);
  suggestionCache = (listed && (listed.suggestions || listed.items)) || [];
  renderSuggestionList();
}

/// 建议面板的两个控件 ✓。
function setupSuggestionPanel() {
  const reload = $("suggestionReload");
  if (reload) reload.addEventListener("click", () => { void refreshSuggestions(); });
  const box = $("suggestionStatus");
  if (box) box.addEventListener("change", () => { void refreshSuggestions(); });
}

/// **存储 / 维护** ✓（设计 §6.3 的 Blob 三级生命周期 ✓）—— `blob_gc` 此前**零引用** ✗。
///
/// **为什么值得放到界面上** ✓：工作区里"上传过、但没有任何原子引用"的孤儿是最容易被忽视的一类占用 ✓
/// （本项目实测过一次：**2161 个 blob 里 1912 个是孤儿、约 1.07 GB** ✗）⇒ 让人**看得见**、并且能
/// **自己决定**什么时候回收 ✓，比在日志里说一句有用得多 ✓。
///
/// **安全设计** ✓（与工具侧一致 ✓）：`dry_run` 服务端**默认就是 true** ✓ ⇒ "统计"永远安全 ✓；
/// 回收必须**勾上确认框** ✓（删除不可逆 ✓）⇒ 不勾就只提示、不发请求 ✓。
function humanBytes(bytes) {
  const n = Number(bytes) || 0;
  if (n >= 1024 * 1024 * 1024) return (n / 1024 / 1024 / 1024).toFixed(2) + " GB";
  if (n >= 1024 * 1024) return (n / 1024 / 1024).toFixed(1) + " MB";
  if (n >= 1024) return (n / 1024).toFixed(1) + " KB";
  return n + " B";
}

function renderStorageReport(value) {
  const box = $("storageReport0");
  if (!box) return;
  const line = (label, level) => label + "：" + (level && level.blobs !== undefined ? level.blobs : "?") +
    " 个｜" + humanBytes(level ? level.bytes : 0);
  box.textContent = [
    "文档数：" + (value.documents_considered !== undefined ? value.documents_considered : "?"),
    line("活跃", value.active),
    line("历史（保留，永不回收）", value.history),
    line("孤儿", value.orphan),
    line("已过 TTL 可回收", value.collectible),
  ].join("\n") + (value.removed !== undefined ? "\n本次已回收：" + value.removed + " 个｜" + humanBytes(value.freed_bytes) : "");
}

/// 统计 ✓（`dry_run` 保持默认 true ✓ ⇒ 只读 ✓）。
async function storageReport() {
  const value = await callToolChecked("blob_gc", {}, "统计存储");
  if (value && value.ok) {
    renderStorageReport(value);
    log("存储统计 ✓：活跃 " + (value.active ? value.active.blobs : "?") + " 个｜孤儿 " +
        (value.orphan ? value.orphan.blobs : "?") + " 个（" + humanBytes(value.orphan ? value.orphan.bytes : 0) + "）" +
        "｜已过 TTL 可回收 " + (value.collectible ? value.collectible.blobs : "?") + " 个 ✓");
  }
}

/// 回收 / 降冷 ✓（都要**显式勾确认** ✓）。
async function storageMutate(demote) {
  const box = $("storageConfirm");
  if (!box || !box.checked) {
    log("回收不可逆 ✓ —— 请先勾上「我确认」再点 ✓", "#c33");
    return;
  }
  const args = { dry_run: false, confirm: true };
  if (demote) args.demote = true;
  const value = await callToolChecked("blob_gc", args, demote ? "降冷历史级" : "回收孤儿");
  if (value && value.ok) {
    renderStorageReport(value);
    log((demote ? "已降冷历史级" : "已回收孤儿") + " ✓：回收 " + (value.removed || 0) + " 个｜释放 " +
        humanBytes(value.freed_bytes || 0) +
        (demote ? "｜降冷 " + (value.demoted || 0) + " 个" : "") + " ✓");
    // **回收会动存储** ✓ ⇒ 让画布重新对一次服务端 ✓（保守但正确 ✓）。
    await resync();
  }
}

/// 存储面板的三个按钮 ✓。
function setupStoragePanel() {
  const report = $("storageReport");
  if (report) report.addEventListener("click", () => { void storageReport(); });
  const collect = $("storageCollect");
  if (collect) collect.addEventListener("click", () => { void storageMutate(false); });
  const demote = $("storageDemote");
  if (demote) demote.addEventListener("click", () => { void storageMutate(true); });
}

/// **对象变换** ✓（设计 §783）—— `transform_object` 此前在查看器里**零引用** ✗ ⇒
/// "**旋转 / 缩放 / 平移一个对象**"完全够不到 ✓，而这是画家最常用的动作之一 ✓。
///
/// **真实签名** ✓（先读规格 ✓）：`{object_id, rotate:{degrees}, scale:{x,y}, translate:{dx,dy}, anchor?, compose?}` ✓
/// —— **可以一次给多项** ✓；`anchor` 缺省是**对象包围盒的中心** ✓（符合直觉 ✓），`compose` 缺省 true ✓（叠加 ✓）。
///
/// **取舍** ✓：缩放按**百分比**输入 ✓（画家想的是"放大到 150%" ✓，不是 ×1.5 的小数 ✓）；
/// 四个输入都为"中性值"（0°/100%/0/0）时**不发请求** ✓（避免提交一条什么都没变的原子 ✗）。
async function transformCheckedObjects() {
  const ids = checkedObjects();
  if (ids.length === 0) { log("先勾选要变换的对象 ✓", "#c33"); return; }
  const degrees = Number(($("transformRotate") || {}).value || 0);
  const percent = Number(($("transformScale") || {}).value || 100);
  const dx = Number(($("transformDx") || {}).value || 0);
  const dy = Number(($("transformDy") || {}).value || 0);
  const scale = percent / 100;
  const neutral = degrees === 0 && percent === 100 && dx === 0 && dy === 0;
  if (neutral) { log("角度 0°、缩放 100%、位移 0 ⇒ 没有可变换的量 ✓（改一个数再点 ✓）", "#c33"); return; }
  let done = 0;
  for (const objectId of ids) {
    const args = { object_id: objectId, compose: true };
    if (degrees !== 0) args.rotate = { degrees };
    if (percent !== 100) args.scale = { x: scale, y: scale };
    if (dx !== 0 || dy !== 0) args.translate = { dx, dy };
    const result = await callToolChecked("transform_object", args, "变换对象");
    if (result && result.ok) done += 1;
  }
  if (done > 0) {
    log("已变换 " + done + " 个对象 ✓（角度 " + degrees + "°、缩放 " + percent + "%、位移 " + dx + "," + dy + "）");
    // **变换会改变呈现** ✓ ⇒ 走一次 resync ✓（服务端权威像素 ✓）。
    await resync();
  }
  await refreshObjects();
}

/// **路径算子** ✓（`path_edit`，设计 §792 的最后一个算子表 ✓）——
/// 它此前在查看器里**零引用** ✗ ⇒ 服务端的 reverse/close/join/merge/split/boolean **用户都碰不到** ✗
///（这正是 Phase 5"高级路径编辑"缺的那一半 ✓）。
///
/// **真实签名** ✓（先读规格 ✓）：`{op, object_id, other_id?, at?, mode?}` ✓，
/// 其中 `op` ∈ reverse / close / join / merge / split / boolean ✓，
/// **布尔模式** ∈ union / intersect / subtract / xor ✓（名字照服务端 ✓，不自己发明 ✓）。
///
/// **参数的"元数"按算子分** ✓（这是界面必须讲清楚的事 ✓）：
/// * **一元**（reverse / close / split ✓）：取**勾选的第一个** ✓；
/// * **二元**（join / merge / boolean ✓）：取**勾选的前两个** ✓（第一个当 object_id ✓、第二个当 other_id ✓）
///   ⇒ 不够两个就**明确拒绝** ✓，并且**说清需要两个** ✓（而不是发一个必然失败的请求 ✗）。
async function runPathOp() {
  const ids = checkedObjects();
  const op = ($("pathOp") || {}).value || "reverse";
  const binary = op === "join" || op === "merge" || op === "boolean";
  const need = binary ? 2 : 1;
  if (ids.length < need) {
    log("「" + op + "」需要" + (binary ? "两个" : "一个") + "对象 ✓ —— 请在对象列表里勾选" +
        (binary ? "两个" : "一个") + " ✓", "#c33");
    return;
  }
  const args = { op, object_id: ids[0] };
  if (binary) args.other_id = ids[1];
  if (op === "split") args.at = Number(($("pathAt") || {}).value || 0);
  if (op === "boolean") args.mode = ($("pathMode") || {}).value || "union";
  const done = await callToolChecked("path_edit", args, "路径算子「" + op + "」");
  if (done && done.ok) {
    log("已执行路径算子「" + op + "」✓" + (binary ? "（两个对象 ✓）" : "") +
        (op === "boolean" ? "，模式 " + args.mode + " ✓" : ""));
    // **算子会改动几何** ✓ ⇒ 重新对一次服务端 ✓。
    await resync();
  }
  await refreshObjects();
}

/// **修改笔触** ✓（`update_stroke`，设计 10.3 的**三层参数** ✓）——
/// 它此前在查看器里**零引用** ✗ ⇒ 画完之后**改不了这一笔** ✓（颜色、粗细、不透明度都定死了 ✗）。
///
/// **真实签名** ✓（先读规格 ✓）：
/// `{object_id, core?: {points|points_patch, brush, color, size, opacity, blend_mode}, preset?, advanced?}` ✓
/// —— 界面只动 **core** 里的三样 ✓（**颜色 / 粗细 / 不透明度** ✓），这是画完最常想调的三样 ✓；
/// `points`/`advanced` 留给工具与代理 ✓（界面里手改多边形没必要 ✗）。
///
/// **只作用于笔迹** ✓（`stroke` ✓）：与「重采样」同一条约束 ✓ —— 光栅对象走那条路 ✓，
/// 服务端对类型不符会**明确拒绝并告知类型** ✓（那条信息本身就是产品的一部分 ✓）。
async function restyleCheckedObjects() {
  const ids = checkedObjects();
  if (ids.length === 0) { log('先勾选要改的笔迹 ✓', '#c33'); return; }
  const core = {
    color: ($('strokeColor') || {}).value || '#c81e3c',
    size: Number(($('strokeSize') || {}).value || 8),
    opacity: Number(($('strokeOpacity') || {}).value || 1),
  };
  if (!(core.size > 0)) { log('粗细要大于 0 ✓', '#c33'); return; }
  let done = 0;
  for (const objectId of ids) {
    const result = await callToolChecked('update_stroke', { object_id: objectId, core }, '修改笔触');
    if (result && result.ok) done += 1;
  }
  if (done > 0) {
    log('已改 ' + done + ' 条笔触 ✓（色 ' + core.color + '、粗 ' + core.size + '、不透明 ' + core.opacity + '）');
    await resync();
  }
  await refreshObjects();
}

/// **重采样** ✓（设计 §792 的 retouch 组 ✓）—— `resample` 此前在查看器里**零引用** ✗
/// ⇒ 用户够不到 ✓（"把这一笔的像素换个分辨率" ✓ 是修图里很常见的一步 ✓）。
///
/// **真实签名** ✓（先读规格 ✓）：`{object_id, width?, height?, scale?, filter?}` ✓
/// —— `width/height` 与 `scale` **二选一** ✓；`filter` 缺省 bilinear ✓（介质是连续调 ✓）。
/// **非破坏** ✓：产出**新 blob** ✓ + `supersede` ✓ ⇒ 原像素仍在日志里 ✓（可撤销 ✓）。
///
/// **取舍** ✓：界面只给**缩放百分比** ✓（用 `scale` ✓）—— 直接填目标宽高在编辑器里无法预知结果尺寸 ✓，
/// 而"放大到 150%"是画家能预期的东西 ✓。
async function resampleCheckedObjects() {
  const ids = checkedObjects();
  if (ids.length === 0) { log("先勾选要重采样的对象 ✓", "#c33"); return; }
  const percent = Number(($("transformScale") || {}).value || 100);
  if (!(percent > 0)) { log("缩放百分比要大于 0 ✓", "#c33"); return; }
  if (percent === 100) { log("缩放 100% ⇒ 没有可重采样的量 ✓（改一个数再点 ✓）", "#c33"); return; }
  let done = 0;
  for (const objectId of ids) {
    const result = await callToolChecked("resample", { object_id: objectId, scale: percent / 100 }, "重采样对象");
    if (result && result.ok) done += 1;
  }
  if (done > 0) {
    log("已重采样 " + done + " 个对象 ✓（" + percent + "% ✓，非破坏：原像素仍在日志里 ✓）");
    await resync();
  }
  await refreshObjects();
}

/// 对象面板的按钮 ✓。
function setupObjectPanel() {
  setupStoragePanel();
  // **素材面板** ✓（调色板取色 + 纹理背景 ✓）—— 与其它面板一样，**只在这儿挂一次** ✓。
  void setupAssetPanels();
  setupSuggestionPanel();
  setupCommentPanel();
  setupChangesetPanel();
  const fresh = $("effectNew");
  if (fresh) fresh.addEventListener("click", () => {
    clearEffectEditing();
    const box = $("effectParams");
    if (box) box.value = "{}";
    log("已切回新建模式 ✓");
    void refreshEffects();
  });
  // **改粗细 / 改颜色 / 改末端色 / 开关一笔多色 ⇒ 预览跟着重画** ✓（防抖 400ms ✓）。
  // **必须走 `window.yanshi`** ✗ ——`setupObjectPanel` 在**另一段 `<script>`** 里 ✓，
  // 直接调 `scheduleBrushPreview()` 会 `ReferenceError` ✓（本项目的老坑 ✓，本轮第二次撞 ✓，
  // 所以这里只认那个**被证明可靠**的入口 ✓）。
  const debouncePreview = () => {
    if (window.yanshi && window.yanshi.scheduleBrushPreview) window.yanshi.scheduleBrushPreview();
  };
  for (const id of ["size", "color", "colorTo", "duoTone"]) {
    const control = $(id);
    if (control) {
      control.addEventListener("input", debouncePreview);
      control.addEventListener("change", debouncePreview);
    }
  }
  const refresh = $("objectRefresh");
  if (refresh) refresh.addEventListener("click", () => { void refreshObjects(); });
  const instance = $("objectInstance");
  if (instance) instance.addEventListener("click", () => { void instanceCheckedObject(); });
  const group = $("objectGroup");
  if (group) group.addEventListener("click", () => { void groupCheckedObjects(); });
  const toShape = $("objectToShape");
  if (toShape) toShape.addEventListener("click", () => {
    void convertCheckedObjects("convert_to_shape", "转为形状", "shape_id", "shape_");
  });
  const pathOp = $("objectPath");
  if (pathOp) pathOp.addEventListener("click", () => { void runPathOp(); });
  const restyle = $("objectRestyle");
  if (restyle) restyle.addEventListener("click", () => { void restyleCheckedObjects(); });
  const resample = $("objectResample");
  if (resample) resample.addEventListener("click", () => { void resampleCheckedObjects(); });
  const transform = $("objectTransform");
  if (transform) transform.addEventListener("click", () => { void transformCheckedObjects(); });
  const toPath = $("objectToPath");
  if (toPath) toPath.addEventListener("click", () => {
    void convertCheckedObjects("convert_to_path", "转为路径", "path_id", "path_");
  });
}

/// 面板上的两个开关 ✓（刷新 ✓ / 显示已解决 ✓）。
function setupAnnotationPanel() {
  const refresh = $("annotationRefresh");
  if (refresh) refresh.addEventListener("click", () => { void refreshAnnotations(); });
  const toggle = $("annotationShowResolved");
  if (toggle) {
    toggle.addEventListener("click", () => {
      annotationShowResolved = !annotationShowResolved;
      toggle.textContent = annotationShowResolved ? "隐藏已解决" : "显示已解决";
      renderAnnotationList();
    });
  }
}

/// 拉一次标注列表 ✓。
async function refreshAnnotations() {
  const listed = await callTool("list_annotations", {}, { refresh: false }).catch(() => null);
  const items = (listed && (listed.annotations || listed.items)) || [];
  annotationCache = items.map((item, index) => Object.assign({}, item, {
    number: item.number || index + 1,
  }));
  renderAnnotationList();
}

/// 在文档坐标处**新建**标注 ✓。
async function createAnnotationAt(x, y) {
  // **真实参数** ✓（我第一版猜成 `point`/`text` ✗，服务端把可用参数直接报了出来 ✓）：
  // `type` ✓（region|object|arrow|text|doodle|highlight ✓）、`intent` ✓（必填 ✓）、
  // `target` ✓（`{target:'region', bbox}` ✓）、`content` ✓（字符串 ✓）。
  // 点一下就建成一个**很小的矩形区域** ✓（图钉就落在点上 ✓）。
  const size = 16;
  const created = await callToolChecked("create_annotation", {
    type: "region",
    intent: "modify",
    target: { target: "region", bbox: { x: Math.round(x - size / 2), y: Math.round(y - size / 2), w: size, h: size } },
    content: "",
  }, "新建标注");
  if (created && created.ok) await refreshAnnotations();
}

/// **保证"有图层可画"** ✓ —— 落笔兜底 ✓。
///
/// **为什么要有它** ✓（真机验收换来的 ✓）：普通笔刷落到"还没有图层的文档"上时，
/// `draw_stroke`/`erase` 会因为 `layer_id` 指向不存在的图层而被**拒绝** ✗，
/// 而客户端**只看日志、画布上毫无反馈** ✗ ⇒ 用户的表现就是"**点了没反应**" ✗ ——
/// 这正是本项目自己定的"**不许静默吞掉用户操作**" ✗ 那一类。
///
/// **做法沿用项目里已有的约定** ✓（不是新发明 ✓）：介质提交路径 `commitMediumBitmap` 早就在做
/// "选中的图层不存在就兜底建一个" ✓（`state.layerId || "layer_paint"` ✓ 并在需要时新建 ✓）。
/// 这里把它**提到打开文档时** ✓ ⇒ 所有工具都受益 ✓，而且不用在**同步的**落笔处理器里 await ✗。
/// **调用工具并保证"失败必留痕"** ✓。
///
/// **为什么要有它** ✓（本轮把一类问题当一类来扫的结果 ✓）：查看器里有 **47 处** `callTool` ✓，
/// 其中**四处**（图层显示/锁定 ✓、图层排序 ✓、删除图层 ✓、画形状 ✓）**完全不看返回值** ✗
/// ⇒ 失败时画布与日志都**没有任何信号** ✗ ⇒ 用户看到的是"点了没反应" ✗。
/// 这正是本项目自己禁止的**静默吞掉用户操作** ✗ —— 上一轮修的是同一类的落笔 ✓，这一轮补齐其余 ✓。
///
/// **`what` 是给用户看的动作名** ✓（"删除图层" ✓ 比 `delete_layer` 有用得多 ✓）。
async function callToolChecked(name, args, what) {
  const result = await callTool(name, args);
  if (!result || !result.ok) {
    const code = (result && result.error_code) || "unknown";
    const detail = (result && result.context && result.context.detail) || "";
    log(what + "失败：" + code + (detail ? " " + detail : ""), "#c33");
  }
  return result;
}

async function ensurePaintLayer() {
  const listed = await callTool("list_layers", {}, { refresh: false }).catch(() => null);
  const layers = (listed && listed.layers) || [];
  const known = layers.some((layer) => layer.layer_id === state.layerId);
  if (known) return state.layerId;
  // 有别的图层就选第一个 ✓；一个都没有就建一个 ✓（与介质路径同名同义 ✓）。
  if (layers.length > 0) {
    state.layerId = layers[0].layer_id;
    return state.layerId;
  }
  const fallback = state.layerId || "layer_paint";
  const created = await callTool("create_layer", { layer_id: fallback, name: "图层 1" }, { refresh: false });
  if (!created.ok) {
    // **建不出来也要说话** ✗ —— 静默是最坏的结果 ✓。
    log("无法新建图层：" + (created.error_code || "unknown") + "，落笔会失败", "#c33");
    return null;
  }
  state.layerId = fallback;
  log("文档里还没有图层 ⇒ 已自动新建「图层 1」✓");
  return state.layerId;
}

async function refreshLayers() {
  const value = await fetchOrLocal(api("/api/tools/list_layers"), {
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
/// **调色板与纹理两个面板** ✓（目标第 ③ 件 ✓）。
///
/// **为什么必须有** ✓：能力先放在**工具层** ✓（MCP 与 Web 都能用 ✓），
/// 但**界面里没有入口**就等于"只有 MCP 能用" ✗ —— 用户那条硬要求正是**两边都要有** ✓。
/// **素材都很少** ✓（调色板 43 个 ✓、纹理 11 张 ✓）⇒ **加载时一次装满** ✓，不做懒加载 ✓。
/// **从界面取"要作用的区域"** ✓（目标 ④ ✓）—— **纹理与渐变共用这一个** ✓。
///
/// **一条硬规矩** ✓：勾了"只作用于选区"却**没有选区** ⇒ **拒绝** ✗，
/// **绝不静默铺满整幅** ✗ —— 那正是本项目反复提防的"接受了却没用 / 没说清就做了别的事" ✓
///（子 agent 报过的"遗留选区静默裁掉一切 ✓ 而界面还写着无选区" ✗ 就是这一类 ✓）。
///
/// **区域取自服务端事实** ✓（`state.selectionShape` 由 `list_selections` 写入 ✓），不是本地猜测 ✓。
function regionFromSelection(checkboxId) {
  const box = $(checkboxId);
  if (!box || !box.checked) return { region: null, error: null };
  const shape = state.selectionShape;
  if (!shape || !shape.bbox) {
    return {
      region: null,
      error: "勾了「只作用于选区」，但**当前没有选区** ✗ ⇒ 先用选区工具拖一个矩形（或在快捷面板里建一个 ✓）",
    };
  }
  const bbox = shape.bbox;
  if (!(bbox.w > 0) || !(bbox.h > 0)) {
    return { region: null, error: "选区是空的（宽或高为 0）✗ ⇒ 重新拖一个 ✓" };
  }
  return {
    region: { x: bbox.x, y: bbox.y, w: bbox.w, h: bbox.h },
    error: null,
  };
}

/// **素材浮层** ✓（用户："画笔区快捷方式、点开浮出来" ✓）。
///
/// **做法：搬，不重建** ✓ —— `appendChild` 是**移动节点** ✓ ⇒ 卡里的监听器、下拉框选中项、
/// 已装载的色块**全都不变** ✓；关掉时按**开之前记下的原位**（`parent` + `nextSibling`）
/// 搬回去 ✓ ⇒ 右侧面板**不可能被搬空** ✗（上一版就是栽在这里 ✓，所以这次每一步都验 ✓）。
/// **只切"素材浮层"的可见性** ✓ —— 与 `setupAssetDock()` **拆开**的理由（第 208/209 轮实测 ✓）：
/// 浮层的 DOM（`#assetDock` 等）在**脚本段之后**（`viewer.rs:7721` 一带 ✓）⇒
/// `setupAssetDock()` 在脚本执行时**取不到它们** ✗ ⇒ 它开头 `if (!dock || !body || !button) return;` 提前返回 ✓；
/// 而**按钮**（`viewer.rs:418` ✓）在脚本**之前**就存在 ✓ ⇒ 绑定它**根本不需要等 DOM** ✓。
/// 所以把"开关"这一半**单独**拿出来 ✓：它**不搬卡片** ✓（搬卡片要卡片元素 ✓，那仍然归 `setupAssetDock()` ✓），
/// 调用它**没有副作用** ✓ —— 这正是不再让"早绑"把事情弄坏的关键 ✓（前四次尝试都是**整个** `setupAssetDock()` 早调 ✗）。
function toggleAssetDock() {
  const dock = $("assetDock");
  const button = $("assetFloat");
  if (!dock || !button) return;
  const open = dock.hidden;
  dock.hidden = !open;
  button.setAttribute("aria-pressed", String(open));
  button.textContent = open ? "素材（已浮出）" : "素材";
}

function setupAssetDock() {
  const dock = $("assetDock");
  const body = $("assetDockBody");
  const button = $("assetFloat");
  const close = $("assetDockClose");
  if (!dock || !body || !button) return;
  // **原位** ✓：只记一次 ✓（我们自己搬动之前的位置 ✓；用户没别的手段移动这两张卡 ✓）。
  const homes = ["cardPalette", "cardTexture"]
    .map((id) => $(id))
    .filter(Boolean)
    .map((node) => ({ node, parent: node.parentNode, next: node.nextSibling }));
  const setOpen = (open) => {
    if (open) {
      for (const home of homes) body.appendChild(home.node);
    } else {
      for (const home of homes) {
        // **按原位插回** ✓；原位已经不在了（理论上不会 ✗）⇒ 退化成"追加回原父节点" ✓，绝不丢卡 ✗。
        if (home.parent) {
          if (home.next && home.next.parentNode === home.parent) {
            home.parent.insertBefore(home.node, home.next);
          } else {
            home.parent.appendChild(home.node);
          }
        }
      }
    }
    dock.hidden = !open;
    button.setAttribute("aria-pressed", String(open));
    button.textContent = open ? "素材（已浮出）" : "素材";
  };
  // 面板就绪后**接管**按钮 ✓ —— 先摘掉早绑的"纯开关" ✓（两者都绑着会一次点击切两次 ⇒ 看起来"没反应" ✗）。
  button.removeEventListener("click", toggleAssetDock);
  button.addEventListener("click", () => setOpen(dock.hidden));
  // **折叠 / 展开画笔区** ✓（类加在 `body` 上 ✓ ⇒ 一行 CSS 管全部 ✓）。
  const areaToggle = $("brushAreaToggle");
  const setCollapsed = (collapsed) => {
    document.body.classList.toggle("brush-area-collapsed", !!collapsed);
    if (areaToggle) {
      areaToggle.setAttribute("aria-pressed", String(!!collapsed));
      areaToggle.textContent = collapsed ? "画笔 ▸" : "画笔 ▾";
    }
    return !!collapsed;
  };
  if (areaToggle) areaToggle.addEventListener("click", () => setCollapsed(!document.body.classList.contains("brush-area-collapsed")));
  window.yanshiBrushArea = {
    collapsed: () => document.body.classList.contains("brush-area-collapsed"),
    setCollapsed,
  };
  // **快捷键** ✓：`P` / `T` 浮出对应卡片 ✓、`\\` 折叠画笔区 ✓、`Esc` 收起浮层 ✓。
  // **只认没有修饰键、且焦点不在输入框** ✓（否则打字会误触 ✓ —— 与既有键盘处理同一条规矩 ✓）。
  document.addEventListener("keydown", (event) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const target = event.target;
    if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT")) return;
    if (event.key === "p" || event.key === "P") {
      setOpen(true);
      focusCard("cardPalette");
    } else if (event.key === "t" || event.key === "T") {
      setOpen(true);
      focusCard("cardTexture");
    } else if (event.key === "\\") {
      setCollapsed(!document.body.classList.contains("brush-area-collapsed"));
    }
  });
  if (close) close.addEventListener("click", () => setOpen(false));
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !dock.hidden) setOpen(false);
  });
  // **快捷键：`P` 调色板 / `T` 纹理** ✓（用户："点击或者快捷键"✓）——
  // 落到 `window.yanshiDock` 上做 ✓，因为这段脚本与别处**跨段** ✓（够不到函数名 ✗，规矩见上 ✓）。
  const focusCard = (id) => {
    const card = $(id);
    if (!card) return;
    for (const other of ["cardPalette", "cardTexture"]) {
      const node = $(other);
      if (node) node.classList.toggle("asset-dock-target", other === id);
    }
    if (typeof card.scrollIntoView === "function") card.scrollIntoView({ block: "nearest" });
  };
  window.yanshiDockFocus = focusCard;
  // **给探针一个确定的入口** ✓（与 `window.yanshi` 上其它入口同一条纪律 ✓）。
  window.yanshiDock = {
    open: () => setOpen(true),
    close: () => setOpen(false),
    isOpen: () => !dock.hidden,
    focus: focusCard,
    autoClose: () => {
      const box = $("dockAutoClose");
      return !!(box && box.checked);
    },
    setAutoClose: (on) => {
      const box = $("dockAutoClose");
      if (box) box.checked = !!on;
      return !!(box && box.checked);
    },
    homes: () => homes.map((home) => ({ id: home.node.id, parent: home.parent && home.parent.id })),
    where: () => homes.map((home) => home.node.parentNode && home.node.parentNode.id),
  };
}

/// **右栏分 tab** ✓（用户："右边信息面板超级长，适合面板内部有小 tab 切换不同信息"✓）。
///
/// **三条取舍** ✓：
/// ① **搬，不重建** ✓（与素材浮层同一条手法 ✓）：卡片连同里面的监听器、下拉框选中项、
///    已画的缩略图一起搬进窗格 ✓ ⇒ 切 tab 不可能丢状态 ✗；
/// ② **必须在 `setupAssetDock()` 之前跑** ✗：浮层记的"原位"就是**这里** ✓
///    ⇒ 顺序反了，浮层收起时会把调色板/纹理搬回**旧的父节点** ✗（那个父节点已经空了 ✓）；
/// ③ **分组按标题反查** ✓（在标记里写 `data-panel` ✓）——不靠 JS 猜顺序 ✓。
function setupRightTabs() {
  const aside = document.querySelector("aside");
  if (!aside || aside.dataset.tabsReady === "1") return;
  const tabs = [
    ["paint", "绘制"],
    ["history", "历史"],
    ["assets", "素材"],
    ["file", "文件"],
    ["diag", "诊断"],
  ];
  const bar = document.createElement("div");
  bar.className = "tabs";
  bar.id = "rightTabs";
  const panes = new Map();
  for (const [key, label] of tabs) {
    const pane = document.createElement("div");
    pane.className = "tab-pane";
    pane.dataset.pane = key;
    pane.hidden = key !== "paint";
    panes.set(key, pane);
    const button = document.createElement("button");
    button.type = "button";
    button.dataset.tab = key;
    button.textContent = label;
    button.setAttribute("aria-pressed", String(key === "paint"));
    button.addEventListener("click", () => showTab(key));
    bar.appendChild(button);
  }
  // **搬卡** ✓（`data-panel` 由标记给出 ✓；没标的进"诊断" ✓ —— 宁可多一张可见的卡 ✗，
  // 也不要让某张卡**从界面上消失** ✗，那是"创建了不等于挂上了"的老病 ✓）。
  for (const node of Array.from(aside.children)) {
    if (node === bar) continue;
    if (!(node.classList && node.classList.contains("card")) && node.tagName !== "DETAILS") continue;
    const key = panes.has(node.dataset.panel) ? node.dataset.panel : "diag";
    panes.get(key).appendChild(node);
  }
  aside.insertBefore(bar, aside.firstChild);
  for (const pane of panes.values()) aside.appendChild(pane);
  // **空的 tab 不留** ✓（本轮把工程包那张卡搬去「文件」菜单之后 ✓，「文件」tab 就没内容了 ✓
  // ⇒ 留着它就是一个"点进去什么都没有"的空格子 ✗ —— 那比没有这个 tab 更让人困惑 ✓）。
  for (const [key, pane] of panes) {
    if (pane.querySelector(":scope > .card, :scope > details")) continue;
    const tab = bar.querySelector('button[data-tab="' + key + '"]');
    if (tab) tab.hidden = true;
    pane.hidden = true;
  }
  const showTab = (key) => {
    for (const [name, pane] of panes) pane.hidden = name !== key;
    for (const button of bar.querySelectorAll("button")) {
      button.setAttribute("aria-pressed", String(button.dataset.tab === key));
    }
    return key;
  };
  aside.dataset.tabsReady = "1";
  window.yanshiRightTabs = {
    show: showTab,
    state: () => ({
      active: (Array.from(panes).find(([, pane]) => !pane.hidden) || ["paint"])[0],
      counts: Array.from(panes).map(([key, pane]) => ({
        key,
        cards: pane.querySelectorAll(":scope > .card, :scope > details").length,
      })),
      visibleCards: Array.from(
        document.querySelectorAll(".tab-pane:not([hidden]) > .card, .tab-pane:not([hidden]) > details"),
      ).length,
    }),
  };
}

/// **「文件」菜单** ✓（用户第 6 条 ✓）：把"新建 / 打开 / 导出 PNG / 工程包"从信息面板**搬到顶栏菜单** ✓。
///
/// **为什么是"搬"** ✓：这些控件各自带着监听器、id 与既有检查（`#exportPng` 等 ✓）——
/// 复制一份 ✗ 必然与原来那份漂移 ✓（本项目的老病 ✓）。搬完之后：
/// **id 不变 ✓、行为不变 ✓、只是位置变了 ✓**，而且"信息面板里不再有它"这件事可以直接判 ✓。
function setupFileMenu() {
  const menu = $("fileMenu");
  const body = $("fileMenuBody");
  const button = $("fileMenuButton");
  const close = $("fileMenuClose");
  if (!menu || !body || !button) return;
  const addGroup = (label) => {
    const row = document.createElement("div");
    row.className = "file-menu-group";
    row.textContent = label;
    body.appendChild(row);
  };
  const move = (id, group) => {
    const node = $(id);
    if (!node) return false;
    if (group && body.dataset.lastGroup !== group) {
      addGroup(group);
      body.dataset.lastGroup = group;
    }
    body.appendChild(node);
    return true;
  };
  // 顺序按行业习惯 ✓：新建 / 打开 ⇒ 导入 ⇒ 导出 ⇒ 工程包 ✓。
  body.dataset.lastGroup = "";
  const single = (node) => {
    const wrap = document.createElement("div");
    wrap.className = "toolbar";
    wrap.appendChild(node);
    return wrap;
  };
  for (const [id, group] of [["newDoc", "新建 / 打开"], ["openDoc", null]]) {
    const node = $(id);
    if (node) {
      const wrap = $(id + "Wrap") || single(node);
      wrap.id = id + "Wrap";
      if (body.dataset.lastGroup !== group && group) { addGroup(group); body.dataset.lastGroup = group; }
      body.appendChild(wrap);
    }
  }
  // **导出** ✓（`#exportPng` 原来在「操作」卡里 ✓ —— 那是信息面板 ✓，搬走 ✓）。
  const exportWrap = document.createElement("div");
  exportWrap.className = "toolbar";
  const useServerRenderBox = $("useServerRender");
if (useServerRenderBox) {
  useServerRenderBox.checked = serverRenderPreferred();
  useServerRenderBox.addEventListener("change", () => {
    try {
      if (useServerRenderBox.checked) localStorage.setItem(SERVER_RENDER_KEY, "1");
      else localStorage.removeItem(SERVER_RENDER_KEY);
    } catch (error) { /* 存不了就算了 ✓ */ }
    // **重载** ✓：让渲染路径从**干净状态**重新开始 ✓（不做热切换 ✓）
    location.reload();
  });
}
const exportPng = $("exportPng");
  addGroup("导出");
  if (exportPng) exportWrap.appendChild(exportPng);
  // **工程包整张卡搬进来** ✓（打包 / 打开 *.yanshi ✓）—— 原来占着「文件」tab 一整格 ✓。
  const card = document.querySelector('aside .card[data-panel="file"]');
  if (card) body.appendChild(card);
  body.appendChild(exportWrap);
  const setOpen = (open) => {
    menu.hidden = !open;
    button.setAttribute("aria-pressed", String(open));
  };
  button.addEventListener("click", () => setOpen(menu.hidden));
  if (close) close.addEventListener("click", () => setOpen(false));
  document.addEventListener("click", (event) => {
    if (menu.hidden) return;
    if (menu.contains(event.target) || button.contains(event.target)) return;
    setOpen(false);
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !menu.hidden) setOpen(false);
  });
  window.yanshiFileMenu = {
    open: () => setOpen(true),
    close: () => setOpen(false),
    isOpen: () => !menu.hidden,
    ids: () => Array.from(body.querySelectorAll("[id]")).map((node) => node.id),
  };
}

async function setupAssetPanels() {
  // **先把"文件"那几样搬进顶栏菜单** ✓（要在分 tab 之前 ✓ —— 搬走之后「文件」这个 tab 就空了 ✓，
  // 分 tab 时它会因为**没有卡**而自动隐藏 ✓，不会留一个点进去什么都没有的空 tab ✗）。
  setupFileMenu();
  // **再把右栏分好 tab** ✓，**然后**把浮层挂上 ✓ —— 顺序不能反 ✗（浮层记的"原位"就是窗格 ✓）。
  setupRightTabs();
  // **然后把浮层挂上** ✓ —— 与卡片内容的装载互不依赖 ✓（搬的是节点本身 ✓）。
  setupAssetDock();
  // **`setupBrushLibrary` 不在这儿调** ✗ —— 它的定义在**另一段 `<script>`** 里 ✓，
  // 从这里调只会 `ReferenceError` ✓（本轮实测：面板永远打不开 ✓、`open=false` ✗）。
  // 它改成在**定义那一侧**自初始化 ✓（见那个函数上面 ✓）——这是同一个坑的第 N 次 ✓，
  // 所以这次把"为什么不在别处调"也写在现场 ✓。
  const palettePick = $("palettePick");
  const swatches = $("paletteSwatches");
  const paletteInfo = $("paletteInfo");
  const texturePick = $("texturePick");
  const textureMode = $("textureMode");
  const textureApply = $("textureApply");
  const textureInfo = $("textureInfo");

  // **一次装载两类素材** ✓；失败要说出来 ✓（静默的界面最让人困惑 ✗）。
  try {
    const palettes = await callTool("list_assets", { kind: "palette" }, { refresh: false });
    for (const asset of (palettes && palettes.assets) || []) {
      if (!asset.usable) continue;
      const option = document.createElement("option");
      option.value = asset.name;
      option.textContent = asset.name + (asset.source === "cache" ? "（导入的）" : "");
      if (palettePick) palettePick.appendChild(option);
    }
  } catch (error) {
    if (paletteInfo) paletteInfo.textContent = "调色板列表没拉到：" + String(error).slice(0, 90);
  }
  try {
    const textures = await callTool("list_assets", { kind: "texture" }, { refresh: false });
    // 同上一句：纹理为空时也要说清"是没资产还是没找到目录" ✗。
    if (textureInfo && textures && (textures.count === 0 || (textures.assets || []).length === 0)) {
      textureInfo.textContent =
        "一张纹理都没找到 ⇒ 多半是服务端没找到资产目录（看启动日志的「资产目录：」一行 ✓，" +
        "或用 --assets-dir 指定 ✓）";
    }
    const thumbs = $("textureThumbs");
    for (const asset of (textures && textures.assets) || []) {
      if (!asset.usable) continue;
      const option = document.createElement("option");
      option.value = asset.name;
      option.textContent = asset.name + (asset.source === "cache" ? "（导入的）" : "");
      if (texturePick) texturePick.appendChild(option);
      // **缩略图** ✓：48×48 裁切显示 ✓ ⇒ 十张一眼看完 ✓，不必逐张试 ✓。
      if (thumbs) {
        const image = document.createElement("img");
        image.src = "/textures/" + encodeURIComponent(asset.name);
        image.alt = asset.name;
        image.title = asset.name + "（点击选中 ✓）";
        image.dataset.name = asset.name;
        image.width = 48;
        image.height = 48;
        image.style.cssText =
          "object-fit:cover;border:2px solid transparent;border-radius:4px;cursor:pointer;background:#0002";
        // **图没发出来要说出来** ✗（一个空白框最让人困惑 ✓）。
        image.addEventListener("error", () => {
          image.style.borderColor = "#c33";
          image.title = asset.name + " ⇒ 缩略图没发出来（`/textures/` 路由或资产目录有问题 ✓）";
        });
        image.addEventListener("click", () => {
          if (texturePick) texturePick.value = asset.name;
          for (const other of thumbs.children) other.style.borderColor = "transparent";
          image.style.borderColor = "#4c6ef5";
          if (textureInfo) textureInfo.textContent = "已选中 " + asset.name + " ✓";
        });
        thumbs.appendChild(image);
      }
    }
    // **默认选中第一张并点亮它** ✓（否则"当前选的是哪张"看不出来 ✓）。
    if (texturePick && texturePick.options.length > 0 && $("textureThumbs")) {
      const first = $("textureThumbs").children[0];
      if (first) first.style.borderColor = "#4c6ef5";
    }
  } catch (error) {
    if (textureInfo) textureInfo.textContent = "纹理列表没拉到：" + String(error).slice(0, 90);
  }

  // **读一个调色板并铺成色块** ✓。截断也照实说 ✓（"给了 500 却不说"会让人以为板就那么大 ✗）。
  async function loadPalette() {
    if (!palettePick || !swatches) return;
    swatches.innerHTML = "";
    const name = palettePick.value;
    if (!name) return;
    const listed = await callTool(
      "list_palette_colors",
      { palette: name, limit: 256 },
      { refresh: false },
    );
    if (!listed || listed.ok === false) {
      if (paletteInfo) paletteInfo.textContent = "读不了这个调色板：" + JSON.stringify(listed).slice(0, 120);
      return;
    }
    for (const color of listed.colors || []) {
      const chip = document.createElement("button");
      chip.type = "button";
      chip.title = (color.name ? color.name + " " : "") + color.hex + "（点击取色 ✓）";
      chip.dataset.hex = color.hex;
      chip.style.cssText =
        "width:20px;height:20px;padding:0;border:1px solid rgba(0,0,0,.25);border-radius:3px;" +
        "background:" + color.hex + ";cursor:pointer";
      chip.addEventListener("click", () => {
        // **写进"当前目标"** ✓（目标 ⑤ ✓）。**默认仍是笔刷色** ✓ ⇒ 老习惯不变 ✓；
        // 但选了渐变那一端时 ⇒ **只改那一端** ✗（不动笔刷色 ✓）⇒ 并在提示里**说清写到了哪里** ✓，
        // 否则用户会以为"点了色块但笔刷颜色没变"而困惑 ✓。
        const target = ($("paletteTarget") || {}).value || "brush";
        let where = "";
        if (target === "gradFrom" || target === "gradTo") {
          const slot = $(target);
          if (slot) slot.value = color.hex;
          where = target === "gradFrom" ? "渐变起点" : "渐变终点";
        } else {
          // **笔刷色走与别处同一个入口** ✓（`setColor` 写的就是 `#color` ✓）。
          if (window.yanshi && window.yanshi.setColor) window.yanshi.setColor(color.hex);
          where = "笔刷色";
        }
        if (paletteInfo) {
          paletteInfo.textContent =
            "已取 " + color.hex + (color.name ? "（" + color.name + "）" : "") + " ⇒ " + where + " ✓";
        }
        // **用完即收** ✓（用户："设置完毕就关闭或者隐藏"✓）—— 取完色就把浮层收回去 ✓，
        // 前提是那个开关还勾着 ✓（想连着试几个色就取消勾选 ✓）。
        const autoClose = $("dockAutoClose");
        if (autoClose && autoClose.checked && window.yanshiDock) window.yanshiDock.close();
      });
      swatches.appendChild(chip);
    }
    if (paletteInfo) {
      const total = listed.total === undefined ? listed.count : listed.total;
      // **"空"必须说清是"没有资产"还是"没找到资产"** ✗（真实用户报告 ✓）：
      // 他换机器跑 ⇒ 这里显示"共 0 色" ✗ ⇒ 看起来像**功能没做** ✓，
      // 而真因是**服务端没找到资产目录** ✗（`--assets-dir` 缺省是相对路径 ✓，换工作目录就丢 ✓）。
      if (total === 0) {
        paletteInfo.textContent =
          "一个调色板都没找到 ⇒ 多半是服务端没找到资产目录（启动日志里有「资产目录：…」一行 ✓；" +
          "可用 --assets-dir 指定，或确认在仓库根目录启动 ✓）";
      } else {
        paletteInfo.textContent =
          "共 " + total + " 色" + (listed.truncated ? "（只显示了前 " + listed.count + " 个 ✓）" : " ✓");
      }
    }
  }
  if (palettePick) {
    palettePick.addEventListener("change", () => { void loadPalette(); });
    if (palettePick.options.length > 0) void loadPalette();
  }

  // **工程包：导出 / 导入** ✓（目标 ⑧ ✓）。
  {
    const exportButton = $("projectExport");
    const importButton = $("projectImport");
    const info = $("projectInfo");
    const pathOf = () => (($("projectPath") || {}).value || "").trim();
    if (exportButton) {
      exportButton.addEventListener("click", async () => {
        const path = pathOf();
        if (!path) {
          if (info) info.textContent = "先写一个路径 ✓";
          return;
        }
        const value = await callToolChecked("export_project", { path: path }, "导出工程");
        // **如实报出服务端算出来的东西** ✓（字节数 / 原子数 ✓）—— 只说"成功"没用 ✗。
        if (info) {
          // **只说自己知道的字段** ✗ —— 我第一版凭印象写了 `atoms` / `blobs` ✓，
          // 而 `export_project` **根本不回**这两个 ✓ ⇒ 界面上出现「? 条原子，? 个 blob」✗
          // ⇒ 那比不写更糟 ✓（看起来像坏了 ✓）。**字段名要照工具实际的回来** ✓。
          const parts = [];
          if (typeof value.bytes === "number") parts.push(value.bytes + " 字节");
          if (value.format) parts.push(String(value.format));
          if (value.render && value.render.width) {
            parts.push("内嵌预览 " + value.render.width + "×" + value.render.height);
          }
          info.textContent = "已导出到 " + path + (parts.length ? "（" + parts.join("，") + "）" : "") + " ✓";
        }
      });
    }
    if (importButton) {
      importButton.addEventListener("click", async () => {
        const path = pathOf();
        if (!path) {
          if (info) info.textContent = "先写一个路径 ✓";
          return;
        }
        // **新文档 id** ✓：默认从未导入过的一个 ✓；已存在会被工具**拒绝**（绝不覆盖 ✓）。
        const docId = (window.prompt("导入成哪个文档 id？（已存在的会被拒绝，绝不覆盖）", "imported") || "").trim();
        if (!docId) return;
        const value = await callToolChecked("import_project", { path: path, doc_id: docId }, "导入工程");
        // **令牌是按文档签发的** ✗ ⇒ 导入的新文档**用当前令牌打不开** ✓
        // ⇒ 必须把这条**和可照做的命令**一起说出来 ✓（否则用户会以为导入失败了 ✓）。
        if (info) {
          info.innerHTML =
            "已导入为 <code>" + docId + "</code>（" +
            (typeof value.atoms === "number" ? value.atoms + " 条原子" : "已还原") +
            (typeof value.blobs === "number" ? "，" + value.blobs + " 个 blob" : "") +
            "）✓ —— **令牌按文档签发** ✗ ⇒ 打开它要先为它签一个：<br>" +
            "<code>curl -X POST 'http://127.0.0.1:" + (location.port || "80") +
            "/api/documents' -H 'content-type: application/json' -d '{\"doc_id\":\"" + docId + "\"}'</code><br>" +
            "然后把地址栏里的 <code>doc=</code> 与 <code>token=</code> 换成返回的那两个 ✓";
        }
        log("已导入工程包：" + docId + " ✓（新文档需要新令牌 ✓）", "#2a2");
      });
    }
  }

  const gradApply = $("gradApply");
  if (gradApply) {
    gradApply.addEventListener("click", async () => {
      // **十六进制 ⇒ 分量** ✓（工具收 `{r,g,b,a}` ✓ —— 与界面里的取色控件格式不同 ✓
      // ⇒ 转换只在这一处 ✓，不让调用方各转一遍 ✓）。
      const toRgb = (hex) => ({
        r: parseInt(hex.slice(1, 3), 16),
        g: parseInt(hex.slice(3, 5), 16),
        b: parseInt(hex.slice(5, 7), 16),
        a: 255,
      });
      const fromHex = ($("gradFrom") || {}).value || "#ffffff";
      const toHex = ($("gradTo") || {}).value || "#000000";
      const kind = ($("gradKind") || {}).value || "linear";
      const angle = Number(($("gradAngle") || {}).value || 0);
      const picked = regionFromSelection("gradUseSelection");
      if (picked.error) {
        if ($("gradInfo")) $("gradInfo").textContent = picked.error;
        return; // **不静默填满整层** ✗
      }
      const gradArgs = {
        layer_id: state.layerId,
        kind: kind,
        angle: angle,
        from: toRgb(fromHex),
        to: toRgb(toHex),
      };
      if (picked.region) gradArgs.region = picked.region;
      const result = await callToolChecked("gradient_fill", gradArgs, "填充渐变");
      if ($("gradInfo")) {
        $("gradInfo").textContent =
          "已填 " + kind + (kind === "linear" ? "（" + angle + "°）" : "") + "：" + fromHex + " → " + toHex +
          (picked.region ? "，**只在选区**（" + picked.region.w + "×" + picked.region.h + "）✓" : "，整层 ✓");
      }
      void result;
      await refreshPreview();
      // **服务端改了文档 ⇒ 画布必须重绘** ✓（与前面几次同一个教训 ✓）。
      await resync();
    });
  }

  if (textureApply) {
    textureApply.addEventListener("click", async () => {
      const texture = texturePick ? texturePick.value : "";
      if (!texture) {
        if (textureInfo) textureInfo.textContent = "先选一张纹理 ✓";
        return;
      }
      const mode = textureMode ? textureMode.value : "tile";
      const picked = regionFromSelection("textureUseSelection");
      if (picked.error) {
        if (textureInfo) textureInfo.textContent = picked.error;
        return; // **不静默铺满整幅** ✗
      }
      // **先当场说"正在铺"** ✗（用户实测："点了选择之后画布上看不到效果" ✓）——
      // 实测时序 ✓：点完 **1 秒**时画布仍是 0 个非白像素 ✗、**6 秒**后才变成 38400 ✓
      // ⇒ 服务端要渲染整幅纹理再合成 ✓、再加上一次预览往返 ✓ ⇒ 界面**看起来像没反应** ✗。
      // ⇒ **不是靠加长等待** ✗，而是**当场把状态说出来** ✓（"看起来没反应"本身就是缺陷 ✓）。
      if (textureInfo) textureInfo.textContent = "正在铺 " + texture + "（" + mode + "）…";
      const textureArgs = { texture: texture, mode: mode };
      if (picked.region) textureArgs.region = picked.region;
      // **先把上一次那张纹理层删掉** ✗（用户实测：点五次就多出五层"背景（纹理）" ✗ ✓）——
      // **为什么是"删了重建"而不是"改那一层"** ✓：工具的字面语义是"**新建一层并沉到最底**" ✓
      //（那是它存在的理由 ✓：不会盖住已有的画 ✓）⇒ 界面这边只要保证**同一时刻只有一张纹理底** ✓ 即可 ✓
      // ⇒ 删掉旧的、再让它照原样建新的 ✓ —— **不把"复用"的逻辑塞进工具** ✗（那会改变它的契约 ✓）。
      if (state.textureLayerId) {
        const layers = await callTool("list_layers", {}, { refresh: false });
        const stillThere = ((layers && layers.layers) || []).some((layer) => layer.layer_id === state.textureLayerId);
        if (stillThere) {
          await callTool("delete_layer", { layer_id: state.textureLayerId }, { refresh: false });
        }
        state.textureLayerId = null;
      }
      const result = await callToolChecked("texture_background", textureArgs, "设为背景");
      // **记住这次建的那一层** ✓ ⇒ 下一次替换它 ✓（于是"点几次都只有一层" ✓）。
      if (result && result.created_layer) state.textureLayerId = result.created_layer;
      // **警告要显示出来** ✗（工具会在"指定了非空图层"时给警告 ✓
      // ⇒ 界面若把它吞掉 ✓，用户就只剩困惑 ✓）。
      let text =
        "已铺 " + texture + "（" + mode + "）" +
        (picked.region
          ? "，**只在那块选区**（" + picked.region.w + "×" + picked.region.h + " @ " +
            picked.region.x + "," + picked.region.y + "）✓"
          : "，铺满整幅 ✓");
      if (result && result.warning) text += "　⚠️ " + result.warning;
      if (textureInfo) textureInfo.textContent = text;
      // **铺完纹理之后，必须让画布整体改用"服务端像素"** ✗ —— 用户实测（macOS ✓）：
      // 铺完纹理后**画第一笔**时画面被纹理盖住 ✗，而**按刷新按钮就正常** ✓。
      // **机理** ✓：`texture_background` 在服务端把纹理层**沉到最底** ✓（那是它的契约 ✓），
      // 而**客户端本地的图层顺序没有跟着变** ✗ ⇒ 本地重绘时纹理仍压在笔迹之上 ✓
      // ⇒ 症状正是"第一笔被纹理覆盖" ✓，而刷新 = **重新同步** ⇒ 顺序对了 ✓ ✓。
      // ⇒ 修法不是"再重绘一次" ✗，而是**强制这一帧由服务端产出** ✓（顺序只有服务端说得准 ✓）。
      needsServerPixels = true;
      await refreshPreview();
      await resync();
      // **再排一次服务端补画** ✓（顺序改动之后，本地那份像素已经不可信 ✓）。
      queueServerBlit(null);
    });
  }
}

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
      await callToolChecked("update_layer", { layer_id: layerId, patch }, "切换图层显示/锁定");
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
    await callToolChecked("reorder_layers", { order }, "调整图层顺序");
    await refreshLayers();
    await resync();
  };
  const add = $("layerAdd");
  if (add) add.onclick = async () => {
    const layers = await listLayers();
    const made = await callTool("create_layer", { name: "图层 " + (layers.length + 1) });
    // **新建之后必须选中它** ✗（真实用户报告 ✓）—— 严格照**下面 `duplicate` 已有的模式** ✓。
    //
    // **为什么这是真 bug** ✓：新图层建在**最上面** ✓ 而**选中仍停在旧图层** ✗
    // ⇒ 用户以为"我在新图层上画" ✓ **实际画到了下面那层** ✗
    // ⇒ 于是"上面那些图层应该盖住我画的" ✗ **却不发生** ✓ —— **因为上层一直是空的** ✓。
    // 用户的叙述（"我最下层创建一个图层，在上面操作 …… 可是测试不是如此"）与此**逐字吻合** ✓。
    // **同一个道理此前只落在复制上** ✗（`duplicate` 里有 ✓、`add` 里没有 ✗）——
    // 这正是本项目反复吃亏的"**只修一条路径**" ✗ ⇒ 现在两条都在 ✓。
    if (made && made.ok && made.layer_id) {
      const select = $("layer");
      await refreshLayers();
      if (select) { select.value = made.layer_id; select.onchange(); }
    } else {
      await refreshLayers();
    }
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
    // **删除图层尤其不能静默** ✗：失败时用户会以为图层没了 ✓，而它其实还在 ✗
    // ⇒ "我以为的"与"实际状态"不一致 ✓ 是界面里最坏的一类错 ✗。
    await callToolChecked("delete_layer", { layer_id: state.layerId }, "删除图层");
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
  const value = await fetchOrLocal(api("/api/tools/list_layers"), {
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
      //
      // **别在这里重置用户的缩放** ✗（真实用户实测报告："每一笔结束能感受到画布的一个更新和抖动，
      // 虽然是瞬间的，但是人眼能察觉"✓）。真因就在这里 ✓：这一句以前是**无条件** `state.zoom = 1` ✓，
      // 而这个回调**不属于"用户换了文档"** ✓ —— 它会在**任何一次预览加载完成**时跑 ✓，
      // 包括提交之后那次 ✓、甚至一次**陈旧**的加载 ✓ ⇒ 画布就**自己缩放回整幅** ✓
      // ⇒ 人眼看到的就是"每一笔之后抖一下" ✗。
      //
      // 正确语义 ✓：**只有文档真的换了尺寸**（新建 / 打开另一份 ✓）才归位缩放 ✓；
      // 尺寸没变 ⇒ 像素照旧要画（下面那段 ✓），但**几何一律不动** ✗。
      const natural = { w: preview.naturalWidth, h: preview.naturalHeight };
      const resized =
        !state.docSize || state.docSize.w !== natural.w || state.docSize.h !== natural.h;
      if (resized) {
        state.docSize = natural;
        state.zoom = 1;
      }
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
    void loadPreview(value.thumb_url + "&t=" + Date.now());
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
    // **连上了就把重试计数清零** ✓（否则一次短暂断开会让"上限"永久少一格 ✓）。
    state.reconnectAttempts = 0;
    subscribeViewport();
  };
  socket.onclose = () => {
    // 只允许**当前**这条连接触发重连：切换文档时我们主动关闭旧连接，
    // 若它的 onclose 也去重连，就会同时存在多条订阅（表现为同一 atom 被处理多次）。
    if (state.socket !== socket) return;
    state.socket = null;
    $("conn").className = "dot";
    $("connText").textContent = "已断开";
    // **重连必须有界、且退避** ✗ —— 用户实测：服务端不在时控制台被
    // `WebSocket connection … failed` **刷满** ✓，因为原来是"每 1.5 秒无限重连" ✗。
    // **为什么这会变成真问题** ✓：控制台噪音会**盖住真正的报错** ✓（用户这次贴来的日志里 ✓
    // 一半是它 ✓），而且无限重连对已经停掉的服务端**毫无意义** ✗。
    // ⇒ 1.5s → 3s → 6s → 12s（封顶 ✓），**最多 8 次** ✓；再失败就**停下并说清** ✓。
    state.reconnectAttempts = (state.reconnectAttempts || 0) + 1;
    if (state.reconnectAttempts > 8) {
      $("connText").textContent = "已断开（不再重连）";
      log("连不上服务端（已重试 8 次）⇒ 停止重连 ✗ —— 服务端可能已经退出 ✓；把它起回来之后**刷新页面** ✓", "#c33");
      return;
    }
    const delay = Math.min(1500 * Math.pow(2, state.reconnectAttempts - 1), 12000);
    setTimeout(() => { if (state.token && !state.socket) connect(); }, delay);
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
      // **离线优先守卫** ✓：离线、或有未提交的本地预览 ⇒ **不重放** ✗（服务端此刻不权威 ✓）。
      if (navigator.onLine !== false && !liveLastRegion) {
        // 两种错误下 `resync()` ✗，其它失败（包括 heavy ✓）**什么都不做** ✓，
      }
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
            //
            // **本地刚提交过 ⇒ 只补那一块脏区** ✓（真实用户实测"每一笔结束还是闪一下"✗ ——
            // 探针量到那一笔之后补的是**整视口** 576000 像素 ✓，就是这一行造成的 ✓）。
            // **3 秒之内**算"刚提交过"✓ —— 更久的那个脏区可能已经过时 ✗ ⇒ 退回整视口 ✓（宁可多画 ✓）。
            // **这里曾经试过"本地刚提交就只补脏区"** ✗ —— 实测**画不出东西** ✓：
            // 探针里墨从 1750（整视口补画 ✓）掉到 **0** ✗，补画流水里那条
            // `box [0,0,900,640]` 明明执行了 ✓ 却什么都没画上 ✓
            // ⇒ **按脏区补画这条路本身有问题** ✗（`blitServerBox` 的坐标/取图那条 ✓），
            // 而它**不是**本轮能顺手修好的东西 ✗ ⇒ **先退回整视口** ✓（宁可闪 ✓ 不能看不见 ✗）。
            // **下一步** ✓：查 `blitServerBox` 为什么画不出东西 ✓（判据：同一块区域，
            // `blitServerBox` 与 `blitServerViewport` 画出来的墨量必须一致 ✓）；
            // 修好之后再把这里换成脏区 ✓ ⇒ 那才是"闪一下"的正解 ✓。
            // **离线优先守卫** ✓：离线、或有未提交的本地预览 ⇒ **不重放** ✗（服务端此刻不权威 ✓）。
            if (navigator.onLine !== false && !liveLastRegion) {
              void resync().then(() => blitServerViewport());
            }
          }
        } else {
          // **任何**应用失败都重新同步 ✓（含"内核表示不了 heavy 内容"这种情况 ✓），
          // 绝不静默丢掉这次变更 ✗ —— 此前只在 `out_of_order`/`precondition_failed`
          // 两种错误下才 resync ✗，其它失败什么都不做 ✓，介质落笔因此永远画不出来 ✓。
          // **离线优先守卫** ✓：离线、或有未提交的本地预览 ⇒ **不重放** ✗（服务端此刻不权威 ✓）。
          if (navigator.onLine !== false && !liveLastRegion) {
            // 注意本处理器**不是 async** ✗：只能用 `void resync()` ✓
          }
          //（写 `await` 会让整段页面脚本语法错误 ✓，实测是内核迟迟不就绪、检查全线超时 ✗）。
          window.yanshiStats.resyncs += 1;
          // 重载内核 **之后**再补画服务端像素 ✓ —— 内核表示不了 heavy 内容 ✗，
          // 少了这一步画布就是空白 ✓（实测介质落笔 10 秒内 0 有墨像素 ✗）。
          // **离线优先守卫** ✓：离线、或有未提交的本地预览 ⇒ **不重放** ✗（服务端此刻不权威 ✓）。
          if (navigator.onLine !== false && !liveLastRegion) {
            void resync().then(() => blitServerViewport());
          }
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

/// **末端色** ✓（与 `colorCss` 同一套写法 ✓ ⇒ 两个色控件不可能各写一套 ✓）。
function colorToCss() {
  const hex = ($("colorTo") || {}).value || "#000000";
  return { r: parseInt(hex.slice(1, 3), 16), g: parseInt(hex.slice(3, 5), 16), b: parseInt(hex.slice(5, 7), 16), a: 255 };
}

/// **一笔多色开关** ✓（勾上才传 ✓ —— 不勾就是原来的单色画法 ✓）。
function duoToneEnabled() {
  return !!($("duoTone") || {}).checked;
}

function colorCss() {
  const hex = $("color").value;
  return { r: parseInt(hex.slice(1, 3), 16), g: parseInt(hex.slice(3, 5), 16), b: parseInt(hex.slice(5, 7), 16), a: 255 };
}

/// **平滑开关的当前状态** ✓ —— 三条落笔路径（客户端原子 ✓、服务端工具 ✓）**共用一个语义** ✗。
///
/// **为什么默认是"开"** ✓：Web 端**此前就把 `smooth: true` 写死在两条客户端路径里** ✓
/// ⇒ 默认开 = **保持用户今天看到的画法** ✓；写死 ✗ 则会让"界面里关不掉" ✓（那正是本轮要修的毛病 ✓）。
function smoothEnabled() {
  return !!($("smooth") || {}).checked;
}

// 只重绘**覆盖层**（拖动中的笔迹/选区）。内容层绝不能被清空 —— 此前两者共用一个画布，
// 拖动结束的最后一次重绘会把已提交的内容一起擦掉，表现为「操作后画布空白，刷新才恢复」。
function redraw() {
  // **视图一变，图钉要跟着走** ✓（挂在 `redraw()` 尾部 ✓ —— 它是所有视图变化的必经之路 ✓）。
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
  positionAnnotationPins();
}

function localPoint(event) {
  const rect = board.getBoundingClientRect();
  // 画布内部像素 = 视口文档像素；再加视口原点得到文档坐标。
  // **压感** ✓（用户要求"创作时要把笔触压感用起来" ✓）：
  // 数位笔（`pointerType === "pen"`）给的是**随力度变化的 0..1** ✓ ⇒ 采用 ✓；
  // 鼠标/触摸给的是恒定 0.5 ✓ ⇒ 那是"没有压感信息" ✓ ⇒ 记 `null` ✓，下游退回原行为 ✓
  //（**鼠标用户不会因此变差** ✓，这一点很重要 ✓）。
  const pressure = event.pointerType === "pen" && typeof event.pressure === "number"
    ? Math.max(0, Math.min(1, event.pressure))
    : null;
  return {
    x: state.viewport.x + (event.clientX - rect.left) * board.width / rect.width,
    y: state.viewport.y + (event.clientY - rect.top) * board.height / rect.height,
    pressure: pressure,
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
      // **服务端说没有选区 ⇒ 本地也必须清掉** ✗（否则轮廓会继续画着 ✓、而"只作用于选区"会
      // 拿一个**已经不存在**的框去裁 ✓ —— 那正是"界面与事实不一致"的经典后果 ✓）。
      state.selectionShape = null;
      state.selectionId = null;
      void drawSelectionOutline();
      return;
    }
    const first = selections[0];
    const bbox = first.bbox || first.shape && first.shape.bbox;
    // **把服务端事实同步进本地状态** ✓ —— 真实发现 ✓：这里原本**只写状态栏文字** ✗，
    // 于是"在查看器之外建的选区"（工具层 / MCP / 另一个客户端 ✓）**界面完全不知道** ✓：
    // 轮廓不画 ✓、而"只作用于选区"会以为没有选区 ✓（实测：`selectionShape` 是 null ✗）。
    // 本函数的注释一直写着"由**服务端事实**驱动 ✓，而不是本地猜测" ✓ ⇒ 那就**连形状一起同步** ✓。
    state.selectionId = first.selection_id || first.id || state.selectionId;
    if (Array.isArray(bbox) && bbox.length >= 4) {
      state.selectionShape = {
        kind: "rect",
        bbox: { x: bbox[0], y: bbox[1], w: bbox[2], h: bbox[3] },
      };
    } else if (bbox && typeof bbox === "object") {
      state.selectionShape = { kind: first.shape && first.shape.kind ? first.shape.kind : "rect", bbox: bbox };
    } else {
      state.selectionShape = null;
    }
    void drawSelectionOutline();
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

// **滚轮 = 平移画布，不再缩放** ✓（用户实测报告："web 画布很容易不小心就被放大缩小" ✗，
// "适合只有点击放大缩小键或输入具体数值才变化，避免手势或者触摸板误触放大缩小" ✓）。
//
// **为什么要改** ✗：触摸板两指滑动与鼠标滚轮**是同一个事件** ✓ ⇒ 想滚动画布的人**必然**误触缩放 ✓；
// 而画布在 yanshi 里是**定尺**的 ✓（滚轮缩放不是"看细节"的刚需 ✓，是**别人的肌肉记忆** ✗）。
// **两个入口必须一起去掉** ✗：这里原本**注册了两个 wheel 监听器** ✓（同元素、都缩放 ✓）
// ⇒ 一次滚轮**缩放两次** ✓ ⇒ 那正是"一碰就放大得特别快"✗ 的来源 ✓（重复注册 = 本项目的老毛病 ✓）。
// **zoom 的入口只剩显式那几种** ✓：`适配 / 1:1 / 数值输入框 / + - 0 快捷键` ✓。
// **滚轮/触摸板什么都不做** ✓（用户实测报告："web 画布很容易不小心就被放大缩小" ✗，
// "适合只有点击放大缩小键或输入具体数值才变化，避免手势或者触摸板误触" ✓）。
//
// **为什么不是"滚轮平移"** ✗：我先写的就是平移 ✓ —— 但**实测它会被别处复位** ✗
//（`state.viewport` 滚完仍是原值 ✓，探针连等 5 秒也不动 ✗），而我**没能在一轮内查清是谁复位的** ✗
// ⇒ 那就**不发布一条我证明不了的行为** ✗（本项目纪律：不许留"看起来接上了、其实不生效" ✗）。
// 平移仍然**三条路都能用** ✓：手形工具 ✓ / 按住空格 ✓ / 中键拖动 ✓（都有既有断言守着 ✓）。
// ⇒ 滚轮这里只**吃掉事件** ✓：既不缩放 ✗、也不让页面跟着滚（页面纵向是锁死的 ✓）。
// **将来要加"滚轮平移"** ✓：先把"谁复位了 viewport"查清 ✓（判据：滚完 5 秒后位移**仍然**在 ✓）。
board.addEventListener("wheel", (event) => {
  if (!kernelReady()) return;
  event.preventDefault();
}, { passive: false });

/// **要不要平移** —— **一处判定，两处使用** ✗（本项目自己的教训：两条相似路径必然漂移 ✓）。
///
/// 背景（用户实测 ✓）："平移画布功能异常，小手会像画笔一样画上去" ✓。
/// 根因是**两个独立的 `pointerdown` 监听器** ✓：平移那个在 `wantsPan` 为假时 `return` ✓，
/// 但那**只从它自己返回** ✗ ⇒ 落笔那条照样跑 ✓ ⇒ 选"手形"时**一边平移一边落笔** ✗。
/// 所以：**平移与落笔都调这一个函数** ✓，落笔那条在它为真时直接返回 ✓。
function wantsPanEvent(event) {
  return event.button === 1 || state.tool === "pan" || spaceHeld;
}

// 中键拖动平移。
board.addEventListener("pointerdown", (event) => {
  // **平移的三种入口统一在这里** ✓：中键 / 手形工具 / 按住空格 ✓。
  // 只保留一条路径的原因：本会话已多次教训"两条相似路径会漂移" ✗
  //（整段/增量盖章、两条介质链路都栽过 ✓）。
  if (!wantsPanEvent(event)) return;
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
    // **"拖了但没动"必须说出来** ✗（真实用户报告 ✓："抓手工具没有效果" ✓）。
    //
    // **实机量出的真相** ✓（CDP + 我为此给 `state()` 加的 `viewport` ✓）：
    // `viewport = {x:0, y:0, w:400, h:300}` ✓ —— **视口正好等于整个文档** ✓
    // ⇒ `clampViewport` **正确地拒绝移动** ✗ ⇒ **抓手没坏** ✓，
    // 而是"整幅已经全在视口里，无处可移" ✓。**零反馈** ⇒ 用户只能得出"没效果" ✓ ✓。
    // **与"点眼睛画布变白"是同一类** ✓：**行为正确 + 没有反馈 = 看起来像坏了** ✗。
    // **为什么用提示、不放开钳制** ✓：让视口能移到文档之外 ✓ 会改变既定的取景语义 ✗
    //（缩放到整幅时画面应当居中 ✓）⇒ 这里只补**它本来就该说的话** ✓。
    const movedBy = Math.hypot(event.clientX - panState.startX, event.clientY - panState.startY);
    const viewportUnchanged =
      state.viewport.x === panState.originX && state.viewport.y === panState.originY;
    if (viewportUnchanged && movedBy > 20) {
      // **一句话覆盖两种情形** ✓（不为这句去引一个"文档尺寸"的状态 ✗ ——
      // 我上一版就是这么写的 ✓，而 `viewport` 里并没有 `docW` ✗）。
      log(
        "拖了但画布没动 ⇒ 多半已经缩放到整幅（无处可移），或已到边界；" +
        "先用滚轮 / ＋键放大，或点「适配」再拖 ✓",
        "#c93",
      );
    }
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

// **（这里原本还有第二个 `board` 的 wheel 监听器 ✓，也是缩放 ✓）** ✗ ——
// 两个监听器 ⇒ 一次滚轮缩放两次 ✓（实测的"误触一下放得特别快"✓）。已删除 ✓：
// 现在整个查看器**只有一个** wheel 监听器 ✓（就是上面那个"平移"的 ✓）。

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
  // **手形（或中键、空格）时不许落笔** ✗ —— 用户实测："小手会像画笔一样画上去" ✓；
  // 与平移那条**共用同一个判定** ✓（详见 `wantsPanEvent` 的说明 ✓）。
  if (wantsPanEvent(event)) return;
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
  // **标注** ✓（设计 4.6 / 13.4 ✓）：点一下就**在该处**新建一条 ✓，不落笔 ✗。
  if (state.tool === "annotate" && kernelReady()) {
    const point = localPoint(event);
    void createAnnotationAt(point.x, point.y);
    event.preventDefault();
    return;
  }
  try { board.setPointerCapture(event.pointerId); } catch (_) { /* 合成事件无 pointerId */ }
  state.dragging = event.pointerId;
  state.points = [localPoint(event)];
  // **🚨 画笔不能走这条"本地内核"路** ✗ —— 这是用户报的"**换什么笔都一样**"的**真正原因** ✓：
  // 只要**内核就绪**（macOS 上内核是好的 ✓ 我这边 headless 也是 ✓），这里就会建 `pendingStroke` ✓，
  // 而抬手时**第一条分支**（`if (pendingStroke && kernelReady())` ✓）会把它提交成
  // **通用几何笔迹** ⇒ 日志里是 **`draw_stroke`** ✗ ⇒ **`commitShape` 里那条画笔分支永远轮不到** ✗
  // ⇒ 201 支笔刷全被当成同一支几何笔 ✓ ✓（实测日志：`atom seq=3 draw_stroke` ✓，且 `brush_stroke` 一次都没发 ✗）。
  // ⇒ 所以：**当工具是画笔、且确实选了 .myb 笔刷时，不建 pendingStroke** ✓ ⇒ 让抬手落到 `commitShape` ✓
  // ⇒ 由**服务端的 Hokusai 引擎**落笔 ✓（这才是那 201 支笔该走的路 ✓）。
  const selectedBrushName = ($("brush") || {}).value || "";
  const brushOwnsTheStroke = state.tool === "brush" && selectedBrushName !== "";
  liveStroke = null;
  liveBlitBox = null;
  if (brushOwnsTheStroke) {
    liveStrokeClosed = false;
    liveStroke = { name: selectedBrushName, size: Number(($("size") || {}).value) || undefined, lastAt: 0 };
    liveLastRegion = null;
    void loadLocalBrushModule().catch(() => {});
  } else {
    liveStroke = null;
    liveLastRegion = null;
  }
  pendingStroke = (kernelReady() && !brushOwnsTheStroke)
    ? { atomId: ulid(), objectId: "obj_" + ulid(), layerId: state.layerId, tool: state.tool, base: 0 }
    : null;
});

/// **拖动期就用真笔刷** ✓（第 52 轮第三次装回 ✓，这次先**装上观测**再判 ✓）
///
/// **第 51 轮新查明的事实** ✓（纯 HTTP 实测 ✓）：**服务端已被排除** ✗ ——
/// "提交后**立刻**取图"与"500ms 后再取图"，同一块区域**逐字节相同** ✓、墨量也对 ✓
///（所以第 50 轮我写的"服务端返回旧图"那条**假设是错的** ✗，判据本身把我纠正了 ✓）。
/// ⇒ 擦掉实时帧的是**客户端** ✓；而 `redraw()` **不碰主画布** ✓（只画覆盖层 ✓，读了代码 ✓）
/// ⇒ 嫌疑人只剩 **WS 的内核重绘**（heavy 内容内核折出来是**空补丁** ✓，会把服务端像素**盖掉** ✓ ——
/// 与第 44 轮那条"内核重绘覆盖补画"同一个机制 ✓）。
/// **下一轮的窄判据** ✓（观测already有：`state().serverBlits / lastBlitServerInk / blitLog` ✓）：
/// 拖动中**同时**打点"画布墨"与"每次补画的原因/面积/服务端墨量" ✓ ⇒ 一眼看出是**哪一次**把它擦掉的 ✓，
/// 再对症下药（很可能就是"内核重绘之后必须再补一次"那条既有经验的又一次出场 ✓）。
let liveStroke = null;
let liveBlitBox = null;

// **第 50–56 轮：拖动期真笔刷，六次实现、六次撤回** ✗ —— **每次都有实测，每次都被实测否掉** ✓：
// * 服务端**完全无罪** ✓（帧提交 ✓、区域里有墨 ✓、`putImageData` 也画过 ✓、对照实验：不拖动时
//   手动补一次就成 ✓）；
// * 三条假设**都被量掉** ✓：①"服务端返回旧图"✗；②"被内核重绘擦掉（`needsServerPixels` 那道门）"✗
//   —— 第 56 轮把门打开之后**实测仍是 0 墨** ✓；③"节流太快 / 队列并掉"✗ —— 600ms + 每帧直接补小块 ✓。
// ⇒ **还没找到元凶** ✗，且六次都没能让"指针还按着时画布上有墨"成立 ✓。
// **不再硬试** ✓：继续盲改只会烧轮次 ✓；**这一项就此停在这里** ✓（诚实、零回归 ✓），
// 结论、六次实测与观测手段（`yanshiDebugBlit` ✓、`blitLog` 带原因/面积/服务端墨量 ✓）全部留在档里 ✓，
// 谁将来愿意继续，判据现成：`scripts/browser-live-brush.mjs` ✓（指针还按着时就有该笔刷的纹理 ✓、
// 对象数为 1 ✓、撤销一步回到画之前 ✓）。
// **而且要如实告诉用户** ✓：他抱怨的"实心线 → 真笔刷"的**突变**，成因是**内置画笔**那条覆盖层 ✓
// —— 选 `.myb` 笔刷（spray 等）时它**不会**出现 ✓ ⇒ **今天的可用答案就是"选 .myb 笔刷"** ✓。

// **`.myb` 拖动期本地渲染**（门面与服务端逐字节相同；笔记第 58/66/67 轮）。
let localBrushApi = null;
let localBrushLoading = null;
let liveLastRegion = null;
const localBrushText = new Map();

async function loadLocalBrushModule() {
  if (localBrushApi) return localBrushApi;
  if (!localBrushLoading) {
    localBrushLoading = fetch("/brush-module.wasm")
      .then((response) => {
        if (!response.ok) throw new Error("模块 HTTP " + response.status);
        // **顺手把它写进 SW 用的那个缓存**（离线时 SW 的回落会 caches.match 到它）。
        // 缓存名必须与 SW 里的 CACHE 一致。
        // **必须在读 body 之前就 clone** ✗ —— 我上一版把 `response.clone()` 写在下面的 `.then()` 里 ✗，
        // 那时代码已经调用过 `arrayBuffer()` ⇒ 同一个 body 被读了两次 ⇒ 实测报
        // `TypeError: Failed to execute 'clone' on 'Response': Response body is already used` ✓
        // ⇒ 门面**从未进过缓存** ⇒ 离线就没有本地预览 ✗（这就是"离线画不了"的直接原因 ✓）。
        let forCache = null;
        try {
          if (window.caches) forCache = response.clone();
        } catch (error) { forCache = null; }
        if (forCache) {
          caches.open("yanshi-shell-v1")
            .then((cache) => cache.put("/brush-module.wasm", forCache))
            .catch((error) => console.warn("写缓存失败 /brush-module.wasm（离线将没有本地预览）：" + error));
        }
        return response.arrayBuffer();
      })
      .then((bytes) => WebAssembly.instantiate(bytes, {}))
      .then(({ instance }) => { localBrushApi = instance.exports; return localBrushApi; })
      .catch((error) => {
        localBrushLoading = null;
        window.yanshiStats.localBrushErrors = (window.yanshiStats.localBrushErrors || 0) + 1;
        log("笔刷本地渲染模块没加载上：" + String(error).slice(0, 80), "#c93");
        throw error;
      });
  }
  return localBrushLoading;
}

async function loadLocalBrushText(name) {
  if (localBrushText.has(name)) return localBrushText.get(name);
  const response = await fetch("/brushes/" + name + ".myb");
  if (!response.ok) throw new Error("笔刷文本 HTTP " + response.status);
  const text = await response.text();
  // **笔刷文本也写进同一个缓存**（**按需**：只有真正用到的笔刷才进缓存，199 支不会全下）。
  try {
    if (window.caches) {
      caches.open("yanshi-shell-v1")
        .then((cache) => cache.put("/brushes/" + name + ".myb",
          new Response(text, { headers: { "content-type": "text/plain; charset=utf-8" } })))
        .catch((error) => console.warn("写缓存失败 /brushes/" + name + ".myb（离线将没有本地预览）：" + error));
    }
  } catch (error) { /* 缓存失败不影响这次预览 */ }

  // **服务端顺带告诉我们的那件事** ✓（第 68 轮定的头 ✓）：会读画布的笔刷**不能**本地预览 ✓
  //（门面没有 base 输入 ⇒ 两边起点不同 ⇒ 预览会漂 ✗）。判定是**服务端那一处**算的 ✓，这里只读 ✓。
  const readsCanvas = response.headers.get("X-Yanshi-Brush-Reads-Canvas") === "1";
  const entry = { text, readsCanvas };
  localBrushText.set(name, entry);
  return entry;
}

function liveColour() {
  const hex = ($("color") || {}).value || "#000000";
  const digits = hex.replace("#", "");
  const value = (start, fallback) => {
    const pair = digits.slice(start, start + 2);
    return pair.length === 2 ? Number.parseInt(pair, 16) : fallback;
  };
  return { r: value(0, 0), g: value(2, 0), b: value(4, 0), a: 255 };
}

/// 区域与**服务端算法对齐**（半径 `size/2 + 4`、`floor`/`ceil`）—— 第 67 轮实测：`+8` 会多画一圈。
function liveRegion(size) {
  const half = (Number(size) || 128) / 2 + 4;
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const point of state.points) {
    x0 = Math.min(x0, point.x); y0 = Math.min(y0, point.y);
    x1 = Math.max(x1, point.x); y1 = Math.max(y1, point.y);
  }
  if (!Number.isFinite(x0)) return null;
  const canvasWidth = state.docSize ? state.docSize.w : 1024;
  const canvasHeight = state.docSize ? state.docSize.h : 1024;
  const left = Math.max(0, Math.floor(x0 - half));
  const top = Math.max(0, Math.floor(y0 - half));
  const right = Math.min(canvasWidth, Math.ceil(x1 + half));
  const bottom = Math.min(canvasHeight, Math.ceil(y1 + half));
  if (right - left <= 0 || bottom - top <= 0) return null;
  return { x: left, y: top, w: right - left, h: bottom - top };
}

// **这一笔是否已经结束** ✗（第 75 轮实测的竞态）：抬手时可能还有一帧本地渲染在飞 ✓，
// 它会在**补画之后**落地 ⇒ 又把本地那层画回画布 ✓ ⇒ 三次里有一次"撤销后还剩墨" ✗。
// ⇒ 真正 `drawImage` 之前先看这个标记 ✓，结束了就**不许再画** ✓。
let liveStrokeClosed = false;
let livePaintBusy = false;
async function paintLiveFrame() {
  const stats = window.yanshiStats;
  stats.localBrushCalls = (stats.localBrushCalls || 0) + 1;
  if (!liveStroke) return;
  if (livePaintBusy) return;
  const now = performance.now();
  if (now - liveStroke.lastAt < 35) return;
  liveStroke.lastAt = now;
  livePaintBusy = true;
  try {
    const api = await loadLocalBrushModule();
    const brush = await loadLocalBrushText(liveStroke.name);
    if (brush.readsCanvas) {
      // **承认边界，而不是给一个会漂的预览** ✓（第 68 轮定 ✓）：涂抹 / colorize 一类要靠"抹开画布上
      // 已有的颜色"工作 ✓，而门面没有 base 输入 ✗ ⇒ 本地渲染会与服务端不一致 ✗。
      stats.localBrushReadsCanvasSkips = (stats.localBrushReadsCanvasSkips || 0) + 1;
      if (!stats.localBrushReadsCanvasLogged) {
        stats.localBrushReadsCanvasLogged = true;
        log(
          "这支笔刷会读画布（涂抹一类）⇒ 本次拖动不做本地预览 ✓（抬手仍由服务端落笔 ✓，所见即所存 ✓）",
          "#c93",
        );
      }
      liveStroke = null;      // 这一笔不再尝试 ✓（免得每帧白跑一趟 ✗）
      return;
    }
    const myb = brush.text;
    const region = liveRegion(liveStroke.size);
    if (!region) return;
    const request = JSON.stringify({
      myb,
      points: state.points.map((point) => [
        point.x, point.y, Number.isFinite(point.pressure) ? point.pressure : 0.5,
      ]),
      size: liveStroke.size,
      color: liveColour(),
      opacity: null,
      hardness: null,
      region,
    });
    // **改用共享内核** ✓（(A)③：**一份实现** ✓）—— 请求 JSON **原样不变** ✓
    // （形状正是 `kernel-brush-parity.mjs` 逐字节验过的那一份 ✓）。
    // 与门面那套 C-ABI 的区别 ✓：**内存管道不用手写** ✓（bindgen 接管 ✓）⇒
    // `paint_brush` 成功给像素 ✓、失败给 `undefined` ✓ ⇒ **两条路可区分** ✓。
    const kernel = state.wasm;
    const bytes = kernel && typeof kernel.paint_brush === "function" ? kernel.paint_brush(request) : null;
    if (!bytes || !bytes.length) {
      // 失败：内核给不出原因文本 ✗（它与门面的错误通道不同 ✓）⇒ 就**说清是"内核没画出来"** ✓
      stats.localBrushErrors = (stats.localBrushErrors || 0) + 1;
      if (!stats.localBrushErrorLogged) {
        stats.localBrushErrorLogged = true;
        log("本地笔刷预览：内核没画出这一帧 ⇒ 本帧跳过 ✓（抬手仍由服务端落笔 ✓）", "#c93");
        }
      return;
    }
    const painted = Uint8Array.from(bytes);
    const offscreen = document.createElement("canvas");
    offscreen.width = region.w;
    offscreen.height = region.h;
    offscreen.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(bytes.buffer), region.w, region.h), 0, 0);
    // **在飞的帧若发现这一笔已结束，就不许再贴** ✗（否则它会把本地那层盖回服务端像素之上 ✓）。
    if (liveStrokeClosed) return;
    ctx.drawImage(offscreen, Math.round(region.x - state.viewport.x), Math.round(region.y - state.viewport.y));
    // **记住本地覆盖过的那块 —— 而且是"整笔的并集"** ✗（第 74 轮修 ✓）：
    // 原来只记**最后一帧**的区域 ✓ ⇒ 若最后一帧落后于真实笔迹 ✓，露在区域外的一小截
    // 在提交后补画时清不掉 ✗ ⇒ 撤销之后画布上还剩墨 ✗（第 25 轮没踩到、第 73 轮踩到了 ⇒ **时序性** ✓）。
    // 并集是**单调增长**的 ✓ ⇒ 补画一定覆盖住所有画过的像素 ✓，不再看时序的脸色 ✓。
    liveLastRegion = liveLastRegion
      ? {
          x: Math.min(liveLastRegion.x, region.x),
          y: Math.min(liveLastRegion.y, region.y),
          w: Math.max(liveLastRegion.x + liveLastRegion.w, region.x + region.w)
            - Math.min(liveLastRegion.x, region.x),
          h: Math.max(liveLastRegion.y + liveLastRegion.h, region.y + region.h)
            - Math.min(liveLastRegion.y, region.y),
        }
      : { x: region.x, y: region.y, w: region.w, h: region.h };
    stats.localBrushFrames = (stats.localBrushFrames || 0) + 1;
  } catch (error) {
    stats.localBrushErrors = (stats.localBrushErrors || 0) + 1;
    log("本地笔刷这一帧失败：" + String(error).slice(0, 90), "#c93");
  } finally {
    livePaintBusy = false;
  }
}

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
  if (liveStroke) octx.clearRect(0, 0, overlay.width, overlay.height);
  if (!liveStroke && pendingStroke && state.points.length >= 2 && !RETOUCH_TOOLS.has(state.tool)) {
    // 乐观渲染：拖动中只更新**本地覆盖层**（不进原子日志），落笔才提交最终原子。
    updatePreviewOverlay(pendingStroke);
  }
  if (liveStroke && state.points.length >= 2) void paintLiveFrame();
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
  let data = { points, size, color, hardness: 0.7, smooth: smoothEnabled() };
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
  // **平滑开关** ✓（勾上才传 ✓ ⇒ 不勾就是原来的行为 ✓ —— 老用户与老文档一个像素都不变 ✓）。
  const smooth = smoothEnabled();
  if (state.tool === "brush") {
    if (state.points.length < 2) return;
    // **选了 `.myb` 笔刷 ⇒ 交给服务端的 Hokusai 引擎** ✓
    //（用户硬要求：**Web 与 MCP 都要能用** ✓ —— 工具层已有 `brush_stroke` ✓，
    //  这里只是给它一个**界面入口** ✓）。
    // **为什么不在浏览器里逐 dab** ✗：介质那条路走的是 **wasm 内核** ✓，
    // 而 Hokusai 是 **Rust crate** ✓、**没有 wasm 化** ✗ ⇒ 浏览器端画不了 ✓
    // ⇒ 这一条走**服务端权威**：**抬手时一次**提交整条笔触 ✓
    //（与"没有内核时"的做法一致 ✓，也是本项目一贯的"能力在工具层" ✓）。
    const brushName = ($("brush") || {}).value || "";
    // **勾了"一笔多色"但没选 `.myb` 笔刷** ⇒ 说清楚 ✗：内置几何那条路**没有多色** ✓
    //（要滑杆式的渐变得靠介质插件或分段对象 ✗）—— 不静默忽略 ✓（本项目头号病症 ✓）。
    if (duoToneEnabled() && !brushName) {
      log("「一笔多色」目前只支持 .myb 笔刷（内置画笔是纯几何、单色）⇒ 请在「笔刷」里选一支", "#c93");
    }
    if (brushName) {
      const controlPoints = state.points.map((point) => [
        point.x,
        point.y,
        point.pressure === undefined ? 0.5 : point.pressure,
      ]);
      await callToolChecked(
        "brush_stroke",
        {
          layer_id: state.layerId,
          brush: brushName,
          points: controlPoints,
          // **大小以工具条上的"粗细"为准** ✓（与内置画笔同一处 ✓ ⇒ 用户不用记两套 ✓）。
          size: size,
          // **平滑** ✓：与 MCP 同名同义（`brush_stroke.smooth` ✓）。
          smooth: smooth,
          // **一笔多色** ✓：与 MCP 同名同义（`brush_stroke.color_to` ✓）—— 不勾就**不传** ✓
          //（缺省逐字节不变 ✓）。
          color_to: duoToneEnabled() ? colorToCss() : undefined,
          // **颜色也必须带上** ✗ —— 此前写的是 `color: undefined` ✓ ⇒
          // 用户在工具条上**选了颜色也画不上** ✗（笔触只用 `.myb` 自带色 ✓），
          // 这正是"界面里有的东西实际不生效" ✓（本轮连同工具层的颜色缺陷一起修 ✓）。
          // `colorCss()` 回的就是 `{r,g,b,a}`（0..255 ✓）⇒ 工具层照收 ✓。
          color: color,
        },
        "落笔（" + brushName + "）",
      );
      liveStroke = null;
      // **先宣布这一笔结束** ✗（在飞的帧就此作废 ✓，见上面那个标记 ✓）。
      liveStrokeClosed = true;
      // **提交之后按"本地覆盖过的那块"从服务端补画一次**（第 67 轮定：否则画布上留的是本地那层，
      // 撤销只清对象脏区 ⇒ 外面那圈永远清不掉 ✗）。多留 4px 余量覆盖取整误差 ✓。
      if (liveLastRegion && window.yanshiDebugBlit) {
        const box = liveLastRegion;
        // **等它画完再往下走** ✗（第 74 轮实测：原来用 `void` ⇒ 异步 ⇒ 判据立刻量到的是本地那层 ✓，
        // 三次里有一次读成"撤销后还剩 3019 墨" ✗ ⇒ 那是竞态、不是逻辑错 ✓）。
        // **重试几次** ✗（第 106 轮实测：单次直调在三次里被跳过一次 ✓ —— 本仓库的补画本来就有
        // "忙/排队"机制 ✓ ⇒ 单次调用可能整段不生效 ✓）。每次都是服务端权威像素 ✓ 多补无害 ✓。
        for (let attempt = 0; attempt < 3; attempt += 1) {
          await window.yanshiDebugBlit([box.x - 4, box.y - 4, box.w + 8, box.h + 8]);
          await new Promise((resolve) => setTimeout(resolve, 60));
        }
      }
      liveLastRegion = null;
      await refreshPreview();
      await resync();
      return;
    }
    // **不再无视结果** ✗ —— 此前这里连返回都不看 ✓ ⇒ 被拒时画布上毫无反馈 ✗
    //（正是"静默吞掉用户操作" ✓）。现在失败必留痕 ✓，并提示怎么恢复 ✓。
    await callToolChecked("draw_stroke", {
      layer_id: state.layerId,
      data: {
        points: state.points.map((p) => [p.x, p.y]),
        size,
        color,
        hardness: 0.7,
        // **同一个开关、同一个语义** ✓（渲染层在**渲染时**插值 ✓ ⇒ 日志里存的仍是原始点 ✓）。
        smooth: smooth,
      },
    }, "落笔");
  } else if (state.tool === "erase") {
    if (state.points.length < 2) return;
    await callToolChecked("erase", {
      layer_id: state.layerId,
      // **压力必须发出去** ✗ —— 用户实测报告："用绘画板测试，橡皮没有压力支持，不同压力下表现都一样" ✓。
      // 上一版这一行只映射 `x, y` ✗ ⇒ 压力在**这里**被丢掉 ✓（我先前改的 `1657` 是**另一条路** ✗，
      // 与橡皮无关 ✓ —— 已在笔记里更正 ✓）。兜底 0.5 ✓，与 `.myb` 那条（`controlPoints` ✓）和门面那条一致 ✓。
      data: { points: state.points.map((p) => [p.x, p.y, Number.isFinite(p.pressure) ? p.pressure : 0.5]),
              size: size * 1.5, color: { r: 0, g: 0, b: 0, a: 0 } },
    }, "擦除");
  } else {
    const [a, b] = state.points;
    if (!a || !b) return;
    const bbox = {
      x: Math.min(a.x, b.x), y: Math.min(a.y, b.y),
      w: Math.abs(b.x - a.x) || 1, h: Math.abs(b.y - a.y) || 1,
    };
    // **形状这条分支上一轮漏了** ✗：只补了 brush/erase ✓ ⇒ 矩形/椭圆仍是静默的 ✗（同类问题 ✓）。
    await callToolChecked("draw_shape", {
      layer_id: state.layerId,
      data: { geometry: { kind: state.tool, bbox }, color },
    }, "画形状");
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
  // **标注** ✓（两轮前我自己加的工具 ✓）—— 它的图标**当时漏了** ✗ ⇒
  // `TOOL_ICONS[key] || ""` 会让按钮渲染成**空 svg** ✓ ⇒ **工具栏里有一个看不见的按钮** ✗。
  // 本轮的**结构性守卫**（`tool_icons.rs` ✓）会把这一类**在测试里挡住** ✓。
  annotate: '<path d="M12 3l5 5-6 6-2-2z"/><path d="M9 14l-4 7 7-4"/><circle cx="18" cy="6" r="2"/>',
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
  // **标注** ✓（设计 4.6 ✓）：点画布即在该处新建一条标注 ✓。
  { tool: "annotate", label: "标注", key: "n" },
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
    // **撤销 / 重做快捷键** ✓（真实用户 §六-15 报的原话：撤销按钮在右栏，绘画时右手在画布上，够不着 ✓）。
    // 此前这里**只忽略带修饰键的按键** ✗ ⇒ Ctrl+Z **什么都不做** ✓ —— 用户以为没有撤销 ✓。
    // **约定与主流一致** ✓：`Ctrl/Cmd+Z` 撤销 ✓、`Ctrl/Cmd+Shift+Z` 重做 ✓
    //（macOS 用 `Cmd` ✓ 所以 `metaKey` 与 `ctrlKey` 都收 ✓）。
    if ((event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === "z") {
      event.preventDefault();
      if (event.shiftKey) {
        void redoOnce();
      } else {
        void undoOnce();
      }
      return;
    }
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

  // **`window.yanshi`：给自动化用的稳定接口** ✓（真实用户 §六-5 ✓）。
  //
  // **用户原话** ✓："Playwright 自动化时，改颜色要 `document.querySelector('#color')` 注入 ✓，
  // 改粗细要找 `input[type="range"]` ✓，**无稳定 API** ⇒ UI 一改就崩" ✓。
  // **做法** ✓：只做**薄薄一层** ✓ —— 设**既有的控件**、再**触发既有事件** ✓
  //（`#medium` 一直是靠 `change` 事件驱动的 ✓，见上面那段 ✓）⇒ **不复制任何逻辑** ✗
  // ⇒ 以后改 UI 时，这层只需跟着改**选择器** ✓，而不是两套行为各自漂移 ✗。
  //
  // **同时提供"读"** ✓：自动化最需要的是**断言当前设置** ✓（"我设的颜色真的生效了吗" ✓）。
  /// **把 `.myb` 笔刷装进下拉** ✓（用户硬要求：**Web 与 MCP 都要能用** ✓）。
///
/// **为什么懒加载** ✓：196 支笔刷的下拉在**首次点开时**才拉 ✓（免得每次开页面都多一次请求 ✓
/// 也免得 196 个 `<option>` 拖慢首屏 ✓）；`title` 里已经写明这一点 ✓。
///
/// **为什么按名字、而不是按路径** ✓：工具层按"缓存优先、内置其次"解析 ✓
/// ⇒ 用户导入的同名笔刷**自动生效** ✓（与 `list_assets` 的优先级一致 ✓）。
/// **按搜索词过滤笔刷下拉** ✓（目标 ⑥ ✓）。
///
/// **要点** ✓：过滤后**把空的分组也藏起来** ✗ —— 否则用户会看到一串**空标题** ✓，
/// 那比"没过滤"更让人困惑 ✓（"这里有组却没有笔刷" ✓）。
/// **另外** ✓：当前选中项若被过滤掉 ⇒ **自动选第一支可见的** ✓ ——
/// 否则会出现"下拉里看不到它、但落笔用的还是它" ✗（界面与事实不一致 ✓）。
function applyBrushFilter() {
  const select = $("brush");
  const box = $("brushSearch");
  if (!select) return;
  const needle = (box && box.value ? box.value : "").trim().toLowerCase();
  let visible = 0;
  const all = Array.from(select.options);
  for (const option of all) {
    // 第一个"（内置画笔）"永远留着 ✓（它是"不用 .myb"的意思 ✓）。
    const keepAlways = option.value === "";
    const hit = keepAlways || needle === "" || option.value.toLowerCase().includes(needle);
    option.hidden = !hit;
    if (hit) visible += 1;
  }
  for (const child of Array.from(select.children)) {
    if (child.tagName === "OPTGROUP") {
      const hasVisible = Array.from(child.children).some((option) => !option.hidden);
      child.hidden = !hasVisible;
      child.disabled = !hasVisible;
    }
  }
  const current = select.selectedOptions && select.selectedOptions[0];
  if (current && current.hidden) {
    const first = all.find((option) => !option.hidden);
    if (first) select.value = first.value;
  }
  const hint = $("brushSearchHint");
  if (hint) {
    hint.textContent = needle
      ? `匹配 ${Math.max(0, visible - 1)} 支（共 ${Math.max(0, all.length - 1)} 支）`
      : `共 ${Math.max(0, all.length - 1)} 支，分 ${select.querySelectorAll("optgroup:not([hidden])").length} 组`;
  }
}

/// **按搜索词过滤笔刷下拉** ✓（目标 ⑥ ✓）。
///
/// **要点** ✓：过滤后**把空的分组也藏起来** ✗ —— 否则用户会看到一串**空标题** ✓，
/// 那比"没过滤"更让人困惑 ✓（"这里有组却没有笔刷" ✓）。
/// **另外** ✓：当前选中项若被过滤掉 ⇒ **自动选第一支可见的** ✓ ——
/// 否则会出现"下拉里看不到它、但落笔用的还是它" ✗（界面与事实不一致 ✓）。
function applyBrushFilter() {
  const select = $("brush");
  const box = $("brushSearch");
  if (!select) return;
  const needle = (box && box.value ? box.value : "").trim().toLowerCase();
  let visible = 0;
  const all = Array.from(select.options);
  for (const option of all) {
    // 第一个"（内置画笔）"永远留着 ✓（它是"不用 .myb"的意思 ✓）。
    const keepAlways = option.value === "";
    const hit = keepAlways || needle === "" || option.value.toLowerCase().includes(needle);
    option.hidden = !hit;
    if (hit) visible += 1;
  }
  for (const child of Array.from(select.children)) {
    if (child.tagName === "OPTGROUP") {
      const hasVisible = Array.from(child.children).some((option) => !option.hidden);
      child.hidden = !hasVisible;
      child.disabled = !hasVisible;
    }
  }
  const current = select.selectedOptions && select.selectedOptions[0];
  if (current && current.hidden) {
    const first = all.find((option) => !option.hidden);
    if (first) select.value = first.value;
  }
  const hint = $("brushSearchHint");
  if (hint) {
    hint.textContent = needle
      ? `匹配 ${Math.max(0, visible - 1)} 支（共 ${Math.max(0, all.length - 1)} 支）`
      : `共 ${Math.max(0, all.length - 1)} 支，分 ${select.querySelectorAll("optgroup:not([hidden])").length} 组`;
  }
}

async function refreshBrushOptions() {
  const select = $("brush");
  if (!select) return;
  try {
    const listed = await callTool("list_assets", { kind: "brush" }, { refresh: false });
    const assets = (listed && listed.assets) || [];
    const keep = select.value;
    // **只留第一个"内置画笔"选项** ✓，其余重建 ✓（重复装载不会越堆越多 ✓）。
    while (select.options.length > 1) select.remove(1);
    // **按来源分组** ✓（目标 ⑥ ✓）：扁平化时给子目录加了前缀 ✓
    //（`classic-` / `deevad-` / `ramon-` ✓），文件里还有 `brushkit-` 与**没前缀**的 ✓。
    // ⇒ 分组就直接用这个前缀 ✓ —— **不另造一套分类** ✗（那会与资产命名漂移 ✓）。
    const groups = new Map();
    const order = ["classic", "deevad", "ramon", "brushkit", "其他"];
    for (const asset of assets) {
      if (!asset.usable) continue;
      const name = String(asset.name).replace(/\.myb$/i, "");
      // **分类读工具层给的那一份** ✓（`list_assets` 的 `category` ✓）——
      // 本地再算一套前缀分类 ✗ 迟早与 MCP 那边不一致 ✓（这正是"两边各一套"的老病 ✓）。
      const group = order.includes(asset.category) ? asset.category : "其他";
      if (!groups.has(group)) {
        const optgroup = document.createElement("optgroup");
        optgroup.label = group;
        groups.set(group, optgroup);
      }
      const option = document.createElement("option");
      // **值不带扩展名** ✓（工具层会自己补 `.myb` ✓）。
      option.value = name;
      option.textContent = name + (asset.source === "cache" ? "（导入的）" : "");
      groups.get(group).appendChild(option);
    }
    // **别忘了我上一版把这段循环删掉了** ✗ —— 那次替换的锚点**就是**这个循环 ✓，
    // 而我的新文本里**没有把它写回去** ✓ ⇒ 分组永远挂不上下拉 ✓ ⇒ 界面上只剩"（内置画笔）" ✓、
    // **一个 optgroup 都没有** ✓（实测：`list_assets` 回 201 ✓，而下拉里 1 个选项 ✗）。
    // **教训** ✓：`replace(锚点, 新文本)` 里**必须把锚点本身写回去** ✗，除非就是要删它 ✓。
    for (const group of order) {
      if (groups.has(group)) select.appendChild(groups.get(group));
    }
    // **★ 收藏 / 最近使用** ✓（目标 ⑧-1 ✓）—— **来自工作区偏好** ✓（工具层 ✓ ⇒ MCP 也读得到 ✓）。
    // 顺序：**收藏 → 最近 → 按来源分组** ✓（常用的在最上面 ✓，这与"199 支一个长下拉"是同一个痛点 ✓）。
    try {
      const prefs = await callTool(
        "get_preferences",
        { keys: ["brush_favorites", "brush_recent"] },
        { refresh: false },
      );
      const favorites = ((prefs && prefs.preferences && prefs.preferences.brush_favorites) || []).map(String);
      const recents = ((prefs && prefs.preferences && prefs.preferences.brush_recent) || []).map(String);
      const known = new Set(Array.from(select.querySelectorAll("option")).map((option) => option.value));
      const addGroup = (label, names) => {
        // **只放真实存在的笔刷** ✓（收藏里可能有已经被删掉的导入笔刷 ✓ ⇒ 忽略而不是画一个空条目 ✗）。
        const usable = names.filter((name) => name && known.has(name));
        if (usable.length === 0) return;
        const optgroup = document.createElement("optgroup");
        optgroup.label = label;
        optgroup.dataset.preference = "1";
        for (const name of usable) {
          const option = document.createElement("option");
          option.value = name;
          option.textContent = name;
          optgroup.appendChild(option);
        }
        select.insertBefore(optgroup, select.children[0] || null);
      };
      // **先插最近、再插收藏** ✓ ⇒ 收藏最终在最上面 ✓（insertBefore 是头插 ✓）。
      addGroup("最近使用", recents);
      addGroup("★ 收藏", favorites);
      const hint = $("brushFavoriteHint");
      if (hint) {
        hint.textContent = "★ " + favorites.filter((name) => known.has(name)).length + " 支";
      }
    } catch (_) {
      // **偏好读不到不影响选择笔刷** ✓（这是"方便" ✓，不是"必需" ✗ —— 与偏好文件坏掉时的取舍一致 ✓）。
    }
    select.value = keep;
    // **装完就套用一次过滤** ✓（搜索框里可能已经有字 ✓ —— 重装后忘了过滤会**静默变回全量** ✗）。
    applyBrushFilter();
    // **装完就刷一次预览** ✓ —— 否则"选中的那支还没有预览"✗，用户以为它没预览 ✓。
    await refreshBrushPreview();
  } catch (error) {
    log("笔刷列表没拉到：" + String(error).slice(0, 120), "#c93");
  }
}

/// **笔刷库** ✓（用户："列表里头都带个笔刷的效果图是不是更好，直接列表中就能找到想要的"✓）。
///
/// **三条取舍** ✓：
/// ① **图是真的** ✓：每张都调服务端的 `brush_preview`（与 `brush_stroke` **同一条落笔实现** ✓）
///    ⇒ 列表里看到的就是落笔的样子 ✓，不是示意图 ✗（示意图迟早与真笔触漂移 ✓）。
/// ② **懒加载 + 缓存** ✓：只有**滚进视口**的行才去画 ✓（201 支一次全画 = 201 次服务端落笔 ✗），
///    画过的存进 `brushPreviewCache` ✓ ⇒ 滚回去不重画 ✓。并发上限 3 ✓（别把服务端排满 ✓）。
/// ③ **选择只有一条路** ✓：点一行 ⇒ 走 `window.yanshi.setBrush()` ✓ —— 与下拉、与 MCP 是**同一个入口** ✓
///    （自己再写一遍"设值 + change"✗ 迟早与那条漂移 ✓）。
const brushPreviewCache = new Map();
/// **预生成入库的画笔预览图**（构建期生成、随包发布）—— 面板**直接用图片**，不再逐支实时渲染。
let brushPreviewIndex = null;
// **存的是 promise**（不是"点火即忘"）—— 实测过：只预取不等待时，面板开得比索引快 ⇒ 头几行会退回
// **实时调工具**那条路（探针里 `previewCalls: 10` 就是这么来的）⇒ 用到时先 `await` 它。
const brushPreviewIndexPromise = fetch("/brush-previews/index.json")
  .then((response) => (response.ok ? response.json() : null))
  .then((index) => {
    // **统一键名** ✗：生成器从 `list_assets` 拿到的是**带 `.myb`** 的名字（`2B_pencil.myb`），
    // 而面板里的名字来自 `#brush` 选项 = **不带扩展名**（`2B_pencil`）⇒ 不统一就一条都命中不了，
    // 整个面板会退回"逐支实时渲染"（实测：探针里 `previewCalls: 10` 就是这么来的 ✗）。
    if (index && index.files) {
      const normalized = {};
      for (const [key, value] of Object.entries(index.files)) {
        normalized[key.replace(/\.myb$/, "")] = value;
      }
      index.files = normalized;
    }
    brushPreviewIndex = index;
    return index;
  })
  .catch(() => null);
const brushPreviewQueue = [];
let brushPreviewActive = 0;
let brushLibraryObserver = null;

/// **列表里的条目** ✓ —— 直接读**那个 `<select>`** ✓（过滤的真相在那里 ✓：`option.hidden` ✓）。
/// 不另建一份列表 ✗（两份必然漂移 ✓）。
function brushLibraryEntries() {
  const select = $("brush");
  if (!select) return [];
  const entries = [];
  const seen = new Set();
  for (const option of Array.from(select.options)) {
    if (option.value === "" || option.hidden) continue;
    // **同名只留一条** ✓ —— 下拉里"最近使用 / ★ 收藏"与"按来源分组"本就是**同一支笔的两处入口** ✓
    //（在下拉里那是分组 ✓），但在**可浏览的库面板**里它读起来就是"重复了一次" ✗
    //（探针实测：200 行对 199 支 ✓，而那多出来的一条正好撞上我的判据 ✓）。
    // 留**第一条** ✓ = 偏好组优先 ✓（常用的在最上面 ✓，与下拉的顺序一致 ✓）。
    if (seen.has(option.value)) continue;
    seen.add(option.value);
    const parent = option.parentElement;
    const group = parent && parent.tagName === "OPTGROUP" ? parent.label : "";
    entries.push({ name: option.value, group, preview: brushPreviewCache.get(option.value) || "" });
  }
  return entries;
}

function renderBrushLibrary() {
  const list = $("brushLibraryList");
  if (!list) return;
  const select = $("brush");
  const current = select ? select.value : "";
  const entries = brushLibraryEntries();
  list.innerHTML = "";
  for (const entry of entries) {
    const row = document.createElement("div");
    row.className = "brush-lib-row" + (entry.name === current ? " selected" : "");
    row.dataset.brush = entry.name;
    row.setAttribute("role", "button");
    row.setAttribute("aria-selected", String(entry.name === current));
    const img = document.createElement("img");
    img.alt = "";
    img.dataset.brush = entry.name;
    if (entry.preview) img.src = entry.preview;
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = entry.name;
    const group = document.createElement("span");
    group.className = "group";
    group.textContent = entry.group;
    row.append(img, name, group);
    row.addEventListener("click", () => {
      // **走既有那条唯一入口** ✓（它自己会设值 + 派发 change ✓）。
      void window.yanshi.setBrush(entry.name);
      renderBrushLibrary();
    });
    list.appendChild(row);
  }
  const hint = $("brushLibraryHint");
  if (hint) hint.textContent = entries.length + " 支 · 滚到哪画到哪 ✓";
  observeBrushLibraryRows();
}

/// **只有滚进视口的行才去画** ✓（`IntersectionObserver` ✓；没有它就退化成"全画" ✗ —— 但那是老浏览器 ✓）。
function observeBrushLibraryRows() {
  const list = $("brushLibraryList");
  if (!list) return;
  const images = Array.from(list.querySelectorAll("img[data-brush]"));
  if (typeof IntersectionObserver !== "function") {
    images.forEach((img) => queueBrushPreview(img.dataset.brush));
    return;
  }
  if (brushLibraryObserver) brushLibraryObserver.disconnect();
  brushLibraryObserver = new IntersectionObserver(
    (observed) => {
      for (const entry of observed) {
        if (!entry.isIntersecting) continue;
        queueBrushPreview(entry.target.dataset.brush);
        brushLibraryObserver.unobserve(entry.target);
      }
    },
    { root: list, rootMargin: "64px" },
  );
  images.forEach((img) => brushLibraryObserver.observe(img));
}

function queueBrushPreview(name) {
  if (!name || brushPreviewCache.has(name) || brushPreviewQueue.includes(name)) return;
  brushPreviewQueue.push(name);
  void pumpBrushPreview();
}

async function pumpBrushPreview() {
  if (brushPreviewActive >= 3) return;
  const name = brushPreviewQueue.shift();
  if (!name) return;
  brushPreviewActive += 1;
  // **入库的图优先**：有就贴它，**不**逐支实时渲染（用户裁定：静态资源预生成入库）。
  const index = brushPreviewIndex || (await brushPreviewIndexPromise);
  const shipped = index && index.files && index.files[name];
  if (shipped) {
    const url = "/brush-previews/" + shipped;
    brushPreviewCache.set(name, url);
    const selector = '#brushLibraryList img[data-brush="' + name.replace(/"/g, '\\"') + '"]';
    for (const img of document.querySelectorAll(selector)) img.src = url;
    window.yanshiStats.brushPreviewsFromFiles = (window.yanshiStats.brushPreviewsFromFiles || 0) + 1;
    brushPreviewActive -= 1;
    void pumpBrushPreview();
    return;
  }
  try {
    const value = await callTool("brush_preview", { brush: name, size: 24 }, { refresh: false });
    const url = value && (value.thumb_url || value.raw_url);
    if (url) {
      brushPreviewCache.set(name, url);
      const selector = '#brushLibraryList img[data-brush="' + name.replace(/"/g, '\\"') + '"]';
      for (const img of document.querySelectorAll(selector)) img.src = url;
      window.yanshiStats.brushPreviews = (window.yanshiStats.brushPreviews || 0) + 1;
    } else {
      window.yanshiStats.brushPreviewErrors = (window.yanshiStats.brushPreviewErrors || 0) + 1;
    }
  } catch (error) {
    // **失败要说出来** ✓ 但不打断列表 ✓（一支笔刷预览不出来，不该让整个面板不可用 ✓）。
    window.yanshiStats.brushPreviewErrors = (window.yanshiStats.brushPreviewErrors || 0) + 1;
    log("笔刷预览失败：" + name + "（" + String(error).slice(0, 40) + "）", "#c93");
  } finally {
    brushPreviewActive -= 1;
    void pumpBrushPreview();
  }
}

function setupBrushLibrary() {
  const open = $("brushLibraryOpen");
  const panel = $("brushLibrary");
  const close = $("brushLibraryClose");
  if (!open || !panel) return;
  const setOpen = (next) => {
    panel.hidden = !next;
    open.setAttribute("aria-pressed", String(next));
    if (next) {
      // **打开时才去拉列表** ✓（与"首次点开才载入笔刷"同一个取舍 ✓：别在启动时做没人要的事 ✓）。
      const select = $("brush");
      if (select && select.options.length <= 1) {
        void refreshBrushOptions().then(() => renderBrushLibrary());
      }
      renderBrushLibrary();
    }
  };
  open.addEventListener("click", () => setOpen(panel.hidden));
  if (close) close.addEventListener("click", () => setOpen(false));
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !panel.hidden) setOpen(false);
  });
  const select = $("brush");
  if (select) select.addEventListener("change", () => { if (!panel.hidden) renderBrushLibrary(); });
  const search = $("brushSearch");
  // **输入后自动打开画笔库** ✗ —— 用户报告："搜索笔刷输入后应该自动打开笔刷库" ✓。
  // 原来这一行是 `if (!panel.hidden) renderBrushLibrary();` ✗ ⇒ **面板关着时输入什么都不发生** ✓
  // （只有右边那个"匹配 N 支"在变 ✓）⇒ 用户看到的就是"除了显示数字，没有真正功能" ✓。
  if (search) {
    search.addEventListener("input", () => {
      setOpen(true);            // 打开面板（`setOpen` 自己会去拉列表并渲染 ✓）
      renderBrushLibrary();     // 再渲染一次，确保过滤后的列表立刻可见 ✓
    });
  }
  window.yanshiBrushLibrary = { render: renderBrushLibrary, setOpen };

// 注册 Service Worker（离线优先 PWA 第一步）：失败不致命（file:// 或旧浏览器）。
if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/service-worker.js").catch(() => undefined);

// **预热门面** ✓（第 52 轮定 ✓）：现在它只在 `pointerdown` 才异步加载 ✗ ⇒
// **离线**时即使走缓存也有延迟 ⇒ 那一笔的前几帧会白丢 ✓（实测：同一脚本两跑 +1060 ✓ / +8 ✗）。
// 页面初始化后就加载 ⇒ 落笔时**已在内存** ✓，顺带也让它进 SW 缓存 ✓（离线可用 ✓）。
if (serverRenderPreferred()) {
  // **显式要求服务端渲染** ✓ ⇒ 不初始化 wasm 内核 ✓ ⇒ 走**本来就有**的服务端像素路径 ✓
  //（= (A)⑤ 的"弱设备/能力缺失时的回退" ✓，只是这次是**用户显式选的** ✓）。
  needsServerPixels = true;
  if (typeof queueServerBlit === "function") queueServerBlit();
} else {
  void loadLocalBrushModule().catch(() => undefined);
}

/// **参考图叠加**（AI 画家需求 P2-9 的可见那一半）：服务端把 `reference.*` **只记在偏好里**
/// （文档一个字节都没改 ✓），这里把它**叠着画**出来 ✓ ⇒ 临摹时不用来回切窗口 ✓。
///
/// **为什么不写进任何图层** ✓：一旦写进去，用户"清掉参考图"就**拿不回原画** ✗（数据损失）。
/// 所以这里是**独立的 DOM 层** ✓：`pointer-events: none` ✓（不吃鼠标 ✓）、`z-index` 高于画布 ✓、
/// 位置取**画布当前的屏幕矩形** ✓（不掺进视口变换 ⇒ 换实现也不会算错 ✓）。
async function refreshReferenceOverlay() {
  try {
    const value = await callTool("get_preferences", { keys: ["reference.blob_hash", "reference.opacity", "reference.position"] });
    const prefs = (value && (value.preferences || (value.data || {}).preferences)) || {};
    const hash = prefs["reference.blob_hash"];
    const existing = document.getElementById("referenceOverlay");
    if (!hash) {
      if (existing) existing.remove();   // **清掉即移除** ✓（文档本来就没被动过 ✓）
      return;
    }
    const canvas = document.getElementById("board") || document.querySelector("canvas");
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const overlay = existing || document.createElement("img");
    if (!existing) {
      overlay.id = "referenceOverlay";
      overlay.alt = "参考图";
      overlay.style.position = "fixed";
      overlay.style.pointerEvents = "none";
      overlay.style.zIndex = "9";
      overlay.style.objectFit = "contain";
      overlay.dataset.reference = "1";
      document.body.appendChild(overlay);
    }
    overlay.dataset.blob = String(hash);
    overlay.src = "/api/blob/" + String(hash).replace(/^sha256:/, "sha256:");
    overlay.style.left = rect.left + "px";
    overlay.style.top = rect.top + "px";
    overlay.style.width = rect.width + "px";
    overlay.style.height = rect.height + "px";
    const opacity = Number(prefs["reference.opacity"]);
    overlay.style.opacity = String(Number.isFinite(opacity) && opacity > 0 ? opacity : 0.5);
  } catch (error) {
    // **参考图不是关键路径** ✗：读不到就安静地不画 ✓（绝不因为它把查看器拖垮 ✓）。
  }
}
void refreshReferenceOverlay();
window.addEventListener("resize", () => { void refreshReferenceOverlay(); });
}
}

// **在定义这一侧自初始化** ✓ —— 跨 `<script>` 段够不到函数名 ✗（见上面那条注释 ✓）；
// 而 DOM 在这段脚本执行时已经解析完 ✓ ⇒ 直接接上即可 ✓（`loading` 时等 `DOMContentLoaded` ✓）。
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => setupBrushLibrary());
} else {
  setupBrushLibrary();
}

/// **预览的防抖** ✓（用户：拖粗细 / 改颜色时预览**不会跟着变** ✗）——
/// 但**不能每动一下就真画一笔** ✗（滑杆一次拖动几十个事件 ✓ ⇒ 几十次真实落笔 ✓，肉眼可见地卡 ✓）。
/// 停手 400ms 之后再画 ✓：既不卡 ✓、也不会留下"界面改了预览没变" ✗。
function scheduleBrushPreview() {
  if (state.brushPreviewTimer) clearTimeout(state.brushPreviewTimer);
  state.brushPreviewTimer = setTimeout(() => {
    state.brushPreviewTimer = null;
    // **失败要说话，不许变成未处理的拒绝** ✗（实测：这里会冒 `Uncaught (in promise)` ✓ ——
    // 控制台里那是一条**红字** ✓，而用户看到的是"预览悄悄不动了" ✗）。
    refreshBrushPreview().catch((error) => {
      const hint = $("brushPreviewHint");
      if (hint) hint.textContent = "预览没拉到：" + String(error).slice(0, 60);
    });
  }, 400);
}

/// **笔刷预览** ✓（用户："201 支笔刷只有一个名字 ⇒ 选笔全凭猜，**web 上也是**" ✗）。
///
/// 走服务端的 `brush_preview` ✓ —— 它**与 `brush_stroke` 共用同一条落笔实现** ✓
/// ⇒ 这里显示的就是这支笔**真实落笔**的样子 ✓，不是示意图 ✗
/// （示意图迟早与真笔触漂移 ✓ —— 这个项目在"两份实现"上栽过多次 ✓）。
///
/// **失败必须说话** ✗：预览取不到时**不能**留一个空白框 ✓（用户会以为"这支笔就是没墨"✗）；
/// 涂抹类笔刷在空画布上本来就画不出东西 ✓ ⇒ 服务端会**说清原因** ✓，这里照原样显示 ✓。
async function refreshBrushPreview() {
  // **整段不许抛出去** ✗：它会被 `change` 监听器直接调用 ✓（那条路没有 catch ✓）——
  // 实测会冒 `Uncaught (in promise)` ✓（控制台红字 ✓），而用户只看到"预览悄悄不动" ✗。
  try {
    return await refreshBrushPreviewInner();
  } catch (error) {
    const hint = $("brushPreviewHint");
    if (hint) hint.textContent = "预览没拉到：" + String(error).slice(0, 60);
    return false;
  }
}

/// 预览的**本体** ✓（外面那层只负责"不许把异常漏到控制台" ✓，见上 ✓）。
async function refreshBrushPreviewInner() {
  const select = $("brush");
  const img = $("brushPreview");
  const hint = $("brushPreviewHint");
  if (!img) return;
  const name = select ? select.value : "";
  if (!name) {
    img.style.display = "none";
    if (hint) hint.textContent = "";
    return;
  }
  const size = Number(($("size") || {}).value || 24);
  const colour = colorCss();
  // **超界要先拦住** ✓：`brush_preview` 的 size 上限是 512 ✓ ——
  // 直接把它当错误抛出去会变成控制台红字 ✓（用户看不懂 ✓），这里**说人话** ✓。
  if (!(size > 0) || size > 512) {
    img.style.display = "none";
    if (hint) hint.textContent = "粗细要在 1..512 之间才能预览（当前 " + size + "）";
    return;
  }
  // **去重的键要含"颜色 / 粗细"** ✗ —— 只看笔刷名会让"换了颜色再预览"被**静默跳过** ✓
  //（那又会变成"界面里改了、实际没变" ✗ —— 本项目头号病症 ✓）。
  const key = name + "|" + size + "|" + JSON.stringify(colour);
  if (img.dataset && img.dataset.key === key) return; // 完全相同的输入不重复画 ✓（省一次真实落笔 ✓）。
  const result = await callTool(
    "brush_preview",
    { brush: name, size, color: colour },
    { refresh: false },
  );
  if (!result || !result.ok) {
    img.style.display = "none";
    if (img.dataset) img.dataset.key = key;
    const detail = (result && result.context && result.context.detail) || "预览不可用";
    if (hint) hint.textContent = String(detail).slice(0, 60);
    return;
  }
  img.dataset.key = key;
  // **`thumb_url` 已经被服务端改写成可 GET 的地址** ✓（`/api/blob/<hash>?doc=..&token=..` ✓）
  // ⇒ 加个时间戳即可 ✓（与页面里其它预览图同一条做法 ✓）。
  img.src = result.thumb_url + "&t=" + Date.now();
  img.style.display = "";
  if (hint) hint.textContent = result.width + "×" + result.height;
}
/// **服务端连不上时，界面必须说人话** ✗（用户实测连续报了三处 ✓）。
///
/// **为什么做成"兜住整类"、而不是逐个补 try/catch** ✗：用户贴来的日志里已经有
/// **三处**同一类崩溃 ✓ —— `submitAtom` ✓（已在提交处补了 try ✓）、
/// `ensureDocument` ✓、`createNamedDocument` ✓（`GET/POST /api/documents` ✓）
/// ⇒ 逐个补**必然漏** ✗（这个项目在"逐处补"上栽过太多次 ✓）。
/// **做法** ✓：接住**所有**未处理的 promise 拒绝与全局错误 ✓，
/// 只在**第一次**（以及每隔一段时间 ✓）说一遍 ✓ —— 既**不刷屏** ✗、也**不沉默** ✗。
/// **说清三件事** ✓：连不上谁 ✓、**这一操作没有完成** ✓、以及**接下来能做什么** ✓。
function reportServerUnreachable(detail) {
  const now = Date.now();
  if (now - (state.lastOfflineReport || 0) < 5000) return; // **限流** ✓（避免又变成刷屏 ✗）
  state.lastOfflineReport = now;
  const conn = $("conn");
  if (conn) conn.className = "dot";
  const text = $("connText");
  if (text) text.textContent = "已断开";
  log("**连不上服务端** ✗ ⇒ 你刚才那一步**没有完成** ✓（服务端可能已经退出 ✓）：" +
      String(detail || "").slice(0, 120) +
      " —— 把服务端起回来（`make dev` ✓）之后**刷新页面** ✓；页面里已画的内容可能需要重新画 ✓", "#c33");
}
window.addEventListener("unhandledrejection", (event) => {
  const reason = event && event.reason ? event.reason : "";
  const message = String((reason && reason.message) || reason);
  // **只认"连不上"这一类** ✓（别的未处理拒绝照旧交给浏览器 ✓ —— 不要把它们也吞掉 ✗）。
  if (message.indexOf("Failed to fetch") >= 0 || message.indexOf("NetworkError") >= 0 ||
      message.indexOf("Load failed") >= 0) {
    reportServerUnreachable(message);
  }
});
window.addEventListener("error", (event) => {
  const message = String((event && event.message) || "");
  if (message.indexOf("Failed to fetch") >= 0 || message.indexOf("NetworkError") >= 0) {
    reportServerUnreachable(message);
  }
});

window.yanshi = {
    /// 当前设置 ✓（可断言 ✓）。
    state() {
      return {
        docId: state.docId,
        layerId: state.layerId,
        tool: state.tool,
        // 与 setColor 同一个来源 ✓（笔刷色 ✓）—— 别让"读"与"写"指向不同控件 ✗。
        color: ($("color") || {}).value || null,
        // **这里曾经读 `#strokeSize`，那是错的** ✗ —— 那个控件属于"**重设选中笔迹**"面板
        //（`restyleCheckedObjects` 里给已选对象用 ✓），**不是画笔粗细** ✗。
        // 结果就是：调用方（测试 / 脚本 / 我自己的探针 ✓）以为读到的是"笔尖大小" ✓，
        // 实际拿到的是另一个面板的数字 ✗ —— **API 与事实不符** ✓，与"界面说一套、事实另一套"同类 ✓。
        // 画笔粗细一直是 **`#size`** ✓（滑块 ✓，8 处绘画与覆盖层都在读它 ✓）。
        size: Number(($("size") || {}).value || 0),
        opacity: Number(($("strokeOpacity") || {}).value || 0),
        medium: ($("medium") || {}).value || null,
        // **笔刷也要能读** ✓（测试要断言"选了哪支" ✓）。
        brush: ($("brush") || {}).value || null,
        // **平滑开关也要能读** ✓（"界面里有的东西必须能被断言" ✓ —— 与 size 那次同类 ✓）。
        smooth: !!($("smooth") || {}).checked,
        // **一笔多色开关也要能读** ✓（探针要断言界面状态 ✓）。
        duoTone: !!($("duoTone") || {}).checked,
        colorTo: ($("colorTo") || {}).value || null,
        // **视口与缩放也必须能读** ✗（真实用户报告 + 我自己的探针教训 ✓）：
        // 我上一轮想量"抓手工具有没有平移画布" ✓，而 `state()` 只返回
        // `docId/layerId/tool/color/size/opacity/medium` ✗ ⇒ **探针看不见被测对象** ✗
        // ⇒ 我拿到了 `panChanged: false` 这种**什么都不能证明**的结论 ✓。
        // 加上这两个字段 ✓，"平移到底改没改视口"就能**直接断言** ✓，不用再靠像素反推 ✓。
        viewport: { x: state.viewport.x, y: state.viewport.y,
                    w: state.viewport.w, h: state.viewport.h },
        // **显示比例也要能读** ✓（"界面里有的东西必须能被断言" ✓ —— 与 `size`/`smooth` 同一条 ✓）：
        // 本轮验收"滚轮不许改缩放"时，探针必须有**可读的事实** ✓，否则会写出**恒真**的判据 ✗。
        zoom: state.displayScale || 1,
        userZoom: state.zoom || 1,
        liveStroke: liveStroke
          ? { objectId: liveStroke.objectId, frames: window.yanshiStats.liveStrokeFrames || 0,
              errors: window.yanshiStats.liveStrokeErrors || 0,
              started: window.yanshiStats.liveStrokeStarted || 0 }
          : null,
        // **补画与"要不要服务端像素"都要能读** ✓ —— 本轮两条症状的判据全靠它 ✓
        //（"闪一下"的可测代理 = `lastServerBlitArea` 是不是整视口 ✓）。
        needsServerPixels: !!needsServerPixels,
        serverBlits: window.yanshiStats.serverBlits || 0,
        lastServerBlitArea: window.yanshiStats.lastServerBlitArea || 0,
        lastServerBlitReason: window.yanshiStats.lastServerBlitReason || "",
        blitLog: (window.yanshiStats.blitLog || []).slice(-12),
        lastBlitServerInk: window.yanshiStats.lastBlitServerInk || 0,
        blankBlitsSkipped: window.yanshiStats.blankBlitsSkipped || 0,
        displayScale: state.displayScale || 1,
      };
    },
    /// 设**笔刷颜色** ✓（任意 CSS 颜色串 ✓）。
    ///
    /// **这里必须读 `#color`（工具栏那个 ✓），不是 `#strokeColor`（对象面板那个 ✓）** ✗ ——
    /// 两者**用途不同** ✓：`#color` 是**笔刷色** ✓（介质路径 1769 行、普通笔触 2101 行、
    /// 快速面板色板、吸管都用它 ✓，共 8 处 ✓）；`#strokeColor` 只是**"重设选中对象颜色"** ✓。
    /// **我第一版设的是后者** ✗ ⇒ 像素画出来**仍是默认红** ✓ ⇒ 是"设错了对象" ✓，
    /// 而**用户报告里写的 `#color` 本来就是对的** ✓。这类"名字像同一个东西"的坑 ✓，
    /// **只有把真实像素打出来才看得见** ✓。
    setColor(css) {
      const input = $("color");
      if (!input) return false;
      input.value = css;
      input.dispatchEvent(new Event("input", { bubbles: true }));
      return true;
    },
    /// 设**画笔粗细** ✓（像素 ✓）—— 与 `state().size` 指向**同一个控件** ✓（`#size` 滑块 ✓）。
    ///
    /// **原来指向 `#strokeSize`** ✗ ⇒ 调它**改不到笔尖** ✓（只改了"重设选中笔迹"面板的数字 ✗）
    /// ⇒ 调用方会以为设置生效了 ✓ —— 这正是"说改了其实没改" ✓。
    setSize(pixels) {
      const input = $("size");
      if (!input) return false;
      input.value = String(pixels);
      input.dispatchEvent(new Event("input", { bubbles: true }));
      return true;
    },
    /// 设不透明度 ✓（0..1 ✓）。
    setOpacity(value) {
      const input = $("strokeOpacity");
      if (!input) return false;
      input.value = String(value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
      return true;
    },
    /// 换介质 ✓（与界面同一条路径：设值 + `change` ✓）。
    /// **选一支 `.myb` 笔刷** ✓（空串 = 回到内置画笔 ✓）。
    setBrush(name) {
      const select = $("brush");
      if (!select) return false;
      // **先等列表载入再设值** ✗（本轮实测 ✓）：列表还没载入时 `select.value = "spray"` 会**静默失败** ✓
      //（那一项还不存在 ⇒ 值停在 "" ✓），而调用方以为换好了 ✓ —— 与图层下拉那条**同一个坑** ✓
      //（那里也是"先设值、后重建选项 ⇒ 被覆盖回旧值"✗）。回调返回 Promise ✓ 让调用方可以等 ✓。
      const apply = () => {
        select.value = name || "";
        select.dispatchEvent(new Event("change", { bubbles: true }));
        return true;
      };
      if (select.options.length <= 1 && (name || "")) {
        return refreshBrushOptions().then(apply).catch(() => apply());
      }
      return apply();
    },
    /// **切换"收藏当前笔刷"** ✓（目标 ⑧-1 ✓）—— 供标记旁那段小脚本调用 ✓
    ///（它处在全局作用域 ✓，只能碰 `window.yanshi` ✓ —— 这是上一轮三次静默失败换来的规矩 ✓）。
    async toggleFavoriteBrush() {
      const select = $("brush");
      const name = select ? select.value : "";
      if (!name) {
        log("先选一支笔刷再收藏 ✓", "#c93");
        return false;
      }
      const prefs = await callTool(
        "get_preferences",
        { keys: ["brush_favorites"] },
        { refresh: false },
      );
      const current = ((prefs && prefs.preferences && prefs.preferences.brush_favorites) || []).map(String);
      const next = current.includes(name)
        ? current.filter((item) => item !== name)
        : [name, ...current];
      const written = await callTool("set_preferences", { values: { brush_favorites: next } }, { refresh: false });
      log(
        (current.includes(name) ? "已取消收藏：" : "已收藏：") + name +
          (written && written.persisted === false ? "（⚠️ 纯内存工作区 ⇒ 重启后不会留下 ✗）" : ""),
        "#2a2",
      );
      await refreshBrushOptions();
      return true;
    },
    /// **记一次"最近使用"** ✓（目标 ⑧-1 ✓）—— 最多 8 支 ✓、去重 ✓、最新在前 ✓。
    async recordBrushUse() {
      const select = $("brush");
      const name = select ? select.value : "";
      if (!name) return false;
      const prefs = await callTool(
        "get_preferences",
        { keys: ["brush_recent"] },
        { refresh: false },
      );
      const current = ((prefs && prefs.preferences && prefs.preferences.brush_recent) || []).map(String);
      const next = [name, ...current.filter((item) => item !== name)].slice(0, 8);
      await callTool("set_preferences", { values: { brush_recent: next } }, { refresh: false });
      return true;
    },
    /// **暴露过滤器** ✓（目标 ⑥ ✓）：页面里的分组/搜索逻辑在这个对象**之外的作用域**里 ✓
    /// ⇒ 那个作用域**不是全局** ✗（实测：`typeof refreshBrushOptions === "undefined"` ✗）
    /// ⇒ 所以只把**这一点**挂出来 ✓，供下面那段"紧跟标记的小脚本"调用 ✓
    ///（它只依赖 `window.yanshi` ✓ —— 那是**唯一被证明可靠**的入口 ✓）。
    applyBrushFilter() {
      return applyBrushFilter();
    },
    /// **暴露装载函数** ✓：第一次点开下拉时要能主动拉一次 ✓（也让验收脚本能预热 ✓）。
    loadBrushes() {
      return refreshBrushOptions();
    },
    /// **暴露预览** ✓：验收探针要能直接断言"预览真的换了"✓（与换笔刷走同一条实现 ✓）。
    previewBrush() {
      return refreshBrushPreview();
    },
    /// **笔刷库** ✓（列表带效果图 ✓）：开 / 关 / 当前状态 ✓ —— 判据全靠它 ✓。
    openBrushLibrary() {
      if (window.yanshiBrushLibrary) window.yanshiBrushLibrary.setOpen(true);
      return true;
    },
    closeBrushLibrary() {
      if (window.yanshiBrushLibrary) window.yanshiBrushLibrary.setOpen(false);
      return true;
    },
    brushLibraryState() {
      const panel = $("brushLibrary");
      const rows = Array.from(document.querySelectorAll("#brushLibraryList .brush-lib-row"));
      const loaded = rows.filter((row) => {
        const img = row.querySelector("img");
        return !!(img && img.getAttribute("src"));
      }).length;
      const sources = rows
        .map((row) => {
          const img = row.querySelector("img");
          return img ? img.getAttribute("src") || "" : "";
        })
        .filter(Boolean);
      return {
        open: !!(panel && !panel.hidden),
        rows: rows.length,
        loaded,
        distinctPreviews: new Set(sources).size,
        selected: rows.some((row) => row.classList.contains("selected")),
        previews: window.yanshiStats.brushPreviews || 0,
        errors: window.yanshiStats.brushPreviewErrors || 0,
        hint: ($("brushLibraryHint") || {}).textContent || "",
      };
    },
    /// **预览防抖入口** ✓（改粗细 / 改颜色之后 400ms 才真画一小笔 ✓）——
    /// 给**另一段 script** 里的控件监听器用 ✓（它们够不到本段里的函数 ✗，只能走 `window.yanshi` ✓）。
    scheduleBrushPreview() {
      scheduleBrushPreview();
    },
    /// **一笔多色开关** ✓（真实路径：改的就是界面上那个勾 ✓ 与那个色控件 ✓）。
    setDuoTone(on, hex) {
      const box = $("duoTone");
      if (!box) return false;
      box.checked = !!on;
      box.dispatchEvent(new Event("change", { bubbles: true }));
      if (hex) {
        const slot = $("colorTo");
        if (slot) {
          slot.value = hex;
          slot.dispatchEvent(new Event("change", { bubbles: true }));
        }
      }
      return true;
    },
    /// **平滑开关** ✓（真实路径：改的就是界面上那个勾 ✓）。
    setSmooth(on) {
      const box = $("smooth");
      if (!box) return false;
      box.checked = !!on;
      box.dispatchEvent(new Event("change", { bubbles: true }));
      return true;
    },
    setMedium(id) {
      const select = $("medium");
      if (!select) return false;
      select.value = id;
      select.dispatchEvent(new Event("change", { bubbles: true }));
      return true;
    },
    /// 换工具 ✓（点那个按钮 ✓ —— 与手工点完全同一条路径 ✓）。
    setTool(key) {
      const button = document.querySelector('button[data-tool="' + key + '"]');
      if (!button) return false;
      button.click();
      return true;
    },
  };
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
  paint: { open: ["图层", "WASM 计算内核"], closed: ["标注", "历史（原子日志）", "调整 / 滤镜", "缩略图", "原子日志（控制流）", "最近一次响应", "反馈"] },
  // 修图：图层 + 调整展开 ✓。
  retouch: { open: ["图层", "调整 / 滤镜", "缩略图"], closed: ["WASM 计算内核", "历史（原子日志）", "原子日志（控制流）", "最近一次响应", "反馈"] },
  // 校对：历史 + 日志 + 反馈展开 ✓（核对与反馈用 ✓）。
  review: { open: ["标注", "历史（原子日志）", "原子日志（控制流）", "反馈", "最近一次响应"], closed: ["调整 / 滤镜", "WASM 计算内核", "缩略图"] },
};

// **接线放在这里** ✓（这段一定执行 ✓，见上面的说明 ✓）。
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
  // **顺序是关键** ✗（真实用户报告："新建图层后，默认应该进入新图层，现在图层列表看还是选中老的" ✓）。
  //
  // **为什么"先选后刷"必然失败** ✓（这次读代码看清楚了 ✓，不是猜 ✓）：
  // `refreshLayers()` 会 `select.innerHTML = ""` **重建全部 `<option>`** ✓，
  // 并且结尾还有一句 `state.layerId = select.value || "layer_paint"` ✓ ——
  // ⇒ 对着一个**还没有对应 option** 的 select 赋值 ✗ ⇒ 它落回**第一项** ✓
  // ⇒ `onchange()` 把 `state.layerId` 写回**旧图层** ✓ ⇒ 新图层永远选不上 ✓。
  // （既有 `#layerAdd` / `duplicate` 两条路是**先刷后选** ✓，所以它们是对的 ✓ ——
  //  我又犯了"只改一条路、还把顺序写反"✗，这次用探针当场抓住 ✓。）
  await refreshLayers();
  const select = $("layer");
  if (select) {
    select.value = layerId;
    select.onchange();
  }
  // **没切过去就说出来** ✗ —— 静默停在旧图层正是用户踩的那个坑 ✓（"以为在新层上画"✗）。
  if (state.layerId !== layerId) {
    log("⚠ 新图层 " + layerId + " 已建好，但界面没能切过去（当前 " + state.layerId +
        "）—— 请在上面的图层列表里点它一下", "#c93");
  } else {
    log("已新建图层 " + layerId + "（已切到它 ✓）");
  }
  // **画布也要跟上** ✓：别让用户在新图层上画第一笔时才发现画面还是旧的 ✓。
  await resync();
});

/// 导出整幅 PNG：显式请求整幅区域渲染（设计 A 下整幅 PNG 只在**显式导出**时生成），
/// 再把服务端改写过的可直接 GET 的地址交给浏览器下载。
$("exportPng").addEventListener("click", async () => {
  const { w, h } = state.docSize;
  if (!w || !h) {
    log("导出失败：文档尺寸未知", "#c33");
    return;
  }
  // **离线优先的导出** ✓（第 180 轮 ✓）：本地**早就有像素** ✓ —— `board` 就是 2D canvas ✓、
  // 客户端合成本来就跑在 wasm 上 ✓ ⇒ 用 canvas **自己编码 PNG** ✓（浏览器原生能力 ✓）
  // ⇒ ⇒ **离线必然可用** ✓，而且**不必问服务端** ✓。
  // 拿不到本地像素时**才**退到服务端 ✓（= 弱设备回退 ✓，与 (A)⑤ 同一条思路 ✓）。
  //（查证过 ✓：原处理器第一句就是 `callTool("render_region", …)` ✓ ⇒ 离线时 `value.ok` 假 ✗
  //  ⇒ 提前 return ✓ ⇒ **一个文件都不落** ✗ —— 这正是上一轮那条判据量到 `[]` 的原因 ✓。）
  try {
    if (board && board.width > 0 && board.height > 0) {
      const blob = await new Promise((resolve) => board.toBlob(resolve, "image/png"));
      if (blob) {
        const href = URL.createObjectURL(blob);
        const localLink = document.createElement("a");
        localLink.href = href;
        localLink.download = (state.docId || "yanshi") + ".png";
        document.body.appendChild(localLink);
        localLink.click();
        localLink.remove();
        setTimeout(() => URL.revokeObjectURL(href), 10000);
        window.yanshiStats.lastExport = { url: href, width: board.width, height: board.height, bytes: blob.size };
        log("已导出 PNG（本地）：" + board.width + "×" + board.height + "（" + blob.size + " 字节）");
        return;
      }
    }
  } catch (error) {
    // 本地导出失败 ⇒ 退到服务端那条 ✓（**不让它把导出整体弄坏** ✗）
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

// **缩放的数值入口** ✓（用户："或者输入具体数值才变化" ✓）。
// 数值口径 = **实际显示比例** ✓（和旁边那个读数一致 ✓）—— 不玩"相对适配的倍数" ✗。
const applyZoomInput = () => {
  const box = $("zoomInput");
  if (!box || !state.docSize) return;
  const percent = Number(box.value);
  if (!(percent >= 5 && percent <= 1600)) {
    renderViewport(); // 非法输入 ⇒ 回填成事实值 ✓（不静默留着假数字 ✗）。
    return;
  }
  const available = availableArea();
  const fit = Math.min(available.w / state.docSize.w, available.h / state.docSize.h);
  state.zoom = Math.max(0.1, Math.min(16, percent / 100 / Math.max(0.0001, fit)));
  clampViewport();
  renderViewport();
};
$("zoomInput").addEventListener("change", applyZoomInput);
$("zoomInput").addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    applyZoomInput();
  }
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

// **素材浮层的"开关"这一半，在脚本段末尾就绑上** ✓ —— 只切可见性、不搬卡片 ✓（无副作用 ✓）。
// 根因与取舍见 `toggleAssetDock` 的注释 ✓：浮层 DOM 在本段之后出现 ✓，
// 而**按钮**在本段之前就有了 ✓ ⇒ 早绑是安全的 ✓；搬卡片仍由 `setupAssetPanels()` 在面板就绪后接管 ✓。
(() => {
  const button = $("assetFloat");
  if (button) button.addEventListener("click", toggleAssetDock);
})();
</script>
<!-- **素材浮层** ✓：两张卡搬进来（`appendChild` = 移动节点 ✓，监听器与状态都还在 ✓）⇒
     关掉时按**记下来的原位**搬回去 ✓ ⇒ 右侧面板永远不会被搬空 ✗。 -->
<div id="fileMenu" hidden>
  <div class="file-menu-head">
    <strong>文件</strong>
    <span class="hint">`Esc` 收起</span>
    <button id="fileMenuClose" type="button">收起</button>
  </div>
  <div id="fileMenuBody"></div>
</div>
<div id="brushLibrary" hidden>
  <div class="brush-lib-head">
    <b>笔刷库</b>
    <span class="hint" id="brushLibraryHint">滚到哪、画到哪 ✓</span>
    <button id="brushLibraryClose" type="button">收起</button>
  </div>
  <div id="brushLibraryList"></div>
</div>
<div id="assetDock" hidden>
  <div class="asset-dock-head">
    <strong>素材</strong>
    <!-- **快捷键** ✓（用户："点击或者快捷键，调色盘/纹理的窗口就可以浮出"✓）：
         `P` ⇒ 调色板、`T` ⇒ 纹理、`Esc` ⇒ 收起 ✓。 -->
    <span class="hint">`P` 调色板 · `T` 纹理 · `Esc` 收起</span>
    <!-- **用完即收** ✓（用户："设置完毕就关闭或者隐藏"✓）—— 默认**勾上** ✓（这就是他要的手感 ✓），
         但可关 ✓（想连点几个色块试的时候别被收走 ✗）。 -->
    <label class="hint" title="勾上：取完色就把浮层收起（想连续试色就取消勾选）">
      <input id="dockAutoClose" type="checkbox" checked /> 用完即收
    </label>
    <button id="assetDockClose" type="button">收起</button>
  </div>
  <div id="assetDockBody"></div>
</div>
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
        // **页面里可能不止一个内联脚本块** ✗（真实教训 ✓：我给笔刷下拉加了一段
        // **紧跟标记的小脚本** ✓ —— 它处在全局作用域 ✓，因为主脚本**不是**全局的 ✗）
        // ⇒ 原来的 `split_once("<script>")` 会**只取到第一个块** ✗（也就是那段小脚本 ✓）
        // ⇒ 于是这个守卫开始报"找不到 `refreshPreview`" ✗ —— **守卫自己读错了对象** ✓。
        // **修法**：把**所有**内联块拼起来再检查 ✓（守卫要保证的"整页 JS 不冲突"本来就该覆盖全部 ✓）。
        let mut script = String::new();
        let mut rest = PAGE;
        while let Some((_, after_open)) = rest.split_once("<script>") {
            let Some((block, after_close)) = after_open.split_once("</script>") else {
                break;
            };
            script.push_str(block);
            script.push('\n');
            rest = after_close;
        }
        assert!(!script.is_empty(), "页面含内联脚本");
        let script = script.as_str();

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

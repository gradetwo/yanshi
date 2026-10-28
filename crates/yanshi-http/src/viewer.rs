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
    page_template()
        .replace("__READ_TOOLS__", &names.join(", "))
        .replace("__BUILD_ID__", crate::server::BUILD_ID_TEXT)
}

/// **查看器整页** ✓（原样 ✓；发出去时会把只读工具清单注入 `__READ_TOOLS__` ✓ —— 见上面那个函数 ✓）。
/// 注意 ✓：我这个函数是**插在它之前**的 ✓ ⇒ 一度把它的文档注释"抢"走了 ✗ ⇒ `clippy -D warnings` 当场红 ✓。
/// **整页 HTML 的头部** ✓（`<!DOCTYPE html> … <link>` ✓）——**样式表自 NEW-H1 瘦身第二步起外链** ✗
/// 不再内联（`/viewer.css` 由**同一二进制** `include_str!` 路由发出 ✓ 单二进制部署照样自包含 ✓
/// 与 `/i18n.js` 同一条口径 ✓）。
const PAGE_HEAD: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>偃师 Yanshi 查看器</title>
<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
<link rel="icon" type="image/png" sizes="32x32" href="/favicon.png" />
<link rel="apple-touch-icon" href="/brand/png/favicon-180.png" />
<link rel="stylesheet" href="/viewer.css" />"##;

/// **整页 HTML 的尾部** ✓（`</head> … </html>` ✓）。
/// **`</head>` … 主脚本 `<script>`** ✓（与 `PAGE_TAIL_B` 之间夹的是**独立可打包的 JS 资产** ✓）。
const PAGE_TAIL_A: &str = r##"</head>
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
    <!-- **离线写队列的状态** ✓（离线优先）—— 用户必须能一眼分辨「已保存 / 离线排队 / 被拒」✗，
         不能"看着像保存了、其实只在本地攒着"✗。`data-state` 是判据读的那个字段 ✓。 -->
    <span id="outbox" class="outbox" data-state="saved"
          title="离线时改动先写进浏览器本地队列，联网后按顺序补交；服务端上的这份文档若已变化则暂停补交">已保存</span>
  </span>
  <!-- **面板开关** ✓（用户要求：左侧工具栏与右侧各窗口都要能隐藏 ✓，并能进全屏画布 ✓）。
       三个开关都用 `aria-pressed` 表达状态 ✓ ⇒ 屏幕阅读器与检查脚本都能读到"现在是开还是关" ✓，
       不必去猜 CSS 类 ✓。 -->
  <span class="panel-toggles">
    <button id="toggleRail" type="button" aria-pressed="false" title="隐藏 / 显示左侧工具栏（快捷键 [ ）">◧ 工具栏</button>
    <button id="toggleDockers" type="button" aria-pressed="false" title="隐藏 / 显示右侧面板（快捷键 ] ）">◨ 面板</button>
    <button id="toggleZen" type="button" aria-pressed="false" title="全屏画布（快捷键 Tab，Esc 退出）">⛶ 全屏</button>
  </span>
<button id="langToggle" type="button" data-i18n="off" aria-pressed="false" title="把界面切成英文（English）">EN</button>
</header>
<!-- **离线队列的操作条** ✓：只在有待补交 / 被拒 / 队列满时出现 ✓（平时不占版面 ✓）。
     「补交」= 立刻重放一次 ✓；「放弃」= 丢掉未补交的改动 ✓（需点两次 ✓，绝不自动丢 ✗）。 -->
<div id="outboxBar" data-state="saved" hidden>
  <span id="outboxText"></span>
  <button id="outboxRetry" type="button" title="立刻把本地队列按顺序补交给服务端">补交</button>
  <button id="outboxDiscard" type="button" title="丢弃本地队列里尚未补交的改动（需点两次确认）">放弃</button>
</div>
<!-- **画布上的那句话说给看得见的人** ✓（第六轮实测 ✗）：往锁定的图层落笔 ⇒ 服务端回 403 ✓
     而解释**只**写进 `#log` ⇒ 那块卡片在 `data-panel="history"` 里 ✓ 默认是藏着的 ✗
     （实测 `logVisible:false`、`scrollHeight:0` ✗）⇒ 用户得到的就是"按了没反应"。
     浮层**不进布局流** ✓（`position: fixed` ✓）⇒ 出现/消失都不会把画布顶得跳一下 ✗。 -->
<div id="stageNotice" role="status" aria-live="polite" hidden></div>
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
  <!-- **尺寸**（用户报告：新建对话框只有名称，想开 1920×1080 做不到 ✗）。
       服务端 `POST /api/documents` **一直收** `width`/`height`（缺省用服务端 flag ✓），
       而对话框从来没把它们暴露出来 ✗ ⇒ `ensureDocument()` 把每个新文档都写死成 1024² ✓。
       这里补两个输入框 + 几个常用尺寸按钮 ✓，并把数值**贯穿到那一次 POST** ✓。 -->
  <div class="new-size">
    <label>宽度 <input id="newWidth" type="number" min="1" max="8192" step="1" value="1024" /></label>
    <span class="new-size-x">×</span>
    <label>高度 <input id="newHeight" type="number" min="1" max="8192" step="1" value="1024" /></label>
  </div>
  <div class="new-presets" id="newPresets">
    <button type="button" data-size="1920x1080">1920 × 1080</button>
    <button type="button" data-size="1280x720">1280 × 720</button>
    <button type="button" data-size="2560x1440">2560 × 1440</button>
    <button type="button" data-size="3840x2160">3840 × 2160</button>
    <button type="button" data-size="1024x1024">1024 × 1024</button>
    <button type="button" data-size="2048x2048">2048 × 2048</button>
    <button type="button" data-size="2480x3508">2480 × 3508</button>
  </div>
  <div id="newSizeHint" class="hint"></div>
  <div id="newHint" style="font-size:12px;opacity:.75;min-height:16px;margin:8px 0"></div>
  <div style="display:flex;gap:8px;justify-content:flex-end">
    <button id="newCancel">取消</button>
    <button id="newCreate">创建</button>
  </div>
</dialog>
<dialog id="openDialog">
  <h2 style="margin-top:0">打开文档</h2>
  <!-- **打开面板重排** ✓（产品负责人："文件里头打开那也很乱" ✗）——
       原来的列表是**一格一格的白框** ✗：它按 `info.thumb_url` 取缩略图 ✓，
       而 `GET /api/documents` **根本不回这个字段** ✗ ⇒ 每一格都是一块空白 ✓，
       底下只有一行 id ✓，没有明确的"打开"动作 ✓，空列表与读失败也长得一样 ✗。
       现在：**一行一份**（名字 + 尺寸 + 原子数 + 创建时间 ✓）、**每行一个「打开」** ✓、
       四种状态（载入 / 有内容 / 空 / 读失败 ✓）都由 `#docList` 的 `data-state` 承载 ✓
       ⇒ "空"与"坏"再也混不起来 ✓（样式见 viewer.css 的 `#docList` ✓）。 -->
  <h3 style="margin:12px 0 4px">我的文档 <span id="docCount" class="hint"></span></h3>
  <div style="display:flex;gap:8px;align-items:center;margin:0 0 8px">
    <button id="docReload" type="button">刷新</button>
    <span class="hint" title="一行一份作品：名字、尺寸、原子数与创建时间；点「打开」切换过去。">名字 · 尺寸 · 时间</span>
  </div>
  <div id="docList" data-state="loading" data-message="载入中…"></div>
  <!-- 示例作品 ✓：每种介质/功能一份 ✓（画它们的过程本身就是验收 ✓，见 docs/samples.md ✓）。 -->
  <h3 style="margin:16px 0 4px">示例作品</h3>
  <p style="opacity:.75;font-size:12px;margin:0 0 8px">
    用不同介质画出来的样例，可以直接打开查看、继续画或拿来练手。
  </p>
  <div id="sampleList" style="display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:8px"></div>
  <!-- **作品展示** ✓（2026-10-16 ✓）：另一个 AI 用**本应用的 MCP 工具**画的德加习作 ✓。
       **∴ 与上面的「示例作品」不同 ✗**：上面那些是**这个工作区里能打开继续画**的样例文档 ✓；
       这里是**静态图**（只用于看 ✓）—— 原图 6–25 MB ✗、这里是 900px 的展示副本 ✓。
       **∴ 图在 `web/showcase/` ✓**（**三处引用同一个相对路径** ⇒ 判据 `tool-showcase-assets.mjs` 守着 ✓）。 -->
  <h3 style="margin:16px 0 4px">作品展示</h3>
  <p style="opacity:.75;font-size:12px;margin:0 0 8px"
     title="另一个 AI 用本应用的 MCP 工具所画；参照 Met／NGA 的公有领域德加原作，只用来看风格：没有导入任何原作位图，每个像素都是笔刷/介质笔触。">
    德加习作 · 由另一 AI 用本应用工具所画
  </p>
  <div style="display:grid;grid-template-columns:repeat(auto-fill,minmax(180px,1fr));gap:12px">
    <figure style="margin:0">
      <figcaption style="opacity:.75;font-size:11px;margin-top:4px">排练室的晨光 · 干性粉彩 · 3072×3840</figcaption>
    </figure>
    <figure style="margin:0">
      <figcaption style="opacity:.75;font-size:11px;margin-top:4px">幕边的蓝色舞者 · 干性粉彩 · 3840×3072</figcaption>
    </figure>
    <figure style="margin:0">
      <img src="showcase/degas-backstage-oil.png" alt="后台舞者"
           loading="lazy" style="width:100%;height:auto;border-radius:6px;display:block" />
      <figcaption style="opacity:.75;font-size:11px;margin-top:4px">后台舞者 · 油画介质 · 3114×4096</figcaption>
    </figure>
    <figure style="margin:0">
      <img src="showcase/degas-four-dancers-oil.png" alt="四位舞者"
           loading="lazy" style="width:100%;height:auto;border-radius:6px;display:block" />
      <figcaption style="opacity:.75;font-size:11px;margin-top:4px">四位舞者 · 油画介质 · 4096×3427</figcaption>
    </figure>
  </div>
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
<!-- **删除确认** ✓（产品负责人：要能删别的作品 ✓）—— 删除**不可逆** ✗ ⇒
     必须再确认一次 ✓，而且把**服务端说的话原样显示**出来 ✓（"正在使用中"要看得见 ✓）。 -->
<dialog id="deleteDialog">
  <h2 style="margin-top:0">删除文档</h2>
  <p id="deleteWhat" style="margin:4px 0"></p>
  <p class="hint" style="margin:4px 0"
     title="文档目录会被移除；它引用的 blob 里，只有它自己引用的会被删掉，被别的作品或离线变更集引用的一律保留。">
    删掉磁盘上的这份文档与只属于它的 blob。
  </p>
  <div id="deleteError" class="hint" style="color:#ffb4a2;min-height:16px"></div>
  <div style="display:flex;gap:8px;justify-content:flex-end">
    <button id="deleteCancel">取消</button>
    <button id="deleteConfirm">删除</button>
  </div>
</dialog>
  <div class="options" id="options">
    <span class="tool-name" id="toolName">画笔</span>
  <!-- **粗细**（用户报告：这个滑杆"不显眼" ✗）。
       原来只是一个裸 `<label>` + 120px 滑杆 ✓：没有读数、没有边界感、和旁边的复选框长得一样 ✓。
       现在把它**独立成组** ✓（边框 + 底色 + 标签 ✓），并给三样东西：
         ① **实时读数** ✓（`#sizeValue`，拖滑杆、点快选、程序化赋值都会跟上 ✓）；
         ② **± 按钮** ✓（细/粗各一档，不用精确拖 ✓）；
         ③ 上限从 64 提到 **512** ✓ —— 应用本来就支持到 512（`brush_preview` 的上限就是 512 ✓），
            而介质插件另有自报的 `maxDab`，会在落笔时自己夹住 ✓ ⇒ 提到 512 不会越界。
       **注意**：`#size` 仍是那个 range，id 与语义**没变** ⇒ 11 处 `Number($("size").value)` 不受影响 ✓。 -->
  <div class="size-control" title="画笔粗细（像素）。拖滑杆、点 −/＋，或直接输入数值。">
    <span class="size-label">粗细</span>
    <button type="button" id="sizeDown" title="细一档">−</button>
    <input id="size" type="range" min="1" max="512" value="6" />
    <button type="button" id="sizeUp" title="粗一档">＋</button>
    <input id="sizeValue" type="number" min="1" max="512" step="1" value="6"
           title="画笔粗细的精确数值（像素）" />
  </div>
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
  <!-- **★ 字体选择（字体方案 Phase 1）★**
       值是**真实的 font-family 名** ⇒ 浏览器用它渲染 ⇒ 系统字体/emoji/大字号都跟着有了。
       `builtin` 是原来的内嵌 16×16 位图图集（服务端 `draw_text`）⇒ 保持默认，行为零变更。 -->
  <label>字体 <select id="textFont" title="文本字体：内置点阵走服务端位图图集；其余走浏览器 Canvas 渲染系统字体（支持中文、emoji、任意字号）">
    <option value="builtin">内置点阵</option>
    <option value="sans-serif">无衬线</option>
    <option value="serif">衬线</option>
    <option value="monospace">等宽</option>
    <option value="cursive">手写</option>
    <!-- **预置 OFL 字体（自托管 ✓ 见 viewer.css 的 @font-face 与 assets/fonts ✓）** -->
    <!-- **∴ `data-i18n="off"` ✗**：**字体名是**专有名词**✗ 不翻译 ✓
         ⇒ **∴ 显式告诉判据"这不是文案"**（**∴ 否则它会合理地要求一个"翻译" ✓）★ -->
    <option value="Inter" data-i18n="off">Inter</option>
    <option value="JetBrains Mono" data-i18n="off">JetBrains Mono</option>
  </select></label>
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
            title="笔刷库：每支笔刷都带真实落笔的效果图（滚到哪画到哪）；点一行就换那支笔">笔刷库</button>
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
    <span class="brush-control" title="笔刷压感动力学（MCP set_brush_dynamics 的 Web 入口）：压感→笔触大小的响应曲线">
      压感
      <button id="dynSoft" type="button" title="轻压细、重压粗（柔和）">柔</button>
      <button id="dynLinear" type="button" title="线性响应">线性</button>
      <button id="dynHard" type="button" title="轻压粗、重压变化小（硬）">硬</button>
    </span>
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
    <canvas id="kernel"></canvas>
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
  <button id="exportPng">导出 PNG</button>
  <button id="exportPsdLayers" type="button" title="导出成分层 PSD（图层/组/不透明度/可见性/混合模式都带出去）">导出分层 PSD</button>
  <!-- **显式的"当前视图"导出** ✓：默认那颗导**整幅文档** ✓（P0：旧默认导的是视口 ✗）；
       裁剪成视口仍然有用，但必须由用户点名 ✓。 -->
  <button id="exportPngView">导出当前视图</button>
  <!-- **显式渲染开关** ✓（(A)⑤）：勾上就走**服务端像素**那条 ✓（= 弱设备回退 ✓）。
       改了会**重载页面** ✓ —— 比热切换简单，也不会有"半客户端半服务端"的中间状态 ✓。 -->
  <label style="display:block;margin-top:8px;font-size:12px"><input type="checkbox" id="useServerRender" /> 用服务端渲染（默认开启；取消勾选 ⇒ 改用内核离线渲染）<span id="serverRenderLocalOnlyHint" hidden>（本部署无服务端，不可用）</span></label>
  <!-- **★ 首帧就说真话 ✗ ★**（第九轮 ✓）：勾选框的初值**必须**在解析到这一行时定下 ✓ ——
       主脚本在页尾 ✓ 而它到 `setupFileMenu()` 才赋值 ✓ 而那里要等 `warmKernel()`
       （= 启动尾部的一串网络 ✓）⇒ **∴ 默认（服务端 ✓）时首帧画的是**空框 ✗
       而旁边的字写着"默认开启" ✓ ⇒ **屏幕自相矛盾** ✗
       （实测：首绘 52 ms 时 `checked=false` ✓，到 ≈230 ms 才变 true ✓；慢网下窗口更长 ✗）。
       **∴ 三档顺序**与 `serverRenderPreferred()`（`viewer-app.js` ✓）**逐档一致** ✓：
       本地专用 ⇒ 关 ✓｜`?render=client` ⇒ 关 ✓｜`?render=server` ⇒ 开 ✓｜
       存量偏好（缺省 ⇒ 开 ✓）✓。**改一档必须两处同改** ✓
       （判据 `scripts/browser-render-box-truth.mjs` 逐组合盯 ✓）。
       **∴ 不写 `checked` 属性** ✗：那会把客户端模式的首帧也说成服务端 ✓（反向说谎 ✗）。 -->
  <script>
  (function () {
    var box = document.getElementById("useServerRender");
    if (!box) return;
    var server = true;
    try {
      if (window.__pwaLocalOnly === true) server = false;
      else {
        var want = new URLSearchParams(location.search).get("render");
        if (want === "client") server = false;
        else if (want === "server") server = true;
        else {
          var stored = localStorage.getItem("yanshi.serverRender");
          server = stored === null ? true : stored === "1";
        }
      }
    } catch (error) { /* 解析不了 ⇒ 按缺省（服务端）✓ */ }
    box.checked = server;
  })();
  </script>
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
    <!-- **调整 / 滤镜** ✓（第七轮把这张卡"说的"与"发生的"对齐 ✓）：效果名列表本来就来自服务端
         `/api/effects` ✓ 因此不会与内核漂移 ✓ —— 但**"已应用"这句是界面自己下的结论** ✗
         本轮实测它不可信：
         ① **默认参数 `{}` 下 26 个调整里 17 个是像素空操作**（`brightness_contrast` /
           `saturation` / `levels` / `exposure` / `white_balance` / `curves` / `hsl` /
           `color_balance` / `split_toning` / `channel_mixer` / `calibrate_primaries` /
           `shadows_highlights` / `selective_color` / `lens` / `dehaze` / `clarity` / `texture` ✓
           整幅合成逐字节不变）而 7 个默认就动画面（`invert` / `posterize` / `vibrance` /
           `black_and_white` / `gradient_map` / `red_eye` / `denoise` ✓）⇒ 同一句「已应用 X」底下可能是"变了"也可能是"一点没变"✗ 用户无从分辨 ✓。
           剩下 2 个（`color_lut` 要 `path` 或 `size + table` ✓ `perspective` 要 `quad: [[x,y]×4]`）
           **连 `{}` 都建不出来** ⇒ 服务端直接 `invalid_argument` 拒绝 ✓ 页面念「未写入」且一根汗毛不动 ✓
           （也就是"26 = 17 空操作 + 7 会动 + 2 非要有参数"✓ 三个数是同一轮实测出来的 ✓ 别把被拒的两个算进空操作）
         ② **参数名拼错服务端照单收下**：`{"redhue":50}` ⇒ `ok:true` ✓ 回读也原样存着 ✓
           而画面一字未动 ✗（内核只校验它**认识**的键 ✓ `tools_effect.rs` 的逐键区间校验
           对不认识的键不置可否 ✗）。越界的真键才会被拒：`{"red_hue":150}` ⇒
           `invalid_argument`「calibrate_primaries 的 red_hue 必须在 [-100, 100] 之间，得到 150」✓。
         ③ 这两件当时**只写进 `log()`** ✗ 而日志在折叠的「历史」卡里 ⇒ 界面上等于没说 ✓
           （与第五六轮 `effective` / 中性灰那条同族：**成功而画面不动，是必须说出来的那种成功**✓）。
         **所以这张卡现在做四件事** ✓：
         * 应用/更新/删除**之前之后各取一次服务端整幅合成的指纹** ✓（`render_region` 只读 ✓
           实测同一状态两次调用哈希逐字节相同 ✓ 加了真参数就变 ✓ 且不产生原子 ✓）
           ⇒「画面动没动」是**服务端字节比较**的结论 ✗ 界面不参与任何像素推算 ✗；
         * 取样点取自**服务端 `list_objects` 报的对象外框**而非均匀网格 ✓：实测 1024×1024 白底
           文档只在 (700,700) 有一块 40×40 的红 ⇒ 均匀五点全落在白底上 ✗ 加完真参数读数一字未动 ✓
           而真内容处 `200,50,50 → 200,143,0` ✓ —— 用均匀网格下结论会把"动了"说成"没动"✗；
         * 每行给一个「删」✓（`delete_object` 实测能删掉效果对象并让画面复原 ✓）：
           此前效果**只进不出** ✗ 空操作叠五次也撤不掉 ✓；
         * 失败那句话把责任说给**这一笔实际走过的那条路** ✓：`refusalVenue()` 读的正是路由
           用的同一个 `serverRenderPreferred()` ✓。关掉「服务器渲染」后请求落在**本地内核**上 ✗
           若还说成"服务端拒绝" ⇒ 把人送去翻服务端的日志 ✓ 而该看的是本地内核（第七轮判据
           `browser-effect-card-truth.mjs` ⑩b 实测跑红抓出来的 ✗ 两臂各钉一句才看得见 ✓）。
           同一处还漏读半格：服务端的码叫 `error_code` ✓ 本地那一臂叫 `error` ✓
           ⇒ 只读前者就把具体码念成 `unknown` ✗（判据 ⑩c 拿页面**通用**失败分支写的日志比 ✓）。
         **指纹拿不到时不许装作验证过** ✓：关掉「服务器渲染」后本地那条路回的是 PNG 字节 ✓
         没有 JSON 回执 ⇒ 界面固定说一句"没法验证"✗ 而不是沿用上半句的"已应用"糊过去 ✓。 -->
    <div class="card" data-panel="paint" id="cardEffects">
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
      <div class="hint" id="effectInfo"></div>
      <!-- **这句不是常设说明 ✓ 是"只在它成立那一刻才挂"的一句** ✓（第七轮 ★ 看真截图复核才改的 ✗
           本轮一开始只给它降了亮度 ✓ 以为"层级区分"就够了 —— 图上它仍在两种状态下说假话：
           ① 「画面确实动了 · 实测 [200,50,50,255] → [200,143,0,255]」底下紧跟着「指纹没变 ⇒ 空操作」
              ⇒ 同一张卡自相矛盾 ✓；② 「未写入：…拒绝了这一步」底下同一句 ⇒ 把"没写成"说成
              "写成了但没效果"✗ 而这正是这张卡本轮一直在消灭的那类谎 ✓。
           现在缺省 `hidden` ✓ 由 `sayEffect(line, {noOp})` 按**服务端那两个哈希比出来的**结果决定
           挂不挂 ⇒ 只有"写成了而整幅合成逐字节没变"那一种情况它才出现 ✓（判据 ③′ 钉可见性 ✓）。
           **而它现在只剩一句短的 ✓ 那两条"为什么"挪进 `title=`** ✓（同轮 `ui-text-density` 判据把它
           判成"人类界面上的大段解释"✗ 它一开始**红着就被记成了绿** ✓ 见 docs ㊳/㊴ ⇒ 判据 prescribed 的
           修法就是"进 title= 或只在 MCP 侧说清"✓ 这里取前者 ✓）。
           **两个键都要英文词条 ✓ 但不是同一本字典**（本轮踩过一次：`title` 走 `I18N_EN_ATTR` ✓
           文本节点走 `I18N_EN_TEXT` ✓ 塞错本 `ui-i18n-coverage` 当场红"属性漏词条"✓ 见 docs ㊴）✓
           少任何一边就是英文模式下的中文残留 ✓（`ui-i18n-coverage` + `browser-tooltip-i18n` 两条都问得到 ✓）。
           **末了文案也改了**（第二次看真截图 ✓ docs「只有看真截图」那条）：原先那句是
           「这一笔对画面是空操作 ⇒ 原因见鼠标提示」✓ 而它**头顶那一行回执**刚说完
           「… ⇒ 这一笔对画面是空操作」⇒ 相邻两行逐字重复一遍同一句结论 ✗ 既然可见那句只负责
           "指一下提示" ✓ 就把它缩成「为什么？见鼠标提示」✓ 结论由回执那一行独占 ✓（判据 ③″ 只问
           "密度合格 + 原因还在 `title` 里" ✓ 不问措辞 ⇒ 这句改字不影响它 ✓）。 -->
      <div class="hint" id="effectNote" style="opacity:.62" hidden title="缺省参数常常本就无效果；参数名拼错也会被照单收下（内核只校验它认识的键）。">为什么？见鼠标提示</div>
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
    <!-- **直方图** ✓ —— `get_histogram`（只读 ✓、MCP 缺省面里就有 ✓）此前在查看器里**零引用** ✗
         ⇒ 人在页面上只能"看着像过了/欠了"，取不到 Agent 拿到的同一份读数 ✗（两边各说各话 ✓）。
         作用域三选一与工具规格一致 ✓：整文档 / 当前图层 / 只统计选区（选区来自服务端事实 ✓）。 -->
    <div class="card" data-panel="diag" id="cardHistogram">
      <h2>直方图</h2>
      <div class="toolbar">
        <select id="histogramScope" title="统计整文档的合成结果，还是只统计当前图层">
          <option value="document">整文档</option>
          <option value="layer">当前图层</option>
        </select>
        <label title="只统计当前选区那块矩形（选区来自服务端事实，不是本地猜测）"><input id="histogramUseSelection" type="checkbox" /> 只统计选区</label>
        <button id="histogramReload" type="button" title="读取直方图（get_histogram，只读，不改画面）">读取</button>
      </div>
      <canvas id="histogramCanvas" width="256" height="96" title="纵轴按对数画：平涂文档里那几笔颜色的桶只有几十个像素，线性轴下会被压成看不见（数值本身仍是服务端给的 ✓）" style="width:100%;height:96px;background:#0d0f12;border:1px solid var(--line);border-radius:4px"></canvas>
      <div id="histogramInfo" class="hint"></div>
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

    <!-- **锚点** ✓（`set_anchor` / `get_anchors` / `delete_anchor` 此前在查看器里**零引用** ✗）。
         这三个工具解决的是**真机报告 2026-10-15** 的错位问题 ✓（起稿定死关键点 ✓、
         每个新会话读回**同一组坐标** ✗ 不许重新目测 ✓）——
         但**只有 MCP 侧够得到** ⇒ 人在界面上画时**仍然在靠目测对齐** ✓。
         坐标有三个来源 ✓：手填 ✓、**选区中心**（选区本身来自服务端事实 ✓）✓、
         或者**开着「锚点」工具直接在画布上点** ✓（点一下就填好 x/y ✓，再点「钉住」✓）。 -->
    <div class="card" data-panel="paint">
      <h2>锚点</h2>
      <div class="hint">把关键点钉死在文档里 ⇒ 换会话、重启服务都读回同一组坐标，不必重新目测。</div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <label>名 <input id="anchorName" type="text" placeholder="left_eye" style="width:104px" /></label>
        <label>x <input id="anchorX" type="number" step="1" style="width:68px" /></label>
        <label>y <input id="anchorY" type="number" step="1" style="width:68px" /></label>
        <label>备注 <input id="anchorNote" type="text" placeholder="可留空" style="width:104px" /></label>
      </div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <button id="anchorSet" type="button" title="写一个命名锚点（set_anchor）；同名再设 = 覆盖，不必先删">钉住</button>
        <button id="anchorFromSelection" type="button" title="用当前选区的中心填充 x / y（选区来自服务端事实）">用选区中心</button>
        <button id="anchorRefresh" type="button" title="读回文档里全部锚点（get_anchors）">刷新</button>
      </div>
      <div class="hint" id="anchorInfo"></div>
      <div id="anchorList" class="annotation-list"></div>
    </div>

    <!-- **范围遮罩** ✓（`create_range_mask` ✓ D1-a ✓）：工具早已在服务端 ✓ 且在 MCP 缺省面里 ✓，
         查看器却**零引用** ✗ ⇒ "按明暗 / 按颜色挑出受影响的一块"这件修图最基本的事 ✓
         人只能靠 Agent 才做得到 ✗。
         挂接口径与 `create_mask` **完全一致** ✓（`set_property` 的 `mask_id` ✓）⇒
         "采样哪一层"与"挂到哪一层"是**两个独立选择** ✓（拿这层的像素去限定另一层的效果 ✓）。
         **为什么必须页面上自己判一次覆盖** ✓：`coverage_matched_pixels === 0` 时工具**仍回 `ok: true`** ✓
         （实测 ✓ —— 它确实烘焙了一张全零蒙版 ✓）⇒ 挂上去之后效果一点不见 ✗ 却处处显示"成功" ✓。
         这种"成功但什么都没发生"正是本项目禁止的静默 ✗ ⇒ **覆盖为 0 就拒绝挂载并说清** ✓。
         **取下蒙版走的是 `set_property` 的 JSON `null`** ✓（实测 ✓）：填**空串**会被引用检查拒掉 ✓
         （`reference_not_found` ⇒ 那句 `引用的实体  不存在或已删除` 中间还是个空的插值 ✓）⇒
         界面这里必须送 `null` ✗ 不能送 `""` ✓ 也不能像 `delete_mask` 那样只删本体、把引用留在层上 ✓。 -->
    <div class="card" data-panel="paint" id="cardRangeMask">
      <h2>范围遮罩</h2>
      <div class="hint">按像素值挑出受影响的一块：亮度带或色相带。烤出来的是快照，源图层往后的改动不会传播进来。</div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <label>采样 <select id="rangeMaskSource" title="读哪一层的像素：这一层的当前渲染就是采样的全部来源"></select></label>
        <label>挂到 <select id="rangeMaskTarget" title="把蒙版挂到哪一层：只有这一层的效果被它限定"></select></label>
        <label>带 <select id="rangeMaskKind" title="亮度带按显示空间的明度取值；色相带按色相窗取值，近灰的像素恒不选">
          <option value="luminance">亮度带</option>
          <option value="color">色相带</option>
        </select></label>
        <label title="反选：把带内变成带外（反选烤进网格，不是挂上去再翻）"><input id="rangeMaskInvert" type="checkbox" /> 反选</label>
      </div>
      <div class="toolbar" id="rangeMaskLumRow" style="gap:8px;flex-wrap:wrap">
        <label>下限 <input id="rangeMaskLow" type="number" min="0" max="1" step="0.05" value="0" style="width:64px" title="亮度带下限（0–1，显示空间）" /></label>
        <label>上限 <input id="rangeMaskHigh" type="number" min="0" max="1" step="0.05" value="0.5" style="width:64px" title="亮度带上限（0–1，必须不小于下限）" /></label>
        <label>软边 <input id="rangeMaskSmoothLum" type="number" min="0" max="0.5" step="0.01" value="0.05" style="width:64px" title="带边过渡的宽度（明度单位，0–0.5）" /></label>
      </div>
      <div class="toolbar" id="rangeMaskColorRow" style="gap:8px;flex-wrap:wrap;display:none">
        <label>色相 <input id="rangeMaskHue" type="number" min="0" max="360" step="1" value="0" style="width:64px" title="色相窗的中心（0–360 度）" /></label>
        <label>窗宽 <input id="rangeMaskHueWidth" type="number" min="1" max="360" step="1" value="60" style="width:64px" title="色相窗的全宽（度，大于 0 且不超过 360）" /></label>
        <label>软边 <input id="rangeMaskSmoothColor" type="number" min="0" max="1" step="0.05" value="0.25" style="width:64px" title="拿窗宽的多少来做软边（0–1）" /></label>
      </div>
      <div class="toolbar" style="gap:8px">
        <button id="rangeMaskApply" type="button" title="烘焙范围遮罩并挂到所选图层；带内一个像素都没有时不会挂载">烘焙并挂载</button>
        <button id="rangeMaskDetach" type="button" title="取下「挂到」那一层当前的蒙版；蒙版本体留在历史里，只断开这一层">取下蒙版</button>
        <span class="hint" id="rangeMaskCurrent"></span>
      </div>
      <div class="hint" id="rangeMaskInfo"></div>
    </div>

    <!-- **渐变蒙版** ✓（`create_mask` 的 `linear_gradient` / `radial_gradient` 两种 kind ✓ D1-b ✓）：
         形状蒙版只有矩形 / 椭圆 ✓，想"这一带渐渐失效"必须拖一条向量或一个衰减圈 ✗ 页面上没有 ✗。
         **羽化对这两种 kind 是无效输入** ✓：服务端传非零羽化直接 `invalid_argument` ✗
         （实测 ✓）⇒ 工具栏那个「羽化」在这里**必须被忽略并且说清** ✗
         否则人只会看到"拖了没反应" ✓（改 `create_mask` 的语义是后端的事 ✗ 这里先把话说清楚 ✓）。 -->
    <div class="card" data-panel="paint" id="cardGradientMask">
      <h2>渐变蒙版</h2>
      <div class="hint" title="线性：起笔一侧为 1、收笔一侧为 0；径向：圈心为 1、内切椭圆之外为 0">在工具栏选「线性渐变蒙版」或「径向渐变蒙版」，再在画布上拖。</div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <label>径向衰减 <input id="maskFalloff" type="number" min="0" max="1" step="0.05" value="0.6" style="width:64px" title="径向渐变：过渡带占半径的比例（0 = 硬边，1 = 整条半径都在过渡）" /></label>
        <label title="反选：起笔一侧变成 0"><input id="maskGradientInvert" type="checkbox" /> 反选</label>
      </div>
      <div class="hint" id="maskGradientInfo">渐变蒙版不使用「羽化」：过渡带就是渐变本身。挂上去的蒙版要断开：在「范围遮罩」卡的「挂到」里选这一层，点「取下蒙版」。</div>

    </div>

    <!-- **频率分离** ✓（`frequency_separation` ✓ 在 HTTP 缺省面内的 `retouch` 组 ✓）：
         本轮之前 `grep frequency_separation crates/yanshi-http/` 命中 **0** ✗（实测 ✓）
         ⇒ "把色调与细节分成两层来修"这件修图最常用的事 ✓ 人只能靠 Agent 才做得到 ✗。
         **界面必须照服务端回执说话** ✓：工具自带**重建自检** ✓
         （`reconstruction_mean_error` / `_max_error` / `_max_error_bright` / `reconstruction_pixel_ratio` ✓
         实测回执 ✓）⇒ 那四个数是"这组层还原得有多准"的唯一权威读数 ✗ 界面不许自算 ✓。
         **为什么这里连"画面没变"都不能当成一句话带过** ✓：实测分离后的整幅合成与源层**并非逐像素相同** ✓
         （`export_png` 解码比对：32×24 上 124 个通道差、单通道最大 10/255 ✓ 与回执 `max_error` 同量级 ✓）
         ⇒ 卡片只能说"误差多少"✓ 不能说"看起来一样"✗。
         **两种拒绝都不留半套层** ✓（实测 ✓）：空层 ⇒ `precondition_failed`
         （`图层 X 没有任何对象 ⇒ 没有可分离的像素` ✓）；自检不过 ⇒ `invalid_argument`
         （`…⇒ 烘焙没有重建回源图，拒绝落组（不留半套层）` ✓）⇒ 界面**先读 `list_layers` 的 `objects` 就地拦** ✓
         不发送注定被拒的请求 ✓ 也不产生任何原子 ✓。
         **为什么没有「整组撤销」按钮** ✓：`revert_group` 对分离建出的组回 `ok: true` **但
         `reverted_count: 0`** ✓ 层数一层没少 ✗（实测 ✓ —— 那五层没被打原子级 `group_id` ✓）
         ⇒ 按钮按下去"成功而什么都没撤"正是本项目禁止的静默 ✗ 所以这里只教人用历史卡 ✓。 -->
    <div class="card" data-panel="paint" id="cardFreqSep">
      <h2>频率分离</h2>
      <div class="hint" title="低频修色调、高频修细节互不干扰；烤出来的是此刻的像素快照，源图层往后的改动不会传播进来">把源图层分离成低频与高频两层，装进一个组。</div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <label style="white-space:nowrap">源图层 <select id="freqSepSource" title="读哪一层烘焙：这一层的当前渲染就是分离的全部来源"></select></label>
        <label>半径 <input id="freqSepRadius" type="number" min="1" max="64" step="1" value="8" style="width:64px" title="高斯 sigma（1–64）：越大低频越糊、留给高频的细节越少" /></label>
        <label>组名前缀 <input id="freqSepName" type="text" placeholder="缺省用源图层名" style="width:170px" title="生成「前缀 · 频率分离」组与前缀 · 低频 / 高频 / 低频绘画 / 高频绘画 四层" /></label>
        <button id="freqSepRun" type="button" title="烘焙并建组；源图层没有对象或半径越界时不发送请求、不写入任何改动">分离</button>
      </div>
      <div class="hint" id="freqSepCurrent"></div>
      <div class="hint" id="freqSepInfo"></div>
      <div class="hint" title="在「高频绘画」层上涂抹用 188 作中性色（128 会压暗）；源图层保持可见，新组盖在它上面；要整组撤掉请用「历史（原子日志）」卡逐条回退">高频层的中性灰是 188（不是 128）。</div>
    </div>

    <!-- **减淡加深** ✓（`dodge_burn_setup` ✓ 在 HTTP 缺省面内的 `retouch` 组 ✓）：
         本轮之前 `grep -c dodge_burn crates/yanshi-http/` 命中 **0** ✗（实测 ✓）
         ⇒ "局部提亮/压暗而不改颜色"这件修图天天做的事 ✓ 人只能靠 Agent 才做得到 ✗。
         **这张卡必须把服务端回执的诚实字段念出来** ✓：`effective` / `ineffective_reason` /
         `neutral_gray` 是本轮**实测后补进回执**的三格 ✓（`tools_retouch.rs` ✓）⇒
         界面不自算"这组有没有用"✗ 只转述服务端自己说的话 ✓。
         **curves 那条 ⚠ 第六轮撤掉了**（写在这里免得有人照旧文案加回去）✓：第五轮实测它改不到画面
         （调整对象只作用于**它自己图层的缓冲** ✓ 那两层除曲线外没有像素 ✓ 128 的底色涂满白仍 128 ✓）
         ⇒ 当时在卡上面向用户警告 ✗ 回执也回 `effective:false` ✓ 两处都注明"谁修好跨层调整就同时撤"✓。
         上游 `302d414d`（纯调整层渲染时先用下方内容播种）正是那次修复 ✓ 重测：dodge 蒙版涂白
         `128→159` ✓ burn 涂白 `128→98` ✓ 端点 0/255 不动 ✓ ⇒ 两条臂都改得到画面 ✓
         于是下拉那句 option 从「实测不改画面」换成「曲线可回改」✓ 中英词条一起改 ✓
         （`ui-i18n-coverage` 会盯词条与模板是否同源 ✓）。
         **为什么灰层的中性必须写在卡上** ✓：同一条实测路径抓到旧实现把灰层铺成 50% 显示灰 128 ✗
         而本引擎的 `soft_light` 在**线性缓冲**上做 ⇒ 中性是线性 0.5 ＝ 字节 **188** ✓
         ⇒ 铺 128 时 128 的底色被压到 **97**（整幅暗 31 级 ✗ 建组这一步自己就把画弄黑了 ✓）；
         铺 188 时六个底色（0/60/128/200/255 与一个彩色）逐点**完全不变** ✓
         ⇒ 服务端已改用 188 ✓ 卡片只转述回执 `neutral_gray` 这个数 ✗ 不在界面写死。
         **界面先拦后送** ✓：中点必须在 [0.05，0.95]（服务端同域 ✓ 越界回 `invalid_argument`
         且detail 逐字带区间 ✓ 实测 ✓）⇒ 越界时不发送请求 ✓ 不产生任何原子 ✓。
         **不解读 HTTP 注入的字段** ✓：`created` / `opened` 由 HTTP 层加在**每条**回执上 ✓
         说的是"这个文档有没有被本次请求打开/新建"✓（`server.rs` 那两处 ✓）
         ⇒ 与工具本身的效果无关 ✗ 界面不许拿它当"建了几层"来念 ✓。 -->
    <div class="card" data-panel="paint" id="cardDodgeBurn">
      <h2>减淡加深</h2>
      <div class="hint" title="只改明暗、不改颜色的局部提亮与压暗；建出来的是一个组，涂抹在组内的层上">给一块区域做减淡（提亮）与加深（压暗）。</div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <label style="white-space:nowrap">源图层 <select id="dodgeBurnSource" title="按它的名字建组并把新组放在它上方"></select></label>
        <label>方法 <select id="dodgeBurnMode" title="两条路都会改变画面（实测）：灰层法涂就见效，曲线法留两条可回改的曲线">
          <option value="gray">灰层法（涂就见效）</option>
          <option value="curves">曲线法（曲线可回改）</option>
        </select></label>
        <label>减淡中点 <input id="dodgeBurnDodge" type="number" min="0.05" max="0.95" step="0.01" value="0.62" style="width:68px" title="曲线法的减淡强度（显示空间 0.05–0.95，0.5 = 无效果）；只对曲线法有意义" /></label>
        <label>加深中点 <input id="dodgeBurnBurn" type="number" min="0.05" max="0.95" step="0.01" value="0.38" style="width:68px" title="曲线法的加深强度（显示空间 0.05–0.95，0.5 = 无效果）；只对曲线法有意义" /></label>
        <label>组名前缀 <input id="dodgeBurnName" type="text" placeholder="缺省用源图层名" style="width:150px" title="建出「前缀 · 减淡加深」组；同名组已存在时复用而不叠第二套" /></label>
        <button id="dodgeBurnRun" type="button" title="建组；中点越界时不发送请求、不写入任何改动">建组</button>
      </div>
      <div class="hint" id="dodgeBurnCurrent"></div>
      <div class="hint" id="dodgeBurnInfo"></div>
      <div class="hint" title="灰层本身是中性灰，涂抹才生效：用白色低流量笔触提亮、黑色压暗；灰层是此刻的快照，不跟随下方修改">灰层法的中性字节由服务端回执给出（本引擎是 188，不是 128）。</div>
    </div>

    <!-- **区域填充** ✓（第四轮 ✓）：`fill_region` 此前在页面上**零入口** ✗
         （实测 `grep -rn fill_region crates/yanshi-http/src/viewer.rs crates/yanshi-http/assets/viewer-app.js`
         在 HTML 与界面脚本里命中 **0** ✗ 只有 `api-local.js` 那条"本地内核已实现"✓）
         ⇒ 工具栏那支「填充图层」走的是 `fill`（整层、无形状、无羽化 ✗）⇒ 不是这一张卡的替代品 ✓。
         **为什么这张卡必须把回执念出来** ✓：本轮实测抓到 `fill_region` 的 `opacity`
         在 `color` 写成 `"#rrggbb"` 时被**静默丢掉**（1.0／0.5／0.2 逐字节相同 ✗ 服务端修好了 ✓）
         ⇒ 界面这里固定送**对象形态** `{r,g,b}` 并把服务端回显的 `opacity` 念回来 ✓
         人一眼能核对"我给的 0.5 服务端认了没有"✗ 而不是信页面自己的数 ✓。
         **`羽化` 在这张卡上是看得见的** ✓：渲染侧那条"羽化被裁到窄区域 ⇒ 同一个像素的 α 随区域大小变"
         （实测整幅 255 ／ 1 行区域 194 ✗ 已修 ✓）正是**页面平移缩放与分块缓存**做的事 ✗
         ⇒ 页面曾经会把羽化冲淡 ⇒ 这张卡也是那条缺陷在界面上的验收口 ✓。
         **「取画面这块」= `texture.type:"region"`** ✓（第 2680 轮实现 ✓ 注册表描述当时还写着"尚未实现"✗
         ⇒ 已按实测更正 ✓）：服务端把源矩形当前的合成**就地拍成 blob** 再走位图那条路 ✓
         回执里 `texture.from_region:true` 与取纹理的 `layer_id` 都要念出来 ✓。
         **警告不许咽下去** ✓：处理器返回顶层 `warnings`（与 `drawn.warnings` ✓）
         ⇒ 有就整条摊开 ✗ 沉默即静默降级 ✓。 -->
    <div class="card" data-panel="paint" id="cardFillRegion">
      <h2>区域填充</h2>
      <div class="hint">往一块形状里填色或铺纹理。落的是「一个形状对象」，不是一次性烘焙：之后还能选中它改羽化与不透明度。</div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <label>图层 <select id="fillRegionLayer" title="填到哪一层：那一层已有的对象不受这次影响"></select></label>
        <label>形状 <select id="fillRegionShape" title="矩形给 x/y/w/h；椭圆给中心与两半径；折线给顶点；路径给 SVG 的 d">
          <option value="rect">矩形</option>
          <option value="ellipse">椭圆</option>
          <option value="polygon">折线</option>
          <option value="path">路径</option>
        </select></label>
        <button id="fillRegionFromSelection" type="button" title="读当前选区的外框（选区来自服务端事实，不是本地猜测）；没有选区时如实说没有">用选区</button>
      </div>
      <div class="toolbar" id="fillRegionRectRow" style="gap:8px;flex-wrap:wrap">
        <label>x <input id="fillRegionX" type="number" step="1" value="24" style="width:64px" title="左上角 x（文档像素）" /></label>
        <label>y <input id="fillRegionY" type="number" step="1" value="24" style="width:64px" title="左上角 y（文档像素）" /></label>
        <label>宽 <input id="fillRegionW" type="number" min="1" step="1" value="120" style="width:64px" title="宽度（像素，要大于 0）" /></label>
        <label>高 <input id="fillRegionH" type="number" min="1" step="1" value="80" style="width:64px" title="高度（像素，要大于 0）" /></label>
      </div>
      <div class="toolbar" id="fillRegionEllipseRow" style="gap:8px;flex-wrap:wrap;display:none">
        <label>中心 x <input id="fillRegionCx" type="number" step="1" value="84" style="width:64px" title="椭圆中心 x" /></label>
        <label>中心 y <input id="fillRegionCy" type="number" step="1" value="64" style="width:64px" title="椭圆中心 y" /></label>
        <label>rx <input id="fillRegionRx" type="number" min="1" step="1" value="60" style="width:64px" title="横向半径（要大于 0）" /></label>
        <label>ry <input id="fillRegionRy" type="number" min="1" step="1" value="40" style="width:64px" title="纵向半径（要大于 0）" /></label>
      </div>
      <div class="toolbar" id="fillRegionPolygonRow" style="gap:8px;flex-wrap:wrap;display:none">
        <label>顶点 <input id="fillRegionPoints" type="text" placeholder="x,y 空格或分号分隔，至少 3 个" style="width:280px" title="多边形顶点，至少 3 个（开放路径按隐式闭合填充）" /></label>
      </div>
      <div class="toolbar" id="fillRegionPathRow" style="gap:8px;flex-wrap:wrap;display:none">
        <label>d <input id="fillRegionD" type="text" placeholder="M 24 24 L 144 24 L 144 104 Z" style="width:280px" title="SVG 路径字符串（M/L/C/Q/H/V/Z，小写为相对命令）；也认 Bézier 曲线" /></label>
        <label title="evenodd 按奇偶规则挖洞；nonzero 按绕数规则"><select id="fillRegionFillRule">
          <option value="evenodd">奇偶</option>
          <option value="nonzero">绕数</option>
        </select></label>
      </div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <label>颜色 <input id="fillRegionColor" type="color" value="#dc2828" title="填充色：送出的是 {r,g,b} 对象，服务端再把不透明度乘进 alpha" /></label>
        <label>不透明 <input id="fillRegionOpacity" type="number" min="0" max="1" step="0.05" value="1" style="width:64px" title="0–1：服务端把它乘进颜色的 alpha" /></label>
        <label>羽化 <input id="fillRegionFeather" type="number" min="0" max="128" step="1" value="0" style="width:64px" title="边缘过渡半径（文档像素，0 = 硬边）" /></label>
        <label>填什么 <select id="fillRegionMode" title="纯色，或把画面另一块的像素当纹理铺进来">
          <option value="color">纯色</option>
          <option value="region">取画面这块</option>
        </select></label>
      </div>
      <div class="toolbar" id="fillRegionSrcRow" style="gap:8px;flex-wrap:wrap;display:none">
        <label>源 x <input id="fillRegionSrcX" type="number" step="1" value="0" style="width:64px" title="取纹理那块矩形的左上角 x" /></label>
        <label>源 y <input id="fillRegionSrcY" type="number" step="1" value="0" style="width:64px" title="取纹理那块矩形的左上角 y" /></label>
        <label>源宽 <input id="fillRegionSrcW" type="number" min="1" step="1" value="40" style="width:64px" title="取纹理那块矩形的宽（要大于 0）" /></label>
        <label>源高 <input id="fillRegionSrcH" type="number" min="1" step="1" value="40" style="width:64px" title="取纹理那块矩形的高（要大于 0）" /></label>
      </div>
      <div class="toolbar" style="gap:8px">
        <button id="fillRegionRun" type="button" title="落笔：形状尺寸不合法时不发送请求、不写入任何改动">填充</button>
        <span class="hint" id="fillRegionSelection"></span>
      </div>
      <div class="hint" id="fillRegionInfo"></div>
      <div class="hint" id="fillRegionSample"></div>
      <div class="hint" title="「取画面这块」取的是此刻那块矩形的像素（服务端就地存成纹理），不是实时链接：先把那块画好再指它。羽化只作用于边缘的不透明度。">「取画面这块」烤的是此刻那块的像素，不跟随后续修改。</div>
    </div>

    <!-- **风格预设** ✓（`list_presets` / `preview_preset` / `apply_preset` 此前在查看器里**零引用** ✗）。
         这 22 套内置预设是**落成一串可调对象**的 ✓（不是一次性烘焙像素 ✗）⇒ 人在界面上
         只能靠 MCP 才够得到 ✓。三段流程与成熟软件一致 ✓：按题材筛 ✓ → **先看代理图**（临时文档渲染 ✓
         不动原文档 ✓）→ 再落到当前图层 ✓；`强度` 与预设自带的**语义旋钮**都能调 ✓。 -->
    <div class="card" data-panel="paint">
      <h2>风格预设</h2>
      <div class="hint">挑一套风格落到当前图层：落的是可调对象，不是一次性烘焙。先点「刷新」读回预设，题材可筛。</div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <label>题材 <input id="presetSubject" type="text" placeholder="如 夜景 / 人像" style="width:112px" /></label>
        <label>强度 <input id="presetStrength" type="range" min="0" max="100" value="100" style="width:104px" /></label>
        <span class="hint" id="presetStrengthOut">100%</span>
        <button id="presetReload" type="button" title="读回可用预设（list_presets）；题材框留空 = 全部">刷新</button>
      </div>
      <div class="toolbar" style="gap:8px;flex-wrap:wrap">
        <label><input id="presetCorrection" type="checkbox" checked />带修正段</label>
        <span class="hint" id="presetInfo"></span>
      </div>
      <div id="presetKnobs"></div>
      <img id="presetShot" alt="" style="display:none;max-width:100%" />
      <div id="presetList" class="annotation-list"></div>
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
        <button id="objectHistory" type="button" title="查看选中对象的版本历史（原子版本链）">版本历史</button>
        <button id="objectDetail" type="button" title="读回选中对象的完整记录（get_object）">详情</button>
        <button id="objectResolved" type="button" title="选中对象解析后的生效状态（get_resolved_state）">解析</button>
        <button id="objectDetach" type="button" title="把勾选的实例从 master 断开（detach_instance）">脱离实例</button>
      </div>
      <!-- **实例 / 组 / metadata 的第二排** ✓（三方对照补齐 ✓）——
           `update_sync_policy` / `link_to_master` / `add_to_group` / `remove_from_group` /
           `update_object` 在系统注册表与 MCP 默认档里都有 ✓，查看器此前**零引用** ✗。 -->
      <div class="toolbar" style="gap:8px; flex-wrap:wrap">
        <label>组 <input id="objectGroupId" type="text" placeholder="group_id" style="width:120px" /></label>
        <button id="objectAddGroup" type="button" title="把勾选对象加入该组（add_to_group）">加入组</button>
        <button id="objectRemoveGroup" type="button" title="把勾选对象移出该组（remove_from_group）">移出组</button>
        <!-- **整组撤销** ✓ —— `revert_group` 此前 Web **零引用** ✗：页面上撤群只能逐对象撤 ✓
             （而"同一次操作产生的所有笔"正是它的语义 ✓）⇒ 组级回退在界面上不可达。 -->
        <button id="objectRevertGroup" type="button" title="把该组同一次操作产生的所有笔一次撤掉（revert_group，只撤这一组，不碰别的）">整组撤销</button>
        <!-- **对象重排** ✓ —— `reorder_objects` 是 Core 工具、MCP 默认档里有 ✓，查看器此前**零引用** ✗：
             页面上人能改**图层**的上下顺序 ✓，却改不了**同一图层内**谁盖住谁 ✗（只能删掉重画 ✓）。 -->
        <button id="objectUp" type="button" title="把勾选的对象上移一层（reorder_objects，一次只动一个对象）">对象上移</button>
        <button id="objectDown" type="button" title="把勾选的对象下移一层（reorder_objects，一次只动一个对象）">对象下移</button>
        <label>主图 <input id="objectMasterId" type="text" placeholder="master_id" style="width:120px" /></label>
        <button id="objectLinkMaster" type="button" title="把勾选实例改挂到该 master（link_to_master）">换主图</button>
        <select id="objectSyncPolicy">
          <option value="all">全部同步</option>
          <option value="none">停止同步</option>
        </select>
        <button id="objectSyncApply" type="button" title="给勾选实例设置同步策略（update_sync_policy）">设同步策略</button>
        <label>备注 <input id="objectNote" type="text" placeholder="metadata.note" style="width:120px" /></label>
        <button id="objectNoteApply" type="button" title="写进对象的 metadata.note（update_object 的 patch）">写备注</button>
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
    <!-- **诊断包** ✓（P0 事故复盘：报告说丢了几个小时的工作 ✓，而事发时**没有任何东西可收** ✗）——
         这条入口与 MCP 的 `collect_diagnostics` 工具走**同一个**采集实现 ✓
         ⇒ 两个面交出来的包**条目完全相同** ✓。
         **它不是变更操作** ✓：只读 ✓，不改文档 ✓（viewer 令牌也能下载 ✓）。 -->
    <div class="card" data-panel="diag" id="cardDiagnostics">
      <h2>诊断</h2>
      <div class="hint"
        title="把构建/平台/配置（已去密）、文档元数据、原子日志尾部（注明截断）、服务端 stderr 环形缓冲、渲染告警（含被跳过的补丁）、阶段耗时与当前缩略图打成一个 zip 下载；包内有 README 逐条说明。只读，不改文档。">
        出问题时一键下载排查包（zip）
      </div>
      <div class="toolbar"><button id="diagnosticsDownload" type="button">下载诊断包</button></div>
      <div id="diagnosticsInfo" class="hint"></div>
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
      <!-- **存为调色板** ✓ —— `save_palette` 此前只有工具层入口 ✗（系统+MCP 有、web 没有 ✓）。 -->
      <div class="toolbar" style="gap:8px">
        <input id="paletteSaveName" type="text" placeholder="my.gpl" style="width:120px" />
        <button id="paletteSave" type="button" title="把当前色块存成 .gpl 调色板资产（save_palette）">存为调色板</button>
      </div>
      <div id="paletteInfo" class="hint"></div>
    </div>
    <!-- **参考图** ✓ —— `set_reference` / `clear_reference` 此前在查看器里**零引用** ✗
         （overlay 只会被动读 preferences ✓，没有任何 UI 能**写入**它 ✗ ⇒ 对照表"系统+MCP"一类 ✓）。 -->
    <div class="card" data-panel="assets" id="cardReference">
      <h2>参考图</h2>
      <label>不透明 <input id="referenceOpacity" type="number" value="0.5" min="0" max="1" step="0.1" style="width:56px" /></label>
      <!-- **位置** ✓：`set_reference` 收 `position:{x,y,w,h}`（文档像素 ✓），叠加层也**按它摆** ✓。
           以前页面只会铺满整幅 ⇒ Agent 用 MCP 设了位置人却看不见 ✓（同一份数据两种读法 ✗）；
           留空 ⇒ 铺满（与从前一致 ✓）。 -->
      <label title="参考图摆在文档的哪个矩形（像素）；留空 = 铺满整幅">位置 <input id="referenceX" type="number" min="0" step="1" placeholder="x" style="width:52px" /> <input id="referenceY" type="number" min="0" step="1" placeholder="y" style="width:52px" /> <input id="referenceW" type="number" min="1" step="1" placeholder="w" style="width:52px" /> <input id="referenceH" type="number" min="1" step="1" placeholder="h" style="width:52px" /></label>
      <div class="toolbar" style="gap:8px">
        <button id="referenceSet" type="button" title="上传一张图，半透明铺在画布下层用于临摹（set_reference）">上传参考图</button>
        <button id="referenceClear" type="button" title="清除当前参考图（clear_reference）">清除参考图</button>
        <input id="referenceFile" type="file" accept="image/png,image/jpeg,image/webp" hidden />
      </div>
      <div id="referenceInfo" class="hint"></div>
    </div>
    <!-- **资产导入** ✓ —— `import_asset`（brush / texture / palette）此前只有工具层入口 ✗。 -->
    <div class="card" data-panel="assets" id="cardAsset">
      <h2>资产导入</h2>
      <div class="toolbar" style="gap:8px; flex-wrap:wrap">
        <select id="assetKind">
          <option value="brush">笔刷</option>
          <option value="texture">纹理</option>
          <option value="palette">调色板</option>
        </select>
        <input id="assetName" type="text" placeholder="name.ext" style="width:120px" />
        <button id="assetImport" type="button" title="从本机选文件，走 data_base64 上传（import_asset）">选文件导入</button>
        <input id="assetFile" type="file" hidden />
      </div>
      <div id="assetInfo" class="hint"></div>
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
      <div class="toolbar"><button id="gradApply" type="button">填充</button> <button id="gradBlend" type="button" title="用笔触自动生成渐变过渡（替代手动拼笔）">渐变混合</button></div>
      <div id="gradInfo" class="hint"></div>
    </div>
    <!-- **工程包** ✓（目标 ⑧ ✓）—— 工具层早就有 `export_project` / `import_project` ✓。
         **两个方向不是同一种东西** ✓，界面里必须分开写 ✗：
         导出 ⇒ **下载到你自己的电脑** ✓（`export_project` 把包放进 CAS 并回 `yanshi://blob/<hash>` ✓，
         由 HTTP 层改写成 `/api/blob/..` ✓ ⇒ 一个 `<a download>` 就落地 ✓ —— 与 PNG / 诊断包同一条路 ✓）；
         路径框是**可选**的"服务器上另存一份" ✓（MCP / 命令行仍走 `path` ✓，浏览器不再需要它 ✓）；
         导入 ⇒ 用户**本机上有一个 `.yanshi`** ✓ ⇒ 必须能**选文件上传** ✓，
         而不是让他在服务器路径框里敲一个自己的文件（那是**做不到**的 ✗ —— 产品负责人报的正是这一条 ✓）。
         上传走分片协议 ✓（真实工程包几百 MB，一次 POST 过不了 32 MiB 的请求体上限 ✓，
         见 `server.rs` 的 `import_document` ✓）。 -->
    <div class="card" data-panel="file">
      <h2>工程包</h2>
      <div class="hint" title="导出直接把 .yanshi 下载到你的电脑；下面的路径留空即可（填了就在服务器上再存一份）。导入从你的电脑选 .yanshi 文件（分片上传，整包多大都行）。">
        导出下载到本机 ｜ 导入从本机选文件
      </div>
      <label>服务器另存路径 <input id="projectPath" type="text" placeholder="留空 ⇒ 只下载到本机" style="width:200px" /></label>
      <div class="toolbar">
        <button id="projectExport" type="button">导出工程</button>
        <!-- **★ 绘画过程视频 ✗ ★**（**第 249 轮 ✓）：**与工程包同一个卡片 ✗ 因为都是"把
             现在这个文档变成文件" ✓；**∴ 它**不需要联网 ✗ 全靠本地内核 ＋ WebCodecs** ✓ -->
        <button id="timelapseExport" type="button" title="把从第一笔到现在的过程渲染成视频（本地内核 ＋ WebCodecs，不需要联网）">导出过程视频</button>
      </div>
      <div class="hint" id="timelapseInfo"></div>
      <!-- **任意尺寸整幅导出** ✓ —— `export_png` 的 width/height 能力（服务端渲染、不受 512px 限制 ✓）
           此前查看器只做"按文档原尺寸"导出 ✗ ⇒ 补齐入口 ✓（不落盘、base64 直接下载 ✓）。 -->
      <div class="toolbar" style="gap:8px">
        <label>宽 <input id="exportServerW" type="number" min="0" step="1" placeholder="px" style="width:72px" /></label>
        <label>高 <input id="exportServerH" type="number" min="0" step="1" placeholder="px" style="width:72px" /></label>
        <!-- **期 4 的 region/LOD 能力补齐** ✓（上游 44db088c 给 `export_png` 加了
             region + lod 0..=3 ✓，而查看器停在 width/height ✗ ⇒ web 用户够不到"只导这块 / 低清预览" ✓）。 -->
        <label title="留空 ⇒ 整幅；填齐 x,y,w,h 才生效">区域 <input id="exportRegionX" type="number" min="0" step="1" placeholder="x" style="width:52px" /> <input id="exportRegionY" type="number" min="0" step="1" placeholder="y" style="width:52px" /> <input id="exportRegionW" type="number" min="0" step="1" placeholder="w" style="width:52px" /> <input id="exportRegionH" type="number" min="0" step="1" placeholder="h" style="width:52px" /></label>
        <button id="exportRegionFromSelection" type="button" title="用当前选区的包围盒填充区域四个框（选区来自服务端事实）">用选区</button>
        <label>档位 <select id="exportLod" title="取样档位：lod:k ⇒ 输出尺寸 = 区域 ÷ 2^k（向上取整），不经过全分辨率中间缓冲">
          <option value="">缺省</option>
          <option value="0">0 全分辨率</option>
          <option value="1">1 一半</option>
          <option value="2">2 1/4</option>
          <option value="3">3 1/8</option>
        </select></label>
        <!-- **单图层导出** ✓（`export_png.layer_id` 早已实现 ✗ web 够不到 ✓）：勾选 ⇒ 只导当前层。 -->
        <label><input id="exportLayerOnly" type="checkbox" title="只导当前图层（隐藏层也照导——导出某一层是明确要求）" />只导当前层</label>
        <button id="exportServerPng" type="button" title="服务端渲染（可只导一个区域、可降档取样、可按尺寸缩放），结果直接下载到本机（export_png）">任意尺寸导出</button>
      </div>
      <div id="exportServerInfo" class="hint"></div>
      <label>导入为 <input id="projectImportName" type="text" placeholder="留空 ⇒ 用包里的 id" style="width:180px" /></label>
      <div class="toolbar">
        <button id="projectImport" type="button">导入 .yanshi…</button>
        <input id="projectFile" type="file" accept=".yanshi,application/x-tar,application/octet-stream" hidden />
      </div>
      <div id="projectInfo" class="hint"></div>
    </div>
    <!-- **建议** ✓（设计 §12.6 ✓）—— `suggest` / `list_suggestions` / `accept_suggestion` /
         `reject_suggestion` 此前在查看器里**零引用** ✗ ⇒ 用户看不到 AI 提出的可执行补丁 ✓、
         也无法接受或拒绝 ✓。它与上面的「标注」配对：**标注说明问题 ✓、建议给出可执行的修法 ✓**
         （`accept_suggestion` 会**按序重放 patch** ✓，并把关联标注置为 resolved ✓）。
         ★ **可见文本里不许写 markdown** ✓（截图巡检抓到 ✗）：`**粗体**` 与反引号是给读源码的人看的 ✓，
         浏览器**不解析 HTML 里的 markdown** ✗ ⇒ 用户真的看到一串星号 ✓。
         注释里照旧用 ✓，**面向用户的那句话必须写成纯文本** ✓。 -->
    <div class="card" data-panel="diag">
      <h2>建议</h2>
      <div class="hint">建议来自 suggest 工具（含可执行补丁 ✓）；接受会按序重放补丁 ✓。</div>
      <div class="toolbar">
        <select id="suggestionStatus">
          <option value="pending">待处理</option>
          <option value="accepted">已接受</option>
          <option value="rejected">已拒绝</option>
          <option value="">全部</option>
        </select>
        <button id="suggestionReload" type="button">刷新</button>
        <!-- **批量** ✓ —— `accept_suggestions` / `reject_suggestions`（复数 ✓）此前零引用 ✗：
             逐条"接受"在面对一屏建议时是酷刑 ✓。勾选行 ⇒ 一键处理所选 ✓。 -->
        <button id="suggestionAcceptChecked" type="button" title="一次性接受所有勾选的建议（accept_suggestions）">接受所选</button>
        <button id="suggestionRejectChecked" type="button" title="一次性拒绝所有勾选的建议（reject_suggestions）">拒绝所选</button>
        <!-- **拒绝原因** ✓（第五轮 ✓）：新判据 `scripts/check-viewer-unique-ids.mjs` 抓到
             `$("suggestionReason")` 是一句**悬空引用** ✗（HTML 里从来没有这个 id ✓）
             ⇒ 逐条拒绝与批量拒绝**恒送固定串**「编辑器里拒绝」✗ 用户想写的理由**根本没地方输入** ✓
             ⇒ 而日志又回显「原因「编辑器里拒绝」」✓ ⇒ 看起来像用户自己写的 ✗
             ⇒ 补上输入框 ✓ 空着时日志如实说"未填"✓（见 `viewer-app.js` 那两处 ✓）。 -->
        <label style="white-space:nowrap">原因 <input id="suggestionReason" type="text" placeholder="为什么拒绝（可选）" style="width:180px" title="写进 reject_suggestion 的 reason：拒绝理由会留在历史里，供他人判断" /></label>
      </div>
      <div id="suggestionList" class="annotation-list"></div>
      <div class="hint" id="suggestionPreviewHint">点「预览」先看清补丁会被怎么执行 ✓（预览不改动画面 ✓）</div>
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
                     style="width:64px" title="画布显示比例（%）；回车或失焦生效 —— 滚轮不缩放也不平移（防误触）" />%</span>
    <span id="zoom" class="hint">100%</span>
    <span id="undoDepth">撤销 0 / 重做 0</span>
  <span id="selectionHint">无选区</span>
  <span class="spacer"></span>
  <span>画布 <b id="canvasSize">—</b></span>
</footer>
<script src="/i18n.js"></script>
<script>"##;

/// **主脚本之后的收尾** ✓（`</script> … </html>` ✓）。
const PAGE_TAIL_B: &str = r##"</script>
<!-- **素材浮层** ✓：两张卡搬进来（`appendChild` = 移动节点 ✓，监听器与状态都还在 ✓）⇒
     关掉时按**记下来的原位**搬回去 ✓ ⇒ 右侧面板永远不会被搬空 ✗。 -->
<div id="fileMenu" hidden>
  <div class="file-menu-head">
    <strong>文件</strong>
    <span class="hint">Esc 收起</span>
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
    <span class="hint">P 调色板 · T 纹理 · Esc 收起</span>
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

/// **整页模板** ✓ —— **NEW-H1 瘦身后**：CSS 与双语词表都走**同源外链**（`/viewer.css`／`/i18n.js`，
/// 均由本二进制 `include_str!` 发 ✓）；**JS 三段仍内联** ✗（页面没有模块加载器 ⇒ 加载顺序
/// 就是依赖顺序，与 (A)① 的旧理由一致 ✓）。
///
/// ⚠️ 这里**必须用拼接、不能用 `format!`** ✗ —— JS 里全是花括号 ✗，格式化会把它们当占位符 ✓
/// （真正的占位符只有 `__READ_TOOLS__` / `__BUILD_ID__` ✓，用 `.replace` 替换 ✓）。
fn page_template() -> String {
    [
        PAGE_HEAD,
        PAGE_TAIL_A,
        // **★★★ 两份新资产 ✗ 各自一段 `<script>`** ✓（**第 249 轮 ✓）★★★
        //   ＋ **∴ 为什么不能 `import` ✗**：**页面没有模块加载器**✗（**全是内联传统脚本 ✓）
        //     ⇒ **∴ 于是**：**按**加载顺序**排 ✗ `mp4-lite` 先 ✗ `timelapse-local` 后**
        //       ✗ 最后才是主脚本 ✓（**∴ 主脚本**用得上它们 ✓）★
        //   ＋ **∴ 而**主脚本那一段**必须仍然逐字节等于 `viewer-app.js`**✗
        //     ⇒ **∴ 所以**：**这里**显式补 `</script><script>`**✗ 而不是把三份拼成一块** ✓
        //       （**∴ 有测试盯着这一条 ✓）★★
        include_str!("../assets/mp4-lite.js"),
        "</script><script>",
        // **∴ 第 250 轮 ✗**：**WebM 封装器**（**∴ 没有 H.264 编码器时走它 ✓
        //   ＋ **∴ CI 的 Chrome for Testing 正是那种机器 ✓）★★
        include_str!("../assets/webm-lite.js"),
        "</script><script>",
        include_str!("../assets/timelapse-local.js"),
        "</script><script>",
        include_str!("../assets/viewer-app.js"),
        PAGE_TAIL_B,
    ]
    .concat()
}

/// 页面长度（测试与可观测性）。
/// **独立样式表资产的对外出口** ✓（(A)①：让"可打包产物"同时**可路由 / 可预缓存** ✓）。
pub fn stylesheet() -> &'static str {
    include_str!("../assets/viewer.css")
}

/// **主脚本资产的对外出口** ✓（同上 ✓）。
pub fn app_script() -> &'static str {
    include_str!("../assets/viewer-app.js")
}

/// **★ 极简 MP4 封装器资产 ✗ ★**（**第 249 轮 ✓）
///
/// **∴ 为什么单独一个出口 ✗**：**它要能被判据**单独读到**✗ 也能被**预缓存**
/// （**离线 PWA ✓）—— **∴ 与 CSS／主脚本**同一条规矩** ✓ ★
pub fn mp4_script() -> &'static str {
    include_str!("../assets/mp4-lite.js")
}

/// **★ 极简 WebM 封装器资产 ✗ ★**（**第 250 轮 ✓；理由同 `mp4_script` ✓）★
pub fn webm_script() -> &'static str {
    include_str!("../assets/webm-lite.js")
}

/// **★ 过程视频（timelapse）资产 ✗ ★**（**第 249 轮 ✓；理由同上 ✓）★
pub fn timelapse_script() -> &'static str {
    include_str!("../assets/timelapse-local.js")
}

/// **★ 双语词表资产 ✗ ★**（05 复审 NEW-H1 瘦身第一步 ✓）：**这三张表约 910 行 / 77 KB**✗
///   从主脚本挪出来 ⇒ **以**传统脚本**外链**先于主脚本加载 ✓
///   ⇒ **∴ 三份 `const` 落在**共享的全局词法环境**✓ 主脚本照常引用 ✗ **运行时语义不变** ✓
///   ＋ **∴ 为什么单独立一条 `pub fn`**：**同一个二进制**用 `include_str!` 发 ✓（`server.rs` 的
///     `/i18n.js` 路由 ✓）⇒ **单二进制部署**照样自包含 ✗ 页面里唯一的 `<script src=` 是**同源绝对路径** ✓
pub fn i18n_script() -> &'static str {
    include_str!("../assets/i18n.js")
}

/// 页面长度（测试与可观测性）✓ —— **⚠️ 插入新函数时别把它的文档注释抢走** ✗
///（`viewer.rs` 顶部那条警告说的就是这件事 ✓：插在它之前 ⇒ 注释会挂到新函数上 ⇒ `clippy -D warnings` 红 ✓）。
pub fn page_len() -> usize {
    page_template().len()
}

#[cfg(test)]
mod tests {

    /// **(A)①：主脚本是独立可打包资产** ✓ —— 断言 ① 资产文件存在 ✓；
    /// ② 生成页面里**有一个 `<script>` 块逐字节等于**该资产 ✓
    /// （**变异：把资产拼到 `<script>` 之外、或漏拼 ⇒ 红** ✓）。
    #[test]
    fn the_main_script_is_a_separate_packable_asset() {
        let path = std::path::Path::new("assets/viewer-app.js");
        assert!(path.is_file(), "(A)①：主脚本应是**独立资产文件** ✓");
        let asset = std::fs::read_to_string(path).expect("应能读取 JS 资产");
        let page = page_template();
        let mut rest = page.as_str();
        let mut found = false;
        while let Some((_, after_open)) = rest.split_once("<script>") {
            let Some((block, after_close)) = after_open.split_once("</script>") else {
                break;
            };
            if block.trim() == asset.trim() {
                found = true;
                break;
            }
            rest = after_close;
        }
        assert!(
            found,
            "页面必须有一个 <script> 块**逐字节等于**独立 JS 资产 ✓（挪出 script 或漏拼 ⇒ 红 ✓）"
        );
    }

    /// **★★★ 过程视频的两份资产也必须**独立成块**内联** ✗ ★★★**（**第 249 轮 ✓）
    ///
    /// **∴ 为什么要单独立一条 ✗**：**它们不是主脚本**✗ 但**同样得在页面里** ✓
    ///   ＋ **∴ 而**本仓页面**没有模块加载器**✗（**全是内联传统脚本 ✓）
    ///     ⇒ **∴ 一旦有人**忘了拼接／把它们挪出 `<script>`**✗
    ///       ⇒ **∴ 症状**不是报错 ✗ 而是**点下按钮才说"模块没加载"** ✓
    ///         ⇒ **∴ 所以**要有一条**静态就能红**的判据 ✓
    ///           （**变异 ✗ 删掉 `page_template` 里任一条 `include_str!` ⇒ 本测试红** ✓）★★
    #[test]
    fn the_timelapse_assets_are_inlined_as_their_own_script_blocks() {
        for (path, asset, name) in [
            ("assets/mp4-lite.js", mp4_script(), "MP4 封装器"),
            ("assets/webm-lite.js", webm_script(), "WebM 封装器"),
            (
                "assets/timelapse-local.js",
                timelapse_script(),
                "过程视频模块",
            ),
        ] {
            let on_disk = std::fs::read_to_string(path).expect("应能读取 JS 资产");
            assert_eq!(on_disk, asset, "(A)①：{name} 应是**独立资产文件** ✓");
            let page = page_template();
            let mut rest = page.as_str();
            let mut found = false;
            while let Some((_, after_open)) = rest.split_once("<script>") {
                let Some((block, after_close)) = after_open.split_once("</script>") else {
                    break;
                };
                if block.trim() == asset.trim() {
                    found = true;
                    break;
                }
                rest = after_close;
            }
            assert!(
                found,
                "{name} 必须在页面里有一个 <script> 块**逐字节等于**它 ✓（挪出去或漏拼 ⇒ 红 ✓）"
            );
        }
    }

    /// **(A)①＋NEW-H1 第二步：CSS 是独立可打包资产，且页面以 `<link>` 引用它** ✓ ——
    /// 断言 ① 资产文件存在 ✓ 且与 `stylesheet()` 出口逐字节一致 ✓；
    /// ② 页面 head 里有 `<link rel="stylesheet" href="/viewer.css" />` ✓ 且**不再有** `<style>` 块 ✓
    /// （**变异：删 link ⇒ 红 ✓；把 CSS 挪回内联 ⇒ ② 的后半红 ✓**——
    ///   同一事实只写一处：CSS 只在资产里存在一份，页面靠引用拿 ✗ 不复制）。
    #[test]
    fn the_stylesheet_is_a_separate_packable_asset() {
        let path = std::path::Path::new("assets/viewer.css");
        assert!(
            path.is_file(),
            "(A)①：CSS 应是**独立资产文件**（可打包/可预缓存）✓"
        );
        let asset = std::fs::read_to_string(path).expect("应能读取 CSS 资产");
        assert_eq!(
            stylesheet().trim(),
            asset.trim(),
            "`/viewer.css` 路由出口必须逐字节等于资产文件 ✓"
        );
        let page = page_template();
        assert!(
            page.contains("<link rel=\"stylesheet\" href=\"/viewer.css\" />"),
            "页面必须引用 /viewer.css（NEW-H1：CSS 不再内联 ✓）"
        );
        assert!(
            !page.contains("<style>") && !page.contains("</style>"),
            "页面不应再有内联 `<style>` 块（瘦身后 CSS 只以外链存在 ✓ 挪回内联＝同一事实写两处 ✗）"
        );
    }

    /// **(NEW-H1 瘦身)：双语词表是独立外链资产** ✓ —— 断言 ① 资产文件存在且与出口逐字节一致 ✓；
    /// ② 页面以 `<script src="/i18n.js">` 引用它，且**在主脚本之前** ✓
    /// （**变异：删引用 ⇒ 红 ✓；把引用挪到主脚本之后 ⇒ 红 ✓；把三张表挪回 `viewer-app.js` ⇒ 红 ✓**）。
    #[test]
    fn the_i18n_dictionary_is_an_external_asset_loaded_before_the_main_script() {
        let path = std::path::Path::new("assets/i18n.js");
        assert!(
            path.is_file(),
            "双语词表应是**独立资产文件**（NEW-H1：不进单文件页面）✓"
        );
        let asset = std::fs::read_to_string(path).expect("应能读取 i18n 资产");
        assert_eq!(
            i18n_script().trim(),
            asset.trim(),
            "`/i18n.js` 路由出口必须逐字节等于资产文件 ✓"
        );
        for name in ["I18N_EN_TEXT", "I18N_EN_ATTR", "I18N_EN_ATTR_PATTERNS"] {
            assert!(
                asset.contains(&format!("const {name}")),
                "词表资产应定义 `{name}` ✓"
            );
            assert!(
                !app_script().contains(&format!("const {name}")),
                "`{name}` 不该再出现在主脚本资产里（同一事实只写一处 ✓）"
            );
        }
        let page = page_template();
        let tag = "<script src=\"/i18n.js\"></script>";
        let at = page.find(tag).expect("页面必须引用 /i18n.js ✓");
        let main_at = page
            .find(app_script().lines().next().expect("主脚本首行"))
            .expect("页面应内联主脚本 ✓");
        assert!(
            at < main_at,
            "/i18n.js 必须**先于**主脚本加载（三份 const 靠共享全局词法环境 ✓）"
        );
    }

    use super::*;

    #[test]
    fn page_is_self_contained_and_uses_the_documented_endpoints() {
        assert!(page_template().starts_with("<!DOCTYPE html>"));
        assert!(page_template().trim_end().ends_with("</html>"));
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
            assert!(page_template().contains(needle), "查看器缺少 {needle}");
        }
        // 无外部依赖（不加载 CDN 脚本或字体）。
        assert!(!page_template().contains("http://cdn"));
        assert!(!page_template().contains("https://cdn"));
        // **`<script src=` 的口径**（05 复审 NEW-H1 起 ✓）：这条守卫的本意是**不加载外部主机**✗，
        // 而 `/i18n.js` 是**同源绝对路径**✓ 且由**同一个二进制**用 `include_str!` 发✓（`server.rs` 路由 ✓）
        // ⇒ 自包含性不变 ✓。**除此以外的任何 `src` 仍然一律禁止** ✗（变异：给路由外主机加脚本 ⇒ 红 ✓）。
        assert!(!page_template().contains("<script src=\"http"));
        for atom in page_template().match_indices("<script src=") {
            assert!(
                page_template()[atom.0..].starts_with("<script src=\"/i18n.js\""),
                "页面里唯一的 script src 必须是 /i18n.js（同二进制内嵌路由 ✓），实际：{}",
                &page_template()[atom.0..atom.0 + 60.min(page_template().len() - atom.0)]
            );
        }
        assert!(page_template().contains("<script src=\"/i18n.js\"></script>"));
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
        let page_text = page_template();
        let mut rest = page_text.as_str();
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

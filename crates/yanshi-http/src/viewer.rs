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
/// **整页 HTML 的头部** ✓（`<!DOCTYPE html> … <style>` ✓）—— 与 `PAGE_TAIL` 之间夹的是
/// **独立可打包的 CSS 资产** ✓（(A)① ✓）。
const PAGE_HEAD: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>偃师 Yanshi 查看器</title>
<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
<link rel="icon" type="image/png" sizes="32x32" href="/favicon.png" />
<link rel="apple-touch-icon" href="/brand/png/favicon-180.png" />
<style>"##;

/// **整页 HTML 的尾部** ✓（`</style> … </html>` ✓）。
/// **`</style>` … 主脚本 `<script>`** ✓（与 `PAGE_TAIL_B` 之间夹的是**独立可打包的 JS 资产** ✓）。
const PAGE_TAIL_A: &str = r##"</style>
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
<button id="langToggle" type="button" data-i18n="off" aria-pressed="false" title="把界面切成英文（English）">EN</button>
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
  <label style="display:block;margin-top:8px;font-size:12px"><input type="checkbox" id="useServerRender" /> 用服务端渲染（弱设备回退）</label>
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
<script>"##;

/// **主脚本之后的收尾** ✓（`</script> … </html>` ✓）。
const PAGE_TAIL_B: &str = r##"</script>
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

/// **整页模板** ✓：把**独立的 CSS 资产**拼回页面 ✓（(A)①：HTML/CSS/JS 要能作为独立产物打包 ✓）。
///
/// ⚠️ 这里**必须用拼接、不能用 `format!`** ✗ —— CSS 与 JS 里全是花括号 ✗，格式化会把它们当占位符 ✓
/// （真正的占位符只有 `__READ_TOOLS__` / `__BUILD_ID__` ✓，用 `.replace` 替换 ✓）。
fn page_template() -> String {
    [
        PAGE_HEAD,
        include_str!("../assets/viewer.css"),
        PAGE_TAIL_A,
        include_str!("../assets/viewer-app.js"),
        PAGE_TAIL_B,
    ]
    .concat()
}

/// 页面长度（测试与可观测性）。
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

    /// **(A)①：CSS 是独立可打包资产** ✓ —— 断言 ① 资产文件存在 ✓；
    /// ② 生成页面里 `<style>…</style>` 之间的内容与资产**逐字节一致** ✓
    /// （**变异：把资产拼到 `</style>` 之外 ⇒ 红** ✓）。
    #[test]
    fn the_stylesheet_is_a_separate_packable_asset() {
        let path = std::path::Path::new("assets/viewer.css");
        assert!(
            path.is_file(),
            "(A)①：CSS 应是**独立资产文件**（可打包/可预缓存）✓"
        );
        let asset = std::fs::read_to_string(path).expect("应能读取 CSS 资产");
        let page = page_template();
        let open = page.find("<style>").expect("页面应有 <style> ✓") + "<style>".len();
        let close = page.find("</style>").expect("页面应有 </style> ✓");
        assert!(open < close, "样式块必须是闭合且有序的 ✓");
        assert_eq!(
            page[open..close].trim(),
            asset.trim(),
            "页面样式块应**逐字节等于**独立 CSS 资产 ✓（顺序/位置错了 ⇒ 红 ✓）"
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
        assert!(!page_template().contains("<script src="));
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

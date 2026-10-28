// **界面双语词表（中文 ⇒ English）—— 独立可路由 / 可预缓存资产** ✓（05 复审 NEW-H1 瘦身第一步：
//   这三张表约 910 行、~150 KB，从主脚本挪出来 ⇒ 单文件页面不再为它们付首屏字节，
//   而它以 `<script src="/i18n.js">` 传统脚本身份先于主脚本加载 ✓ 三份 `const` 落在
//   **共享的全局词法环境** ⇒ 主脚本照常引用，运行时语义逐字节不变 ✓）。
// 消费方与词条纪律（漏词条当场红）见 `scripts/ui-i18n-coverage.mjs` ✓。
// ------------------------------------------------------- 界面双语（中文 / English）
// **按"精确匹配中文原文"翻译**，而不是给 67 处元素逐个加 key：一行 HTML 都不用改，
// 而且**表里没有的文案保持原样**（安全降级）。**默认中文** ⇒ 既有行为与既有检查脚本都不受影响；
// 英文是**显式切换**（顶栏开关 / `?lang=en` / 记住的选择）。日志与诊断**不翻译**（它们是给人看的长句）。
const I18N_EN_TEXT = {
  // **本地内核退回服务端渲染那句**（第六轮新增的浮层 ✓ 见 `localKernelUnusableNow` ✓）：
  //   固定整句 ⇒ 走精确键 ✓（不必造句式 ✓）。漏了这条 ✗ 英文界面就会夹一整句中文 ✓。
  "本地内核没能把这份文档装进来 ⇒ 这一页先改用服务端渲染（画面照常，刷新后可再试本地）。":
    "The local kernel could not load this document, so this page switched to server rendering. The picture is normal; reload to try the local kernel again.",
  // **范围遮罩卡 + 渐变蒙版卡** ✓（本轮新增 ✓）：模板文本节点 ✓ 与卡片上那几句固定回执 ✗
  // 都在 `ui-i18n-coverage` / `browser-i18n` 的扫描范围内 ✓（`#rangeMaskInfo` 不是 `#log` ✓）。
  "范围遮罩": "Range mask",
  "按像素值挑出受影响的一块：亮度带或色相带。烤出来的是快照，源图层往后的改动不会传播进来。":
    "Pick the affected pixels by value: a luminance band or a hue band. The baked result is a snapshot — later edits to the source layer do not propagate into it.",
  "采样": "Sample",
  "挂到": "Attach to",
  "带": "Band",
  "亮度带": "Luminance band",
  "色相带": "Hue band",
  "反选": "Invert",
  "下限": "Low",
  "上限": "High",
  "软边": "Feather edge",
  "色相": "Hue",
  "窗宽": "Window",
  "烘焙并挂载": "Bake and attach",
  "渐变蒙版": "Gradient mask",
  "在工具栏选「线性渐变蒙版」或「径向渐变蒙版」，再在画布上拖。":
    "Pick Linear gradient mask or Radial gradient mask in the toolbar, then drag on the canvas.",
  "径向衰减": "Radial falloff",
  "渐变蒙版不使用「羽化」：过渡带就是渐变本身。挂上去的蒙版要断开：在「范围遮罩」卡的「挂到」里选这一层，点「取下蒙版」。":
    "Gradient masks do not use Feather — the transition band is the gradient itself. To unlink a mask you attached: pick the layer under Attach to in the Range mask card and press Detach mask.",
  "亮度带的上下限都要在 0–1 之间。": "Both ends of a luminance band must be within 0–1.",
  "下限必须不大于上限（要挑带外请勾「反选」，不要把区间反过来填）。":
    "The low end must not exceed the high end (to select outside the band, tick Invert instead of reversing the range).",
  "软边超出允许范围：亮度带是 0–0.5（明度单位）。":
    "Feather edge out of range: a luminance band accepts 0–0.5 (in luma units).",
  "色相要在 0–360 之间（度）。": "Hue must be within 0–360 degrees.",
  "窗宽必须大于 0 且不超过 360（度）。": "The window must be greater than 0 and at most 360 degrees.",
  "软边超出允许范围：色相带是 0–1（占窗宽的比例）。":
    "Feather edge out of range: a hue band accepts 0–1 (as a fraction of the window width).",
  "未挂载：服务端拒绝了这次烘焙（原因见下方日志）。":
    "Not attached: the server rejected this bake (see the log below for the reason).",
  "未挂载：本地内核没有这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试。":
    "Not attached: the local kernel has no such endpoint (this request never reached the server) ⇒ tick Server render back on and retry.",
  "取下蒙版": "Detach mask",
  "该层当前没有蒙版": "This layer has no mask right now",
  "那一层本来就没有蒙版 ⇒ 没有可取下的东西（没有写入任何改动）。":
    "That layer has no mask to begin with ⇒ nothing to detach (no change was written).",
  "没取下：那一步失败了（原因见下方日志）。":
    "Not detached: that step failed (see the log below for the reason).",
  "带内没有像素 ⇒ 未挂载：把上下限拉开（如 0–1），或去掉反选再试。":
    "No pixels fall in this band ⇒ not attached: widen the range (for example 0–1), or untick Invert and try again.",
  "已烘焙但未挂载：挂接那一步失败了（原因见下方日志）。":
    "Baked but not attached: the attach step failed (see the log below for the reason).",
  // **频率分离卡** ✓（本轮新增 ✓）：静态文案进这张表 ✓ 带数字的动句走 `i18nText()` 的模式 ✓。
  "频率分离": "Frequency separation",
  "把源图层分离成低频与高频两层，装进一个组。":
    "Separate the source layer into a Low and a High layer inside one group.",
  "高频层的中性灰是 188（不是 128）。": "The neutral grey of the high layer is 188 (not 128).",
  "把源图层烤成「低频 + 高频」两层（外加两层空白绘画层），装进一个组：低频修色调、高频修细节互不干扰。烤出来的是此刻的像素快照，源图层往后的改动不会传播进来。":
    "Bake the source layer into a Low and a High layer (plus two blank paint layers) inside one group: fix colour on the low layer and detail on the high layer without disturbing each other. What you get is a pixel snapshot of this moment — later edits to the source layer do not propagate in.",
  "源图层": "Source layer",
  "半径": "Radius",
  "组名前缀": "Group name prefix",
  "分离": "Separate",
  "请选择要分离的图层": "Pick the layer to separate",
  "该层没有对象 ⇒ 服务端会直接拒绝（没有可分离的像素）":
    "This layer holds no objects ⇒ the server would refuse it outright (there are no pixels to separate)",
  "高频层的中性灰是 188（不是 Photoshop 式的 128）：在「高频绘画」层上涂抹要用 188 作中性色，用 128 会压暗。源图层保持可见，新组盖在它上面；要整组撤掉请用「历史（原子日志）」卡逐条回退。":
    "The neutral grey of the high layer is 188 (not Photoshop's 128): to paint on the High paint layer use 188 as neutral — 128 darkens. The source layer stays visible with the new group on top of it; to undo the whole group, revert the entries one by one in the History (atom log) card.",
  "半径要在 1–64 之间（高斯 sigma）⇒ 未分离，没有发送请求、没有写入任何改动。":
    "Radius must be within 1–64 (gaussian sigma) ⇒ not separated: no request was sent and no change was written.",
  "那一层不在服务端的图层清单里（可能刚被删）⇒ 未分离。":
    "That layer is not in the server's layer list (it may have just been deleted) ⇒ not separated.",
  "未分离：本地内核没有这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试。":
    "Not separated: the local kernel has no such endpoint (this request never reached the server) ⇒ tick Server render back on and retry.",
  "未分离：服务端拒绝了这次分离（原因见下方日志）。它拒绝时不建组 ⇒ 不会留下半套层。":
    "Not separated: the server rejected this separation (see the log below for the reason). It builds no group when it refuses ⇒ no half-finished layers are left behind.",
  // **减淡加深卡** ✓（本轮新增 ✓）：静态文案进这张表 ✓ 带数字的动句走 `i18nText()` 的模式 ✓。
  // **两条短标签必须压到两三个词** ✓：`方法` 那一列和频率分离卡的数字列挤在同一行 toolbar 里 ✓
  //（上一轮实机截图看到过长句被截断 ✗）。
  "减淡加深": "Dodge and burn",
  "给一块区域做减淡（提亮）与加深（压暗）。":
    "Dodge (lighten) and burn (darken) an area.",
  "方法": "Method",
  "灰层法（涂就见效）": "Grey layer (paint and it works)",
  "曲线法（曲线可回改）": "Curves (two editable curves)",
  "减淡中点": "Dodge midpoint",
  "加深中点": "Burn midpoint",
  "建组": "Build group",
  "灰层法的中性字节由服务端回执给出（本引擎是 188，不是 128）。":
    "The grey layer's neutral byte comes from the server receipt (188 in this engine, not 128).",
  // **减淡加深卡的四句固定动句** ✓：整句不含数据 ⇒ 走字典而不是模式 ✓
  //（`#dodgeBurnCurrent` / `#dodgeBurnInfo` 都不在 `#log` 豁免名单里 ✓）。
  "请选择源图层": "Pick the source layer",
  "减淡/加深中点要在 0.05–0.95 之间（0.5 = 无效果）⇒ 未建组，没有发送请求、没有写入任何改动。":
    "Dodge / burn midpoints must be within 0.05–0.95 (0.5 = no effect) ⇒ no group was built: no request was sent and no change was written.",
  "未建组：本地内核没有 dodge_burn_setup 这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试。":
    "No group built: the local kernel has no dodge_burn_setup endpoint (this request never reached the server) ⇒ tick Server render back on and retry.",
  "⚠ 组建好了，但服务端回执 effective=false ⇒ 这一臂不会改变画面（完整原因见下方日志）。":
    "⚠ The group was built, but the server receipt says effective=false ⇒ this route will not change the picture (full reason in the log below).",
  // **区域填充卡** ✓（第四轮新增 ✓）：静态文案进这张表 ✓ 带数字的动句走 `i18nText()` 的模式 ✓。
  // **判据只查模板**（`ui-i18n-coverage` ✓）⇒ 这里少一条 ✗ 英文模式下卡片就漏中文 ✓；
  // 而"翻译要塞得下那个框" ✓（上一轮实机截图看到过长句被截断 ✗）⇒ 标签一律压到两三个词 ✓。
  "区域填充": "Region fill",
  "往一块形状里填色或铺纹理。落的是「一个形状对象」，不是一次性烘焙：之后还能选中它改羽化与不透明度。":
    "Fill a shape with a colour or a texture. What lands is a single shape object, not a baked bitmap — you can select it later and still change its feather and opacity.",
  "形状": "Shape",
  "矩形": "Rectangle",
  "椭圆": "Ellipse",
  "折线": "Polygon",
  "中心 x": "Centre x",
  "中心 y": "Centre y",
  "顶点": "Vertices",
  "奇偶": "Even-odd",
  "绕数": "Non-zero",
  "填什么": "Fill with",
  "纯色": "Flat colour",
  "取画面这块": "Patch texture",
  "源 x": "Source x",
  "源 y": "Source y",
  "源宽": "Source width",
  "源高": "Source height",
  // **可见句压短、长解释搬进 `title=`** ✓（第五轮 ✓）：`ui-text-density` 的界是"≥24 个汉字且带
  // `；`/`（` ⇒ 红"✗ 上一轮这句 27 个汉字 ✓ 是它漏了没跑 ✗ 本轮跑到了 ⇒ 搬进属性 ✓
  // 于是同一条句子改在 `I18N_EN_ATTR` 里 ✓ 这里只留短句 ✓。
  "「取画面这块」烤的是此刻那块的像素，不跟随后续修改。":
    "Patch texture bakes the pixels of that rectangle as of now — it does not follow later edits.",
  // **卡片运行时写进 DOM 的固定句** ✓（`ui-i18n-coverage` 查不到这些 ✗ 只有 `browser-i18n` 会看到 ✓）：
  // 校验没过、被拒、隐藏层提醒… 都在 `#fillRegionInfo` / `#fillRegionSelection` 里 ✗ 不在 `#log` 豁免名单 ✓。
  "那一层不在服务端的图层清单里（可能刚被删）⇒ 未填充。":
    "That layer is not in the server's layer list (it may have just been deleted) ⇒ not filled.",
  "不透明度要在 0 到 1 之间 ⇒ 未填充，没有发送请求、没有写入任何改动。":
    "Opacity must be within 0 to 1 ⇒ not filled: no request was sent and no change was written.",
  "羽化半径不能是负数 ⇒ 未填充。": "The feather radius cannot be negative ⇒ not filled.",
  "形状的尺寸要大于 0 ⇒ 未填充，没有发送请求、没有写入任何改动。":
    "The shape size must be above 0 ⇒ not filled: no request was sent and no change was written.",
  "椭圆的两个半径都要大于 0 ⇒ 未填充。": "Both ellipse radii must be above 0 ⇒ not filled.",
  "折线至少要 3 个顶点（形如 `x,y`，空格或分号分隔）⇒ 未填充。":
    "A polygon needs at least 3 vertices (written as `x,y`, space or semicolon separated) ⇒ not filled.",
  "路径要写 `d`（SVG 路径字符串，M/L/C/Q/H/V/Z）⇒ 未填充。":
    "A path needs its `d` (an SVG path string, M/L/C/Q/H/V/Z) ⇒ not filled.",
  "取纹理那块矩形的宽高都要大于 0 ⇒ 未填充。": "The texture rectangle's width and height must both be above 0 ⇒ not filled.",
  "未填充：本地内核没有这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试。":
    "Not filled: the local kernel has no such endpoint (this request never reached the server) ⇒ tick Server render back on and retry.",
  "未填充：服务端拒绝了这次落笔（原因见下方日志）。它拒绝时不写原子 ⇒ 不会留下半个形状。":
    "Not filled: the server rejected this stroke (see the log below for the reason). It writes no atom when it refuses ⇒ no half-finished shape is left behind.",
  "该点不在画布内 ⇒ 没有实测读数": "That point is outside the canvas ⇒ no measured reading",
  "当前没有可用选区 ⇒ 先用选区工具拖一块（外框来自服务端事实，不是本地猜测）":
    "There is no selection to read ⇒ drag one with the selection tool first (its box comes from server-side facts, not a local guess)",

  // **★ 内核卡片的服务端渲染提示（browser-i18n 实测红 ✓）★**：
  //   `setWasmState("已按设置跳过（用服务端渲染）")` 写进 DOM ⇒ 英文模式下必须有词条 ✓。
  "已按设置跳过（用服务端渲染）": "Skipped per setting (server-side rendering)",
  // **参考图降级提示** ✓（本轮补齐 ✓）：这句整句固定 ⇒ 走字典即可 ✓
  //（它写在 `#referenceInfo` 里，不在 `#log` 的豁免范围内 ✓，英文模式必须有词条 ✗）。
  "已清除参考图": "Reference cleared",
  "参考图由外部设置，本浏览器没有它的原图（存储里是裸像素）⇒ 重新上传一次就能一直显示":
    "The reference was set from outside; this browser has no copy of the original (the store keeps raw pixels) ⇒ upload it once more and it will keep showing",
  // **破图收起那一句** ✓（本轮新增 ✓）：写在同一个 `#referenceInfo` 里 ✓ 不是 `#log` ✗
  //   ⇒ 英文模式没有词条就是中文残留 ✓（`browser-reference-overlay` 判据扫的就是这条 ✓）。
  "参考图叠不上去：存储里那份是裸像素（本浏览器没有它的原图）⇒ 已收起画布上的这一层，重新上传一次就能一直显示":
    "The reference cannot be overlaid: the stored copy is raw pixels (this browser has no original) ⇒ that layer is folded away; upload it once more and it will keep showing",
  // **三方对照补齐新增的固定文案** ✓（`ui-i18n-coverage` 要求模板里每个中文文本节点都有词条 ✓）。
  // **直方图卡 + 整组撤销**（本轮新增 ✓）：模板文本节点全部要有词条 ✗。
  "直方图": "Histogram",
  "整文档": "Whole document",
  "当前图层": "Current layer",
  "只统计选区": "Selection only",
  "读取": "Read",
  "整组撤销": "Revert group",
  "解析": "Resolve",
  "脱离实例": "Detach",
  "组": "Group",
  "加入组": "Add to group",
  "移出组": "Remove from group",
  "主图": "Master",
  "换主图": "Link master",
  "全部同步": "Sync all",
  "停止同步": "No sync",
  "设同步策略": "Set sync policy",
  "备注": "Note",
  "写备注": "Save note",
  "存为调色板": "Save as palette",
  "接受所选": "Accept selected",
  "拒绝所选": "Reject selected",
  "参考图": "Reference",
  "上传参考图": "Upload reference",
  "清除参考图": "Clear reference",
  "资产导入": "Import asset",
  "选文件导入": "Choose file",
  "宽": "W",
  "高": "H",
  "任意尺寸导出": "Export at size",
  // **∴ 字体选择器的分组标题（Phase 4 ✓）✗**：**族名不翻译 ✗ 只有这个标题要词条** ✓
  "系统字体": "System fonts",
  // **∴ 预置字体名（**Inter／JetBrains Mono ✓）**不在这里** ✗ ★**：
  //   **∴ 它们是**专有名词**✗ 不该翻译 ✓**（**∴ 而**判据禁止 `zh == en` ✓ —— **∴ 那条规则是对的 ✓**
  //   **∴ 它**抓的正是"廉价占位翻译" ✓）⇒ **∴ 所以**：**用 `data-i18n="off"` 显式声明
  //   "这不是可翻译文案"** ✗（**∴ 判据的跳过规则里就有它 ✓）★★
  // **★ 字体选择（字体方案 Phase 1）★**：标签与 5 个选项都是文本节点 => 必须有词条。
  "字体": "Font",
  "内置点阵": "Built-in bitmap",
  "无衬线": "Sans serif",
  "衬线": "Serif",
  "等宽": "Monospace",
  "手写": "Cursive",
  // **★ gaps 补丁新增的三个入口（第 254 轮）★**：按钮文本会成为文本节点
  //   => 必须在词表里（判据 `ui-i18n-coverage` 实测抓到这 3 条）。
  "导出分层 PSD": "Export layered PSD",
  "版本历史": "Version history",
  "渐变混合": "Gradient blend",
  // **★ 笔刷压感动力学入口（第 254 轮）★**：三个预设按钮是**新界面元素**
  //   ⇒ 必须有词条，否则英文模式下漏出中文（判据 `ui-i18n-coverage` 实测抓到）。
  "压感": "Pressure",
  "柔": "Soft",
  "线性": "Linear",
  "硬": "Hard",
  "检测中…": "checking…",
  "检测中": "checking",
  "用不同介质画出来的样例，可以直接打开查看、继续画或拿来练手。": "Samples painted with different media; open one to look at it, keep painting, or practise on it.",
  "问题反馈、协作沟通、缺陷上报：": "Feedback, collaboration and defect reports:",
  "安全漏洞请勿开公开 issue，直接发邮件。": "Please do not open a public issue for a security problem; send mail instead.",
  "用工具栏的「标注」在画布上点一下就新建 ✓；点图钉可选中 ✓。": "Click once on the canvas with the Annotations tool to create one, and click a pin to select it.",
  // **键必须与 `viewer.rs` 里面向用户的那句中文逐字相同** ✓（查表是**精确匹配** ✗）
  // ⇒ 中文改了这里不改 ✓，英文界面就会退回中文 ✗。
  "建议来自 suggest 工具（含可执行补丁 ✓）；接受会按序重放补丁 ✓。": "Suggestions come from the suggest tool and carry an executable patch; accepting one replays it in order.",
  "点「预览」先看清补丁会被怎么执行 ✓（预览不改动画面 ✓）": "Preview first to see how a patch would be applied, since previewing does not change the picture",
  // **拒绝原因输入框** ✓（第五轮 ✓：`suggestionReason` 此前是悬空引用 ✗ ⇒ 见 `viewer.rs` 那张卡 ✓）
  "原因": "Reason",
  "✓ —— 不是从你电脑上选文件 ✗。": "✓ — and not a file picked from your computer ✗.",
  "刷新": "Refresh",
  "收起": "Collapse",
  // **离线写队列的界面** ✓（`browser-i18n` 会把**整页**的中文都算上 ✓ ——
  // 含 `hidden` 的节点 ✓ ⇒ 这两颗按钮**必须**有词条 ✗，否则英文模式下立刻红 ✓）。
  "已保存": "Saved",
  "补交": "Retry",
  "放弃": "Discard",
  "确认放弃": "Confirm discard",
  "离线": "Offline",
  "队列已满": "Queue full",
  "撤销": "Undo",
  "重做": "Redo",
  "导出 PNG": "Export PNG",
  "导出当前视图": "Export current view",
  "文件": "File",
  "图层": "Layers",
  "角度": "Angle",
  "只作用于选区": "Only inside the selection",
  "调色板": "Palette",
  "纹理": "Texture",
  "已连接": "connected",
  // **断线这一族此前只有"已连接"有词条** ✓（`browser-i18n` 实测红：英文顶栏里剩「已断开」✓）
  // ⇒ 顶栏的连接状态是**运行时改写**的文本节点 ✓，不在 `#log` 的豁免范围内 ✗。
  "已断开": "disconnected",
  "已断开（不再重连）": "disconnected (no more retries)",
  // **读取失败这一族的整句写法**（没有错误码时兜底 ✓）⇒ 也必须有词条 ✗。
  "读取失败": "read failed",
  "读取失败 ✓": "read failed ✓",
  "缩略图不可用（本部署无服务端渲染）": "No thumbnail available (this deployment has no server-side rendering)",
  "一张纹理都没找到 ⇒ 多半是服务端没找到资产目录（看启动日志的「资产目录：」一行 ✓，或用 --assets-dir 指定 ✓）":
    "Not a single texture found ⇒ most likely the server did not find its asset directory (check the “asset directory:” line in the startup log ✓, or point at one with --assets-dir ✓)",
  "id 即名字": "the id is the name",
  // **对象重排按钮** ✓（`reorder_objects` 的 Web 入口 ✓）：模板里的文本节点 ⇒ 必须有词条 ✗。
  "对象上移": "Object up",
  "对象下移": "Object down",
  "名称": "Name",
  // **新建文档对话框的尺寸**（用户：只有名称，想开 1920×1080 做不到 ✗）——
  // 这两个是**模板里的文本节点** ✓ ⇒ `ui-i18n-coverage` 要求有词条 ✓，否则静态判据立刻红 ✓。
  "宽度": "Width",
  "高度": "Height",
  "新 id 即新文档": "a new id is a new document",
  "一笔多色": "Multi-colour stroke",
  "平滑": "Smooth",
  "快捷面板": "Quick panel",
  "Esc 关闭": "Esc closes",
  "清除选区": "Clear selection",
  "绘制": "Paint",
  "历史": "History",
  "诊断": "Diagnostics",
  // **诊断包入口**（第 1166 轮 ✓）：`viewer.rs:509 / :511` 的说明与按钮 ——
  // 这条入口是 P0 事故复盘那轮加的 ✓，**没进词表** ✗ ⇒ 英文模式下它照旧写中文 ✓。
  "出问题时一键下载排查包（zip）": "Download a diagnostic bundle (zip) in one click when something goes wrong",
  "下载诊断包": "Download diagnostics bundle",
  "操作": "Actions",
  // **混合模式下拉的选项**（第 1131 轮加了 7 个 ✓ CI 的 `browser-i18n` 实测漏了它们 ✓）：
  //   ⇒ 它们是 `BLEND_MODE_LABELS` 在**运行时**创建的 `<option>` ✓ ⇒ 不在模板里
  //     ⇒ `ui-i18n-coverage`（只看模板文本节点）**抓不到** ✗
  //     ⇒ 只能靠运行时判据 `browser-i18n` ✓ —— 而它上一跑在 CI 上报了 7 条中文 ✓。
  //   **⇒ 修法**：加词条（`i18nWalk` 会遍历 `<option>` 的文本节点 ✓；面板重建后
  //     `MutationObserver` 还会再走一遍 ✓）。
  //   **本轮补上内核后来长的六种** ✓（清单与内核同源 ⇒ 词条也必须同源 ✗
  //   少一条就是英文模式下的中文残留 ✓ `browser-blend-modes.mjs` 会连着查 ✓）。
  "正常": "Normal",
  "正片叠底": "Multiply",
  "滤色": "Screen",
  "叠加": "Overlay",
  "变暗": "Darken",
  "变亮": "Lighten",
  "相加": "Add",
  "线性减淡（相加的别名）": "Linear dodge (alias of Add)",
  "相减": "Subtract",
  "颜色减淡": "Color dodge",
  "差值": "Difference",
  "线性光": "Linear light",
  "柔光": "Soft light",
  "一致性自检": "Consistency check",
  "＋ 图层": "＋ Layer",
  // **★ `browser-i18n` 报到的那一条**漏翻 ✗ ★**（第 89 轮 ✓）：
  //   **∴ 症状 ✗**：`browser-i18n` 的「**整页**」统计里有 **1 条**中文**✗
  //     （**∴ 在 `<label>` 里 ✓）：
  //       "**用服务端渲染（默认开启；取消勾选 ⇒ 改用内核离线渲染）✓"** ✓**** ✓✓
  //     ⇒ **∴ 而**它是**服务端导出的 HTML 里**的**静态文案**✗
  //       ⇒ **∴ 运行时词典**没有它** ⇒ **∴ 于是**英文模式下**原样显示 ✓**** ✓✓
  //   **∴ 修法**：**加一条词条**✗（**∴ 不必**改 HTML 或 Rust ✓）
  //     ⇒ **∴ 因为** `I18N_EN_TEXT`**是**运行时**替换的 ✓**** ✓✓
  "用服务端渲染（默认开启；取消勾选 ⇒ 改用内核离线渲染）":
    "Render on the server (on by default; clear the box to use the offline kernel instead)",
  // **本地专用部署的说明** ✓（`#serverRenderLocalOnlyHint` ✓ 模板里隐藏 ⇒ 运行到那里才显示 ✓）：
  // 英文里前面那句以 ")" 收尾 ⇒ 值带一个前导空格 ✓（否则两个右括号会粘在一起 ✗）。
  "（本部署无服务端，不可用）": " (no server in this deployment; unavailable)",
  "用服务端渲染（弱设备回退）": "Use server rendering (fallback for weak devices)",
  // **★ 作品展示（2026-10-16 ✓）★**：首页新加的那一节 ✓ —— **∴ 界面文案必须双语** ✓
  //   （判据 `ui-i18n-coverage.mjs` 只看**文本节点** ✓ ⇒ 「作品展示」＋ 四句图注都要词条 ✓；
  //    `title` 属性归 `I18N_EN_ATTR` ✗，这里不放 ✓）。
  //   **∴ 记录一笔 ✗**：我加这一节时**只加了中文** ✓ ⇒ CI 的 shard 1 **当场红** ✓
  //   （判据没漏 ✓，是**我改界面文案却没跑 i18n 判据** ✓）⇒ 现在补齐 ✓。
  "作品展示": "Gallery",
  "另一个 AI 用本应用的 MCP 工具画的德加习作（只参照风格，未导入原作位图）":
    "Degas studies painted by another AI with this app's MCP tools (style reference only; no source bitmap imported)",
  "排练室的晨光 · 干性粉彩 · 3072×3840": "Morning rehearsal · dry pastel · 3072×3840",
  "幕边的蓝色舞者 · 干性粉彩 · 3840×3072": "Blue dancers at the wings · dry pastel · 3840×3072",
  "后台舞者 · 油画介质 · 3114×4096": "Dancers backstage · oil medium · 3114×4096",
  "四位舞者 · 油画介质 · 4096×3427": "Four dancers · oil medium · 4096×3427",
  "适配": "Fit",
  "调整 / 滤镜": "Adjustments / filters",
  "调整": "Adjustment",
  "滤镜": "Filter",
  "应用": "Apply",
  "＋新建": "＋ New",
  "（当前文档没有调整/滤镜）": "(this document has no adjustment or filter)",
  // **历史卡那格空态** ✓（第七轮收尾被 `browser-i18n` 抓到 ✗ 不是新代码 ✗ 是**新走到的状态**）：
  // `viewer-app.js:5143` 在"筛选后一条原子都不剩"时把整格写成这句中文 ✓ 而它**不在 `viewer.rs` 模板里**
  // ⇒ 静态那条 `ui-i18n-coverage` 看不见（它只扫模板 ✓ 与 ㉚/㊲ 同族）✓ 英文模式下屏幕就留这一句中文 ✓。
  "（没有匹配的原子）": "(no matching atoms)",
  // **调整/滤镜卡的第七轮新句** ✓：`#effectInfo` 的**定式句**（不带数据的那几条 ✓）走文本词典 ✓
  // 带数据的整句走模式词条（见下面 m37…m39 ✓）。卡上那句固定说明也是文本节点 ✗ 漏一条
  // 英文模式就留一句中文（`browser-i18n` 会抓到 ✓）。
  // **`ui-text-density` 把原来那句长说明判成"人类界面上的大段解释"** ✓（它先红着被记成了绿 ✗ 见 docs ㊴）
  // ⇒ 屏幕上只留一句短的 ✓ 那两条"为什么"挪进 `#effectNote` 的 `title=`（判据 prescribed 的修法 ✓）
  // ⇒ **长的那条归 `I18N_EN_ATTR`** ✗ 不放这张表（同一个键放错表错过一次 ✓ 见那张表开头那条注 ✓）。
  // **第二次看真截图又把这句改短了一截** ✓（docs「只有看真截图」那条）：原先它开头把回执那句
  // 「这一笔对画面是空操作」逐字重复了一遍 ⇒ 相邻两行说同一句 ⇒ 结论由回执独占 ✓ 这句只负责指路 ✓。
  "为什么？见鼠标提示": "Why? See the hover hint",
  "删": "Delete",
  // **按钮切字那一处原本是漏的** ✓（第七轮实测 ✓）：点一行进入编辑态后 `button.textContent = "更新这个"`
  // ⇒ 英文界面里那颗按钮变成中文 ✗ 而 `browser-i18n` 从不点那一行 ⇒ 静态与运行时都没人守 ✗。
  "更新这个": "Update this one",
  "参数不是合法 JSON ⇒ 没有发送请求、没有写入任何改动。":
    "The parameters are not valid JSON ⇒ no request was sent and no change was written.",
  "回执说成功了，但效果清单里没把它读回来 ⇒ 现在没法说它存成了什么参数（刷新页面再看，或看下方日志）。":
    "The receipt says it worked, but the effect list did not read it back ⇒ there is nothing honest to say about the parameters it stored (reload the page, or check the log below).",
  // **请求根本没出门 ≠ 服务端拒绝** ✓（与「区域填充」那张卡同一口径 ✓）：三种动作各一条固定句 ✓。
  "未写入：本地内核没有这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试。":
    "Nothing written: the local kernel has no such endpoint (this request never reached the server) ⇒ tick Server render back on and retry.",
  "未更新：本地内核没有这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试。":
    "Nothing updated: the local kernel has no such endpoint (this request never reached the server) ⇒ tick Server render back on and retry.",
  "未删除：本地内核没有这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试。":
    "Nothing deleted: the local kernel has no such endpoint (this request never reached the server) ⇒ tick Server render back on and retry.",
  "对象": "Objects",
  "勾选对象后可实例化 / 编组 / 变换": "select objects to instance, group or transform them",
  "实例化": "Instance",
  "编组": "Group",
  "转为形状": "To shape",
  "转为路径": "To path",
  "变换": "Transform",
  "重采样": "Resample",
  "执行路径算子": "Run a path operator",
  "改笔触": "Edit stroke",
  "缩放%": "Scale %",
  "笔触色": "Stroke colour",
  "选中笔迹的粗细": "size of the selected stroke",
  "不透明": "Opacity",
  "位置": "Position",
  "路径算子": "Path operator",
  "reverse 反向": "reverse",
  "close 闭合": "close",
  "join 连接": "join",
  "merge 合并": "merge",
  "split 切开": "split",
  "boolean 布尔": "boolean",
  "布尔模式": "Boolean mode",
  "union 并": "union",
  "intersect 交": "intersect",
  "subtract 差": "subtract",
  "xor 异或": "xor",
  "切口节点": "Cut node",
  "渐变": "Gradient",
  "填充当前图层": "Fill the current layer",
  "起点": "Start",
  "终点": "End",
  "类型": "Type",
  "线性（角度 ↓）": "Linear (angle ↓)",
  "径向（从中心散开）": "Radial (outward from the centre)",
  "填充": "Fill",
  "历史（原子日志）": "History (atom log)",
  "全部类型": "All types",
  "全部操作者": "All actors",
  "重新载入": "Reload",
  "打一个存档点": "Take a checkpoint",
  "刷新存档点": "Refresh checkpoints",
  "详情": "Details",
  "回到此处": "Jump here",
  "开始变更集": "Start a changeset",
  "提交": "Commit",
  "放弃（整体撤销）": "Discard (undo the whole changeset)",
  "点上面任意一条，看它改了什么 ✓": "Click any row above to see what it changed",
  "缩略图": "Thumbnail",
  "原子日志（控制流）": "Atom log (control flow)",
  "点色块取色": "Click a swatch to take its colour",
  "取色写入": "Pick writes to",
  "笔刷色（缺省）": "brush colour (default)",
  "渐变起点": "gradient start",
  "渐变终点": "gradient end",
  "铺成背景": "Tile as background",
  "铺法": "Tiling",
  "平铺（缺省）": "Tile (default)",
  "拉伸": "Stretch",
  "等比铺满": "Cover",
  "设为背景": "Set as background",
  "WASM 计算内核": "WASM compute kernel",
  "本地乐观渲染": "Local optimistic rendering",
  "已加载": "loaded",
  // **WASM 状态卡的三种终态**（第 1132 轮；CI 的 `browser-i18n` 实测漏了第一条）：
  //   ⇒ 之前只在**服务端渲染模式**下出现 ✓ ⇒ 客户端模式的判据从没见过它们 ✗
  //     ⇒ 分片顺序一变（前面的判据把页面留在服务端模式）⇒ 整页中文 1 条 ⇒ 红 ✗
  //   ⇒ 三种都补上；`版本不匹配`／`不可用` 是失败态，判据不常走到，但同样要能翻译 ✓。
  "已按设置跳过（用服务端渲染）": "Skipped by setting (server rendering)",
  "不可用": "unavailable",
  "版本不匹配": "version mismatch",
  "首笔": "first stroke",
  "首帧": "first frame",
  "内核预热": "kernel warm-up",
  "最近一次响应": "last response",
  "反馈": "Feedback",
  "标注": "Annotations",
  "显示已解决": "Show resolved",
  "存储 / 维护": "Storage / maintenance",
  "孤儿数据": "Orphaned data",
  "统计": "Statistics",
  "回收孤儿": "Collect orphans",
  "降冷历史": "Demote history",
  "我确认（删除不可逆）": "I confirm (deleting cannot be undone)",
  "建议": "Suggestions",
  "待处理": "Open",
  "已接受": "Accepted",
  "已拒绝": "Rejected",
  "全部": "All",
  "评论": "Comments",
  "发表": "Post",
  "缩放": "Zoom",
  "无选区": "no selection",
  "画布": "Canvas",
  "新建 / 打开": "New / Open",
  "导出": "Export",
  "工程包": "Project archive",
  "路径": "Path",
  "导出工程": "Export project",
  // **∴ 第 249 轮 ✗**：**新按钮的文本**必须进这张表**✓ 否则 `ui-i18n-coverage` 判红
  //   （**∴ 它只看**页面里的文本节点**✗ 而**按钮文字就是文本节点 ✓）★
  "导出过程视频": "Export timelapse video",
  // **工程包卡片的重排文案**（第 1166 轮 ✓）：这四句是"导出下载到本机、导入从本机选文件"那次改动加的，
  // 当时**没进词表** ✗ ⇒ 英文模式下整张卡仍是中文 ✓。
  "导出下载到本机 ｜ 导入从本机选文件": "Export downloads to this machine ｜ Import picks a file from this machine",
  "服务器另存路径": "Server-side save path",
  "导入为": "Import as",
  "导入 .yanshi…": "Import .yanshi…",
  "导入为新文档": "Import as a new document",
  "用完即收": "Puts itself away",
  "Esc 收起": "Esc collapses it",
  "滚到哪、画到哪 ✓": "painted as you scroll",
  "打包整个文档为": "Pack the whole document as",
  "路径是服务器上的路径": "the path is a path on the server",
  "P 调色板 · T 纹理 · Esc 收起": "P palettes, T textures, Esc collapses",
  "偃师 Yanshi 查看器": "Yanshi Viewer",
  "偃师 Yanshi": "Yanshi",
  "文件 ▾": "File ▾",
  "新建": "New",
  "打开…": "Open…",
  "未连接": "disconnected",
  "◧ 工具栏": "◧ Toolbar",
  "◨ 面板": "◨ Panels",
  "⛶ 全屏": "⛶ Full screen",
  "⛶ 退出全屏（Esc）": "⛶ Exit full screen (Esc)",
  "新建文档": "New document",
  "取消": "Cancel",
  "创建": "Create",
  "打开文档": "Open document",
  "示例作品": "Sample works",
  "我的文档": "My documents",
  // **文档列表表头**（第 1166 轮 ✓）：`viewer.rs:113` 的列头（名字 / 尺寸 / 时间 ✓）——
  // 它随"打开面板重排"一起加进来时**漏了词条** ✗ ⇒ 英文模式下这四个字原样留着 ✓。
  "名字 · 尺寸 · 时间": "Name · Size · Time",
  "导入本地图片": "Import a local image",
  "另存为副本": "Save a copy",
  "另存为…": "Save as…",
  "关闭": "Close",
  // **删除文档**（第 1166 轮 ✓）：文件管理那件事加进来的确认对话框 ✓ ——
  // 标题 / 说明 / 按钮三处都漏了词条 ✗（`viewer.rs:147 / :151 / :156` ✓）。
  "删除文档": "Delete document",
  "删掉磁盘上的这份文档与只属于它的 blob。": "Delete this document from disk, together with the blobs only it uses.",
  "删除": "Delete",
  "画笔": "Brush",
  "粗细": "Size",
  "颜色": "Colour",
  "末端色": "End colour",
  "强度": "Strength",
  "羽化": "Feather",
  "字号": "Font size",
  "工作区": "Workspace",
  "绘画": "Paint",
  "修图": "Retouch",
  "校对": "Proof",
  "介质": "Medium",
  "示范点（v1）": "Example (v1)",
  "油画（v2）": "Oil (v2)",
  "水彩（v2）": "Watercolour (v2)",
  "马克笔（v2）": "Marker (v2)",
  "铅笔（v2）": "Pencil (v2)",
  "像素（v2）": "Pixel (v2)",
  "三条落笔路径 · 只作用于当前图层": "Three stroke paths, all constrained to the current layer",
  // **导出面板 region/LOD 补齐**（期 4 ✓）：新增控件的静态文案。
  "当前没有可用选区 ⇒ 先用选区工具拖一个（或清除选区后手填四个框）": "No selection available ⇒ drag a rectangle with the selection tool first (or clear it and fill the four boxes by hand)",
  "区域宽/高必须大于 0": "Region width/height must be greater than 0",
  "用选区": "Use selection",
  "只导当前层": "Current layer only",
  "没有当前图层 ⇒ 先选中一层再勾「只导当前层」": "No active layer ⇒ select a layer before ticking \"Current layer only\"",
  "区域": "Region",
  "档位": "Level",
  "缺省": "Default",
  "0 全分辨率": "0 full resolution",
  "1 一半": "1 half",
  "2 1/4": "2 quarter",
  "3 1/8": "3 eighth",
  // **命名锚点面板** ✓（`set_anchor` / `get_anchors` / `delete_anchor` 的 Web 入口 ✓）：静态文案。
  "锚点": "Anchors",
  "把关键点钉死在文档里 ⇒ 换会话、重启服务都读回同一组坐标，不必重新目测。":
    "Pin key points into the document ⇒ the same coordinates come back after a new session or a restart, no re-eyeballing.",
  "名": "Name",
  "钉住": "Pin",
  "用选区中心": "Use selection centre",
  "还没有锚点 ✓（填好名字与坐标点「钉住」，或开着「锚点」工具在画布上点一下 ✓）":
    "No anchors yet ✓ (fill in a name and coordinates and press Pin, or click the canvas with the Anchor tool ✓)",
  // **各面板的"空态"这一族** ✓：它们由 JS 在运行时写进文本节点 ✓ ⇒ 英文模式真机扫得到 ✗
  //（本轮把「对象」「批注」面板接进开机流程后 ✓，这两句第一次在英文模式下露出来 ✓）。
  "还没有标注 ✓": "No annotations yet ✓",
  "当前图层还没有对象 ✓": "No objects on this layer ✓",
  "还没有存档点 ✓（上面那个按钮可以打一个 ✓）": "No checkpoints yet ✓ (the button above creates one ✓)",
  "还没有变更集 ✓（点「开始变更集」再画 ✓）": "No changesets yet ✓ (press Start a changeset, then paint ✓)",
  "还没有评论 ✓": "No comments yet ✓",
  "还没有建议 ✓（AI 或代理可以用 suggest 提出 ✓）": "No suggestions yet ✓ (an AI or an agent can raise one with suggest ✓)",
  "填入": "Fill in",
  "坐标已填入 ✓（给个名字再点「钉住」）": "Coordinates filled in ✓ (give it a name, then press Pin)",
  "✗ 要先给名字 ✓（建议用能自解释的英文名，如 left_eye）":
    "✗ a name is required ✓ (prefer a self-explanatory English one, like left_eye)",
  "✗ x / y 都要填数字 ✓（可在画布上点一下「锚点」工具，或用选区中心填充）":
    "✗ both x and y must be numbers ✓ (click the canvas with the Anchor tool, or use the selection centre)",
  "当前没有可用选区 ⇒ 先用选区工具拖一个（或直接在框里手填 x / y ✓）":
    "No selection available ⇒ drag one with the selection tool first (or type x / y by hand ✓)",
  // **风格预设面板** ✓（`list_presets` / `preview_preset` / `apply_preset` 的 Web 入口 ✓）。
  "风格预设": "Style presets",
  "挑一套风格落到当前图层：落的是可调对象，不是一次性烘焙。先点「刷新」读回预设，题材可筛。":
    "Drop a look onto the current layer as editable objects, not a one-off bake. Press Refresh to read the library; the subject box filters it.",
  "题材": "Subject",
  "带修正段": "Include correction",
  "预览": "Preview",
  "预览中（临时文档渲染，不动原文档）…":
    "Rendering a proxy (throwaway document — yours is untouched)…",
  "预览失败（原因见日志）": "Preview failed (the log says why)",
  "没有匹配的预设 ⇒ 把题材清空再看": "No matching presets ⇒ clear the subject and retry",
  "还没有当前图层 ⇒ 先选一个图层再预览": "No current layer ⇒ pick a layer before previewing",
  "还没有当前图层 ⇒ 先选一个图层再应用": "No current layer ⇒ pick a layer before applying",
  "画笔 ▾": "Brush ▾",

  "笔刷": "Brushes",
  "（内置画笔）": "(built-in brushes)",
  "笔刷库": "Brush library",
  "素材": "Assets",
  "搜笔刷": "Search brushes",
  "★ 收藏": "★ Favourites"
};
const I18N_EN_ATTR = {
  // **效果卡那句"为什么"从可见长句改成 `title=`** ✓（第七轮 `ui-text-density` 红出来的 ✓ 见 docs ㊴）
  // ⇒ 它是**属性文案** ⇒ 归这张表 ✗ 放 `I18N_EN_TEXT` 会被 `ui-i18n-coverage` 判"属性漏词条"（实测 ✓）。
  "缺省参数常常本就无效果；参数名拼错也会被照单收下（内核只校验它认识的键）。":
    "Default parameters often have no effect at all, and a misspelled parameter name is simply stored as-is (the kernel only validates keys it recognises).",
  // **参考图叠加层 `<img>` 的 alt** ✓（本轮把 `alt` 纳进 `I18N_ATTRS` ✓）：
  // 图解不开时浏览器就把 alt 就地画在画布上 ✓ 那是**那块画面上唯一可见的字** ✓
  // 不翻 ⇒ 英文界面在画布正中冒中文 ✓（真机截图里看得见 ✓）。
  // **注意与 `I18N_EN_TEXT` 里那条「参考图」= "Reference" 不是一回事** ✗：
  // 那一条是**卡片标题** ✓ 两条各自成立 ⇒ 必须放这张表 ✗ 放进文本表就是重复键 ✓
  // 会把标题的译文悄悄顶掉 ✓（本轮第一次就放错了地方 ✓ 实测 alt 仍是中文 ✓）。
  "参考图": "Reference image",
  // **范围遮罩卡 + 渐变蒙版卡** ✓（本轮新增 ✓）：`ui-i18n-coverage` 现在也查模板属性 ✓。
  "读哪一层的像素：这一层的当前渲染就是采样的全部来源":
    "Which layer's pixels are read: that layer's current render is the only sampling source",
  "把蒙版挂到哪一层：只有这一层的效果被它限定":
    "Which layer carries the mask: only that layer's effect is limited by it",
  "亮度带按显示空间的明度取值；色相带按色相窗取值，近灰的像素恒不选":
    "A luminance band selects by luma in display space; a hue band selects by hue window, and near-grey pixels are never selected",
  "反选：把带内变成带外（反选烤进网格，不是挂上去再翻）":
    "Invert: select outside the band instead (the inversion is baked into the grid, not applied when attaching)",
  "亮度带下限（0–1，显示空间）": "Lower end of the luminance band (0–1, display space)",
  "亮度带上限（0–1，必须不小于下限）": "Upper end of the luminance band (0–1, must not be below the lower end)",
  "带边过渡的宽度（明度单位，0–0.5）": "Width of the band-edge transition (luma units, 0–0.5)",
  "色相窗的中心（0–360 度）": "Centre of the hue window (0–360 degrees)",
  "色相窗的全宽（度，大于 0 且不超过 360）": "Full width of the hue window (degrees, above 0 and at most 360)",
  "拿窗宽的多少来做软边（0–1）": "How much of the window width becomes the soft edge (0–1)",
  "线性：起笔一侧为 1、收笔一侧为 0；径向：圈心为 1、内切椭圆之外为 0":
"Linear: the start side is 1, the end side is 0; radial: the centre is 1, outside the inscribed ellipse is 0",
  "低频修色调、高频修细节互不干扰；烤出来的是此刻的像素快照，源图层往后的改动不会传播进来":
    "Fix colour on the low layer and detail on the high layer without disturbing each other; what you get is a pixel snapshot of this moment — later edits to the source layer do not propagate in",
  "在「高频绘画」层上涂抹用 188 作中性色（128 会压暗）；源图层保持可见，新组盖在它上面；要整组撤掉请用「历史（原子日志）」卡逐条回退":
    "Paint on the High paint layer with 188 as neutral (128 darkens); the source layer stays visible with the new group on top; to undo the whole group revert the entries one by one in the History (atom log) card",
  "烘焙范围遮罩并挂到所选图层；带内一个像素都没有时不会挂载":
    "Bake the range mask and attach it to the chosen layer; nothing is attached when the band holds no pixels",
  "取下「挂到」那一层当前的蒙版；蒙版本体留在历史里，只断开这一层":
    "Detach the current mask of the layer named in Attach to; the mask itself stays in history, only this layer is unlinked",
  "径向渐变：过渡带占半径的比例（0 = 硬边，1 = 整条半径都在过渡）":
    "Radial gradient: the transition band as a fraction of the radius (0 = hard edge, 1 = the whole radius fades)",
  "反选：起笔一侧变成 0": "Invert: the side you started on becomes 0",
  // **减淡加深卡** ✓（第五轮 ✓）的属性文案 ✓。
  "只改明暗、不改颜色的局部提亮与压暗；建出来的是一个组，涂抹在组内的层上":
    "Local lightening and darkening that changes value, not colour; it builds a group and you paint inside its layers",
  "按它的名字建组并把新组放在它上方": "Names the new group after this layer and places the group above it",
  "两条路都会改变画面（实测）：灰层法涂就见效，曲线法留两条可回改的曲线":
    "Both routes measurably change the picture: the grey layer works as you paint, curves leave two curve objects you can edit later",
  "曲线法的减淡强度（显示空间 0.05–0.95，0.5 = 无效果）；只对曲线法有意义":
    "Curves-route dodge strength (display space 0.05–0.95, 0.5 = no effect); only meaningful for the curves route",
  "曲线法的加深强度（显示空间 0.05–0.95，0.5 = 无效果）；只对曲线法有意义":
    "Curves-route burn strength (display space 0.05–0.95, 0.5 = no effect); only meaningful for the curves route",
  "建出「前缀 · 减淡加深」组；同名组已存在时复用而不叠第二套":
    "Builds a group named after the prefix; when a group of that name exists it is reused instead of stacking a second set",
  "建组；中点越界时不发送请求、不写入任何改动":
    "Build the group; when a midpoint is out of range nothing is sent and nothing is written",
  "灰层本身是中性灰，涂抹才生效：用白色低流量笔触提亮、黑色压暗；灰层是此刻的快照，不跟随下方修改":
    "The grey layer is neutral by itself and only painting makes it count: white low-flow strokes lighten, black darkens; the grey layer is a snapshot of now and does not follow later edits below it",
  // **频率分离卡** ✓ 的属性文案 ✓。
  "读哪一层烘焙：这一层的当前渲染就是分离的全部来源":
    "Which layer is baked: that layer's current render is the only source for the separation",
  "高斯 sigma（1–64）：越大低频越糊、留给高频的细节越少":
    "Gaussian sigma (1–64): larger means a blurrier low layer and less detail left for the high layer",
  "生成「前缀 · 频率分离」组与前缀 · 低频 / 高频 / 低频绘画 / 高频绘画 四层":
    "Creates a \"prefix · Frequency separation\" group with four layers: prefix · Low / High / Low paint / High paint",
  "烘焙并建组；源图层没有对象或半径越界时不发送请求、不写入任何改动":
    "Bake and build the group; when the source layer holds no objects or the radius is out of range, no request is sent and nothing is written",
  // **译文要塞得下那个框** ✓（实机截图看到的 ✗ 长句在 170 px 的输入框里被截成 "Defaults to the source l"✓）：
  // 意思不减 ✗ 只是把"the … layer name"这种从句写法压成一行 ✓。
  "缺省用源图层名": "Layer name by default",
  // **区域填充卡** ✓（第四轮新增 ✓）的 `title` / `placeholder` 文案 ✓。
  "填到哪一层：那一层已有的对象不受这次影响":
    "Which layer receives the fill: the objects already on it are untouched this time",
  "矩形给 x/y/w/h；椭圆给中心与两半径；折线给顶点；路径给 SVG 的 d":
    "Rectangle takes x/y/w/h; an ellipse a centre and two radii; a polygon its vertices; a path an SVG d string",
  "读当前选区的外框（选区来自服务端事实，不是本地猜测）；没有选区时如实说没有":
    "Reads the bounding box of the current selection (a server-side fact, not a local guess); says so when there is none",
  "左上角 x（文档像素）": "Top-left x (document pixels)",
  "左上角 y（文档像素）": "Top-left y (document pixels)",
  "宽度（像素，要大于 0）": "Width (pixels, must be above 0)",
  "高度（像素，要大于 0）": "Height (pixels, must be above 0)",
  "椭圆中心 x": "Ellipse centre x",
  "椭圆中心 y": "Ellipse centre y",
  "横向半径（要大于 0）": "Horizontal radius (must be above 0)",
  "纵向半径（要大于 0）": "Vertical radius (must be above 0)",
  "多边形顶点，至少 3 个（开放路径按隐式闭合填充）":
    "Polygon vertices, at least 3 (an open path is filled as if closed)",
  "x,y 空格或分号分隔，至少 3 个": "x,y pairs, space or semicolon separated, at least 3",
  "SVG 路径字符串（M/L/C/Q/H/V/Z，小写为相对命令）；也认 Bézier 曲线":
    "SVG path string (M/L/C/Q/H/V/Z, lowercase = relative); bezier segments accepted",
  "evenodd 按奇偶规则挖洞；nonzero 按绕数规则":
    "even-odd cuts holes by parity; nonzero by winding number",
  "填充色：送出的是 {r,g,b} 对象，服务端再把不透明度乘进 alpha":
    "Fill colour: sent as an {r,g,b} object, the server multiplies opacity into alpha",
  "0–1：服务端把它乘进颜色的 alpha": "0–1: the server multiplies it into the colour's alpha",
  "边缘过渡半径（文档像素，0 = 硬边）": "Edge transition radius (document pixels, 0 = hard edge)",
  "纯色，或把画面另一块的像素当纹理铺进来": "A flat colour, or the pixels of another patch of the canvas tiled in as a texture",
  "取纹理那块矩形的左上角 x": "Top-left x of the rectangle to take the texture from",
  "取纹理那块矩形的左上角 y": "Top-left y of the rectangle to take the texture from",
  "取纹理那块矩形的宽（要大于 0）": "Width of the texture rectangle (must be above 0)",
  "取纹理那块矩形的高（要大于 0）": "Height of the texture rectangle (must be above 0)",
  "落笔：形状尺寸不合法时不发送请求、不写入任何改动":
    "Drop the stroke: when the shape size is invalid no request is sent and nothing is written",
  // **从可见文本搬进 `title=` 的那句长解释** ✓（第五轮 ✓）：文案一个字没改 ✓ 只是换了位置 ✗
  // 于是词条跟着从 `I18N_EN_TEXT` 挪到这里 ✓（判据按属性查表 ✓ 漏了就成"英文界面 + 中文 tooltip"✓）。
  "「取画面这块」取的是此刻那块矩形的像素（服务端就地存成纹理），不是实时链接：先把那块画好再指它。羽化只作用于边缘的不透明度。":
    "Patch texture reads that rectangle's pixels as they are right now (the server stores them into a texture on the spot) — it is not a live link: paint that patch first, then point at it. Feather only affects opacity along the edge.",

  // **工具栏那两支新蒙版笔** ✓：`renderToolStrip()` 运行时把标签写进 `title` / `aria-label` ✓
  // ⇒ 静态判据看不到 ✗（它只读模板 ✓）⇒ 补词条才不会被"英文界面 + 中文 tooltip"打脸 ✓。
  "矩形蒙版": "Rectangle mask",
  "椭圆蒙版": "Ellipse mask",
  "线性渐变蒙版": "Linear gradient mask",
  "径向渐变蒙版": "Radial gradient mask",
  "蒙版画笔": "Mask brush",
  "蒙版擦除": "Mask eraser",
  // **直方图卡 + 整组撤销**（本轮补齐 ✓）：四个新 tooltip。
  // 静态判据 `ui-i18n-coverage` 不查属性 ✗，但 `browser-i18n` 在英文模式下会看到 ✗ ⇒ 都要有词条 ✓。
  "统计整文档的合成结果，还是只统计当前图层":
    "Statistics over the composited whole document, or over the current layer alone",
  "只统计当前选区那块矩形（选区来自服务端事实，不是本地猜测）":
    "Count only the rectangle of the current selection (the selection is server-side state, not a local guess)",
  "读取直方图（get_histogram，只读，不改画面）":
    "Read the histogram (get_histogram; read-only, it changes nothing on the canvas)",
  "把该组同一次操作产生的所有笔一次撤掉（revert_group，只撤这一组，不碰别的）":
    "Revert every stroke produced by that group's single operation (revert_group; only that group, nothing else)",
  "把勾选的对象上移一层（reorder_objects，一次只动一个对象）":
    "Move the checked object up one level (reorder_objects; one object at a time)",
  "把勾选的对象下移一层（reorder_objects，一次只动一个对象）":
    "Move the checked object down one level (reorder_objects; one object at a time)",
  "纵轴按对数画：平涂文档里那几笔颜色的桶只有几十个像素，线性轴下会被压成看不见（数值本身仍是服务端给的 ✓）":
    "The vertical axis is logarithmic: in a flatly painted document the colour buckets hold only tens of pixels and a linear axis crushes them out of sight (the numbers themselves come straight from the server ✓)",
  // **命名锚点面板** ✓（属性文案：静态判据不查属性 ✓，但英文模式下的 tooltip 归这里 ✓）。
  "写一个命名锚点（set_anchor）；同名再设 = 覆盖，不必先删":
    "Write a named anchor (set_anchor); setting the same name again simply overwrites it — no need to delete first",
  "用当前选区的中心填充 x / y（选区来自服务端事实）":
    "Fill x / y with the centre of the current selection (the selection comes from server-side state)",
  "读回文档里全部锚点（get_anchors）": "Read back every anchor stored in the document (get_anchors)",
  "可留空": "optional",
  // **风格预设面板** ✓（属性文案：按钮 tooltip + 题材 placeholder ✓）。
  "读回可用预设（list_presets）；题材框留空 = 全部":
    "Read the available presets (list_presets); leave the subject box empty for all of them",
  "如 夜景 / 人像": "e.g. night / portrait",
  "不落原文档，先看代理图（preview_preset + render，用当前图层）":
    "Render a proxy first — a throwaway document, your document stays untouched (preview_preset + render on the current layer)",
  "把这套预设落到当前图层（apply_preset）；强度与旋钮按上面的值":
    "Apply this preset to the current layer (apply_preset); strength and knobs use the values above",
  "把这条锚点的名字与坐标填回上面的框（改坐标时不必重新目测 ✓）":
    "Put this anchor's name and coordinates back into the boxes above (adjusting a coordinate without re-eyeballing ✓)",
  "删掉这个锚点（delete_anchor；不存在会报错，不会静默成功 ✓）":
    "Delete this anchor (delete_anchor; a missing anchor is an error, never a silent success ✓)",
  // **导出面板 region/LOD 补齐**（期 4 ✓）：两个新 title。
  "留空 ⇒ 整幅；填齐 x,y,w,h 才生效": "Leave empty ⇒ whole canvas; all four of x, y, w, h must be filled to take effect",
  "取样档位：lod:k ⇒ 输出尺寸 = 区域 ÷ 2^k（向上取整），不经过全分辨率中间缓冲": "Sampling level: lod:k ⇒ output size = region ÷ 2^k (rounded up), skipping the full-resolution intermediate buffer",
  "用当前选区的包围盒填充区域四个框（选区来自服务端事实）": "Fill the four region boxes with the current selection's bounding box (the selection comes from server-side state)",
  "只导当前图层（隐藏层也照导——导出某一层是明确要求）": "Export only the current layer (hidden layers export too — exporting a named layer is an explicit request)",
  "服务端渲染（可只导一个区域、可降档取样、可按尺寸缩放），结果直接下载到本机（export_png）": "Server-side rendering (export one region, downsampled by level, or scaled to a size); the result downloads straight to this machine (export_png)",
  "文本字体：内置点阵走服务端位图图集；其余走浏览器 Canvas 渲染系统字体（支持中文、emoji、任意字号）": "Text font: the built-in bitmap uses the server-side glyph atlas; the others render system fonts with the browser canvas (Chinese, emoji and any size)",
  "导出成分层 PSD（图层/组/不透明度/可见性/混合模式都带出去）": "Export as a layered PSD (layers, groups, opacity, visibility and blend modes included)",
  "查看选中对象的版本历史（原子版本链）": "Show the version history of the selected object (its atom version chain)",
  "用笔触自动生成渐变过渡（替代手动拼笔）": "Generate a gradient transition from brush strokes instead of dabbing it by hand",
  // 新按钮的 title 也要有词条（静态判据不查属性，但 browser-i18n 在英文模式下会看到）。
  "笔刷压感动力学（MCP set_brush_dynamics 的 Web 入口）：压感→笔触大小的响应曲线": "Brush pressure dynamics (the web entry for MCP set_brush_dynamics): the pressure-to-size response curve",
  "轻压细、重压粗（柔和）": "Light pressure thin, heavy pressure thick (soft)",
  "线性响应": "Linear response",
  "轻压粗、重压变化小（硬）": "Light pressure thick, little change under heavy pressure (hard)",
  "文件：新建 / 打开 / 导入 / 导出（顶栏这一个入口，信息面板里不再重复）": "File: new / open / import / export; this single entry point lives in the top bar and is not repeated in the info panel",
  "隐藏 / 显示左侧工具栏（快捷键 [ ）": "Show or hide the left toolbar (shortcut [ )",
  "隐藏 / 显示右侧面板（快捷键 ] ）": "Show or hide the right panels (shortcut ] )",
  "全屏画布（快捷键 Tab，Esc 退出）": "Full-screen canvas (shortcut Tab, Esc to exit)",
  "退出全屏画布（Esc）": "Exit full-screen canvas (Esc)",
  "文档以 id 作为名字（也是主键）。换个名字即可并存多份作品；重名会提示。": "A document is named by its id, which is also its primary key; a different name keeps several works side by side, and a duplicate name is reported",
  "例如 我的第一幅画": "for example my-first-painting",
  // **粗细控件与新建尺寸的 tooltip**（本改动新加 ✓）——
  // `ui-i18n-coverage` **不检查**属性（它只管文本节点）✗，但 `I18N_EN_ATTR` 是
  // 英文模式下 tooltip 的真源 ✓ ⇒ 新加的属性文案都要有词条 ✓。
  "画笔粗细（像素）。拖滑杆、点 −/＋，或直接输入数值。":
    "Brush size in pixels; drag the slider, use the minus and plus buttons, or type a number.",
  "细一档": "one step thinner",
  "粗一档": "one step thicker",
  "画笔粗细的精确数值（像素）": "exact brush size in pixels",
  "支持浏览器能解码的任何格式（PNG/JPEG/WebP）。图片在新图层上按原始像素导入。": "Any format the browser can decode (PNG/JPEG/WebP); the image arrives on a new layer at its original pixels",
  "以新 id 保存一份完整副本（原文档保留，可逆）。文档以 id 为主键，因此这里填的是新文档的 id。": "Save a complete copy under a new id, leaving the original in place; the new id goes here because a document's id is its primary key",
  "新文档 id": "new document id",
  "一笔多色：同一条笔迹上从「颜色」渐变到「末端色」（Loaded Brush；与 MCP 的 brush_stroke.color_to 同一条实现）": "One stroke, several colours: it fades from the start colour to the end colour along the stroke (Loaded Brush, the same implementation as the MCP brush_stroke.color_to)",
  "布局预设（绘画 / 修图 / 校对）": "Layout presets (paint / retouch / proof)",
  "画笔 = MyPaint .myb（Hokusai 引擎）；介质 = 我们自己的插件（油画/水彩/…）；内置画笔 = 纯几何无物理。每条笔触只作用于当前图层：跨图层只是普通叠加，介质的湿搅/混色不跨层。": "Brush = MyPaint .myb through the Hokusai engine; Medium = our own plugins (oil, watercolour, …); built-in brushes are pure geometry with no physics. Every stroke affects only the current layer: crossing layers is plain compositing and a medium's wet mixing does not cross layers",
  "折叠 / 展开画笔区（快捷键 \\ 也可；折叠只隐藏控件，不改你选好的笔与颜色）": "Collapse or expand the brush section (the backslash shortcut also works); collapsing hides controls without changing your brush or colour",
  "MyPaint .myb 笔刷（Hokusai 引擎 ⇒ 由服务端落笔；首次点开时载入）": "MyPaint .myb brushes on the Hokusai engine, so strokes are committed by the server and the list loads the first time you open it",
  "把落笔的点当平滑曲线（Catmull-Rom，曲线过这些点）——手绘的折线不再有硬角": "Treat the stamped points as a smooth curve (Catmull-Rom, so it passes through them), which removes the hard corners of a hand-drawn polyline",
  "笔刷库：每支笔刷都带真实落笔的效果图（滚到哪画到哪）；点一行就换那支笔": "Brush library: every brush carries a preview of a real stroke, painted as you scroll; clicking a row switches to that brush",
  "把调色板 / 纹理浮到画布上（再点一次收回，卡片会回到原来的位置）": "Float the palettes and textures over the canvas; click again to put them back where they were",
  "这支笔刷真实落一小笔的样子（服务端 brush_preview，与落笔同一条实现）": "What a real stroke with this brush looks like, from the server's brush_preview, the same implementation that paints strokes",
  "名字片段，如 knife / pen": "part of a name, such as knife or pen",
  "把当前选中的笔刷加入/移出收藏（存在工作区偏好里 ✓，MCP 也能读到 ✓）": "Add or remove the selected brush from favourites; kept in the workspace preferences and readable through MCP",

  // **属性文案（title / placeholder / aria-label）的英文词条补齐** ✓（本轮 i18n 属性覆盖巡检抓的 ✓）：
  // 原来 `ui-i18n-coverage` **只查文本节点** ✗ ⇒ 模板里 41 条中文 tooltip **一条词条都没有** ✓
  // ⇒ 英文界面下按钮文字是英文 ✓、**悬停提示整句中文** ✗（`browser-i18n` 也抓不到 ✓，
  // 因为它同样只看可见文本 ✓）。判据已补成**属性也查** ✓（`scripts/ui-i18n-coverage.mjs` ✓）。
  "一次性拒绝所有勾选的建议（reject_suggestions）": "Reject every checked suggestion at once (reject_suggestions)",
  "一次性接受所有勾选的建议（accept_suggestions）": "Accept every checked suggestion at once (accept_suggestions)",
  "为什么拒绝（可选）": "why you rejected it (optional)",
  "写进 reject_suggestion 的 reason：拒绝理由会留在历史里，供他人判断": "Goes into reject_suggestion's reason: a rejection's rationale stays in the history for others to judge",
  "一行一份作品：名字、尺寸、原子数与创建时间；点「打开」切换过去。": "One row per work: name, size, atom count and creation time; press Open to switch to it.",
  "上传一张图，半透明铺在画布下层用于临摹（set_reference）": "Upload an image and lay it translucently under the canvas for tracing (set_reference)",
  "上移一层": "Move layer up",
  "下移一层": "Move layer down",
  "丢弃本地队列里尚未补交的改动（需点两次确认）": "Drop the changes still queued in this browser (needs two clicks to confirm)",
  "从本机选文件，走 data_base64 上传（import_asset）": "Pick a file from this machine and upload it as data_base64 (import_asset)",
  "写一条评论…": "Write a comment\u2026",
  "写进对象的 metadata.note（update_object 的 patch）": "Writes into the object's metadata.note (a patch for update_object)",
  "删除图层（可撤销）": "Delete layer (undoable)",
  "勾上：取完色就把浮层收起（想连续试色就取消勾选）": "Checked: the dock closes after every pick; uncheck to try several colours in a row",
  "勾选对象后可「实例化」「编组」「变换」（实例与 master 联动，设计 9.1）。「重采样」只作用于光栅对象（导入的图片、PSD 合成图）——手画的笔迹是 stroke，服务端会明确拒绝并告知类型（先用「转为形状」或改为导入图片）。": "Select objects to instance, group or transform them (instances follow their master, design 9.1). Resample only applies to raster objects (imported pictures, PSD composites): a hand-drawn stroke is not raster, and the server refuses it and says why (convert it to a shape first, or import a picture instead).",
  "参照 Met／NGA 的公有领域德加原作，只用来看风格：没有导入任何原作位图，每个像素都是笔刷/介质笔触。": "Modelled on public-domain Degas studies at the Met and the NGA, as a style reference only: no source bitmap is imported, every pixel comes from brush or medium strokes.",
  "图层列表": "Layer list",
  "复制图层（含对象）": "Duplicate the layer with its objects",
  "孤儿是「上传过、但没有任何原子引用」的数据（设计 §6.3）；回收按 TTL 走。": "Orphans are blobs that were uploaded but no atom references (design 6.3); reclaiming follows the TTL.",
  "导出直接把 .yanshi 下载到你的电脑；下面的路径留空即可（填了就在服务器上再存一份）。导入从你的电脑选 .yanshi 文件（分片上传，整包多大都行）。": "Export downloads the .yanshi straight to your computer; leave the path below empty (filling it keeps a second copy on the server). Import picks a .yanshi from your computer, uploaded in chunks so any package size works.",
  "工具": "Tools",
  "把 CC0 纸张 / 画布纹理铺成背景（会新建一层并沉到最底）。": "Lay a CC0 paper or canvas texture as the background; it adds a new layer at the very bottom.",
  "把从第一笔到现在的过程渲染成视频（本地内核 ＋ WebCodecs，不需要联网）": "Render everything from the first stroke to now as a video (local kernel plus WebCodecs, no network needed)",
  "把勾选实例改挂到该 master（link_to_master）": "Re-point the checked instances at this master (link_to_master)",
  "把勾选对象加入该组（add_to_group）": "Add the checked objects to this group (add_to_group)",
  "把勾选对象移出该组（remove_from_group）": "Take the checked objects out of this group (remove_from_group)",
  "把勾选的实例从 master 断开（detach_instance）": "Detach the checked instances from their master (detach_instance)",
  "把当前色块存成 .gpl 调色板资产（save_palette）": "Save the current swatches as a .gpl palette asset (save_palette)",
  "把构建/平台/配置（已去密）、文档元数据、原子日志尾部（注明截断）、服务端 stderr 环形缓冲、渲染告警（含被跳过的补丁）、阶段耗时与当前缩略图打成一个 zip 下载；包内有 README 逐条说明。只读，不改文档。": "Bundles build, platform and configuration (secrets stripped), document metadata, the tail of the atom log (truncation noted), the server's stderr ring buffer, render warnings including skipped patches, stage timings and the current thumbnail into one zip; a README inside explains each part. Read-only: it changes no document.",
  "把界面切成英文（English）": "Switch the interface to English",
  "文档目录会被移除；它引用的 blob 里，只有它自己引用的会被删掉，被别的作品或离线变更集引用的一律保留。": "The document's directory is removed; of its blobs only the ones it alone references are deleted, and anything another work or an offline changeset references is kept.",
  "新建图层": "New layer",
  "清除当前参考图（clear_reference）": "Clear the current reference image (clear_reference)",
  "点色块即取色（写回笔刷颜色）；来源是随包发布的，或你自己导入的。": "Click a swatch to take its colour (it writes back to the brush colour); palettes either ship with the package or come from your own imports.",
  "画布显示比例（%）；回车或失焦生效 —— 滚轮不缩放也不平移（防误触）": "Canvas display scale (percent); Enter or blur applies it. The wheel neither zooms nor pans, so it cannot be nudged by accident",
  "留空 ⇒ 只下载到本机": "Leave empty to download only to this machine",
  "留空 ⇒ 用包里的 id": "Leave empty to use the id inside the package",
  "离线时改动先写进浏览器本地队列，联网后按顺序补交；服务端上的这份文档若已变化则暂停补交": "While offline, changes go into a queue in this browser and are replayed in order once you are back online; replay pauses if the server's copy of this document has moved on",
  "立刻把本地队列按顺序补交给服务端": "Replay the local queue to the server in order right now",
  "给勾选实例设置同步策略（update_sync_policy）": "Set the sync policy of the checked instances (update_sync_policy)",
  "给当前图层填一层渐变（天空 / 底色 / 光照过渡）——确定性：同样的输入永远同样的像素。": "Fill the current layer with a gradient (sky, ground colour, light falloff). Deterministic: the same input always yields the same pixels.",
  "读回选中对象的完整记录（get_object）": "Read back the full record of the selected object (get_object)",
  "选中对象解析后的生效状态（get_resolved_state）": "The resolved, effective state of the selected object (get_resolved_state)",
  "参考图摆在文档的哪个矩形（像素）；留空 = 铺满整幅": "Which rectangle of the document (in pixels) the reference sits on; leave blank to cover the whole canvas",
  // **★ 图层面板那几颗 tooltip ✗ ★**（第六轮 ✓）：它们是 `renderLayerPanel` **运行时**建的 ✓
  //   `ui-i18n-coverage` 只管模板文本节点 ⇒ **看不见这条路** ✓ 于是英文界面里鼠标停在眼睛／锁／混合模式
  //   上冒出来的仍是中文 ✗（真机截图确认过：整页英文、tooltip 中文 ✓）。
  "隐藏图层": "Hide this layer",
  "显示图层": "Show this layer",
  "解锁图层": "Unlock this layer",
  "锁定图层（锁定后不能改内容）": "Lock this layer (a locked layer refuses content changes)",
  "混合模式": "Blend mode",
  // 批注输入框的占位符 ✓ 同一个盲区（`placeholder` 也在 `I18N_ATTRS` 里 ✓）。
  "写点什么…": "Write something…",
  // 没有服务端时那条说明 ✓（整句是常量 ⇒ 走精确键 ✓）。
  "本部署没有服务端 ⇒ 只能用本地内核；此开关不可用":
    "This deployment has no server ⇒ only the local kernel is available; this switch does nothing",
  // 效果列表那两行 ✓：拼接的两段都是**已知常量** ⇒ 整句仍是确定字符串 ⇒ 精确键 ✓
  "点一下：编辑这个调整 ✓": "Click to edit this adjustment ✓",
  "点一下：编辑这个滤镜 ✓": "Click to edit this filter ✓",
  "删掉这个效果对象（服务端落一条原子，可以再用撤销退回来）":
    "Delete this effect object (the server writes one atom; undo brings it back)",

  // **工具栏那 21 支笔的标签** ✓（第六轮实测 ✓）：`renderToolStrip()` 写的是
  //   `label + " (快捷键)"` ⇒ **整句精确键永远配不上** ✗（括号里那个字母是键盘提示 ✓ 不是词 ✓）
  //   ⇒ 于是英文界面里 24 颗按钮的 tooltip 与 `aria-label` 全是中文 ✓（无障碍读屏也跟着念中文 ✗）。
  //   走下面 `attrEnglish` 里那条「标签 (KEY)」的拆解规则 ⇒ 只补**标签**这一层 ✓ 快捷键原样带过去 ✓。
  "画笔": "Brush",
  "矩形": "Rectangle",
  "椭圆": "Ellipse",
  "橡皮": "Eraser",
  "仿制": "Clone stamp",
  "修复": "Healing brush",
  "涂抹": "Smudge",
  "液化推": "Liquify push",
  "液化旋": "Liquify twirl",
  "液化缩": "Liquify pinch",
  "吸管": "Eyedropper",
  "移动对象": "Move object",
  "移动图层": "Move layer",
  "平移画布": "Pan canvas",
  "选区": "Selection",
  "清除选区": "Clear selection",
  "文本": "Text",
  "介质": "Medium",
  "标注": "Annotation",
  "锚点": "Anchor",
  "填充图层": "Fill layer",
  // 刷压强度那行的 tooltip ✓ 与缩略图的 alt ✓（同一条盲区里剩下的两个「整句常量」✓）。
  "落笔强度": "Stroke strength",
  "缩略图": "Thumbnail",
  // **开机展示图那两张的 `alt`** ✓（第六轮判据抓的 ✓）：`viewer.rs:163/168` 把中文名**写死在模板里** ✓
  //   ⇒ 这是**我们自己的文案** ✗ 不是用户数据 ✗ 与同名的 `figcaption`（走 `I18N_EN_TEXT` ✓ 已有词条 ✓）
  //   是两处不同的位置 ✗ 必须在这张表里再立一条 ✓（`alt` 属于无障碍文本 ✓ 与 `aria-label` 同一类 ✓）。
  "后台舞者": "Dancers backstage",
  "四位舞者": "Four dancers",

};
/** **属性的参数化句式** ✓（第六轮 ✓）：`I18N_EN_ATTR` 是**整句精确匹配** ✓
 *  而下面这几条 tooltip 形如「**数据 + 后缀**」✓（资产名／色号／预设名是**服务端数据** ✗ 不翻 ✓）
 *  ⇒ 精确键永远配不上 ✗ 只能按句式认 ✓。
 *  **与文本那侧的 `m1…m36` 同一条设计** ✓：只翻译框架 ✓ 数据原样带过去 ✓。 */
const I18N_EN_ATTR_PATTERNS = [
  // 「资产名（点击选中 ✓）」——纹理缩略图
  [/^(.*)（点击选中 ✓）$/, (m) => m[1] + " (click to select ✓)"],
  // 「资产名 ⇒ 缩略图没发出来（`/textures/` 路由或资产目录有问题 ✓）」——缩略图 404 那条
  //   ⚠️ 原句里 `/textures/` 两侧**带反引号** ✓（产品文案就这么写 ✓）⇒ 句式必须照抄 ✗ 少写就永远配不上 ✓
  [/^(.*) ⇒ 缩略图没发出来（`\/textures\/` 路由或资产目录有问题 ✓）$/,
    (m) => m[1] + " ⇒ the thumbnail never arrived (the /textures/ route or the asset directory is broken ✓)"],
  // 「色号（点击取色 ✓）」——调色板色块（前面可能带颜色名 ✓ 都算数据 ✓）
  [/^(.*)（点击取色 ✓）$/, (m) => m[1] + " (click to pick this colour ✓)"],
  // 「预设名 预览」——笔刷／预设的小图 alt
  [/^(.*) 预览$/, (m) => m[1] + " preview"],
  // 离线队列那条**带数字**的说明 ✓（上限 N 条 / M MB ✓ 可选的「其他文档还有 K 条」✓）
  [/^离线时改动先写进浏览器本地队列，联网后按顺序补交；服务端上的这份文档若已变化则暂停补交（上限 (\d+) 条 \/ (\d+) MB）(。其他文档还有 (\d+) 条未补交)?$/,
    (m) => "Offline edits are queued in this browser and replayed in order once you reconnect; " +
      "if the server's copy of this document has changed, replaying pauses (limit " + m[1] +
      " entries / " + m[2] + " MB)" +
      (m[4] ? ". Another " + m[4] + " entries are still queued for other documents" : "")],
];

//! **工具注册表**（`ALL_TOOLS` + 参数常量）——**纯数据**，无逻辑。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **边界**：`ID_ARGS` / `LAYER_ID` 两个参数常量、`pub const ALL_TOOLS: &[ToolSpec]`，
//! 以及 `PHOTO_TOOLS` / `SVG_TOOLS` 两个 workflow 预设白名单常量（设计 §7 F 族）。
//! `dispatch` **留在 `tools.rs`**（它含逻辑）。
//!
//! **可见性**：`ALL_TOOLS` 保持 `pub`（父模块 `use tools_registry::*` 即可用 ✓）。

use super::*;

// ---------------------------------------------------------------------------
// 工具表（10.2）
// ---------------------------------------------------------------------------

const ID_ARGS: &[ParamSpec] = &[param!("object_id", String, true, "对象 id")];
const LAYER_ID: &[ParamSpec] = &[param!("layer_id", String, true, "图层 id")];

/// import_image 的参数表（RAW 第二包，设计 9.4 条件注册）：
/// feature `raw` **关**时与基线逐项一致（多一个参数都算漂移，注册表守卫
/// 测试钉死）；**开**时恰好多一组 `raw_options`——工具总数不变。
#[cfg(not(feature = "raw"))]
const IMPORT_IMAGE_PARAMS: &[ParamSpec] = &[
    param!("layer_id", String, true, "目标图层"),
    // **`width`／`height` 放在 `bitmap` **里面**，不是顶层** ✓（第 921 轮；我为此撞过两次 ✗）
    //   **⇒ 依据** ✗：顶层给 `height` 会被框架拒绝 ✓（错误信息列出可用参数 ✓），
    //     而 `bitmap.width`／`bitmap.height` 是**接受**的 ✓（真包导出里就是这么写的 ✓）
    //   ⇒ 所以描述里要写清位置 ✓ —— 否则调用方（包括我自己的探针 ✓）会照 top-level 写 ✗。
    param!(
        "bitmap",
        Object,
        true,
        "{blob_hash,size,mime_type}；**尺寸写在这里面**：{width,height}（不是顶层）"
    ),
    param!("region", Any, true, "放置区域 {x,y,w,h}"),
    param!("object_id", String, false, "对象 id（缺省自动生成）"),
    param!(
        "medium",
        Object,
        false,
        "{id,version} 介质描述符：随这条原子一并记录（设计 11.1）"
    ),
];
/// import_image 的参数表（feature `raw` 开）：基线五项 + `raw_options` 一组。
#[cfg(feature = "raw")]
const IMPORT_IMAGE_PARAMS: &[ParamSpec] = &[
    param!("layer_id", String, true, "目标图层"),
    // **`width`／`height` 放在 `bitmap` **里面**，不是顶层** ✓（第 921 轮；我为此撞过两次 ✗）
    //   **⇒ 依据** ✗：顶层给 `height` 会被框架拒绝 ✓（错误信息列出可用参数 ✓），
    //     而 `bitmap.width`／`bitmap.height` 是**接受**的 ✓（真包导出里就是这么写的 ✓）
    //   ⇒ 所以描述里要写清位置 ✓ —— 否则调用方（包括我自己的探针 ✓）会照 top-level 写 ✗。
    param!("bitmap", Object, true, "{blob_hash,size,mime_type}；**尺寸写在这里面**：{width,height}（不是顶层）"),
    param!("region", Any, true, "放置区域 {x,y,w,h}"),
    param!("object_id", String, false, "对象 id（缺省自动生成）"),
    param!("medium", Object, false, "{id,version} 介质描述符：随这条原子一并记录（设计 11.1）"),
    param!("raw_options", Object, false, "仅当 bitmap 是 RAW 源（mime image/x-yanshi-rawsrc）时可用：展开参数 {wb: as_shot|auto（缺省 as_shot）, demosaic: malvar|bilinear（缺省 malvar）, pre_exposure_ev: -4..4（缺省 0，不 clamp、越界报错）, lens_correction: off|auto（缺省 off，D5 定版；auto = 文件自带校正逐项优先 + Lensfun 库匹配）, lens_override: string|null（手动点名镜头库中的镜头，查无此镜报错）, highlight_recovery: chroma|floor（缺省 chroma 真色度恢复，floor = 第三包保底机制）}；给到非 RAW blob 报错"),
];

/// 核心层 112 个 + 扩展组中已实现的工具（10.2）。
pub const ALL_TOOLS: &[ToolSpec] = &[
    // ---- 查询 ----
    ToolSpec {
        name: "get_document",
        profile: Profile::Core,
        summary: "读取文档元信息与统计（尺寸、head、图层/对象数、渲染水位、文档级缩略图）。**缩略图是一份缓存**：落后于 head 时会**当场重建**再返回 —— 所以「要一张最新的文档缩略图」就用它（`wait_for_render=true` 不承诺缩略图是新的，只承诺这一笔的渲染完成了）",
        mutating: false,
        params: &[param!(
            "preview_size",
            Any,
            false,
            "缩略图档位 64/128/256 或 false 跳过（缺省 256）"
        )],
    },
    // **★ 命名锚点（真机报告 2026-10-15）★**：解决"跨会话坐标漂移 ⇒ 草稿层与彩色层错位" ✓。
    //   三个工具刻意用**最小**接口（Muse 的方案原文 ✓）：set / get / delete ✓。
    ToolSpec {
        name: "set_anchor",
        profile: Profile::Core,
        summary: "**★ 在文档里钉死一个命名锚点 ★**（真机报告 2026-10-15：解决跨会话「错位」✓）。起稿阶段把关键点定死（left_eye／right_eye／nose_tip／mouth_left…✓），之后**每个新会话**（以及**重启之后** ✓）都用 `get_anchors` 读回**同一组坐标** ✓，**不许重新估算** ✗ —— 那正是草稿层与彩色层对不齐的根因 ✓（用户已实测：MCP 坐标系本身没有问题 ✓）。**锚点存在文档里** ⇒ 随原子日志持久化 ✓ ⇒ 清 `/tmp`、换 session、重启服务都**不丢** ✓。同名再设 = **覆盖** ✓（改主意时直接覆盖，不必先删 ✓）。",
        mutating: true,
        params: &[
            param!("name", String, true, "锚点名（建议用能自解释的英文名 ✓，如 left_eye ✓）"),
            param!("x", Number, true, "画布坐标 x（逻辑像素，与 points 同一坐标系 ✓）"),
            param!("y", Number, true, "画布坐标 y"),
            param!("note", String, false, "可选备注（如「往外眼角偏 2px」✓）"),
        ],
    },
    ToolSpec {
        name: "get_anchors",
        profile: Profile::Core,
        summary: "**★ 读回全部命名锚点 ★**（配 `set_anchor` ✓）。**开画前先读它** ✓：拿到的是**这份文档定死过的**坐标 ✓，不是要你重新目测的估计值 ✗。返回 `{anchors: {名字: {x, y, note?}}, count}` ✓。空文档返回空表（不是错误 ✓）。",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "delete_anchor",
        profile: Profile::Core,
        summary: "删掉一个命名锚点（**不存在就报错** ✓，不静默成功 ✗ —— 免得你以为删掉了 ✓）。",
        mutating: true,
        params: &[param!("name", String, true, "要删的锚点名")],
    },
    ToolSpec {
        name: "calibrate",
        profile: Profile::Core,
        summary: "**★ 坐标校准（Muse 建议 2）★**：在四角与中心画品红实心方块 ⇒ **渲染整幅** ⇒ **在像素里把每个标记量回来** ⇒ 回报每点的 `expected`/`actual`/`dx`/`dy`/像素数 ✓。**它测的是整条链** ✓（写原子 → 折叠 → 栅格化 → 取原始像素 → 定位 ✓）⇒ 任何环节引入平移/缩放/翻转都会被它抓到 ✓。**定位是「排除工具」** ✓（不是找 bug：MCP 坐标系已被实测证明无误 ✓）—— 它回报偏差在容差内时 ✓，错位就该回到**你自己的估算**上 ✓。**缺省自清理**（`cleanup: true` ✓）⇒ 跑完画布对象数回到原样 ✓（不污染正要画的画 ✓）。",
        mutating: true,
        params: &[
            param!("size", Number, false, "标记边长（像素，缺省 16 ✓；限 4–96 ✓）"),
            param!("points", Any, false, "自定义测点 `[[x,y],…]`（缺省＝四角 + 中心共 5 点 ✓，按 size 内缩 ✓）"),
            param!("tolerance", Number, false, "容差（像素，缺省 1.0 ✓）"),
            param!("layer_id", String, false, "标记放哪个图层（缺省自建 `__calibration` ✓）"),
            param!("cleanup", Boolean, false, "跑完是否撤掉标记（缺省 true ✓）"),
        ],
    },
    ToolSpec {
        // **只读的"观察口"** ✓（第 856 轮 ✓）：外部实测报告 #13 —— "一旦落笔，画布就变成
        // **完全不透明的黑盒**" ✗ ⇒ agent 只能"导出 PNG 再解析" ✓。
        // 本工具**不改动画面的任何像素** ✓（只渲染 1×1 区域并读出 ✓）⇒
        // 它同时解掉另一处待决策：`tool-reference-delta-e` 的"**想测量却必须先改画面**" ✗。
        name: "sample_color",
        profile: Profile::Core,
        summary: "读取画布上某一点的**显示空间颜色**（只读 ✓，不改变画面 ✓）。落笔后用它可以自查颜色，不必导出 PNG 再解析 ✓",
        mutating: false,
        params: &[
            param!("x", Number, true, "画布 x（文档坐标，像素 ✓）"),
            param!("y", Number, true, "画布 y（文档坐标，像素 ✓）"),
        ],
    },
    ToolSpec {
        // **批量取色** ✓（YANSHI_ISSUES §18.2 ✓）：`sample_color` 每个点要渲染**两次**
        // （`render_region` ＋ `render_region_raw` ✗）⇒ 一笔一取色要付 N 倍代价 ✗。
        // 这里**一次调用、一次渲染**（覆盖所有点的包围盒 ✓），再按坐标索引像素 ✓。
        name: "sample_colors",
        profile: Profile::Core,
        summary: "一次读多个点的颜色（只读 ✓）：一次渲染覆盖所有点的包围盒，再逐点索引 ⇒ 比 N 次 sample_color 省得多",
        mutating: false,
        params: &[
            param!("points", Array, true, "[[x,y], …] 画布坐标（文档坐标，像素 ✓）；一次调用取 N 个点，只渲染一次"),
        ],
    },
    ToolSpec {
        name: "get_histogram",
        profile: Profile::Core,
        summary: "读取直方图（只读 ✓，只回数值不回图）：每通道 256 桶计数 + 亮度桶 + 均值/中位数/百分位摘要。作用域三选一：缺省整文档渲染结果；给 layer_id 只统计那一层；给 region 只统计该区域（两者都给 = 该层在该区域内）",
        mutating: false,
        params: &[
            param!("layer_id", String, false, "只统计这个图层（缺省 = 整文档合成结果）"),
            param!(
                "region",
                Object,
                false,
                "只统计这个区域 {x,y,w,h}（文档坐标，像素；缺省 = 整幅画布）"
            ),
        ],
    },
    ToolSpec {
        name: "get_state",
        profile: Profile::Core,
        summary: "读取当前 HEAD 状态摘要与预览地址",
        mutating: false,
        params: &[
            param!("include_objects", Boolean, false, "是否包含对象列表"),
            param!("include_hidden", Boolean, false, "是否包含不可见图层/对象"),
            param!(
                "preview_size",
                Any,
                false,
                "缩略图档位 64/128/256 或 false（缺省 256）"
            ),
        ],
    },
    ToolSpec {
        name: "list_documents",
        profile: Profile::Core,
        summary: "列出工作区里的文档（doc_id、宽高、层数、存活对象数）—— 续画时「先列再开」",
        mutating: false,
        params: NO_PARAMS,
    },
    ToolSpec {
        name: "list_layers",
        profile: Profile::Core,
        summary: "列出图层（z 序、可见性、不透明度、混合模式、蒙版）",
        mutating: false,
        params: NO_PARAMS,
    },
    ToolSpec {
        // **一次拿画布状态** ✓（YANSHI_ISSUES §18.4 ✓）
        //   ⇒ **∴ 报告里说"一次调用拿到尺寸 ＋ 层列表 ＋ 选区 ＋ 掩码"✗ ⇒ **∴ 免得 agent 打四五次 ✓**
        name: "describe_canvas",
        profile: Profile::Core,
        // **★ 把"内存只增不减"这条实测性质写进工具面 ★**（第 1560 轮；外部 `B5`）
        //   **⇒ 他们的原话**：「服务端内存只增不减，只能靠 close/reopen 释放 ⇒
        //     这个性质**我是自己试出来的**，工具面没有告诉我」✗
        //   **⇒ ⇒ 他们的实测**：`close → reopen` ⇒ **4 450 MB → 1 736 MB**（16k 原子回放 0.8 s）✓
        //     ⇒ 以及 **≈ 300 KB/原子**（3 600 原子 ⇒ RSS 1 088 MB）✓
        //   **⇒ ⇒ ⇒ 所以**：**summary 直接说清**三件事 —— RSS 会随原子增长、
        //     **`/api/documents` 的 close+open 能释放**、单会话有天花板但文档本身没有 ✓
        summary: "一次返回画布状态：尺寸、层列表、选区、掩码，外加**真实内存**（`server_rss_mb`）\
                  与**每原子均值**（`bytes_per_atom_avg`，<100 原子时报 0=不可判）（只读 ✓）。\
                  **内存只增不减**：渲染缓存按原子增长（实测 ≈300 KB/原子，4K 一笔 ≈300 KB）⇒ \
                  **要释放就 `close` 再 `open` 同一文档**（实测 4 450 MB → 1 736 MB，回放 0.8 s）⇒ \
                  **单会话有天花板、文档本身没有** ⇒ 长跑请分阶段、每阶段重开",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        // **★ 一个工具问"另一个工具怎么调" ★**（第 1557 轮；外部 `B10` 的建议）
        //   **他们的原话**：「这些工具的报错都列了合法参数（比大多数工具强），但我是靠
        //     **连续 4 次撞墙**才拼出正确签名的。建议…或提供 `explain_tool`」✓
        //   **⇒ ⇒ 所以**：把**已经存在**的 spec 与 schema **原样吐出来** ⇒ 不必去 `tools/list`
        //     里翻（AI 常常只读 summary 就开始调）✓
        //   **⇒ ⇒ ⇒ 三个字段是重点**：**`required`（互斥时谁必填）、`params` 的描述（含单位）、
        //     `mutating`（会不会改文档）** ✓ —— 这三样正是"撞墙"的来源 ✓
        name: "explain_tool",
        profile: Profile::Core,
        summary: "查一个工具怎么调：**必填项／可选参数／单位与取值范围／是否改文档**（只读 ✓）。\
                  比 `tools/list` 好读（单工具、含 required 与互斥说明），\
                  也免去「先调错一次、从报错里拼签名」",
        mutating: false,
        // **两个写法都收** ✓（外部第 10 轮：`tool` 是人的第一反应 ⇒ 只收 `name` 就让人白失败一次 ✗）。
        // 两者都**不必填**在 schema 层（由 handler 校验并给出有用的话）✓；
        // 都给时以 `name` 为准 ✓（更具体的那个）。
        params: &[
            param!(
                "name",
                String,
                false,
                "要查的工具名（如 `stroke_path`）；给错名字会列出**最接近的 3 个**候选。**与 `tool` 二选一**"
            ),
            param!(
                "tool",
                String,
                false,
                "`name` 的另一种写法（同义）⇒ 两者都给时以 `name` 为准"
            ),
        ],
    },
    ToolSpec {
        name: "list_objects",
        profile: Profile::Core,
        summary: "列出对象（可按图层与类型过滤）",
        mutating: false,
        params: &[
            param!("layer_id", String, false, "按图层过滤"),
            param!("type", String, false, "按对象类型过滤"),
            param!("include_hidden", Boolean, false, "是否包含不可见对象"),
        ],
    },
    ToolSpec {
        name: "get_object",
        profile: Profile::Core,
        summary: "读取对象详情（含包围盒与版本链）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("include_history", Boolean, false, "是否包含版本链"),
        ],
    },
    ToolSpec {
        name: "get_path",
        profile: Profile::Core,
        summary: "读回 path 形状的完整几何（本地坐标，不含 object.transform）：结构化 subpaths、可直接塞回 draw_shape 的 geometry、等价 SVG d 字符串（最短往返数字，再解析逐位一致）、控制点包围盒 bbox、展平总长 length（闭合含回首段）、每条子路径的 node_counts（edit_path 的节点下标口径）。对象不存在/已删除/不是 path 形状都会明确报错，不回退成 get_object 的原始 data",
        mutating: false,
        params: &[param!("object_id", String, true, "path 形状对象 id（draw_shape 建的 kind=path 对象）")],
    },
    ToolSpec {
        name: "edit_path",
        profile: Profile::Core,
        summary: "节点级编辑 path 形状（不必整形重发）：节点下标口径为「节点 0 = 子路径 start，节点 k = 第 k-1 段的终点」。move_node 把锚点移到 to，相连的两个存储控制柄按同一位移刚性平移（柄相对锚点的偏移不变）；insert_node 在第 segment 段的参数 t（缺省 0.5）处插锚点（直段取插值点、三次段 de Casteljau 切分，几何不变；闭合子路径可给 segment=段数作用于回首段）；delete_node 删锚点并按合并规则接回相邻段（只剩一段的子路径拒绝）；set_handle 把一侧控制柄设到 to（该侧是直段时升格为三次段、另一侧控制点取段上 1/3 或 2/3 点；闭合回首段的柄会落成显式三次段）。编辑后整份几何复验、bbox 同步更新",
        mutating: true,
        params: &[
            param!("object_id", String, true, "path 形状对象 id（draw_shape 建的 kind=path 对象）"),
            param!("op", String, true, "move_node | insert_node | delete_node | set_handle"),
            param!("subpath", Integer, true, "子路径下标（0 起）"),
            param!("node", Integer, false, "节点下标（move_node / delete_node / set_handle 用；0 = start，k = 第 k-1 段终点）"),
            param!("segment", Integer, false, "段下标（insert_node 用；闭合子路径给段数 = 回首段）"),
            param!("t", Number, false, "insert_node 的段内参数（0 与 1 之间，缺省 0.5）"),
            param!("side", String, false, "set_handle 的侧：in（上一段的 c2）| out（下一段的 c1）"),
            param!("to", Any, false, "目标点 [x, y]（move_node 的新锚点位置 / set_handle 的新控制点位置，绝对坐标）"),
        ],
    },
    ToolSpec {
        name: "render_region",
        profile: Profile::Core,
        summary: "渲染区域并返回预览地址（8.3 局部渲染）",
        mutating: false,
        params: &[
            param!("region", Any, true, "区域 {x,y,w,h} 或 [x,y,w,h]"),
            param!("raw", Boolean, false, "返回未编码的直通 RGBA8（写入 CAS 并返回 raw_url），供逐像素自检"),
            param!("output_path", String, false, "给了就把预览 PNG（**全尺寸**，不受 max_px 限制）落盘到该路径（导出目录或系统临时目录内，守卫同 export_png），响应给 image_file 元数据而非内嵌图；与 raw 同给报错"),
            param!(
                "show_coords",
                Boolean,
                false,
                "**把实际导出范围叠成水印**（Muse 建议 4 ✓，缺省 `false` ✓）。只为**看图时自证「取到的是哪一块」** ✓ —— `requested_region`／`rendered_region`／`clipped` 已经能给数据 ✓，但图上原先看不出来 ✗。**缺省与以前逐字节相同** ✓；打开后**只动左上角那条带** ✓（其余像素逐字节不变 ✓）；**只影响导出的像素** ✓，不写文档、不落原子 ✓。仅对 `raw: true` 有效 ✓。"
            ),
            param!(
                "include_image",
                Boolean,
                false,
                "是否内嵌图像。**图片以 MCP 的 `image` 内容块返回**（base64 在那里 ✓，不在 text 里 ✗）；\\n\
                 text 块里只有 `yanshi://blob/…` 地址或缺图原因。缺省上限 512px，超限会**自动缩小**后内嵌 ✓\\n\
                 （并回 scaled_from 记录原尺寸 ✓）；连缩小都失败才回 image_omitted + 原因 ✓。"
            ),
            param!("max_px", Number, false, "内嵌上限（缺省 512）⇒ 超限时回 image_omitted + 可读原因；想看图就把它调大"),
            // **★ `active_layer` ✓ ★**（**目标第 4 条 ✓**；第 529 轮 ✓）：
            // **∴ 三段合成的切点放在该层上 ⇒ 改它时只重渲该层，其下复用 ✓**；
            // **∴ 不传 ⇒ 切在"最上层以外"✓（**与今天一致 ✓**）**。
            // **⚠️ 它必须在这里声明 ✗** —— **∴ 未声明的键**进不了 `args` ✗**
            //（**实测：`args` 键只有 `doc_id`／`raw`／`region` ✓，`active_layer` 被丢弃 ✓**）。
            // **★ `layer_id`：只渲染**指定的一层** ✗ ★**（第 185 轮 ✓；**PWA 实测 P1-2 ✓）**：
            //   **∴ 为什么必须有它 ✗**：**图层面板**要给**每一行**一张缩略图**✗
            //     ⇒ **∴ 那**必须能**单独渲染一层** ✓**** ✓✓
            //   **⚠️ 与上面 `active_layer` **同一个坑** ✗**：**未声明的键**进不了 `args` ✗**
            //     ⇒ **∴ 我**第一版**只**读了 `args["layer_id"]`**✗
            //       ⇒ **∴ 而**实测**回了**：**
            //         `render_region 不接受参数 layer_id（拼写错误？）`** ✓**** ✓✓
            //       ⇒ **★ 所以**：**读参数**与**声明参数**是**两件事** ✓ ★**** ✓✓
            //   **∴ 与** `active_layer` 的**差别 ✗**：**那个**切三段合成**✗；**这个**只画一层 ✓**
            //     ⇒ **∴ 语义**不同**✗ ⇒ **∴ 不能**互相代替 ✓**** ✓✓
            param!(
                "layer_id",
                String,
                false,
                "只渲染**指定的一层**（单层缩略图用）⇒ 返回该层单独的 PNG；\\n\
                 不传 ⇒ 行为与从前完全一致。**与 `active_layer` 不同**：那个切三段合成，这个只画一层。"
            ),
            param!(
                "active_layer",
                String,
                false,
                "当前层 id（可选）：把三段合成的切点放在该层上 ⇒ 改它时只重渲该层、其下复用；不传则切在最上层以外"
            ),
        ],
    },
    // **诊断包** ✓（P0 事故复盘：事发时柜台是空的 ✓）—— 见 `crate::diagnostics` ✓。
    // **必须非变更** ✓：只读令牌（viewer）也要能收集证据 ✓，否则"出事了却只有 owner 能取证"✓。
    ToolSpec {
        name: "collect_diagnostics",
        profile: Profile::Core,
        summary: "把排查所需的信息与文件打成**一个 zip**：构建/平台/配置（去密）、文档元数据、原子日志尾部（注明截断）、服务端 stderr 环形缓冲、渲染告警（含被跳过的补丁）、阶段耗时、缩略图与包内 README。**只读**，viewer 令牌也能用；缺省直接返回 base64，给 path 就写到导出目录",
        mutating: false,
        params: &[
            param!(
                "path",
                String,
                false,
                "写到这个路径（导出目录或系统临时目录内，见 export_png 的沙箱规矩）；缺省不落盘，直接在结果里返回 archive_base64"
            ),
            param!(
                "include_thumbnail",
                Boolean,
                false,
                "是否带当前缩略图（缺省 true；过大或没有会在 thumbnail.json 里说明，不会为它现场渲染整幅）"
            ),
            param!(
                "max_bytes",
                Integer,
                false,
                "zip 上限（字节，缺省 4194304 = 4 MiB）；超限按固定顺序裁剪并在 README 里注明"
            ),
        ],
    },
    // ---- 图层 ----
    ToolSpec {
        name: "create_layer",
        profile: Profile::Core,
        summary: "创建图层",
        mutating: true,
        params: &[
            param!("layer_id", String, false, "图层 id（缺省自动生成）"),
            param!("name", String, false, "图层名"),
            param!(
                "type",
                String,
                false,
                "raster | vector | layer_group | mask（不认识的值会显式报错，不再静默落盘）"
            ),
            param!(
                "kind",
                String,
                false,
                "图层类型别名：raster | vector（路线甲）；与 type 都给且不一致会显式报错"
            ),
            param!("parent_id", String, false, "父图层（图层组）"),
            param!("z_index", Integer, false, "z 序"),
            param!(
                "if_exists",
                String,
                false,
                "重名时的行为：缺省 error（返回 precondition_failed）；**reuse** = 已存在就复用、不报错也不新建（返回 reused: true）"
            ),
        ],
    },
    ToolSpec {
        name: "update_layer",
        profile: Profile::Core,
        summary: "修改图层属性（名称、可见性、不透明度、混合模式、锁定、蒙版）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "图层 id"),
            param!(
                "patch",
                Object,
                true,
                "属性补丁，如 {visible:false, opacity:0.5}"
            ),
        ],
    },
    ToolSpec {
        name: "delete_layer",
        profile: Profile::Core,
        summary: "删除图层（tombstone，级联到后代与其中对象）",
        mutating: true,
        params: LAYER_ID,
    },
    ToolSpec {
        name: "duplicate_layer",
        profile: Profile::Structure,
        summary: "复制图层（含其上的对象）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "要复制的图层"),
            param!("new_layer_id", String, false, "新图层 id；缺省自动生成"),
            param!("name", String, false, "新图层名；缺省为「原名 副本」"),
        ],
    },
    ToolSpec {
        name: "reorder_layers",
        profile: Profile::Core,
        summary: "重排图层（必须携带完整目标 z 序）",
        mutating: true,
        params: &[param!("order", Array, true, "完整图层 id 顺序（自底向上）")],
    },
    // ---- 导入 ----
    ToolSpec {
        name: "put_blob",
        profile: Profile::Core,
        summary: "把一个服务端本地文件放进内容寻址存储，返回 blob_hash（import_image 的前置）",
        // **不是 `mutating`**：它只写 blob 存储，**不动文档状态**（不产生原子）✓
        mutating: false,
        params: &[
            param!("path", String, false, "服务端本地文件路径（相对 --root，或 root 下的绝对路径）；**也可以直接给 http(s) URL** ⇒ 先拉取再入库（连接 10s／总 30s 超时、重定向 ≤5 跳且禁 https→http 降级、体积上限与上传同源 256MiB、五类失败分开报错）；与 data_base64 二选一，同给报错"),
            param!("data_base64", String, false, "与 path 二选一（同给报错）：文件字节的 base64 正文（**不带 `data:` 前缀**）；解码后与 path 通道走同一入库与同一体积上限"),
            param!("mime_type", String, false, "写进返回的 mime_type（缺省 image/png）；不影响存储编码"),
        ],
    },
    ToolSpec {
        name: "import_image",
        profile: Profile::Core,
        summary: "导入位图（blob 先行，创建像素图层对象）",
        mutating: true,
        // **加参数就必须同时登记** ✓ —— 这个错我犯过三次 ✗（`at`、`right_id`、`shape_id`、
        // `path_id` ✓），每次都是框架先报错、测试后失败 ✓。参数表在 IMPORT_IMAGE_PARAMS
        // （feature `raw` 开时多一组 raw_options，关时与基线逐项一致，RAW 第二包）✓。
        params: IMPORT_IMAGE_PARAMS,
    },
    ToolSpec {
        name: "create_selection",
        profile: Profile::Structure,
        summary: "创建选区（形状 + 羽化 + 反选 + 组合模式）：约束其后的**笔触与擦除**落笔范围（覆盖度逐像素施加）",
        mutating: true,
        params: &[
            param!("selection_id", String, true, "选区 id"),
            param!("shape", Object, false, "几何 {kind:rect|ellipse|polygon|path, bbox, points?, subpaths?(path), d?(path：SVG 路径字符串，与 subpaths 二选一，入口转 subpaths), object_id?(path：引用已有 path 形状对象，与 create_mask 同口径——与 subpaths/d 二选一、坏 id 明确报错；入口展开成内联 subpaths 快照落库并记 resolved_from，fill_rule 以被引用对象为准), fill_rule?(path: evenodd|nonzero)}；给了 from_mask 时可省略（缺省用蒙版的 shape），其余情况必填（缺了明确报错）"),
            param!("feather", Number, false, "羽化过渡宽度（像素，缺省 0）"),
            param!("invert", Boolean, false, "反选（缺省 false）"),
            param!("mode", String, false, "缺省 new = 新建（id 已存在则报错）。add/subtract/intersect 对**已存在的 id** 做**位图布尔合并**（新旧形状光栅化后按 mode 合成、落成 bitmap 选区），不是覆盖；id 不存在时按新形状创建"),
            param!("linked_layer", String, false, "仅约束该图层的落笔（缺省约束全文档）"),
            param!("from_mask", String, false, "从已有蒙版一键转换：传 mask_id，克隆其 shape/feather/invert/blobs/patches；此时 shape 可省略，显式传的 shape/feather/invert 优先"),
        ],
    },
    ToolSpec {
        name: "delete_selection",
        profile: Profile::Structure,
        // **这句以前写的是**「已画内容按日志重算恢复原样」** ⇒ 实测**不是这样** ✗**（第 1554 轮；外部 `B7`）
        //   **⇒ 他们的实测**：被该选区裁剪过的笔触，删除选区前后**墨迹完全一致**（3812 px、x 0.23–0.35 不变）✓
        //   **⇒ ⇒ 而本仓的测试注释也写「已画内容保持不变」**（`tests/selection_tools.rs:5`）✓
        //     ⇒ **所以是 summary 一家写错**，实现与测试都是对的 ✓
        //   **⇒ ⇒ ⇒ 后果**：他们**因为相信 summary**，设计了"绝不删选区"的绕行方案、**白绕一圈** ✗
        //     ⇒ **一句错文档的代价 = 一轮工程决策** ✓（**文档也是接口**）
        summary: "删除选区（tombstone）：删除后笔触/擦除不再受约束；**已画内容不变** \
                  （裁剪在渲染期按日志重算，但**删选区不会重算已提交的笔触** ⇒ \
                  与 `tests/selection_tools.rs` 的实测一致）",
        mutating: true,
        params: &[param!("selection_id", String, true, "选区 id")],
    },
    ToolSpec {
        name: "list_selections",
        profile: Profile::Structure,
        summary: "列出选区（设计 4.4 的 Selection 模型：shape/feather/mode/invert/linked_layer）",
        mutating: false,
        params: &[],
    },
    // ---- 绘制 ----
    ToolSpec {
        name: "draw_stroke",
        profile: Profile::Core,
        summary: "**纯几何矢量笔迹**（几何插值、实心、**无笔刷物理** ✗）：data.color 支持 [r,g,b,a] 0-1 线性 / 0-255 字节、{r,g,b,a}、#RRGGBB。                  想要笔毛 / 干湿 / 压感的笔触 ⇒ 用 brush_stroke（.myb）或 medium_stroke（介质插件）。                  **每条笔触只作用于它自己的图层** ✓：跨图层只是普通叠加；介质的湿搅 / 混色**不跨图层** ✓（先把底下那层画完，或用同一层叠 ✓）。",
        mutating: true,
        params: &[
            param!("label", String, false, "笔迹标签（部位名）⇒ 记进原子，供 list_atoms(label=…) 追查"),
            param!("group_id", String, false, "组标识：一组笔共享一个 id ⇒ 用 revert_group 整组撤销（不给则不成组）"),
            param!("layer_id", String, true, "目标图层"),
            param!(
                "data",
                Object,
                true,
                "{points,size,color,hardness,opacity,seed,smooth...}；\
                 points 支持 [[x,y]] 或 [[x,y,pressure]]（pressure 0..1，缺省 1.0）——\
                 按点给压力即可画出提按顿挫（内核早就支持，此前只是没写在这里）；\
                 **smooth: true** ⇒ 把这些点当 **Catmull-Rom 平滑样条的控制点**（曲线过它们、不拉走）⇒ \
                 手写的折线不再有硬角，而日志里存的仍是**原始采样点**（平滑只在渲染时发生 ⇒ 可随时关掉）"
            ),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "fill_region",
        profile: Profile::Core,
        // **AI 画家需求 P0-1.1**：一次完成**带形状**的填充 ✓（矩形/椭圆/多边形 ✓）。
        // **复用既有的形状绘制** ✓：映射成 `draw_shape` 认的 `{kind, bbox}`（见 `object.rs:518` ✓ /
        // `selection.rs:14` ✓ 里记的 `rect | ellipse | polygon | lasso`）⇒ **不另写绘制** ✗。
        // **`texture` 已落地** ✓（外部第 71 次报告 ✓）：**bitmap 来源**（blob_hash + source 矩形）✓
        // ⇒ 每像素按**最近源像素**取样 ✓（`fill_coverage_sampled` ✓）；`region` 来源**尚未实现** ✓
        // ⇒ 给 `region` 会**报错说清** ✗，**绝不静默忽略** ✗（那是本仓库的头号病症 ✓）。
        summary: "按形状填充一块区域（rect/ellipse/polygon；opacity 乘进颜色的 a）；texture 传对象即可用**位图纹理填充**（{type:\"bitmap\", blob_hash, source:{x,y,w,h}, width, height}，每像素按最近源像素取色、羽化只乘 alpha；region 来源未实现会给明确报错）。**要『整体压暗/提亮但保住对比度』请用图层混合模式**（不在这里）：建一层灰再 set_property(key=blend_mode, value=multiply) —— multiply 在 sRGB 空间做 new=old×k，均值和 std 同时按 k 缩；而 fill 的 opacity 是叠加式 new=old×(1−a)+c×a，std 会按 (1−a) 被压扁（第 833 轮，外部 §139）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            // **描述要跟上能力** ✓：本仓库的教训是"**过时的描述会让 agent 主动放弃可用的能力**" ✗
            // （见下面 `draw_text` 那条实测纠正 ✓）⇒ `feather` 在**同一句里**说清 ✓。
            param!("shape", Object, true, "{type: rect|ellipse|polygon, x,y,w,h | cx,cy,rx,ry | points；可选 feather = 羽化半径（像素，缺省 0 = 不羽化）}"),
            param!("feather", Number, false, "羽化半径（像素；也可写进 shape.feather；缺省 0 = 不羽化）；实际模糊在渲染侧做，文档里存的是这个参数"),
            // **类型要与实现一致** ✗（第 842 轮 ✓，外部报告 #10 ✓）：实现走 `.and_then(Value::as_str)` ✓
            // ⇒ 它**接受** "#rrggbb" ✓，而这里原本声明成 `Object` ✗ ⇒ **字符串在 schema 层就被拒** ✓
            // ⇒ 调用方按描述传 `"#rrggbb"` 会拿到 `expected object, got string` ✗ ——
            // **"描述说支持、schema 拒绝"** ✓ ⇒ 与同仓 `brush_stroke` / `preview` / `test_color` 一致 ⇒ 用 `Any` ✓。
            param!("color", Any, false, "填充色 {r,g,b,a} 或 #rrggbb；**任一分量 > 1 即按字节**（0..255），否则按 0..1 的比例。**给了 `texture` 时可省略**（那时颜色全部来自源位图）"),
            param!("opacity", Number, false, "不透明度 0..1（乘进颜色的 a；缺省 1）"),
            param!("texture", Any, false, "**纹理填充**（缺省 = 纯色；`\"smooth\"` 同缺省）⇒ 传对象：{\"type\":\"bitmap\", \"blob_hash\":\"<源位图哈希>\", \"source\":{x,y,w,h}（源矩形，文档坐标）, \"width\":…, \"height\":…（源位图像素尺寸）, \"mime_type\":\"image/x-yanshi-raw\"（可选）, \"min_distance\":25（也可写 min_distance_from_target；与目标的最小间距，缺省 0 = 不拦）, \"jitter\":0.3（**可选，0..1**：每个平铺块整体随机平移 ±jitter/2×源块边长 ⇒ **防「墙纸重复」**；缺省 0 = 原样平铺，逐字节等同旧行为）}。语义：**每个像素从源矩形按最近源像素取色**（**不是全图平均** —— 平均会让深处看不出纹理）；**羽化只乘 alpha**（颜色不被背景污染）；宽度/高度比源矩形大时**按模平铺**。**`type:\"region\"`（从图层形状取）尚未实现**，会给明确报错 ⇒ 先把源区域导出成位图再传 blob_hash"),
        ],
    },
    ToolSpec {
        name: "draw_shape",
        profile: Profile::Core,
        summary: "绘制形状（矩形/椭圆/多边形/路径 path，可描边）：path 的 geometry 形如 {kind:\"path\", fill_rule:\"evenodd\"|\"nonzero\"（缺省 evenodd）, subpaths:[{start:[x,y], closed:bool, segments:[{op:\"L\",to:[x,y]} | {op:\"C\",c1:[x,y],c2:[x,y],to:[x,y]}]}]}，或用 d 给 SVG 路径字符串（支持 M/L/C/Q/H/V/Z 与小写相对命令，Q 自动转 C；与 subpaths 二选一，落库前统一转成 subpaths）；path 不给 color ⇒ 纯描边（开放路径），给 color ⇒ 填充（开放路径按隐式闭合填充、描边不封口）；非法 path 载荷带字段路径报错、不回退成多边形",
        mutating: true,
        params: &[
            param!("label", String, false, "笔迹标签（部位名）⇒ 记进原子，供 list_atoms(label=…) 追查"),
            param!("group_id", String, false, "组标识：一组笔共享一个 id ⇒ 用 revert_group 整组撤销（不给则不成组）"),
            param!("layer_id", String, true, "目标图层"),
            param!(
                "data",
                Object,
                true,
                "{geometry,color,stroke_width,stroke_color}；geometry.kind 可为 rect/ellipse/polygon/path（path 的字段表见 summary）"
            ),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "stroke_path",
        profile: Profile::Core,
        summary: "沿路径用真笔刷描边：把 path 形状对象（path_object_id）或内联 subpaths 按弧长等距烘焙成 brush_stroke 落笔（与手绘笔触同一写路径：介质/撤销/判据同源）。每个 subpath 独立成一笔。pressure_profile=simulate_pressure（缺省）时压感两端 smoothstep 斜坡归零、中段满压；constant 时全程恒定。何时用它而不是 draw_shape 的 path 描边：要笔刷物理（压感变宽、飞白、混色）时用这个；只要纯几何线宽时用 draw_shape 的 stroke_width。",
        mutating: true,
        params: &[
            param!("label", String, false, "笔迹标签（部位名）⇒ 记进原子，供 list_atoms(label=…) 追查"),
            param!("group_id", String, false, "组标识：一组笔共享一个 id ⇒ 用 revert_group 整组撤销（不给则不成组）"),
            param!("layer_id", String, true, "目标图层（与 brush_stroke 同语义）"),
            param!("path_object_id", String, false, "已有 path 形状对象 id（与 subpaths / d 三选一，多给报错）"),
            param!("subpaths", Array, false, "内联子路径（与 draw_shape 的 path 载荷同构：[{start:[x,y], closed:bool, segments:[{op:\"L\",to} | {op:\"C\",c1,c2,to}]}]；与 path_object_id / d 三选一）"),
            param!("d", String, false, "SVG 路径字符串（与 path_object_id / subpaths 三选一）：支持 M/L/C/Q/H/V/Z 与小写相对命令，Q 按公式自动转 C；解析错误带字符位置，解析结果走与 subpaths 同一份校验"),
            param!("brush", String, true, "笔刷名（同 brush_stroke：assets/brushes 或缓存里的 .myb，可省 .myb）"),
            param!("size", Number, false, "覆盖笔刷自带半径（直径像素，同 brush_stroke）；同时决定烘焙采样间距 Δs = max(1, 0.2×size)；不给时按 10 估算采样间距（笔刷半径仍以 .myb 为准）"),
            param!("color", Any, false, "颜色（同 brush_stroke：{r,g,b,a} / [r,g,b,(a)] / \"#RRGGBB\"）"),
            param!("brush_params", Object, false, "覆盖 .myb 字段（同 brush_stroke）"),
            param!("pressure_profile", String, false, "压感剖面：simulate_pressure（缺省）/ constant"),
            param!("pressure_max", Number, false, "剖面峰值 0..1（缺省 1.0）"),
            param!("ramp_ratio", Number, false, "simulate 时双端斜坡各占弧长比 0..0.5（缺省 0.25）"),
            param!("virtual_speed", Number, false, "虚拟手速 px/ms（缺省 0.5）：决定烘焙点的 time_ms 与吃速度的笔刷行为"),
            param!("preview", Boolean, false, "缺省 true = 每笔产出预览图；false = 不产出（连续落笔提速；透传 brush_stroke）"),
            param!("wait_for_render", Boolean, false, "缺省 true = 等渲染落盘再返回；false = 不等（透传 brush_stroke）"),
            // **★ 补上两个**早就该有的参数** ★**（第 1556 轮；外部 `B9`）
            //   **⇒ 外部实测**：`stroke_path` 不接受 `clip_to_selection` / `opacity`，
            //     而报错**猜「拼写错误？」** ✗（**因为它们在 spec 里没列，校验先拒** ✓）
            //   **⇒ ⇒ 而实现**早就透传了**：本工具把 `args` 克隆后只剔除**自己的 7 个键**，
            //     剩下的**原样交给 `brush_stroke`** ✓ ⇒ **所以缺的只是 spec 这两行** ✓
            //   **⇒ ⇒ ⇒ 而 `brush_stroke` 两个都有**（`opacity` 映射 MyPaint 的 `opaque`、
            //     `clip_to_selection` 支持 `latest`/`combined`）⇒ **补齐即等价** ✓
            param!("opacity", Number, false, "整笔不透明度 0..1（**透传 brush_stroke**：映射到 MyPaint 的 `opaque`，即每枚 dab 的不透明度）；缺省用 .myb 自带的"),
            param!("clip_to_selection", String, false, "只把墨落在该选区里（**透传 brush_stroke**：选区 id / latest / combined，add·subtract·intersect 生效）；缺省不裁剪"),
        ],
    },
    ToolSpec {
        name: "combine_shapes",
        profile: Profile::Core,
        summary: "简易布尔（位图路线）：把 2 个以上 draw_shape 形状（rect/ellipse/polygon/path 都吃）的填充覆盖率在像素场里按选区位图布尔公式合成（union=max、subtract=max(0,a−b)、intersect=a·b，左折叠、subtract 第一个是被减数），marching squares 提轮廓 + RDP 简化后落成一个新形状（单环=polygon，多环/带孔=path+evenodd）。与 path_boolean 的分工：那边解析精确但只吃 polygon 且会 tombstone 输入；这边吃全部形状种类且不删输入，代价是轮廓精度受光栅限制（亚像素等值线 + simplify 容差）。结果为空集明确报错。engine=vector 时走矢量布尔（平面排列+面分类，吃多环带孔与相切/共线重叠，退化输入明确报错），结果恒落成 path+evenodd。",
        mutating: true,
        params: &[
            param!("object_ids", Array, true, "形状对象 id 数组（至少 2 个；subtract 时第一个是被减数，其余按顺序减）"),
            param!("mode", String, false, "union（缺省）/ subtract / intersect"),
            param!("result_id", String, false, "结果对象 id（缺省 combine_<ulid>）"),
            param!("simplify", Number, false, "轮廓 RDP 简化容差（像素，缺省 0.5；0 = 不简化只去重复点。值越大点越少、轮廓越偏离原边界）"),
            param!("layer_id", String, false, "结果图层（缺省取第一个输入对象的图层）"),
            param!("color", Any, false, "结果填充色（与 draw_shape 同格式；缺省克隆第一个输入对象的样式）"),
            param!("engine", String, false, "bitmap（缺省，位图路线，存量行为不变）/ vector（矢量布尔：展平后做真多边形布尔，结果恒为 path+evenodd，轮廓精度只受 tolerance 限）"),
            param!("tolerance", Number, false, "矢量展平容差（像素，缺省 0.15，范围 (0, 5]）；只在 engine=vector 时可传。vector 时 simplify 不适用（传了报错）"),
        ],
    },
    ToolSpec {
        name: "list_fonts",
        profile: Profile::Core,
        mutating: false,
        // **字体方案 Phase 3**：让 MCP／无头侧**先知道有哪些字体**（否则 `draw_text{font:…}`
        // 只能靠猜 ✓）。**∴ 列的是**系统字体 + 预置字体**（`assets/fonts/` ✓）★
        summary: "列出可用字体族（系统字体 + assets/fonts 里的预置字体）⇒ 给 draw_text 的 font 参数用",
        params: &[],
    },
    ToolSpec {
        name: "draw_text",
        profile: Profile::Core,
        // **实测纠正** ✓（外部绘画 agent 报 ✓）：它用 `draw_text{"text":"YANSHI 黄昏 ab"}` 一次就
        // **完整画出两个汉字** ✓，而描述里还写着"CJK 字体子集属后续项" ✗
        // ⇒ **过时的描述会让 agent 主动放弃可用的能力** ✗（与"参数被静默忽略"是同一类损失 ✓）。
        summary: "绘制文本对象（内核用内置位图字体光栅化；ASCII 与常用 CJK 都能画）",
        mutating: true,
        params: &[
            param!("label", String, false, "笔迹标签（部位名）⇒ 记进原子，供 list_atoms(label=…) 追查"),
            param!("group_id", String, false, "组标识：一组笔共享一个 id ⇒ 用 revert_group 整组撤销（不给则不成组）"),
            param!("layer_id", String, true, "目标图层"),
            param!(
                "data",
                Object,
                true,
                "{text,font,size,color,position,align}"
            ),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "fill",
        profile: Profile::Core,
        // **描述必须说出参数藏在哪** ✗：外部绘画 agent 实测把它当 Photoshop 的"填充"用 ✓
        //（`fill {x:0,y:0,w:900,h:700}` 或 `fill {color:…, region:…}` ✓）⇒ 一律被拒 ✓，
        // 而它把"没填上"归因成**坐标偏移** ✗ —— 一句话的描述要负一半责任 ✓。
        summary: "用纯色填充：颜色与区域**都放在 `data` 里** ⇒ `data:{color, region:{x,y,w,h}}`，或 `data:{color, object_id}` 只填某个对象；要画纯色矩形也可以用 `draw_shape`（kind=rect）✓",
        mutating: true,
        params: &[
            param!("label", String, false, "笔迹标签（部位名）⇒ 记进原子，供 list_atoms(label=…) 追查"),
            param!("group_id", String, false, "组标识：一组笔共享一个 id ⇒ 用 revert_group 整组撤销（不给则不成组）"),
            param!("layer_id", String, true, "目标图层"),
            param!("data", Object, true, "{color,region|object_id}"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "erase",
        profile: Profile::Core,
        summary: "擦除（对象新版本或区域擦除）",
        mutating: true,
        params: &[
            param!("label", String, false, "笔迹标签（部位名）⇒ 记进原子，供 list_atoms(label=…) 追查"),
            param!("group_id", String, false, "组标识：一组笔共享一个 id ⇒ 用 revert_group 整组撤销（不给则不成组）"),
            param!("layer_id", String, true, "目标图层"),
            param!("data", Object, true, "{region|points, size, hardness, opacity}：region={x,y,w,h} 矩形擦（自动展开成盖章）、points=[[x,y],…] 沿路径擦；两者必给其一"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    // ---- 修改 ----
    ToolSpec {
        name: "update_object",
        profile: Profile::Core,
        summary: "修改对象属性（可见性、启用/停用、锁定、z 序、图层归属、元数据）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("patch", Object, true, "属性补丁"),
        ],
    },
    ToolSpec {
        name: "update_stroke",
        profile: Profile::Core,
        summary: "修改笔触（10.3 三层参数：核心 / preset / advanced）——\
                  只作用于笔迹（stroke）与路径（path）；光栅对象（raster_patch）的颜色已烘进 blob，\
                  会明确拒绝并说明如何重画",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!(
                "core",
                Object,
                false,
                "核心参数 {points|points_patch,brush,color,size,opacity,blend_mode}"
            ),
            param!("preset", String, false, "命名预设"),
            param!(
                "advanced",
                Object,
                false,
                "geometry/appearance/structure/metadata"
            ),
        ],
    },
    ToolSpec {
        name: "convert_to_shape",
        profile: Profile::Structure,
        summary: "把路径（或笔迹）转换成多边形形状（设计 792）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "要转换的路径或笔迹 id"),
            param!("shape_id", String, false, "转换后形状的 id；缺省自动生成"),
        ],
    },
    ToolSpec {
        name: "convert_to_path",
        profile: Profile::Structure,
        summary: "把笔迹转换成路径对象（设计 792）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "要转换的笔迹对象 id"),
            param!("path_id", String, false, "转换后路径的 id；缺省自动生成"),
        ],
    },
    ToolSpec {
        name: "transform_object",
        profile: Profile::Structure,
        summary: "旋转/缩放/平移一个对象（设计 783）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("rotate", Object, false, "{degrees} 旋转"),
            param!("scale", Object, false, "{x, y} 缩放"),
            param!("translate", Object, false, "{dx, dy} 平移"),
            param!("anchor", Object, false, "{x, y} 变换中心；缺省为对象包围盒中心"),
            param!("compose", Boolean, false, "是否叠加到现有变换上（缺省 true）"),
        ],
    },
    ToolSpec {
        name: "restore_object",
        profile: Profile::Structure,
        summary: "恢复已删除的对象（设计 783）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "get_object_history",
        profile: Profile::History,
        summary: "一个对象的原子版本链（设计 776）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "get_stroke_log",
        profile: Profile::History,
        summary: "**★ 一笔一行的可审计记录（Muse 建议 3）★**：只列**笔触**（不把建图层之类倒出来 ✓），每行给 `kind`/`brush`/`layer_id`/`object_id`/`points`（原样 ✓）/`bbox`（原子里的 `region` ✓）/`first_point`/`last_point`/`pressure{min,max,first,last}`/`timestamp` ✓。**为什么要它** ✗：`find_atom` **按设计只返回元数据** ✓（实测**没有坐标** ✗），要审计「我到底画在哪了」得用 `get_atom` 一条条取 ✓ ⇒ 这个工具把坐标面直接摊平 ✓。**跟文档走** ✓（它就是原子日志的视图 ✓，不是 `/tmp` 里的旁路文件 ✓ ⇒ 清 `/tmp`、换 session、重启都在 ✓）。**分页** `limit`（缺省 50，上限 500）/`offset` ✓，可按 `layer_id`/`object_id`/`since_seq` 过滤 ✓。**已知限制** ✗：笔触原子**不保存逐点 `time_ms`** ✓（实测传了 4 元组后，原子里的 `points` 只有 3 个分量 ✗）⇒ 只能给**原子时间戳** ✓。",
        mutating: false,
        params: &[
            param!("limit", Number, false, "返回条数（缺省 50 ✓，上限 500 ✓）"),
            param!("offset", Number, false, "偏移（缺省 0 ✓，配 `next_offset` 翻页 ✓）"),
            param!("since_seq", Number, false, "只看序号大于它的原子（缺省 0 ✓）"),
            param!("group_id", String, false, "只看该组的原子（配合 revert_group 与 brush_stroke 的 group_id）"),
            param!("layer_id", String, false, "只看某图层 ✓"),
            param!("object_id", String, false, "只看某个对象 ✓"),
            param!(
                "order",
                String,
                false,
                "`oldest`（缺省 ✓）或 `newest`（最近的在前面 ✓）"
            ),
        ],
    },
    ToolSpec {
        name: "find_atom",
        profile: Profile::History,
        summary: "按条件检索原子（设计 776）",
        mutating: false,
        params: &[
            param!("kind", String, false, "原子种类"),
            param!("actor", String, false, "提交者"),
            param!("object_id", String, false, "涉及的对象"),
            param!("layer_id", String, false, "涉及的图层"),
            param!("since_seq", Integer, false, "起始序号（不含）⇒ 配合回执的 `next_since` 翻页，直到 `truncated:false`"),
            param!("limit", Integer, false, "最多返回多少条（缺省 200，上限 2000 ✓；被夹时 `limit_clamped:true` 且 `limit` 回显真正用的值）"),
        ],
    },
    ToolSpec {
        name: "get_diff",
        profile: Profile::History,
        summary: "两个序号之间的日志差分（设计 776）",
        mutating: false,
        params: &[
            param!("from_seq", Integer, true, "起点序号（不含）"),
            param!("to_seq", Integer, false, "终点序号（含）；缺省到 HEAD"),
            param!("limit", Integer, false, "最多返回多少条原子"),
        ],
    },
    ToolSpec {
        name: "get_ancestors",
        profile: Profile::History,
        summary: "这个对象依赖谁（引用图的向上方向，设计 776）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "get_descendants",
        profile: Profile::History,
        summary: "谁依赖这个对象（引用图的向下方向，设计 776）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "submit_offline",
        profile: Profile::Changeset,
        summary: "重连时提交离线期间的原子；任一条不过校验则整批进悬空变更集（设计 12.4）",
        mutating: true,
        params: &[param!("atoms", Array, true, "[{kind, payload, actor?, session?}] 离线期间追加的原子")],
    },
    ToolSpec {
        name: "blob_gc",
        profile: Profile::History,
        summary: "Blob 三级生命周期：统计活跃/历史/孤儿；默认只报告，回收需 confirm",
        mutating: true,
        params: &[
            param!("dry_run", Boolean, false, "默认 true：只统计不删除"),
            param!("confirm", Boolean, false, "真删必须显式 true（删除不可逆）"),
            param!("ttl_days", Number, false, "孤儿 TTL，默认 7 天（设计 6.3）"),
            param!("demote", Boolean, false, "同时把「历史级」降冷到归档（设计 6.3 的后台迁移）"),
        ],
    },
    ToolSpec {
        name: "list_stashes",
        profile: Profile::Changeset,
        summary: "列出悬空变更集（UI 的“分支对比”用，设计 12.4）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "apply_stash",
        profile: Profile::Changeset,
        summary: "把悬空变更集强制应用到当前 HEAD（可能产生视觉错误，设计 12.4）",
        mutating: true,
        params: &[param!("stash_id", String, true, "悬空变更集 id")],
    },
    ToolSpec {
        name: "discard_stash",
        profile: Profile::Changeset,
        summary: "丢弃悬空变更集（不删 blob：Stash 的 blob 归历史级保留）",
        mutating: true,
        params: &[param!("stash_id", String, true, "悬空变更集 id")],
    },
    ToolSpec {
        name: "import_psd",
        profile: Profile::Core,
        summary: "只读导入 PSD 的合成图（不做图层结构，不写回 PSD）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "落到哪个图层"),
            param!("blob_hash", String, true, "已上传的 PSD 字节（blob 先行）"),
            param!("object_id", String, false, "新对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "import_psd_layers",
        profile: Profile::Core,
        summary: "★ 按图层树导入 PSD（第 253 轮）：每层建一个 yanshi 图层，组建成图层组",
        mutating: true,
        params: &[
            param!("blob_hash", String, true, "已上传的 PSD 字节（blob 先行）"),
            param!("name_prefix", String, false, "给导入的图层名加统一前缀（便于辨认/清理）"),
        ],
    },
    ToolSpec {
        name: "export_psd_layers",
        profile: Profile::Core,
        summary: "★ 导出成分层 PSD（第 253 轮）：图层/组/不透明度/可见性/混合模式都带出去，含压平合成图",
        mutating: false,
        params: &[
            param!("filename", String, false, "建议的下载文件名（缺省 <doc>.psd）"),
            param!("output_path", String, false, "给了就把同一份 PSD 再落一份到该路径（导出目录或系统临时目录内，守卫同 export_png），响应多 path / mime_type / overwritten；不给只回 blob_hash + yanshi:// url（旧行为）"),
        ],
    },
    ToolSpec {
        name: "resample",
        profile: Profile::Retouch,
        summary: "重采样光栅对象的像素到新尺寸（非破坏：产出新 blob + supersede）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "要重采样的光栅对象"),
            param!("width", Integer, false, "目标宽；与 height 一起给，或用 scale"),
            param!("height", Integer, false, "目标高"),
            param!("scale", Number, false, "缩放倍数（与 width/height 二选一）"),
            param!("filter", String, false, "nearest | bilinear（缺省 bilinear：介质是连续调）"),
        ],
    },
    ToolSpec {
        name: "resolve_conflict",
        profile: Profile::Conflict,
        summary: "解决采样性替换冲突（设计 12.3：组合宏，折叠器零改动）",
        mutating: true,
        params: &[
            param!("resolution", String, true, "keep_ours | keep_theirs | discard | merge"),
            param!("conflict_layer_id", String, false, "冲突图层 id；缺省自动找唯一的冲突图层"),
            param!("formal_layer_id", String, false, "keep_ours 时把我们的对象移回的正式图层"),
            param!("opponent_atom_id", String, false, "对方原子 id（keep_ours/discard 需要）"),
        ],
    },
    ToolSpec {
        name: "begin_transaction",
        profile: Profile::Changeset,
        summary: "开始一个事务：写操作失败时自动回滚已落的原子（设计 777）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "commit_transaction",
        profile: Profile::Changeset,
        summary: "收尾当前事务（原子保留，且可作为一组撤销）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "begin_changeset",
        profile: Profile::Changeset,
        summary: "开始一个变更集：此后本会话的提交都并入它（设计 793）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "commit_changeset",
        profile: Profile::Changeset,
        summary: "收尾当前变更集（原子保留）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "abort_changeset",
        profile: Profile::Changeset,
        summary: "放弃当前变更集：把它里面的原子整体撤销",
        mutating: true,
        params: &[],
    },
    ToolSpec {
        name: "get_changesets",
        profile: Profile::Changeset,
        summary: "列出日志里的变更集（设计 793）",
        mutating: false,
        params: &[
            param!("limit", Integer, false, "最多返回多少个变更集"),
        ],
    },
    ToolSpec {
        name: "revert_changeset",
        profile: Profile::Changeset,
        summary: "整体撤销一个变更集（设计 793）",
        mutating: true,
        params: &[
            param!("changeset_id", String, true, "要撤销的变更集 id"),
        ],
    },
    ToolSpec {
        name: "path_edit",
        profile: Profile::Structure,
        // ⚠️ **summary 不要再枚举 op** ✗（第 870 轮 ✓，子代理实测发现过时 ✓）：
        // 旧文写「本片只实现 reverse/close/join」✗，而实际 op 已有 merge/split（见下一行 ✓）
        // ⇒ 枚举会**随实现漂移** ✗ ⇒ 改为**指向参数**（参数本身就是权威 ✓，不会过时 ✓）。
        summary: "笔迹路径编辑（设计 792；**支持哪些操作见 op 参数** ✓ —— 不在摘要里枚举 ✓，避免随实现漂移 ✗）",
        mutating: true,
        params: &[
            param!("op", String, true, "reverse | close | join | merge | split | convert_to_path | convert_to_shape | boolean"),
            param!("object_id", String, true, "目标对象 id（笔迹或路径）"),
            param!("other_id", String, false, "join / merge 的第二个对象"),
            param!("at", Integer, false, "split 的切口节点下标（仅开放路径）"),
            param!("right_id", String, false, "split 产生的右半 id；缺省自动生成"),
            // `convert_to_shape` 经 `path_edit` 调用时也用这两个参数 ✓
            //（上一轮踩过同一坑：加了算子却忘了登记参数 ✓ ⇒ 框架的参数校验直接拒绝 ✓）。
            param!("shape_id", String, false, "convert_to_shape 产生的形状 id；缺省自动生成"),
            // `convert_to_path` 经 `path_edit` 调用时用它 ✓。
            // **这是第三次犯同一个错** ✗（前两次是 `at`/`right_id` 与 `shape_id` ✓）：
            // 加算子/委托时**必须同时登记参数** ✓ —— 框架会在第一次调用时报出来 ✓，
            // 但那时已经是测试失败 ✓。以后加算子时**先写参数表** ✓。
            param!("path_id", String, false, "convert_to_path 产生的路径 id；缺省自动生成"),
            param!("mode", String, false, "boolean 的模式：union | intersect | subtract | xor"),
            param!("result_id", String, false, "boolean 结果 id 前缀；缺省自动生成"),
        ],
    },
    ToolSpec {
        name: "get_dependency_graph",
        profile: Profile::Core,
        summary: "查看对象依赖图（设计 9.4；544 的传播闭包）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "update_sync_policy",
        profile: Profile::Core,
        summary: "设置实例的同步策略（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("policy", String, true, "all 或 none"),
        ],
    },
    ToolSpec {
        name: "update_override",
        profile: Profile::Core,
        summary: "设置实例的覆盖变换（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("transform", Object, false, "{matrix,pivot} 覆盖变换；缺省清除覆盖"),
        ],
    },
    ToolSpec {
        name: "detach_instance",
        profile: Profile::Core,
        summary: "把实例脱离为独立对象（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("object_id", String, false, "脱离后新对象的 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "link_to_master",
        profile: Profile::Core,
        summary: "改写实例指向的 master（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("master_id", String, true, "新的 master 对象 id"),
        ],
    },
    ToolSpec {
        name: "get_resolved_state",
        profile: Profile::Core,
        summary: "查看实例解析到的 master 与包围盒（设计 9.2）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "create_instance",
        profile: Profile::Core,
        summary: "创建实例（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("layer_id", String, true, "所属图层"),
            param!("master_id", String, true, "master 对象 id"),
            param!("local_transform", Object, false, "{dx,dy} 或 {matrix,pivot}"),
        ],
    },
    ToolSpec {
        name: "create_group",
        profile: Profile::Core,
        summary: "创建对象组（设计 9.4）",
        mutating: true,
        params: &[
            param!("group_id", String, true, "组对象 id"),
            param!("layer_id", String, true, "所属图层"),
            param!("members", Array, false, "成员对象 id 列表"),
        ],
    },
    ToolSpec {
        name: "add_to_group",
        profile: Profile::Core,
        summary: "把对象加入组（设计 9.4）",
        mutating: true,
        params: &[
            param!("group_id", String, true, "组对象 id"),
            param!("object_id", String, true, "要加入的对象 id"),
        ],
    },
    ToolSpec {
        name: "remove_from_group",
        profile: Profile::Core,
        summary: "把对象移出组（设计 9.4）",
        mutating: true,
        params: &[
            param!("group_id", String, true, "组对象 id"),
            param!("object_id", String, true, "要移出的对象 id"),
        ],
    },
    ToolSpec {
        name: "set_group_transform",
        profile: Profile::Core,
        summary: "平移整组（设计 9.4）",
        mutating: true,
        params: &[
            param!("group_id", String, true, "组对象 id"),
            param!("delta", Object, true, "{dx,dy} 平移增量"),
        ],
    },
    ToolSpec {
        name: "move_object",
        profile: Profile::Core,
        summary: "移动/变换对象",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("delta", Object, false, "{dx,dy} 平移"),
            param!("transform", Object, false, "{matrix,pivot} 仿射变换"),
        ],
    },
    // ---- 删除 ----
    ToolSpec {
        name: "replace_object_data",
        profile: Profile::Core,
        summary: "替换对象的数据（设计 5.2 的 `supersede`）：文本可换文字、笔触可换点列/颜色等，对象保持可编辑",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("data", Object, true, "新的对象数据（整体替换）"),
            param!("type", String, false, "可选：同时声明对象类型（缺省沿用原类型）"),
        ],
    },
    ToolSpec {
        name: "delete_object",
        profile: Profile::Core,
        summary: "删除对象（tombstone）",
        mutating: true,
        params: ID_ARGS,
    },
    // ---- 原生矢量（路线甲）----
    ToolSpec {
        name: "vector_erase",
        profile: Profile::Core,
        summary: "矢量橡皮：按脊线截断擦除矢量图层的笔触（擦掉整笔/截成多段都如实计数，不碰像素）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "矢量图层 id（非矢量图层会显式报错）"),
            param!(
                "points",
                Array,
                true,
                "擦除轨迹 [[x, y], …]（文档坐标；也接受 {x, y} 对象式）"
            ),
            param!(
                "radius",
                Number,
                true,
                "橡皮半径 px（截断带 = 轨迹两侧各 radius + 该处剖面半径）"
            ),
        ],
    },
    ToolSpec {
        name: "vector_hit_test",
        profile: Profile::Core,
        summary: "矢量笔触命中测试：按脊线距离 ≤ 剖面半径 + 容差返回命中的笔触 id 与余量",
        mutating: false,
        params: &[
            param!("layer_id", String, true, "矢量图层 id（非矢量图层会显式报错）"),
            param!("x", Number, true, "查询点 x（文档坐标）"),
            param!("y", Number, true, "查询点 y（文档坐标）"),
            param!(
                "tolerance",
                Number,
                false,
                "额外容差 px（缺省 0；负数会显式报错）"
            ),
        ],
    },
    // ---- 撤销 ----
    ToolSpec {
        name: "revert_group",
        profile: Profile::Core,
        summary: "整组撤销：把同一次操作产生的所有笔（共享一个 group_id）一次挖掉",
        mutating: true,
        params: &[param!("group_id", String, true, "要撤销的组标识")],
    },
    ToolSpec {
        name: "revert_to_seq",
        profile: Profile::Core,
        summary: "回到「第 n 笔之后」的文档状态（撤销所有 seq 大于 n 的笔迹）⇒ 阶段回退不需要循环 undo",
        mutating: true,
        params: &[param!("seq", Integer, true, "目标序号：撤销 seq 大于它的笔迹原子")],
    },
    ToolSpec {
        name: "revert_atoms",
        profile: Profile::Core,
        summary: "一次撤销多个原子（配合 list_atoms 找到的坏笔 ⇒ 精准挖掉，不必整层重画）",
        mutating: true,
        params: &[param!("atom_ids", Any, true, "要撤销的原子 id 数组")],
    },
    ToolSpec {
        name: "revert",
        profile: Profile::Core,
        summary: "撤销原子（不删除原原子；级联失效由折叠报告）",
        mutating: true,
        params: &[param!("atom_id", String, true, "目标原子 id")],
    },
    ToolSpec {
        name: "redo_last",
        profile: Profile::Core,
        // **与 `undo_last` 对称** ✓（设计 793：`revert(revert(x)) ≡ reapply(x)` ✓）。
        summary: "重做最后 N 笔被撤销的笔迹（发给 `reapply`；同样不动结构原子）",
        mutating: true,
        params: &[param!("count", Integer, false, "重做几笔（缺省 1）")],
    },
    ToolSpec {
        name: "undo_last",
        profile: Profile::Core,
        // **给人用的粒度** ✓（目标 ⑦ ✓）：调用方只想说"撤销最后一笔" ✓，
        // 而不该被迫先自己找出原子 id ✓（那是 `revert` 的粒度 ✓，对人不合适 ✓）。
        summary: "撤销最后 N 笔**笔迹**（一笔 = 同一个 object_id 的全部原子）；**不动结构**（图层等）与历史原子",
        mutating: true,
        params: &[param!("count", Integer, false, "撤销几笔（缺省 1）")],
    },
    ToolSpec {
        // **只读的撤销状态** ✓（F02 ✓）：`remaining_gestures` **本来只由 `undo_last` / `redo_last`
        // 的响应回报** ✗ ⇒ 落笔之后查看器**没有新数字** ✗ ⇒ 「刚画完却不能撤销」 ✓。
        // 这个工具**只算不动** ✓：复用 `collect_gestures`（与 `undo_last` **同一个数** ✓），
        // 不改文档、不发 revert ✓ ⇒ 查看器可以**随时问一次** ✓。
        name: "get_undo_status",
        profile: Profile::Core,
        summary: "只读查询可撤销/可重做的笔数（不改变任何内容）：复用撤销路径的同一套归并，保证与 undo_last 报的数一致。落笔之后问它即可刷新界面按钮。",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "reapply",
        profile: Profile::Core,
        summary: "恢复被撤销的原子（不恢复级联链）",
        mutating: true,
        params: &[param!("atom_id", String, true, "目标原子 id")],
    },
    // ---- 历史 ----
    ToolSpec {
        name: "set_reference",
        profile: Profile::Core,
        // **AI 画家需求 P2-9**：临摹时**半透明看参考图** ✓，不用来回切窗口 ✓。
        // **关键取舍** ✓：参考图**只记进偏好** ✓（`reference.blob_hash/opacity/position` ✓），
        // **绝不写进任何图层** ✗ ⇒ 文档**逐字节不受影响** ✓（判据断言的正是这条 ✓）⇒ `mutating: false` ✓；
        // 偏好不跟着文档走 ⇒ **换进程需重设**（C3：summary 与回执 note 同写这句）。
        // 显示由查看器负责 ✓（在画布上叠一层半透明图 ✓，服务端不参与绘制 ✓）。
        summary: "设置参考图（只记进偏好，不写进文档；换进程需重设；查看器负责半透明叠加）；用 clear_reference 移除",
        mutating: false,
        params: &[
            // **前置条件必须写在参数说明里** ✓（第 875 轮）
            //   **⇒ 为什么** ✗：`set_reference` **拒绝**存储里不存在的哈希 ✓（"存下来也永远比不了" ✓）
            //     ⇒ 而这条**先前只写在错误信息里** ✗ ⇒ 调用方（**包括我自己写的判据** ✓）
            //       可以照着 spec 传一个"形似"的 `sha256:000…0` ✓ ⇒ 被拒 ⇒ **误以为是功能缺失** ✓
            //       （那条判据就这样误诊了三个多月 ✓ —— 第 867 轮才查明 ✓）
            param!("blob_hash", String, true, "参考图的 blob 哈希（sha256:…）；**必须先在存储里**（用 `put_blob`／`import_image` 造出来）⇒ **不存在的哈希会被拒绝**（它存下来也永远比不了）"),
            param!("opacity", Number, false, "不透明度 0..1（缺省 0.5）"),
            param!("position", Object, false, "显示位置 {x, y, w, h}（缺省铺满画布）"),
        ],
    },
    ToolSpec {
        name: "clear_reference",
        profile: Profile::Core,
        // **移除参考图** ✓ —— 同样**只动偏好** ✓ ⇒ 文档逐字节不变 ✓（可逆 ✓）。
        summary: "移除参考图（只把偏好里的键删掉；文档从未被改过 ⇒ 自然逐字节还原）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "get_preferences",
        profile: Profile::Core,
        // **工作区级偏好** ✓（目标 ⑧-1 ✓：收藏的笔刷、最近使用 ✓）。
        // **为什么做成工具** ✓：界面要存"我收藏了哪几支" ✓ —— 那也得**两边都能用** ✓
        //（MCP 端同样能读到"这个人常用什么" ✓），所以它属于工具层 ✓，不是界面本地存储 ✗。
        summary: "读工作区偏好（收藏的笔刷等）；可只取指定的几个键",
        mutating: false,
        params: &[param!("keys", Array, false, "只要这几个键（缺省全部）")],
    },
    ToolSpec {
        name: "set_preferences",
        profile: Profile::Core,
        summary: "合并写入工作区偏好（值为 null 表示删除该键）",
        mutating: true,
        params: &[param!("values", Object, true, "要合并进来的键值对")],
    },
    ToolSpec {
        name: "get_log",
        profile: Profile::Core,
        summary: "读取原子日志元数据（MCP 轮询通道）",
        mutating: false,
        params: &[
            param!("since_seq", Integer, false, "只返回 seq 大于它的原子"),
            param!("limit", Integer, false, "最多返回条数"),
            param!("kind", String, false, "按原子类型过滤"),
            param!("actor", String, false, "按操作者过滤"),
            // **★ 必须在这里声明 ✗ ★**（第 187 轮 ✓）：**未声明的键**进不了 `args` ✗**
            //   ⇒ **∴ 只读 `args["order"]` **不**够 ✓（**上一轮 `layer_id` 踩过 ✓）** ✓✓
            param!(
                "order",
                String,
                false,
                "排序：缺省**升序**（最早的 N 条，行为与从前一致）；\n\
                 `desc` ⇒ **最新的 N 条**且**从新到旧**（历史面板要这个）。"
            ),
        ],
    },
    // ---- Job ----
    ToolSpec {
        name: "get_job",
        profile: Profile::Core,
        summary: "查询 Job 状态机与结果",
        mutating: false,
        params: &[param!("job_id", String, true, "job id")],
    },
    ToolSpec {
        // 设计 6.3 的孤儿 GC。此前 `Document::collect_garbage` 已实现但**没有任何入口** ✗：
        // 渲染产生的 blob（预览/导出/逐像素自检）不属于任何原子引用，属于「孤儿」，
        // 只能靠 TTL 到期回收，而没有任何东西触发 GC。实测一个工作区因此累积到 1.3GB。
        //
        // 默认**干跑**（`confirm=false`）只报告，不删除任何东西；`confirm=true` 才真正回收。
        name: "collect_garbage",
        profile: Profile::Structure,
        summary: "Blob 孤儿回收（设计 6.3）：默认只报告，confirm=true 才删除",
        mutating: false,
        params: &[
            param!("confirm", Boolean, false, "true 才真正删除孤儿；缺省 false 只报告"),
            param!(
                "ttl_seconds",
                Integer,
                false,
                "覆盖孤儿 TTL（秒）。只影响「多久没被引用才算过期」；缺省用服务端配置（7 天）"
            ),
        ],
    },
    ToolSpec {
        name: "get_render_status",
        profile: Profile::Core,
        summary: "查询某个原子是否已渲染。`rendered=true` 只说明**这一笔的像素已经渲染出来了**；`thumbnail_current=false` 说明 `thumb_url` 指向的**文档缩略图**还没跟上 head（它是缓存，要新鲜请再调 `get_document`，它会当场重建）",
        mutating: false,
        params: &[param!("atom_id", String, true, "原子 id")],
    },
    ToolSpec {
        name: "cancel_job",
        profile: Profile::Core,
        summary: "取消运行中的 Job",
        mutating: false,
        params: &[param!("job_id", String, true, "job id")],
    },
    // ---- 在飞操作：观察与取消（外部测试报告 P1）----
    ToolSpec {
        name: "get_inflight",
        profile: Profile::Core,
        summary: "**看文档上有没有正在跑的变更操作**（谁在跑、跑了多久、是否已被请求取消）。客户端超时后先用它分清\"慢\"与\"挂死\"，再决定重试还是取消——盲目重试会重复落笔",
        mutating: false,
        params: &[param!("doc_id", String, false, "文档 id（缺省 = 当前文档；也可看全部 = \"*\"）")],
    },
    ToolSpec {
        name: "cancel_operation",
        profile: Profile::Core,
        summary: "**请求取消当前文档上在飞的变更操作**（协作式：长循环在安全点看到标志后停手，不再提交新原子；`batch` 会整体回滚）。这是超时客户端的\"中止\"按钮，不是重试",
        // **声明成非变更工具** ✗：它**不碰文档** ✓；但它是控制面动作 ⇒ 在实现里**显式查角色** ✓
        //（若声明成 mutating ✓，那么"忙时"它自己会被 busy 拒绝 ✗ —— 那正是最需要它的时候 ✗）。
        mutating: false,
        params: &[param!("doc_id", String, false, "文档 id（缺省 = 当前文档）")],
    },
    // ---- 批量 ----
    ToolSpec {
        name: "batch",
        profile: Profile::Core,
        summary: "批量调用（同一变更集，可整体撤销）。\
                 **连续作画提速** ✓：`silent: true` 时子调用**不产出预览图** ✓\
                 ⇒ 也就**不编码 PNG** ✓（实测每笔省 ~50 ms ✓，4K / `Clouds.myb` / size 180 ✓）。\
                 需要中途看进展就用 `preview_every_n_strokes: N`（每 N 笔放行一次 ✓）\
                 或 `preview_interval_ms: M`（每 M 毫秒放行一次 ✓）—— 放行的那一次**照常产出预览** ✓。\
                 **单笔绘制**请用 `brush_stroke(preview: false)` ✓（同一条免编码路 ✓），\
                 要图时**不传**它即可 ✓（默认行为不变 ✓）。\
                 顶层 `ok` 即全成（与 `all_ok` 一致 ✓，A6 方案 ①）：有子调用失败 ⇒ `ok:false` + `error` 指路 `failed_calls` ✓。",
        mutating: true,
        params: &[
            param!("calls", Array, true, "[{tool, arguments}]"),
            param!("preview_every_n_strokes", Number, false, "每 N 个调用放行一次预览（长批次中途给 AI 看进展；缺省 0 = 不放行，行为不变）"),
            param!("preview_interval_ms", Number, false, "每 M 毫秒放行一次预览（与上一个参数是「或」的关系；缺省 0 = 不放行）"),
            param!(
                "preview",
                Boolean,
                false,
                "true = 批处理完直接带回一张预览图（省掉手动再调 render_region 的往返；缺省 false，行为不变）"
            ),
            param!("message", String, false, "变更集说明"),
            param!(
                "silent",
                Boolean,
                false,
                "true = 不生成每次调用的预览（大批量排线时省下大量临时 IO；缺省 false，行为不变）"
            ),
        ],
    },
    // ---- 扩展：history ----
    ToolSpec {
        name: "checkpoint",
        profile: Profile::History,
        summary: "创建检查点（用户语义，锚定 anchor_seq）",
        mutating: true,
        params: &[
            param!("name", String, false, "检查点名"),
            param!("message", String, false, "说明"),
        ],
    },
    ToolSpec {
        name: "get_checkpoints",
        profile: Profile::History,
        summary: "列出检查点",
        mutating: false,
        params: NO_PARAMS,
    },
    ToolSpec {
        name: "declare_head",
        profile: Profile::History,
        summary: "求值起点跳变（declare_head，5.5）",
        mutating: true,
        params: &[
            param!("base_type", String, true, "atom | checkpoint"),
            param!("base_id", String, true, "目标原子或检查点 id"),
            param!("reason", String, false, "原因"),
        ],
    },
    ToolSpec {
        name: "revert_to",
        profile: Profile::History,
        summary: "回到某个原子时刻（通过 declare_head 实现）",
        mutating: true,
        params: &[param!("atom_id", String, true, "目标原子 id")],
    },
    ToolSpec {
        name: "restore_checkpoint",
        profile: Profile::History,
        summary: "恢复到检查点（通过 declare_head 实现）",
        mutating: true,
        params: &[param!("checkpoint_id", String, true, "检查点 id")],
    },
    // ---- 扩展：annotation ----
    ToolSpec {
        name: "create_annotation",
        profile: Profile::Annotation,
        summary: "创建标注（独立 append-only 通道，不进原子日志）",
        mutating: false,
        params: &[
            param!(
                "type",
                String,
                true,
                "region|object|arrow|text|doodle|highlight"
            ),
            param!(
                "intent",
                String,
                true,
                "modify|add|remove|replace|style|move|resize|color"
            ),
            param!(
                "target",
                Object,
                true,
                "{target:'region',bbox} 或 {target:'object',object_id}"
            ),
            param!("content", String, false, "内容或路径"),
            param!("suggestion_id", String, false, "关联建议 id（4b：标注 → 建议的追踪）"),
        ],
    },
    ToolSpec {
        name: "update_annotation",
        profile: Profile::Annotation,
        summary: "更新标注（追加新版本）；status=pending 表示重开（已 resolved/rejected 的标注须先重开才能再解决）",
        mutating: false,
        params: &[
            param!("annotation_id", String, true, "标注 id"),
            param!("content", String, false, "新内容"),
            param!("intent", String, false, "新意图"),
            param!("status", String, false, "仅支持 pending：把已解决/已拒绝的标注重开为待处理"),
        ],
    },
    ToolSpec {
        name: "delete_annotation",
        profile: Profile::Annotation,
        summary: "删除标注（标记为 rejected，保留历史）",
        mutating: false,
        params: &[param!("annotation_id", String, true, "标注 id")],
    },
    ToolSpec {
        name: "list_annotations",
        profile: Profile::Annotation,
        summary: "列出标注（按状态/作者/意图/对象过滤）",
        mutating: false,
        params: &[
            param!("status", String, false, "pending|resolved|rejected"),
            param!("actor", String, false, "按作者过滤"),
            param!("intent", String, false, "按意图过滤"),
            param!("object_id", String, false, "按对象目标过滤"),
            param!("suggestion_id", String, false, "按关联建议过滤（4b：查「引用了该建议的标注」）"),
        ],
    },
    ToolSpec {
        name: "get_annotation",
        profile: Profile::Annotation,
        summary: "读取标注当前版本与修订历史",
        mutating: false,
        params: &[param!("annotation_id", String, true, "标注 id")],
    },
    ToolSpec {
        name: "resolve_annotation",
        profile: Profile::Annotation,
        summary: "标记标注已解决（可关联解决它的原子）",
        mutating: false,
        params: &[
            param!("annotation_id", String, true, "标注 id"),
            param!("atom_id", String, false, "解决它的原子"),
        ],
    },
    ToolSpec {
        name: "reject_annotation",
        profile: Profile::Annotation,
        summary: "拒绝标注",
        mutating: false,
        params: &[param!("annotation_id", String, true, "标注 id")],
    },
    // ---- 扩展：structure（蒙版；内核已实现按形状覆盖率 + 羽化 + 反选调制图层 alpha）----
    ToolSpec {
        name: "create_mask",
        profile: Profile::Structure,
        summary: "创建蒙版（形状覆盖率 + 羽化 + 反选），随后用 set_property 把 mask_id 挂到图层",
        mutating: true,
        params: &[
            param!("mask_id", String, true, "蒙版 id"),
            // **不是必填** ✓（外部第 10 轮 `selection-mask-flat-rect` 实测 ✓）：给了 `from_selection`
            // 时形状从选区克隆 ✓，而这里原来标 `required: true` ✗ ⇒ **派发前的必填校验**直接拦下
            // ⇒ **文档里写的「此时 shape 可省略」一次也用不了** ✗（"描述说支持、schema 拒绝"家族 ✓）。
            param!("shape", Object, false, "几何 {kind:rect|ellipse|polygon|path, bbox, points?}；**与 `from_selection` 二选一**（后者会把选区的形状/羽化/反选/位图一并克隆过来）。kind 与 type 互为别名（只有 type 时按 type 走）；kind=path 时三选一：subpaths（内联子路径，与 draw_shape 的 path geometry 同格式）、d（SVG path 字符串，入口转 subpaths）或 object_id（引用已有 path 形状对象），覆盖率走 path 填充同一条路"),
            param!("feather", Number, false, "羽化过渡宽度（像素，缺省 0）"),
            param!("invert", Boolean, false, "反选（缺省 false）"),
            param!("mode", String, false, "缺省 new = 新建（id 已存在则报错）。add/subtract/intersect 现在**只能对一个已存在的 id** 放行，但落盘是**覆盖**旧形状，**不是**布尔合并 ⇒ 结果等于最后那个形状。真正的合并尚未实现（见 docs/design/brush-capability-gap.md 三百二十七节）"),
            param!("linked_layer", String, false, "关联图层 id"),
            param!("from_selection", String, false, "从已有选区一键转换：传 selection_id 或 \"latest\"，克隆其 shape/feather/invert/blobs/patches；此时 shape 可省略，显式传的 shape/feather/invert 优先"),
        ],
    },
    ToolSpec {
        name: "paint_mask",
        profile: Profile::Structure,
        summary: "蒙版画笔/擦除：在蒙版上按笔刷沉积灰度（Target-Value Lerp 合成）",
        mutating: true,
        params: &[
            param!("mask_id", String, true, "目标蒙版 id（Q2：也接受选区 id，直接修整选区边缘，即快速蒙版语义）"),
            param!("points", Array, true, "笔触控制点 [[x,y,pressure?],...]"),
            param!("mode", String, false, "paint（加选）/erase（减选），缺省 paint"),
            param!("mask_value", Number, false, "目标灰度 0-1（缺省 paint=1/erase=0，可取中间值做半透明选区）"),
            param!("size", Number, false, "笔刷直径像素（缺省 20）"),
            param!("softness", Number, false, "笔尖柔边 0-1（缺省 0.5）"),
            param!("opacity", Number, false, "不透明度 0-1（缺省 1）"),
            param!("flow", Number, false, "流量 0-1，与 opacity 相乘得每 dab 实际不透明度（缺省 1）"),
            param!("humanize", Boolean, false, "人性化抖动包络（缺省 false）"),
            param!("preset", String, false, "笔刷预设名（缺省基础圆笔；颜色恒强制纯白）"),
        ],
    },
    ToolSpec {
        name: "delete_mask",
        profile: Profile::Structure,
        summary: "删除蒙版（tombstone；blobs/patches 保留，撤销可回退）",
        mutating: true,
        params: &[param!("mask_id", String, true, "目标蒙版 id")],
    },
    // ---- 扩展：retouch（调色与滤镜；内核已实现的子集）----
    ToolSpec {
        name: "add_adjustment",
        profile: Profile::Retouch,
        summary: "新增调整图层对象（调色）：brightness_contrast / saturation / vibrance / invert / levels（可分通道）/ exposure / white_balance / curves / hsl / posterize / color_balance / split_toning / black_and_white / channel_mixer / gradient_map / shadows_highlights / selective_color / color_lut / red_eye / denoise / lens / perspective / dehaze / clarity / texture（Presence 三件套，amount −1..1，0=恒等）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("adjustment_type", String, true, "调整类型（见工具说明）"),
            param!(
                "params",
                Object,
                false,
                "类型参数，如 {brightness:0.1, contrast:1.2}。恒等值：saturation 的 amount 是乘子（1.0 = 不变，0 = 灰度，>1 增强）；vibrance 的 amount 是加量（0 = 不变）。hsl 支持 params.ranges.<档>={hue, saturation, lightness}，8 段 reds/oranges/yellows/greens/aquas/blues/purples/magentas；档内三键与全局三键同名不同义——全局 hue ±360、saturation 是乘子 0..8（恒等 1.0）、lightness ±1，档内 hue ±180、saturation 是百分比因子 ±100（恒等 0）、lightness ±100（恒等 0）；selective_color 用 params.adjust.<档>=[c,m,y,k]，9 档 reds/yellows/greens/cyans/blues/magentas/whites/neutrals/blacks（每分量 [-1,1]）；levels 支持 channel: rgb|r|g|b（缺省 rgb，分通道色阶）"
            ),
            param!("opacity", Number, false, "整体不透明度 0-1（缺省 1）"),
            param!(
                "z_index",
                Integer,
                false,
                "层内 z 序（缺省放到同层最上方，作用于全部下方内容）"
            ),
            param!("name", String, false, "对象名（元数据）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）；若该 id 是已删除对象（墓碑），将复活它并整组换新参数（响应带 revived: true）"),
        ],
    },
    ToolSpec {
        name: "apply_adjustments",
        profile: Profile::Retouch,
        summary: "批量配方：一次按序建多个调整对象（先全量校验，任一项非法则整组不落成、报错指明第几项）。每项 {adjustment_type, params, layer_id?, opacity?, z_index?, name?, object_id?}；项内没给 layer_id 时用顶层 layer_id",
        mutating: true,
        params: &[
            param!(
                "adjustments",
                Array,
                true,
                "配方数组（1..32 项）：[{adjustment_type, params, layer_id?}, …]，按数组顺序落成"
            ),
            param!("layer_id", String, false, "顶层缺省图层（项内 layer_id 优先）"),
        ],
    },
    ToolSpec {
        name: "add_filter",
        profile: Profile::Retouch,
        summary:
            "新增滤镜图层对象：box_blur / gaussian_blur / brightness_contrast / saturation / invert",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("filter_name", String, true, "滤镜名（见工具说明）。**`relief`（厚涂浮雕）的参数与实测范围**（第 254 轮修过一处设计缺陷后**重测** ✓）：`strength` **0.5–10** 都可用且**均值不漂**（实测漂移 ≤0.4/255 ✓），**越大只是浮雕越强**（对比 +0.0 ⇒ +2.2 ✓）—— **0.5 轻微／2 明显／5–10 强烈** ✓；`height_gain` 10–30 常见；`azimuth` 0–360（缺省 135）、`elevation` 1–89（缺省 45）✓ ★需要额外参数的：`dehaze` 必须给 `air: [r,g,b]`（可用 `estimate_dehaze` 估计）✓；其余 13 个不需要 ✓。"),
            param!("params", Object, false, "滤镜参数（按 `filter_name` 取；每个滤镜的键见工具说明）。\n**`brightness_contrast` 约定（期 3 A1 起，滤镜/调整两条路同一实现、同一约定）**：显示空间枢轴口径——先平移亮度、再绕显示中灰 0.5 缩放对比、按亮度比回写（色相不扭、HDR 不钳）。\n· `brightness`：**显示平移，恒等值 `0`**（`±0.5` ⇔ 中灰显示 ±0.25）\n· `contrast`：**绕枢轴缩放，恒等值 `1`**（`0` = 全图压平为显示中灰，**不再是全黑**）\n要**不变**请**省略参数**或写 `{brightness:0, contrast:1}`。\n**`saturation` 约定**：`amount` 是乘子，恒等值 `1`（`0` = 灰度，`>1` 增强）。"),
            param!("opacity", Number, false, "整体不透明度 0-1（缺省 1）"),
            param!(
                "z_index",
                Integer,
                false,
                "层内 z 序（缺省放到同层最上方，作用于全部下方内容）"
            ),
            param!("name", String, false, "对象名（元数据）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）；若该 id 是已删除对象（墓碑），将复活它并整组换新参数（响应带 revived: true）"),
        ],
    },
    ToolSpec {
        name: "update_adjustment",
        profile: Profile::Retouch,
        summary: "修改调整图层参数（叠加到现有参数上）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "调整对象 id"),
            param!("params", Object, true, "要覆盖的参数"),
            param!("opacity", Number, false, "整体不透明度 0-1"),
        ],
    },
    ToolSpec {
        name: "update_filter",
        profile: Profile::Retouch,
        summary: "修改滤镜参数（叠加到现有参数上）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "滤镜对象 id"),
            param!("params", Object, true, "要覆盖的参数"),
            param!("opacity", Number, false, "整体不透明度 0-1"),
        ],
    },
    ToolSpec {
        name: "list_effects",
        profile: Profile::Retouch,
        summary: "列出当前文档的调整/滤镜对象（含参数与生效顺序）",
        mutating: false,
        params: &[param!("layer_id", String, false, "按图层过滤")],
    },
    // ---- 风格预设系统（设计 docs/design/style-preset-system.md；展开器在 presets.rs）----
    ToolSpec {
        name: "list_presets",
        profile: Profile::Retouch,
        summary: "列出照片风格预设（22 套内置 + YANSHI_PRESET_DIR 外挂）：id/中英文名/状态（established/fixed_v2/needs_review）/标签/适用与慎用题材/旋钮；选风格前先读它，不要盲选",
        mutating: false,
        params: &[
            param!("tag", String, false, "按标签筛（如 夜景 / 人像 / 风光）"),
            param!("subject", String, false, "按适用题材子串筛（如 霓虹 / 森林）"),
            param!("status", String, false, "按状态筛（established / fixed_v2 / needs_review）"),
        ],
    },
    ToolSpec {
        name: "get_preset",
        profile: Profile::Retouch,
        summary: "读一套预设的完整定义：修正段（full/light 基线）+ 风格段配方与中性值、旋钮映射、实测结论与 unsupported 说明，并附 strength=1 的展开预览",
        mutating: false,
        params: &[param!("preset_id", String, true, "预设 id（先用 list_presets 查）")],
    },
    ToolSpec {
        name: "apply_preset",
        profile: Profile::Retouch,
        summary: "把一套风格预设展开并落到图层上：先全量校验任一非法整组不落；确定性 object_id，二次应用同预设走更新（Supersede）不叠加；同 id 对象已被删除（墓碑）时复活并换新参数、响应以 revived_object_ids 点名；strength 0..1 插值（0=风格段恒等）、knobs 语义旋钮覆盖、include_correction 缺省 true",
        mutating: true,
        params: &[
            param!("preset_id", String, true, "预设 id（先用 list_presets 查）"),
            param!("layer_id", String, true, "目标图层"),
            param!("strength", Number, false, "全局强度 0..1（缺省 1；0 时风格段逐参数等于中性值）"),
            param!("knobs", Object, false, "语义旋钮覆盖 {旋钮id: 数值}（可用旋钮见 list_presets / get_preset；越界或未知旋钮报错）"),
            param!("include_correction", Boolean, false, "是否同时落修正段（缺省 true；只调风格时给 false）"),
        ],
    },
    ToolSpec {
        name: "preview_preset",
        profile: Profile::Retouch,
        summary: "只展开不落原文档：按 strength/knobs 算出将要落的全部操作清单（含确定性 object_id 与实际参数），供迭代调风格时先看再 apply_preset；可选 render 走临时文档渲染出代理图（临时文档在所有出口删除，净状态变化为零，故仍为只读工具）",
        mutating: false,
        params: &[
            param!("preset_id", String, true, "预设 id（先用 list_presets 查）"),
            param!("strength", Number, false, "全局强度 0..1（缺省 1）"),
            param!("knobs", Object, false, "语义旋钮覆盖 {旋钮id: 数值}"),
            param!("include_correction", Boolean, false, "展开清单是否含修正段（缺省 true）"),
            param!("layer_id", String, false, "render 时必填：源图层 id（不给 render 时忽略）"),
            param!("render", Object, false, "可选渲染：{max_edge 缺省 512（128–2048）, format png|jpeg 缺省 jpeg}；不给则只展开不渲染"),
        ],
    },
    // ---- 扩展：retouch（修图）----
    ToolSpec {
        name: "estimate_dehaze",
        profile: Profile::Retouch,
        summary: "估计去雾参数（只读）：扫描当前画面给出建议大气光 air 与暗通道均值，供 add_filter(dehaze) 使用",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "frequency_separation",
        profile: Profile::Retouch,
        summary: "频率分离（期 4 D1）：把源图层烘焙成低频层 + 高频层（linear_light）+ 两层绘画空层，装进一个组；低频修色调、高频修细节互不干扰。低频/高频是导入时刻的像素快照：源图层此后的修改不传播（与 raster patch 同类）。高频层的中性灰是 188（线性 0.5 的 sRGB 编码），不是 PS 式的 128，在高频层上涂抹请用 188 作中性色。半透明像素的组 alpha 不精确保持源 alpha，按不透明照片层设计。重建刻画（实测）：中高调（源字节 ≥ 128）≤ 2 LSB；暗部受 8 位 HF 字节网格限制，近黑强细节处可达约 16 LSB，均值 ≤ 3 LSB；自检只计全不透明邻域像素。空层/纯调整层会明确拒绝，不留半套组",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "源图层 id（必填）"),
            param!("radius", Number, false, "分离半径（高斯 sigma，缺省 8.0，域 [1, 64]）"),
            param!("name", String, false, "组名前缀（缺省源图层名）"),
        ],
    },
    ToolSpec {
        name: "dodge_burn_setup",
        profile: Profile::Retouch,
        summary: "减淡加深建组（期 4 D1）：curves 变体（缺省）建两个带全黑蒙版的曲线层（减淡中点上凸、加深中点下凹，中点参数给显示空间值、工具按 §9.1 EOTF 转线性落盘），用 paint_mask 以白色低不透明度笔刷涂抹生效区；gray 变体建一个 50% 灰 soft_light 层，用黑/白低流量笔触涂抹。curves 变体零烘焙、可反复调曲线；gray 变体单层直观、代价是灰层为烘焙快照。重复调用同名组返回 already_exists，不叠第二套。回执另给 `masks: {dodge, burn}`（按角色标注蒙版，`mask_ids` 数组本身保留；gray 组两格为 null）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层 id（必填；新组落在它上方，组名缺省取它的名字）"),
            param!("mode", String, false, "curves（缺省）或 gray"),
            param!("dodge_mid", Number, false, "减淡曲线显示空间中点（缺省 0.62，域 [0.05, 0.95]，0.5 = 无效果）"),
            param!("burn_mid", Number, false, "加深曲线显示空间中点（缺省 0.38，域 [0.05, 0.95]）"),
            param!("name", String, false, "组名前缀（缺省源图层名）"),
        ],
    },
    ToolSpec {
        name: "auto_retouch",
        profile: Profile::Retouch,
        summary: "自动模式（期 4 D3）：从图像统计里用有原理的算法求出每组参数并落成一组确定性对象（曝光/色阶/影调/白平衡/色彩/细节/降噪，细节组含 clarity/texture/dehaze，降噪组含 denoise/sharpen）；甜点带只作收缩先验与钳制。估计取自目标层位图/RAW 源对象的子集渲染（调整/滤镜对象排除在外），故重跑幂等、已落成的 auto 对象不污染估计。白平衡估的是显影后残余偏色（RAW 机内白平衡已在显影时施加）。每项 rationale 点名估计数字与收缩原因；skipped 逐对象列出原因（含未请求的组）。同层重跑原地更新（确定性 id + 复活路）；目标层已有人工调整/滤镜对象时拒绝（除非 force=true）；RAW 导入的缺省曝光对象例外：未动过（名 + ev 0.5 全匹配）或已被 auto 置零（ev 0.0）的不触发拒绝，曝光落成时先把它置零防叠加（回执 neutralized_default_exposure 点名），改过 ev 的视为人工照常拒绝。dry_run 或 strength=0 时只报告不落成（applied 为空，planned 给收缩后估计值）。重跑时某组新跳过不会删掉旧对象（v1 已知局限）。",
        mutating: true,
        params: &[
            param!("layer_id", String, false, "目标图层 id（缺省：按 z 序首个含位图/RAW 对象的图层）"),
            param!("strength", Number, false, "强度 0–1（缺省 1.0；0 = 只报告不落成；各参数向中性值缩放，denoise detail 与 sharpen radius 为模式常数不缩放）"),
            param!("groups", Array, false, "组白名单字符串数组（缺省全部 7 组：exposure/levels/tone/wb/color/detail/noise；去雾挂在 detail 组下）"),
            param!("dry_run", Boolean, false, "只报告不落成（缺省 false）"),
            param!("force", Boolean, false, "目标层已有人工调整/滤镜对象时仍执行（缺省 false = 拒绝）"),
        ],
    },
    ToolSpec {
        name: "liquify_push",
        profile: Profile::Retouch,
        summary: "基础液化（推力）：把笔迹范围内的像素沿 direction 推开（反向映射 + 双线性重采样）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("direction", Array, true, "推力方向 [dx,dy]（非零）"),
            param!("size", Number, false, "影响**直径**（缺省 80；没有 radius 参数，直径＝半径 ×2 ⇒ 想要半径 20 就传 40）"),
            param!("strength", Number, false, "强度 0-2（缺省 0.5）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "liquify_twirl",
        profile: Profile::Retouch,
        summary: "液化（旋转）：把笔迹范围内的像素绕笔迹点旋转（强度为弧度上限，负值反向）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("size", Number, false, "影响直径（缺省 80）"),
            param!("strength", Number, false, "强度 0-2（缺省 0.5，1.0 ≈ 57°）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "liquify_pinch",
        profile: Profile::Retouch,
        summary: "液化（收缩/膨胀）：正强度把内容吸向中心，负强度向外膨胀",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("size", Number, false, "影响直径（缺省 80）"),
            param!("strength", Number, false, "强度 -2..2（缺省 0.5；正=收缩）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "patch",
        profile: Profile::Retouch,
        summary: "图章补丁：把 source_region 的像素抓取为 blob，并作为 raster_patch 落到 target 位置（blob 先行，6.3）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("source_region", Object, true, "源区域 {x,y,w,h}"),
            param!("target", Array, true, "落点左上角 [x,y]"),
            param!("opacity", Number, false, "不透明度 0-1（缺省 1）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "smudge",
        profile: Profile::Retouch,
        summary: "涂抹：沿笔迹方向把后方的已有内容拖到前方（每 stamp 后退 smudge_length 像素采样）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("size", Number, false, "笔刷直径（缺省 24）"),
            param!("smudge_length", Number, false, "采样后退距离（缺省 12）"),
            param!("deposit", Number, false, "**沉积量 0..1（缺省 0 ＝ 只拖不补 ✓）**：`>0` 时把被拖过去的色往**更实**压一点 ⇒ 既带色、又不造新硬边"),
            param!("hardness", Number, false, "硬度 0-1（缺省 0.6）"),
            param!("opacity", Number, false, "不透明度 0-1（缺省 1）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "heal_stamp",
        profile: Profile::Retouch,
        summary: "修复画笔：复制 source_offset 处的纹理，并把低频颜色/明度对齐到目标处",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("source_offset", Array, true, "采样偏移 [dx,dy]，源 = 目标 + 偏移"),
            param!("size", Number, false, "笔刷直径（缺省 24）"),
            param!("hardness", Number, false, "硬度 0-1（缺省 0.6）"),
            param!("opacity", Number, false, "不透明度 0-1（缺省 1）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "clone_stamp",
        profile: Profile::Retouch,
        summary: "仿制图章：把 source_offset 处的已有内容复制到 points 轨迹上（源为应用本对象前的图层内容）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("source_offset", Array, true, "采样偏移 [dx,dy]，源 = 目标 + 偏移"),
            param!("size", Number, false, "笔刷直径（缺省 24）"),
            param!("hardness", Number, false, "硬度 0-1（缺省 0.6）"),
            param!("opacity", Number, false, "不透明度 0-1（缺省 1）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    // ---- 扩展：collab ----
    ToolSpec {
        name: "suggest",
        profile: Profile::Collab,
        summary: "记录一条建议（suggest 原子，含 patch）：AI 解析标注后提出可执行的补丁步骤",
        mutating: true,
        params: &[
            param!("patch", Array, true, "补丁步骤 [{\"tool\":\"add_filter\",\"arguments\":{...}}, ...]（非空）"),
            param!("annotation_id", String, false, "关联的标注 id（写入建议以追踪来源）"),
            param!("summary", String, false, "人类可读的说明"),
            param!("priority", Integer, false, "优先级 0..9（越大越紧急，缺省 5）；list_suggestions 按它排序"),
        ],
    },
    ToolSpec {
        name: "accept_suggestions",
        profile: Profile::Collab,
        summary: "批量接受建议（最多 64 条）：逐条重放 patch，个别失败不中断，逐条返回结果",
        mutating: true,
        params: &[
            param!("suggestion_ids", Array, true, "建议 id 列表"),
        ],
    },
    ToolSpec {
        name: "preview_suggestion",
        profile: Profile::Collab,
        summary: "预览建议（不应用）：逐步校验工具名与参数、报告目标与变更类别，便于人工/AI 审阅",
        mutating: false,
        params: &[
            param!("suggestion_id", String, false, "建议 id（与 patch 二选一）"),
            param!("patch", Array, false, "直接给出 patch 做预校验（与 suggestion_id 二选一）"),
        ],
    },
    ToolSpec {
        name: "accept_suggestion",
        profile: Profile::Collab,
        summary: "接受建议：按序重放 patch（只允许会产生状态效果的步骤），提交 accept_suggestion 原子并把关联标注置为 resolved",
        mutating: true,
        params: &[
            param!("suggestion_id", String, true, "建议 id（suggest 原子的 id）"),
        ],
    },
    ToolSpec {
        name: "reject_suggestion",
        profile: Profile::Collab,
        summary: "拒绝建议：记录原因，并把引用该建议的标注置为 rejected（12.6）",
        mutating: true,
        params: &[
            param!("suggestion_id", String, true, "建议 id（suggest 原子的 id）"),
            param!("reason", String, false, "拒绝原因"),
        ],
    },
    ToolSpec {
        name: "reject_suggestions",
        profile: Profile::Collab,
        summary: "批量拒绝建议（最多 64 条）：逐条记录原因并联动标注状态，个别失败不中断",
        mutating: true,
        params: &[
            param!("suggestion_ids", Array, true, "建议 id 列表"),
            param!("reason", String, false, "统一的拒绝原因"),
        ],
    },
    ToolSpec {
        name: "new_document",
        profile: Profile::Core,
        // **描述必须与实现一致** ✓（真实用户报告 ✓）：它原来写着"新建（**或清空重建**）" ✗
        // ⇒ 而实现**明确拒绝**覆盖已存在的 id ✓ ⇒ **遵循这份描述的 Agent 会撞墙** ✗ ——
        // 契约与行为不符 ✓ 比功能缺失更坏 ✗（调用方会按描述写出**必然失败**的代码 ✓）。
        // **语义照实写清** ✓：新画布 = **新的 doc_id** ✓（文档 id 就是持久单元 ✓，这是设计 ✓）。
        summary: "新建文档（doc_id / 宽高 / 背景色）：新 id ⇒ 建好并返回 created:true / opened:false；**若该 doc_id 已存在 ⇒ 打开它**（返回 opened:true / created:false，内容与尺寸都不动）；要一块干净画布请换新 doc_id",
        mutating: true,
        params: &[
            param!("doc_id", String, false, "文档 id（缺省用当前会话的文档）"),
            param!("width", Integer, true, "宽（像素）"),
            param!("height", Integer, true, "高（像素）"),
            param!("background", Object, false, "背景 {r,g,b,a}（0..255；缺省不透明白）"),
        ],
    },
    ToolSpec {
        name: "medium_stroke",
        profile: Profile::Core,
        summary: "**要介质插件（油画 / 水彩 / 马克笔 / 铅笔 / 像素）的模拟就用这个** ✓：**载墨量 `load`、湿度 `wetness`、光谱混色 `color_mixing=pigment` 都在这里** ✓（**∵ `brush_params` 走 `.myb`，那里没有这两项** ✓）；服务端原生调用插件，产出带 medium 描述符的补丁。                  与邻居的分工：要 **201 支 .myb 笔刷**（MyPaint 物理）用 brush_stroke；要**纯几何矢量**笔迹用 draw_stroke。                  **每条笔触只作用于它自己的图层** ✓：跨图层只是普通叠加；介质的湿搅 / 混色**不跨图层** ✓（先把底下那层画完，或用同一层叠 ✓）。                  **覆盖率／落墨量以 clipped 为准** ✓：`painted_pixels` 即真实落墨（本工具无选区裁剪，同值字段 `painted_pixels_raw`）；顶层 `landed`／`landed_reason` 直接给结论。",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("medium", String, true, "介质 id：oil | watercolor | marker | pencil | pixel | example"),
            param!("points", Array, true, "笔迹采样点 [[x,y,pressure?], ...]（pressure 0..1）"),
            param!("size", Number, false, "笔尖大小（缺省 24）"),
            // **写法与画笔 / 形状 / 笔迹统一** ✓：同一个解析器 ✓ ⇒ 用户不必记三套 ✓。
            param!("color", Any, false, "笔尖色：{r,g,b,a}（0..255）/ [r,g,b,(a)]（0..1 线性或 0..255 字节） ⇒ **任一分量 > 1 即按 0..255 字节解释**（想要近黑请用 `#RRGGBB` 或 `{r:1,…}`） / \"#RRGGBB\"（缺省不透明黑）；显式空间见 `color_space`（不传 ⇒ 量级切换）"),
            param!("color_to", Any, false, "**末端颜色**（写法同 color）⇒ 一笔之内从 color 渐变到它（**介质也能画渐变了** ✓，第 254 轮补：验收实测过它缺失 ✗）；给了它就必须同时给 color ✓（解释空间同 `color_space`）"),
            param!("color_space", String, false, "颜色解释空间（C1）：`srgb` ⇒ 0..1 浮点数组按 sRGB 字节解释（`v*255` 取整再解码）；`linear` ⇒ 浮点数组按线性直通（即使分量 > 1 也不按字节解释）；不传 ⇒ 现行量级切换（任一分量 > 1 即字节，否则线性）。作用于 `color` 与 `color_to`；对象式 / hex 恒为字节，不受本参数影响。回执 `applied.color_space` 回显实际生效的单位（`linear` / `srgb_bytes` / `hex`）"),
            param!("color_mixing", String, false, "颜色混合模式：rgb（**缺省**，sRGB 空间插值）/ linear（**线性光混色**，物理正确：红+蓝各 50% 比 rgb 亮 **35 级**）/ pigment（**光谱 K/S 混色**：黄+蓝混出绿而不是灰）；**三种都走 `mix_colors` 同一处实现** ✓"),
            param!("smooth", Boolean, false, "true ⇒ 控制点按 **Catmull-Rom** 重采样（与 brush_stroke / draw_stroke 同一条实现 ✓）⇒ 手写的折线不再有硬角；缺省 false ⇒ 与前完全一致"),
            param!("symmetry", String, false, "★ 对称模式（第 253 轮）：none（缺省）/ vertical / horizontal ⇒ 一笔画两份（镜像那份与原笔像素相同，轴取画布中心线）"),
            param!("load", Number, false, "载墨 0..1（缺省 1；越画越少）"),
            param!("wetness", Number, false, "湿度 0..1（缺省 0.4）"),
            param!(
                "texture",
                Number,
                false,
                "纹理强度 0..1（缺省 0 = 插件原本的笔痕；越大越平滑，油画大面积铺色时用得上）"
            ),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "export_project",
        profile: Profile::Core,
        summary: "把一份文档打成 .yanshi 工程包（未压缩 tar：原子日志 + 元数据 + 引用到的 blob，blob 用 zlib 逐字节无损压缩；能证明重放得出来的位图缺省不装）；回可直接下载的 url，path 可选（另存一份到服务器）",
        mutating: false,
        params: &[
            param!("path", String, false, "可选：**服务器上**的另存路径（建议以 .yanshi 结尾；内容其实是未压缩 tar）。不填 ⇒ 只回可直接下载的 url（浏览器走这条）"),
            param!("doc_id", String, false, "要导出的文档（缺省当前会话的文档）"),
            param!(
                "include_bitmaps",
                Boolean,
                false,
                "缺省 false：能证明重放得出来的位图不装（包更小）；true：每个被引用的位图都装（老行为，排查用）"
            ),
        ],
    },
    ToolSpec {
        name: "import_project",
        profile: Profile::Core,
        // **与 `export_project` 配对** ✓（用户当初报的正是"没有导入导出工程" ✓；导出早就有了 ✓，导入一直没有 ✗）。
        summary: "导入 .yanshi 工程包（未压缩 tar）；**不会覆盖**已存在的 doc_id，blob 会按内容哈希核对",
        mutating: true,
        params: &[
            param!("path", String, true, "工程包路径（.yanshi）"),
            param!("doc_id", String, false, "导入成哪个文档（缺省用包内 meta.json 记的那个）"),
        ],
    },
    ToolSpec {
        name: "verify_package",
        profile: Profile::Core,
        summary: "体检一个 .yanshi 工程包（未压缩 tar）：读 atoms.jsonl 的全部 blob 引用，按 canonical 规则分本体/派生 tile，与包内 blobs/ 对账；回 missing_body/missing_derived 两栏 + 恒等式（missing_body + missing_derived == missing_total）；complete 为真 ⇔ 缺本体为 0（只缺派生 ⇒ 完整，可重放）；有意省掉的可重放位图单列 omitted，不算缺失",
        mutating: false,
        params: &[
            param!("path", String, true, "工程包路径（.yanshi）"),
        ],
    },
    ToolSpec {
        name: "delete_document",
        profile: Profile::Core,
        // **产品负责人要的是"真删"** ✓ —— 原来的 `DELETE` 路由只关内存 ✗
        //（那是"隐藏"不是"缺失" ✓）：现在**两个面**都能真删 ✓，而且共用同一个实现 ✓。
        summary: "**真删**一份文档（磁盘上的原子日志与元数据目录一并移除）；有实时连接时拒绝；**只删只有它引用的 blob**，被别的文档或 Stash 引用的一律保留（其他来源的孤儿不在范围内）",
        mutating: true,
        params: &[param!(
            "document_id",
            String,
            true,
            "要删除的文档 id。**注意它跟 `doc_id` 不是一回事**：`doc_id` 是本次调用的会话文档（框架会按需创建），`document_id` 才是要被删掉的那一份"
        )],
    },
    ToolSpec {
        name: "scatter_strokes",
        profile: Profile::Core,
        // **AI 画家需求 P0-1.3**：区域内随机撒笔触 —— 画头发、胡须、背景纹理、破碎色彩的核心工具。
        // **必带 `seed`**：本仓库的底线是"同输入 ⇒ 逐字节同输出"，随机撒点若不可复现就没法评审与回归。
        summary: "在区域内随机撒一批笔触（必带 seed ⇒ 同 seed 逐字节可复现）；画毛发/背景纹理/破碎色彩",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("seed", Number, true, "随机种子（必填；同 seed ⇒ 同结果）"),
            // **`points` 要写进声明** ✓（外部 `bug_report.md` BUG-04 的 ROI 第 3 条 ✓）：
            //   能力**早就有**（多边形内重采样 + 如实计数 ✓，配 `tool-scatter-polygon.mjs` ✓），
            //   而声明只写 `{x, y, w, h}` ✗ ⇒ **能力存在但没公开** ✗（调用方按声明面不敢用 ✓）。
            //   ⚠️ 他们说"按声明面调用会被 `validate_args` 拒绝" ✗ —— **实测不成立** ✓
            //   （`area:{points:…}` 直接 `ok:true` ✓：嵌套对象是 `additionalProperties:true` ✓），
            //   所以这是**文档缺口**，不是拒绝 ✓。
            param!("area", Object, true, "撒点区域：`{x, y, w, h}` 矩形，或 `{points: [[x,y], …]}` 多边形（≥3 点多边形内重采样，落不下就少落并如实计数）"),
            param!("palette", Array, true, "颜色数组 [{r,g,b,a} 或 #rrggbb]（随机取用）"),
            param!("count", Number, false, "撒几笔（缺省 24，上限 2000）"),
            param!("brush", String, true, "笔刷名（同 brush_stroke）"),
            param!("size_range", Array, false, "笔尖直径范围 [min, max]（缺省 [6, 18]）"),
            param!("opacity_range", Array, false, "不透明度范围 [min, max]（缺省 [0.4, 1.0]）"),
            param!("direction", String, false, "random（缺省）/ horizontal / vertical / 角度数字"),
            // **接上选区裁剪** ✓（外部 `bug_report.md` BUG-04 残留第 3 条 ✓）：
            //   全部落笔工具里**只有 `scatter_strokes` 没接** `clip_to_selection` ✗ ——
            //   而它内部逐笔调的就是 `write_brush_stroke` ✓（那条链早就支持 ✓）⇒ 只差**传下去** ✓。
            param!("clip_to_selection", String, false, "只把散点落在该选区里（透传 `brush_stroke`：选区 id / latest / combined）；缺省不裁剪"),
        ],
    },
    ToolSpec {
        name: "gradient_blend",
        profile: Profile::Core,
        // **AI 画家需求 P0-1.2**：用户实测"左暗右亮要手动拼 20 笔" ⇒ 这里把"拼笔"交给服务端。
        // 它**逐笔调用 `write_brush_stroke`** ⇒ 与手画**同一条落笔实现**（不是另一套笔触）。
        summary: "两点之间自动生成过渡笔触（位置与颜色同时插值）；笔数 = steps ⇒ 替代手动拼笔",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("from", Object, true, "起点 {x, y, color}（color 与 brush_stroke 同口径：数组 [r,g,b,(a)]／对象 {r,g,b,a}／\"#RRGGBB\"／\"#RRGGBBAA\" 都收）"),
            param!("to", Object, true, "终点 {x, y, color}（与 from 同口径）"),
            param!("brush", String, true, "笔刷名（同 brush_stroke）"),
            param!("size", Number, true, "笔尖直径（像素）"),
            param!("steps", Number, false, "生成几笔（缺省 10，含首末两点）"),
            param!("smooth", Boolean, false, "同 brush_stroke.smooth（缺省关）"),
        ],
    },
    ToolSpec {
        name: "gradient_fill",
        profile: Profile::Core,
        // **针对用户报过的"大面积背景难处理"** ✓：平铺纹理是一条路 ✓，**渐变**是另一条 ✓ ——
        // 而且渐变**不带纹理噪声** ✓，做天空、底色、光照过渡比笔刷铺要稳得多 ✓。
        summary: "在图层或指定区域填渐变（linear 线性 / radial 径向）；双色、可给角度与中心",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("from", Object, true, "起点色 {r,g,b,a}（0..255）"),
            param!("to", Object, true, "终点色 {r,g,b,a}"),
            param!("kind", String, false, "linear（缺省）/ radial"),
            param!("angle", Number, false, "linear 的方向角（度；0 = 从左到右，90 = 从上到下）"),
            param!("center", Object, false, "radial 的中心 {x,y}（缺省取区域中心）"),
            param!("radius", Number, false, "radial 的半径（缺省取区域对角线的一半）"),
            param!("region", Any, false, "只填这块 {x,y,w,h}；不给就整层（按画布尺寸）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "texture_background",
        profile: Profile::Core,
        // **它改文档** ✓ ⇒ `mutating: true`（新建图层 + 落一层底图 ✓；`update_layer` 的强制检查会认它 ✓）。
        // **两种用法** ✓：不给 `region` ⇒ **整幅背景** ✓（自建一层并**沉到最底** ✓）；
        // 给了 `region` ⇒ **纹理补丁** ✓（只在那一块铺 ✓、**不重排图层** ✗、没给 layer_id 时新建一层放在**原位** ✓）。
        summary: "把 CC0 纹理铺成背景或补丁（tile/stretch/cover；给 region 就只铺那一块）",
        mutating: true,
        params: &[
            param!("texture", String, true, "纹理名（assets/textures 或工作区缓存里的 .png）"),
            param!("mode", String, false, "tile（缺省）/ stretch / cover"),
            param!("region", Any, false, "只铺这块区域 {x,y,w,h}；不给就铺满整幅（并沉到底当背景）"),
            param!("layer_id", String, false, "铺到哪个图层（缺省新建一个并沉到最底）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "save_palette",
        profile: Profile::Core,
        // **AI 画家需求 P1-5（写的一侧）**：读的一侧早有 `list_palette_colors`，缺的是"把一份调色板存下来"。
        // **格式走 `.gpl`**（GIMP 调色板）—— 见 `service.rs:1687` 的解析器：它只认 `.gpl`/`.kpl`/`.json`，
        // 而 `.gpl` 天生是**扁平列表**，与 `colors: [...]` 同形（`.json` 那种是**命名字典**，塞不进扁平数组）。
        // **名字必须带扩展名**（`import_asset` 的硬约束：扩展名要与种类相符）。
        // 内部复用既有的资产导入路径 ⇒ 存进去就**读得出来**（与 `list_palette_colors` 同源）。
        summary: "把一份调色板按名字存进工作区（.gpl；同名覆盖）；存完直接回读返回",
        mutating: false,
        params: &[
            param!("name", String, true, "调色板名（**要带扩展名**，如 my.gpl；见 import_asset 的约束）"),
            param!("colors", Array, true, "颜色数组 [{r,g,b,a} 或 #rrggbb]；**任一分量 > 1 即按字节**（0..255），否则按 0..1 的比例"),
        ],
    },
    ToolSpec {
        name: "analyze_region",
        profile: Profile::Core,
        // **AI 画家需求 P2-10**：让 AI **先看再画** ✓（读一块区域的主色/亮度/冷暖 ✓）。
        // **纯读** ✓ ⇒ `mutating: false`（与 render_region 同类 ✓）。
        // **字段定义写在这里** ✓（别让调用方猜 ✗）：
        //   `dominant_colors` —— 每通道取**高 4 位**分桶 ✓（16³ 桶 ✓）⇒ 按像素数排序 ✓
        //     ⇒ 每项给该桶内像素的**平均色** ✓ 与 `count`/`share` ✓（比回桶中心准 ✓）；
        //   `avg_brightness` —— `0.299r + 0.587g + 0.114b` 的均值 ÷ 255 ✓（**0..1** ✓）；
        //   `warm_cool_ratio` —— **暖像素占（暖+冷）的比例** ✓（`r > b` 记暖 ✓、`b > r` 记冷 ✓，
        //     相等不计 ✓）⇒ **恒在 0..1** ✓、**不会除零** ✓（红块≈1 ✓、蓝块≈0 ✓）。
        summary: "分析一块区域的颜色：主色（分桶取平均）/ 平均亮度（0..1）/ 暖冷比（暖/(暖+冷)，0..1）",
        mutating: false,
        params: &[
            param!("region", Any, true, "区域 {x,y,w,h} 或 [x,y,w,h]（同 render_region）"),
            // **`compare_with_reference` 归位到只读工具** ✓（A① ✓，第 298 轮 ✓）：
            // 实现**本来就在这里**（`read_analyze_region` 内 ✓），而**声明**原先错放在
            // `gradient_fill` 上 ✗（那个工具**从未实现**它 ✓）⇒ 于是 `analyze_region` 会**拒绝**
            // 这个参数 ✓（错误体："`analyze_region` 不接受参数 `compare_with_reference`；可用参数：`region`" ✓）
            // ⇒ `tool-reference-delta-e.mjs` 长期红 ✓。
            // **∴ 本轮把声明挪到实现所在处** ✓，并把 `gradient_fill` 上那条**从未兑现**的声明删掉 ✓
            //（删掉不是"减功能" ✓，而是**删掉一句谎报** ✓ —— `gradient_fill` 从来不比 ΔE ✓）。
            // **为什么不挪实现** ✗：`gradient_fill` 是**会画画**的工具 ✓ ⇒ 连调三次就画三次 ✗
            // ⇒ "不改画面就问 ΔE"这条路只有**只读工具**能给 ✓ ⇒ 语义本来就在 `analyze_region` ✓。
            param!("compare_with_reference", Boolean, false, "true = 与文档的参考图逐像素比 ΔE（CIE76）；缺参考图 / 尺寸不一致会**明确作答**，不会静默给 0"),

        ],
    },
    ToolSpec {
        name: "list_palette_colors",
        profile: Profile::Core,
        summary: "读一个调色板的颜色（.gpl / .kpl / open-color 那种 .json）⇒ 供界面显示与取色",
        mutating: false,
        params: &[
            param!("palette", String, true, "调色板名（assets/palettes 或工作区缓存里的文件）"),
            param!("limit", Integer, false, "最多返回多少色（缺省 512；0 = 不限）"),
        ],
    },
    ToolSpec {
        name: "brush_stroke",
        profile: Profile::Core,
        // **Hokusai 引擎驱动的笔触** ✓（用户裁定：采纳 Hokusai ✓）。
        // **它和 `medium_stroke` 的关系** ✓：两条独立引擎 ✓ ——
        // 介质插件是我们自己的 wasm ABI ✓，Hokusai 读的是 libmypaint 的 `.myb` ✓
        // ⇒ 前者给"我们自己的介质" ✓，后者给"**196 支现成的 MyPaint 笔刷**" ✓。
        // **选用指南必须写在描述里** ✓（AI 外部实测 ✓：它**误用 `medium_stroke` 画了四版** ✗，
        // 才发现这个工具才是"真笔刷" ✓ —— 因为描述里只说了"怎么调"，**没说"什么时候该用它"** ✗）。
        // **行业做法** ✓：好的工具/API 文档第一句就是"**何时用它、而不是用它的邻居**" ✓
        //（MCP 官方对工具描述的要求也是这一条 ✓：模型靠它选工具 ✓）。
        summary: "**要 MyPaint 笔刷物理（dab / 笔毛 / 干湿 / 压感）就用这个** ✓：**要载墨量／湿度／光谱混色（一笔蘸多少颜料、湿画法 vs 干笔）⇒ 用 `medium_stroke` 的 `load`／`wetness`／`color_mixing=pigment`，不是这里的 `brush_params`** ✓（**∵ `.myb` 的规范名里没有这两项** ✓）；201 支 .myb 笔刷，可带 color 画彩色、带 color_to 画**一笔多色**（Loaded Brush ✓）。                  与邻居的分工：要**介质插件**（油画 / 水彩 / 马克笔 / 铅笔 / 像素的我们自己的模拟）用 medium_stroke；                  要**纯几何、无物理**的矢量笔迹用 draw_stroke。                  **★ ⚠️ 切换笔刷或改 `brush_params` 之前，先用 `swatch_brush_strokes` 批量试 3–5 支，选最好的再真画 ★**（实测教训：AI 直接真画试错 ⇒ 画坏要 undo ⇒ 效率低一半 ✗。真画一笔 = 写文档 + 生成 atom + 污染历史；**swatch 不写文档、不生成 atom ⇒ 试 10 支也不心疼** ✓）。                  工作流：`list_brushes` 查 → `swatch_brush_strokes` 批量试 → 选一支 `brush_stroke` 真画 → 看缩略图 → 不满意再 swatch 换 ✓（**不要只试一次就开画** ✗）。                  **★ 压感指南 ★**：`points` 的第三个分量是 **pressure 0..1** ✓，它**按点生效**（不是整笔一个值 ✓）。**每一笔都该起伏**：起笔 **0.2–0.3** → 行笔 **0.8–1.0** → 收笔 **0.2–0.3** ✓；转折/顿笔处可以**短距离内压低再抬起**（形成「顿」✓）。**全程 0.8 会让笔触像矢量图** ✗（粗细一致、边缘生硬 ✓）—— 可直接抄的示例：`points: [[40,100,0.25],[110,96,0.9],[180,104,0.95],[250,100,0.3]]` ✓。**⚠️ `pressure_curve: 「auto」` 只对「压感敏感」的笔刷有效** ✓ —— 先用 `list_brushes` 的 `pressure_sensitive`／`pressure_settings` 确认（实测 `2B_pencil` 墨量 **1023→3470** ✓，而 `100%_Opaque` 几乎看不出差别 ✗）。                  **每条笔触只作用于它自己的图层** ✓：跨图层只是普通叠加；介质的湿搅 / 混色**不跨图层** ✓（先把底下那层画完，或用同一层叠 ✓）。                  **覆盖率／落墨量以 clipped 为准** ✓：`painted_pixels` 只是裁剪前的铺开面积（同值字段 `painted_pixels_raw`），真实落墨看 `painted_pixels_clipped`；顶层 `landed`／`landed_reason` 直接给结论。",
        mutating: true,
        params: &[
            // **预览开关** ✓（第 5 轮 ✓，P0 ✓）：连续作画时 `preview: false` ⇒ 这一笔不编预览 PNG ✓
            // ⇒ 省下实测 ~130 ms/笔（4K、`Clouds.myb`、`size 180` ✓）✓；默认 `true` ⇒ 行为不变 ✓。
            // **★ P0 #4：覆盖 `.myb` 的任意字段 ✗ ★**（**名字用 libmypaint 规范名 ✓ ＝ `.myb` 的 JSON 键 ✓）
            //   ⇒ **∴ AI 不必换笔刷文件就能调** `smudge`／`opaque`／`dabs_per_actual_radius` 这类** ✓ ★★★
            // **★ 但 `dabs_per_basic_radius` 在本引擎里**不会生效 ★**（第 1589 轮实测 + 根因）
            //   **⇒ 为什么**：服务端把路径**每 2 px 切一段**、每段调一次内核
            //     （`divisions = distance / 2.0`）⇒ **有效 dab 间距被 2 px 钉死** ⇒
            //     **笔刷自己的 `dabs_per_basic_radius` 被覆盖** ✓
            //   **⇒ ⇒ 实测**：直线（size 4）剖面**三档完全相同**；急弯弧 painted
            //     **886 / 876 / 890**（±1.6%）⇒ **零差异** ✗
            //   **⇒ ⇒ ⇒ 所以**要在**说明里讲清**（**否则下一个使用者会像外部那样白试三轮**）✓
            //   **⇒ ⇒ ⇒ ⇒ 要更密 ⇒ 加笔数、加 `size`、用 `smudge`**（**`smudge` 实测有效**）✓
            param!(
                "brush_params",
                Object,
                false,
                "覆盖 .myb 字段：{规范名: 数值}，如 {\"smudge\":1.0,\"opaque\":0.2}；**注意 `dabs_per_basic_radius` 在本引擎里不生效**（路径每 2px 采样一次，有效 dab 间距被钉死；实测三档零差异）⇒ 要更密请加笔数／加 size／用 smudge；字段名就是 .myb 的 JSON 键**也可直接用四组语义别名**：hand_jitter／speed_gain／color_variance／smudge_amount ✓（一个词管一组 .myb 项 ✓）；，拼错会报错并给候选。**常见字段与合理范围**（多为 0..1，除注明外）：`opaque`／`smudge`／`smudge_length`（0..5）／`dabs_per_actual_radius`（1..20，越大越密；>20 明显变慢）／**`dabs_per_second`（0..500；>0 才会按时间出 dab ⇒ 它决定 `time_ms` 有没有用）**／`speed1_slowness`／`speed1_gamma`（速度映射的形状）／`hardness`／`elliptical_dab_angle`（度）。**`radius` 是对数**（想改笔尖大小请用 `size`）✓"
            ),
            param!(
                "hand_tremor",
                Number,
                false,
                "手抖 0..1（缺省 0 = 不启用）：线条别那么直。≡ brush_params 的 hand_jitter"
            ),
            param!(
                "speed_ink",
                Number,
                false,
                "笔速感应 0..1（缺省 0 = 不启用）：慢笔多墨、快笔飞白。增量语义（在笔刷原生基础上叠加，不覆盖）。需 points 带 time_ms 才有速度信号，无时间数据时 warning 提醒"
            ),
            param!(
                "color_breath",
                Number,
                false,
                "色彩呼吸 0..1（缺省 0 = 不启用）：一笔内颜色微妙变化。≡ brush_params 的 color_variance"
            ),
            param!(
                "paint_push",
                Number,
                false,
                "推颜料 0..1（缺省 0 = 不启用）：笔过之处拖带底色混色（湿画法）。空画布上无影响。实测校准：日常 0.06–0.12，清晰自然 0.18–0.28，艺术夸张 0.4–0.6，0.75+ 为刮刀"
            ),
            param!(
                "pressure_shape",
                Number,
                false,
                "压力曲线 -1..1（缺省 0 = 不启用）：一笔内压力自动变化，告别死笔触。正值：中间粗两头细（通用）；负值：起笔粗收笔细（书法顿笔出锋）"
            ),
            param!(
                "stroke_taper",
                Number,
                false,
                "起收锋 0..1（缺省 0 = 不启用）：笔画两头几何收细，中段不受影响。模拟锋利笔尖，两头细但墨色扎实"
            ),
            param!(
                "dry_brush",
                Number,
                false,
                "枯笔 0..1（缺省 0 = 不启用）：飞白质感，dab 间距拉大。中国画皴法、速写排线用"
            ),
            param!(
                "edge_soft",
                Number,
                false,
                "边缘柔化 0..1（缺省 0 = 不启用）：笔触边缘羽化，水彩边缘虚实控制用"
            ),
            param!(
                "tilt_shade",
                Number,
                false,
                "侧锋排线 0..1（缺省 0 = 不启用）：扁笔侧锋，排线铺调子。素描、水彩铺色用"
            ),
            param!(
                "scatter_texture",
                Number,
                false,
                "飞白肌理 0..1（缺省 0 = 不启用）：dab 随机散布，画斑驳质感。墙面、岩石、树皮用"
            ),
            param!(
                "edge",
                String,
                false,
                "边缘意图（缺省 hard）：hard=硬边，soft=羽化边缘，lost=消失边（融入背景）"
            ),
            param!("pressure_shape", Number, false, "压力曲线 -1..1（缺省 0 = 不启用）：一笔内压力自动变化。正值中间粗两头细（通用），负值起笔粗收笔细（书法顿笔出锋）"),
            param!("path_guide", Object, false, "曲线尺吸附（二期）：{object_id|subpaths, snap_radius}——把落点往一条 path 曲线拉（权重 (1−d/R)²，d 为落点到规范折线的距离）；只改落点坐标，压感/时间不动。与 direction_guides 并存不冲突：那是方向场（改生成笔触的走向），本参数改的是已有落点的位置，两者独立生效。缺省不传 ⇒ 点列逐位不变"),
            param!("stroke_taper", Number, false, "起收锋 0..1（缺省 0 = 不启用）：笔画两头几何收细，中段不受影响。模拟锋利笔尖"),
            param!("preview", Boolean, false, "缺省 true = 产出这一笔的预览图；false = 不产出（连续作画提速；预览仍可由 export_png / get_document 取）"),
            param!(
                "wait_for_render",
                Boolean,
                false,
                "缺省 true = 等到这一笔的渲染落盘再返回（状态与图都是最新的）；**false = 不等** ⇒ 立刻返回、渲染留到后面做。**何时该设 false**：连续作画（每笔都不看图）时等渲染是**纯浪费** —— 实测它占单笔耗时的 **66%**（500 对象时 preview_ms 11.0 / total 16.8 ms）。**代价**：紧接着读像素（get_pixels／analyze_region）或导图时可能读到**上一笔**的状态。"
            ),
            param!("sensors", Object, false, "真实传感器输入（缺省不传 = 全 0 = 历史行为）：{xtilt: -1..1, ytilt: -1..1, twist_deg: 0..360, viewzoom: log(缩放倍率)}，各字段可省略。喂给引擎的 tilt 全家 / barrel_rotation / viewzoom 输入——笔刷在这些输入上挂了曲线时才有效果（用 get_brush_dynamics 查）；越界报错不夹紧"),
            param!("layer_id", String, true, "目标图层"),
            // **★ 对称模式 ✗ ★**（**第 251 轮 ✓）：**画一笔同时在**镜像位置**再画一遍** ✓
            //   ＋ **∴ 借鉴 ✗**：**Krita／Procreate 的对称是**工具态**✗ 不写进笔迹数据**
            //     ⇒ **∴ 撤销／清画布**都不影响它 ✓ ★
            //   ＋ **∴ 取值 ✗**：`none`（缺省）／`vertical`（**轴=画布竖直中线 ✓）／
            //     `horizontal`（**轴=水平中线 ✓）✗ 别的值**报错 ✗ 不静默** ✓ ★★
            param!("symmetry", String, false, "对称模式：none（缺省）/ vertical / horizontal ⇒ 一笔画两份（镜像那份与原笔压力相同）"),
            // **P1 #12 视觉锚点**（第 254 轮）：回执里除了缩略图，再给**结构化锚点**（精确坐标），
            // AI 据此接下一笔，不必从像素里估。
            param!("anchor_mode", String, false, "视觉锚点：**`data`（缺省，也是当前唯一可用的值）** ⇒ 一张干净缩略图 ＋ `anchors` 结构化数据（笔画起止/包围盒/栅格/画布尺寸 ✓）。**本参数只提供 `data`** ✓；`dual`（两张图：干净 ＋ 带 Overlay）**本版未实现**，传了会**明确报错**（不静默降级 ✓），已记在需求文档 P1 #12。**`data` 已足够**：AI 按 `canvas` 尺寸等比换算即可在缩略图上定位 ✓"),
            param!("grid_spacing", Number, false, "anchors.grid.spacing（画布坐标的标尺间距，缺省 200）"),
            // **P1 #11 humanize**（第 254 轮）：默认拟人（非对称起收笔包络）；只在调用方**没给压力**时生效。
            param!("humanize", Boolean, false, "拟人模式（缺省 true）：点里**没给压力**时套上非对称起收笔包络（起 12%／行 60%／收 28%）；显式给了压力则一位像素都不改；传 false 走精确坐标"),
            param!("pressure_curve", String, false, "**只影响「给了恒定压感」这一种情况** ✓：`flat`（**缺省** ⇒ 不动你给的 pressure ✓）/ `auto`（**推荐** ✓）＝当你给的是**整笔恒定**的 pressure（如全程 0.8）时**自动补上起轻行重收轻的包络** ⇒ 笔触不再像矢量图 ✓（程序生成的坐标最容易犯 ✗）；给了**有起伏**的 pressure 时 `auto` 也**原样尊重** ✓。**没给 pressure 时**由 `humanize` 决定（开着就加包络 ✓，与本参数无关 ✓）；**`humanize:false` 时一律不动** ✓。**⚠️ 只对「压感敏感」的笔刷有效** ✓ —— 用 `list_brushes` 看每支的 `pressure_sensitive`／`pressure_settings` ✓（实测 `2B_pencil` 墨量 1023→3470 ✓，而 `100%_Opaque` 几乎看不出差别 ✗：它的压感只作用在 `opaque_multiply` 的阶跃上 ✓）"),
            param!("dwell", Number, false, "曲率驻留 0..1（**缺省不给 ⇒ 点列一个都不动**）：转弯处按 κ^(2/3) 多插点 ⇒ 拐角颜料更足（P1 #8 的一半；刻意 opt-in，免得动到既有像素判据）"),
            // **P1 #8 生理颤动**：opt-in（缺省不给 => 点列一个都不动）。
            param!("tremor", Number, false, "生理颤动 0..1（**缺省不给 => 点列一个都不动**）：叠一层**带限** 1D value noise（手腕 λ≈90px ＋ 震颤 λ≈28px，沿局部法向）—— 与仓库既有的白噪声抖动不同，它相邻点相关 => 像手而不是像砂纸"),
            param!("tremor_seed", Integer, false, "颤动噪声种子（缺省 0；同种子同波形 => 确定性）"),
            // **P1 #10 罩染**：opt-in（缺省不给 => 点列与透明度都不动）。
            param!("glaze", Boolean, false, "罩染（缺省 false）：true => 单笔自动拆成多遍**微偏移薄涂**（古典油画的透明罩染；整笔透明度按遍数摊薄），适合皮肤/暗部的丝滑渐变"),
            param!("glaze_passes", Integer, false, "罩染遍数 1..8（缺省 3；只在 `glaze: true` 时有意义）。**实测范围（第 254 轮）**：变暗随遍数增长、**8 遍即饱和**（8 与 12 逐像素**完全相同** ⇒ 再多没有意义）；常用 **2–3**（皮肤/暗部统一）／**5–8**（厚重罩染）✓"),
            param!("brush", String, true, "笔刷名（assets/brushes 或工作区缓存里的 .myb；可省 .myb）"),
            param!(
                "points",
                Array,
                true,
                "[[x,y,pressure],…] 或 [[x,y,pressure,time_ms],…]：压力 0..1（可省，缺省 0.5）；\
                 第 4 项是可选的**毫秒时间戳** ⇒ 引擎用它算**真实速度**（吃速度的笔刷设置如 \
                 speed1_*／speed2_*／dabs_per_second 才有意义；不给 ⇒ 与以前逐字节相同）"
            ),
            param!("size", Number, false, "覆盖笔刷自带半径（直径像素；不给就用 .myb 里的设置）"),
            // **AI 实测报的 P0 缺口** ✓：本工具**以前不能设颜色** ✗ ⇒
            // "有 myPaint 物理但不能画彩色" ✓，而能设色的 `medium_stroke` / `draw_stroke` **没有 myPaint 物理** ✗
            // ⇒ **没有任何一个工具同时具备两者** ✓。
            // **做法照 MyPaint** ✓（不自己发明 ✓）：颜色就是 `.myb` 的 `color_h/s/v` ✓ ⇒ 给了就**覆盖** ✓
            // —— 与"在 MyPaint 里选了笔刷之后照常选颜色"完全一致 ✓。
            // **写法与其它绘制工具统一** ✓（本轮 ✓）：`parse_spec_color` ✓ 收 `{r,g,b,a}` /
            // `[r,g,b,(a)]`（0..1 线性或 0..255 字节）/ `"#RRGGBB"` ✓ ——
            // Web 的颜色选择器给的就是 `"#RRGGBB"` ✓（此前查看器只传 `undefined` ✗ ⇒ 选了色也画不上 ✗）。
            param!("color", Any, false, "笔尖颜色：{r,g,b,a}（0..255）/ [r,g,b,(a)]（0..1 线性或 0..255 字节） ⇒ **任一分量 > 1 即按 0..255 字节解释**（想要近黑请用 `#RRGGBB` 或 `{r:1,…}`） / \"#RRGGBB\" ⇒ **覆盖 .myb 默认色** ✓；不给则用笔刷自带色。**`a` 目前被忽略**（笔触透明度用 `opacity` ✓；实测 `a=0` 与 `a=255` 落墨量相同 ✓，判据 `ink_pixels` 那条钉着） ✓；显式空间见 `color_space`（不传 ⇒ 量级切换）"),
            param!("color_mixing", String, false, "`color` → `color_to` 的**混色空间**：`rgb`（缺省，sRGB 空间插值 ⇒ 与以前逐字节相同）/ `linear`（线性光混色，物理正确：红+蓝各 50% 比 rgb **亮 35 级**）/ `pigment`（光谱 K/S 混合：黄+蓝混出绿）。**不给 `color_to` 时无意义**"),
            param!("color_to", Any, false, "**末端颜色**（写法同 color）⇒ 一笔之内从 color 渐变到它（Loaded Brush ✓）：花瓣 / 叶尖那种渐变**一笔就能画** ✓，不必分两笔（交界不会有硬边 ✓）；给了它就必须同时给 color ✓（解释空间同 `color_space`）"),
            param!("color_space", String, false, "颜色解释空间（C1）：`srgb` ⇒ 0..1 浮点数组按 sRGB 字节解释（`v*255` 取整再解码）；`linear` ⇒ 浮点数组按线性直通（即使分量 > 1 也不按字节解释）；不传 ⇒ 现行量级切换（任一分量 > 1 即字节，否则线性）。作用于 `color` 与 `color_to`；对象式 / hex 恒为字节，不受本参数影响。回执 `applied.color_space` 回显实际生效的单位（`linear` / `srgb_bytes` / `hex`）"),
            // **与 `draw_stroke` 对齐** ✓（外部绘画 agent 实测痛点 ✓："要 MyPaint 物理"和"要能控透明度"
            // 以前**无法同时满足** ✗ ⇒ 它只能放弃画笔/介质引擎、改用 `draw_stroke` 画云 ✓）。
            param!("opacity", Number, false, "整笔不透明度 0..1（映射到 MyPaint 的 `opaque` = **每枚 dab 的不透明度** ✓）；缺省用 `.myb` 自带的 ✓"),
            param!("hardness", Number, false, "笔尖硬度 0..1（映射到 MyPaint 的 `hardness` ✓：0=软边、1=硬边）；缺省用 `.myb` 自带的 ✓"),
            param!("smooth", Boolean, false, "true ⇒ 把 points 当 **Catmull-Rom 平滑样条的控制点**（曲线过这些点，不把它们拉走）⇒ 手写的折线不再有硬角；缺省 false ⇒ 与前完全一致"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("style", String, false, "confident = 起笔重收笔轻；sketchy = 确定性抖动与断笔；缺省不改（一个字节都不变）"),
            param!("clip_to_selection", String, false, "只把墨落在该选区里（选区 id，或 latest；\"combined\" 则按创建顺序组合全部选区，add/subtract/intersect 生效）；缺省 = 不裁剪。注意：传的 id 必须与建选区时用的名字完全一致（不要自己加 sel- 之类前缀；不确定就用 latest 取最新那个）"),
            param!("label", String, false, "笔迹标签（部位名，如「鼻子-轮廓」）⇒ 记进原子并可由 get_atom 读回；缺省不记"),
            param!("group_id", String, false, "组标识：一组笔共享一个 id ⇒ 用 revert_group 整组撤销（不给则不成组）"),
        ],
    },
    ToolSpec {
        name: "smart_fill",
        profile: Profile::Core,
        summary: "智能填充：一笔一笔画出带人味的排线填充，替代 fill_region 的塑料感。输入形状 + 主方向，自动生成平行排线（带方向抖动），每条线用 hand_tremor + pressure_shape 保证自然。三种模式：hatch（单向排线）、cross_hatch（交叉）、contour（沿轮廓收缩）。 **⇒ `cross_hatch` 的排线是**真的画出来了**（笔触分两组角度，实测 0° 与 140–150° 两个峰）；看不见排线通常不是几何问题，而是**笔刷的单笔 alpha 太低** ⇒ 多笔叠加后只剩平均色 ⇒ 要看见排线请用不透明笔（如 `100%_Opaque.myb`）或减少叠笔。",
        mutating: true,
        params: &[
            param!("shape", Object, true, "形状：{\"type\":\"rect\",\"x\":100,\"y\":100,\"w\":200,\"h\":150} 或 {\"type\":\"circle\",\"cx\":200,\"cy\":200,\"r\":80} 或 {\"type\":\"polygon\",\"points\":[[x,y],...]} 或 {\"type\":\"path\",\"d\":\"M…Z\"}（path 也可给 subpaths 数组；缺 type 但带 d/subpaths 时按 path 解析）；可选 \"holes\"（[[[x,y],...]] 或子形状数组）挖孔"),
            param!("fill_direction", Number, false, "主笔刷方向（度，0=水平，90=垂直），缺省 -30（人类自然斜角）；传了 direction_guides/direction_from 时被局部方向场替代"),
            param!("direction_guides", Array, false, "方向场引导点 [[x,y,angle_deg],...]，排线顺应局部走向（单位向量 IDW 插值）"),
            param!("direction_from", String, false, "\"shape_contour\"（顺边缘）/\"shape_normal\"（垂直边缘放射）/\"mask_gradient\"（从形状梯度推方向）；**三者都依赖选区（`clip_to_selection`）⇒ 无选区时失败**"),
            param!("clip_to_shape", Boolean, false, "true=过冲+形状裁剪（默认，无漏白）；false=素描模式，线不出界"),
            param!("direction_jitter", Number, false, "方向抖动 ±度数，缺省 8"),
            param!("color", Array, true, "颜色 [r,g,b,a]，0..1"),
            param!("brush", String, false, "笔刷名，缺省 calligraphy"),
            param!("size", Number, false, "笔刷大小，缺省 15"),
            param!("mode", String, false, "hatch（单向排线，缺省）/ cross_hatch（交叉）/ contour（沿轮廓收缩，仅 rect/circle）"),
            param!("layer_id", String, true, "图层 id（必填）"),
            CLIP_TO_SELECTION_PARAM,
        ],
    },
    ToolSpec {
        name: "smart_blend",
        profile: Profile::Core,
        summary: "智能混色：两色之间自然过渡（Two-Pass Wet Blend）。Pass 1 斜切排线铺色（blend_direction+72°），OKLab 感知均匀插值 + 黄金分割抖动打破带；Pass 2 湿笔揉边（blend_direction-25°，大笔刷高 paint_push 轻扫）。解决 sRGB 插值发灰、排线条带、上游颜色绑架三大问题。",
        mutating: true,
        params: &[
            param!("shape", Object, true, "形状：同 smart_fill（rect/circle/ellipse/polygon/path；可选 \"holes\" 挖孔）"),
            param!("color_a", Array, true, "起始颜色 [r,g,b,a]，0..1（渐变起点，blend_direction 的 0 端）"),
            param!("color_b", Array, true, "结束颜色 [r,g,b,a]，0..1（渐变终点，blend_direction 的 1 端）"),
            param!("blend_direction", Number, false, "渐变方向（度，0=从左到右，缺省 0；只传 direction_guides 未传本参数时取引导向量平均角）。注意排线会自动取局部方向场+72° 斜切"),
            param!("direction_guides", Array, false, "方向场引导点 [[x,y,angle_deg],...]，排线顺应局部走向"),
            param!("direction_from", String, false, "\"shape_contour\"/\"shape_normal\"/\"mask_gradient\"（见 smart_fill）；**三者都依赖选区（`clip_to_selection`）⇒ 无选区时会失败**"),
            param!("brush", String, false, "笔刷名，缺省 calligraphy"),
            param!("size", Number, false, "笔刷大小，缺省 15"),
            param!("layer_id", String, true, "图层 id（必填）"),
            CLIP_TO_SELECTION_PARAM,
        ],
    },
    ToolSpec {
        name: "smart_glaze",
        profile: Profile::Core,
        summary: "智能罩染：在已画好的底稿上罩一层透明色（夕阳暖光、投影冷光、水彩花瓣）。干透叠色 paint_push=0.0 绝不搅动底色；用 target_opacity 反推 dab_alpha（防 Per-Dab 累积涂死）；Beer-Lambert 色彩逐层浓郁演进；黄金角摆动防摩尔纹；逐层内缩形成自然水彩边缘。",
        mutating: true,
        params: &[
            param!("shape", Object, true, "形状：同 smart_fill（rect/circle/ellipse/polygon/path）"),
            param!("color", Array, true, "罩染颜色 [r,g,b,a]，0..1"),
            param!("layers", Number, false, "罩染层数，缺省 3（1-8）"),
            param!("target_opacity", Number, false, "整次罩染完成后的目标视觉遮盖率 0.05-0.95，缺省 0.55。传这个最直观，不用自己算 Dab 累积"),
            param!("layer_opacity", Number, false, "（旧参数）期望的单层不透明度，缺省 0.28；传了 target_opacity 则忽略此项"),
            param!("brush", String, false, "笔刷名，缺省 watercolor_glazing（柔边；不要用 calligraphy 硬边笔）"),
            param!("size", Number, false, "笔刷大小，缺省 15"),
            param!("fill_direction", Number, false, "主排线方向（度），缺省 -30"),
            param!("direction_jitter", Number, false, "方向抖动 ±度数，缺省 8"),
            param!("direction_rotate", Number, false, "层间角度摆动幅度（度），缺省 12；不要用等差旋转，会产生摩尔纹"),
            param!("layer_id", String, true, "图层 id（必填）"),
            CLIP_TO_SELECTION_PARAM,
        ],
    },
    ToolSpec {
        name: "smart_edge_soften",
        profile: Profile::Core,
        summary: "智能柔边：沿边缘路径法线做 paint_push 短程拖抹，消除生硬边缘。Pass 1 沿法线接力拖曳（pressure_shape=+0.35 起笔轻收笔尖，大宽度自动分级接力）；Pass 2 沿切线轻扫梳理（消除梳子纹）。支持 inward/outward/both，闭合路径按质心自动定向。",
        mutating: true,
        params: &[
            param!("edge_path", Array, false, "边缘路径 [[x,y],...]（至少 2 点）；与 shape 二选一"),
            param!("path_guide", Object, false, "曲线尺吸附（二期）：{object_id|subpaths, snap_radius}——先把 edge_path 输入点列往一条 path 曲线拉（权重 (1−d/R)²）再柔化；只作用于 edge_path（shape 派生边界是精确几何、不吸附），只改落点坐标。与 direction_guides 并存不冲突（方向场改笔触走向、本参数改落点位置）"),
            param!("shape", Object, false, "形状（rect/circle/ellipse/polygon/path，同 smart_fill），自动取边界为边缘路径；与 edge_path 二选一"),
            param!("soften_width", Number, false, "柔化宽度 px，缺省 20（2-100）；大于 10px 自动分级接力"),
            param!("soften_strength", Number, false, "柔化强度 0.1-1，缺省 0.5（映射为 paint_push）"),
            param!("max_steps", Integer, false, "边缘采样点上限，缺省 400 ⇒ 超出时**等距抽稀**（防大形状生成上千个原子）"),
            param!("soften_direction", String, false, "inward（向内）/ outward（向外）/ both（双向互溶，缺省）"),
            param!("color", Array, false, "柔边颜色 [r,g,b,a] 0..1；建议传边缘主体色；不传则用笔刷自带色"),
            param!("brush", String, false, "笔刷名，缺省 calligraphy"),
            param!("size", Number, false, "笔刷大小，缺省 12"),
            param!("layer_id", String, true, "图层 id（必填）"),
            CLIP_TO_SELECTION_PARAM,
        ],
    },
    ToolSpec {
        name: "smart_texture",
        profile: Profile::Core,
        summary: "智能肌理：按材质类型在区域内自动生成肌理笔触（canvas 经纬线/fabric 斜纹织线/skin 漫游皮纹/stone 碎裂短线+斑块/wood 长流线+木节）。分层抖动网格+簇状调制防结块与筛子感；20% 骨相层+80% 肌理层双尺度；笔数按目标覆盖率反推（上限 240）；颜色从底色自动推导（80% 暗部+20% 微高光，Alpha 永不 1.0）。**在空（全透明）层上调用不会产生可见像素**（`Alpha ≤ base_alpha` 且底色由图层平均色推出 ⇒ 空层没有底色）⇒ 请先铺底再上肌理。",
        mutating: true,
        params: &[
            param!("shape", Object, true, "形状：同 smart_fill（rect/circle/ellipse/polygon/path；可选 \"holes\" 挖孔）"),
            param!("texture_type", String, true, "canvas（粗麻经纬线）/fabric（斜纹织线）/skin（漫游皮纹）/stone（碎裂短线+斑块）/wood（长流线+木节）/fold（衣褶：亮暗成对偶极子，配合 direction_guides 用）"),
            param!("direction_guides", Array, false, "方向场引导点 [[x,y,angle_deg],...]，肌理走向顺应局部场"),
            param!("direction_from", String, false, "\"shape_contour\"/\"shape_normal\"/\"mask_gradient\"（见 smart_fill）；**三者都依赖选区（`clip_to_selection`）⇒ 无选区时会失败**"),
            param!("density", Number, false, "肌理密度 0-1，缺省 0.5"),
            param!("neutral", Boolean, false, "为 true 时把肌理基色减半 ⇒ 上完肌理**画面均值基本不变**（第 1382 轮；缺省 false ⇒ 行为不变）"),
            param!("size", Number, false, "肌理笔触大小，缺省 6"),
            param!("color", Array, false, "肌理基准色 [r,g,b] 0..1；缺省或 \"auto\" 时自动采样图层平均色"),
            param!("brush", String, false, "笔刷名，缺省 calligraphy"),
            param!("layer_id", String, true, "图层 id（必填）"),
            // **`wait_for_render`：透传给每条内层笔触** ✓（第 639 轮；外部 D6 ✓）
            //   ⇒ 缺省 `true`（行为不变）；给 `false` ⇒ 内层不再每笔等渲染 ✓
            //   （实测 22.4 ms/笔 ⇒ 6.6 ms/笔，**3.4×**）
            param!("wait_for_render", Boolean, false, "内层每条笔触是否等渲染完成（缺省 true；给 false 可明显加速，代价是回执可能早于画面就绪）"),
            CLIP_TO_SELECTION_PARAM,
        ],
    },
    ToolSpec {
        name: "smart_select",
        profile: Profile::Core,
        summary: "智能选取（魔棒）：点种子像素，按 4D 预乘 OKLabA 感知距离做 Scanline 连通泛洪（contiguous=false 时为全图色彩范围选取），1px 抗锯齿软边烘焙进位图，微孔自动闭合，可选磁力边缘细化。选区以 L8 位图 blob 存储，add/subtract/intersect/invert 照常用。返回诊断指标（面积/覆盖率/漏出告警）与可选 Quick-Mask 预览。",
        mutating: true,
        params: &[
            param!("selection_id", String, false, "选区 id；不传自动生成 sel_wand_N"),
            param!("seed", Array, false, "种子点 [x, y]（与 seeds 二选一）"),
            param!("seeds", Array, false, "多个种子点 [[x1, y1], ...]，取并集"),
            param!("tolerance", Number, false, "容差 0-1，缺省 0.2；二次感知标定，0.1 吃平涂+AA 边，0.45 吃体积过渡"),
            param!("contiguous", Boolean, false, "true（缺省）连通泛洪；false 全图按色选取"),
            param!("refine_edges", Boolean, false, "磁力边缘细化（缺省 true）：法向 Sobel 吸附 + 联合双边滤波"),
            param!("layer_id", String, false, "只在该图层取色；不传则取合成后像素"),
            param!("mode", String, false, "new（缺省）/add/subtract/intersect"),
            param!("feather", Number, false, "羽化半径像素，缺省 0；>0 时高斯烘焙进位图"),
            param!("preview", Boolean, false, "true 则返回 Quick-Mask 预览缩略图 blob"),
        ],
    },
    ToolSpec {
        name: "fit_background_field",
        profile: Profile::Core,
        summary: "把**掩膜外**的像素拟合成**平滑多项式背景场**，并铺进**掩膜内**（一次落平，不出笔触）。\n\n**⇒ 什么时候用它** ✓：想要**平滑的底**（sky/墙/桌面），而不想要笔触或折痕 —— 多项式是**全局**的 ⇒ **结构上不可能有格点折痕** ✓；\n**⇒ 什么时候别用** ✗：背景有**高频细节**（树叶/织物）⇒ 多项式**不能表示** ⇒ 会糊 ⇒ 那种情况用 `smart_sample`（笔触）或直接 `import_image` ✓。\n**⇒ 与 `smart_sample` 的分工**：这个是**像素**、那个是**笔触** ⇒ **底用这个、上面再用笔触** ✓。\n**⇒ 返回** `residual_rms`（三通道拟合残差）⇒ **调用方据此判断 degree 够不够** ✓（残差大 ⇒ 加 degree 或换工具）\n\n\\n\\n**⇒ ⇒ ⇒ 怎么用 `residual_rms` 判 `degree` 够不够**（第 1465 轮；外部三张真实画的标定）✓：\
它是**每通道均方根误差，单位就是灰度级 0-255** ⇒ **< 15 可用**、**> 25 说明背景不是多项式形状**（改用色带或笔触）⇒ 外部实测 `8.5`/`11`/`12` 三张都在 2~3 阶内解决 ✓ **⇒ 该阈值是经验值，不是定理** ✓\n\n**⇒ ⇒ ⇒ 返回里的**诊断字段**（第 1476 轮补齐说明）** ✓：`residual_rms`（绝对残差）｜`sample_std`（样本波动）｜`residual_ratio`（两者之比，**无量纲**）｜`clamped_pixels`（**过冲**被夹紧的通道数）｜`field_range`（填进去的实际灰度范围）｜`samples`（用了多少个采样点）｜`filled_pixels`（填了多少个像素）｜`mask_bbox`（掩膜的解析边界，**掩膜画歪时看它**）｜`placed`（落位那一步的完整响应）｜`field_uniform`（**是否单色**）｜`degree`（作为**阶数**报出）＋ `effective_sample_stride`/`effective_margin`（**生效值**，名字已说明）⇒ **每个都对应一类「看起来不对」的原因** ✓\n\n**⇒ ⇒ ⚠ 前提：掩膜外**必须有明暗变化**（第 1454 轮）** ✗：\\n若掩膜外的像素**本来就是单色**（空图层、或外面被纯色盖住）⇒ 拟合出来**必然是那个单色** ⇒ 看起来像**「什么都没画」** ✗（常见于白底文档）⇒ 这时返回里 `field_uniform` 为 `true` 且 `warnings` 会**明说** ✓\\n⇒ **所以**：**先用 `fill_region` 铺出有明暗的底，再用它** ✓；**纯色底请直接 `fill_region`**，不要用这个工具 ✓",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层 id"),
            param!("mask", Object, true, "掩膜形状（写法同 smart_fill 的 shape：rect/circle/ellipse/polygon/path，path 用 d 字符串或 subpaths 数组，支持 holes）⇒ **掩膜内**会被拟合场填掉，**掩膜外**的像素是拟合样本"),
            param!("degree", Number, false, "多项式阶数 **0..3**（缺省 2）。**0 = 常数项** ⇒ 等于「用掩膜外的均值填满」（不产生任何渐变）；越高越能贴合弯曲的背景，但也越容易过冲"),
            param!("sample_stride", Number, false, "采样步长 1..16（缺省 2）：每隔 N 个像素取一个样本 ⇒ 越大越快、越不敏感于噪声"),
            param!("margin", Number, false, "掩膜 bbox 外扩比例（缺省 0.25）⇒ 决定拿多远处的像素当样本；外扩不够时样本数可能不足"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "smart_sample",
        profile: Profile::Core,
        summary: "从参考图/参考图层采样区域内的明暗与色彩分布，自动生成顺应等明度线的多尺度笔触。两阶段：大笔软边铺底 + 梯度加权肌理笔触（落笔概率 ∝ 明暗梯度，方向取等明度切向）。",
        mutating: true,
        params: &[
            param!("ref", String, true, "参考源：文档内参考图层 layer_id，或 blob_hash（sha256:…），或 PNG 文件路径"),
            param!("shape", Object, true, "目标绘制形状（rect/circle/ellipse/polygon/path，支持 holes）"),
            param!("ref_bbox", Array, false, "可选 [x,y,w,h]：参考图采样框；缺省与 shape 的 bbox 相同（同尺寸对齐）"),
            param!("samples", Number, false, "明暗采样网格阶数 S（S×S 双线性色彩场），范围 4..24，缺省 8"),
            param!("texture_type", String, false, "笔触肌理风格：smooth/fabric/fold/canvas/skin/stone/wood，缺省 fabric"),
            param!("density", Number, false, "铺笔密度 0..1，缺省 0.6"),
            param!("size", Number, false, "基准笔刷直径，缺省按区域自适应"),
            // **`brush` 之前漏声明** ✓（第 720 轮；外部审计 S-02 第 ② 组）
            //   ⇒ handler 一直读它（缺省 `calligraphy` ✓）而 `validate_args` 只放行声明过的键 ✗
            //   ⇒ **传 `brush` 就 `invalid_argument`** ⇒ 用户**没法换笔刷** ✓（能力不可达 ✓）。
            param!("brush", String, false, "用哪支笔刷画这些笔触（缺省 calligraphy ✓）"),
            param!("layer_id", String, true, "目标绘制图层 id（必填）"),
            CLIP_TO_SELECTION_PARAM,
        ],
    },
    ToolSpec {
        name: "smart_erase_shape",
        profile: Profile::Core,
        summary: "按几何形状（支持带孔多边形与反相）精确擦除图层内容：挖 V 领、裁掉人物轮廓外的溢色。单次提交 = 1 个 Undo 原子，不污染选区栈。",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层 id（必填）"),
            param!("shape", Object, true, "擦除形状（rect/circle/ellipse/polygon/path，支持 holes）"),
            param!("invert", Boolean, false, "反相擦除（true = 保留 shape 内部，擦除 shape 外部溢色），缺省 false"),
            param!("feather", Number, false, "边缘羽化半径 0..256，缺省 1.0（1px 抗锯齿软边）"),
            param!("opacity", Number, false, "擦除强度 0..1，缺省 1.0"),
            param!("size", Number, false, "橡皮直径，缺省 24（扫描线行距 = 0.5×size）"),
        ],
    },
    ToolSpec {
        name: "smart_hatch_follow",
        profile: Profile::Core,
        summary: "沿一条或多条引导曲线生成顺应流向、错落无鳞片感的排线笔触。法向分层 + 黄金分割相位错位消鳞片，AR(1) 相关角度游走锁切向消小叉，曲率自适应长度。",
        mutating: true,
        params: &[
            param!("guides", Array, true, "一条或多条引导折线：[[[x0,y0],[x1,y1],...], ...]"),
            param!("width", Number, false, "每条引导线两侧的排线铺展带宽（像素），缺省 40"),
            param!("shape", Object, false, "可选边界裁剪形状（支持 holes）"),
            param!("density", Number, false, "排线密度 0..1，缺省 0.5"),
            param!("size", Number, false, "笔刷粗细，缺省 5"),
            param!("color", Array, true, "颜色 [r,g,b,a]（必填）"),
            param!("brush", String, false, "笔刷名，缺省 calligraphy"),
            param!("layer_id", String, true, "图层 id（必填）"),
            CLIP_TO_SELECTION_PARAM,
        ],
    },
    ToolSpec {
        name: "transform_selection",
        profile: Profile::Structure,
        summary: "对已有选区做仿射变换：平移 / 轴对齐缩放 / 旋转。纯几何选区在 JSON 几何层原地精确变换（多次变换不糊）；位图选区做逆向双线性重采样。feather 按面积缩放因子调整，invert 保持。",
        mutating: true,
        params: &[
            param!("selection_id", String, true, "选区 id（必填；也可用 \"latest\" 取最新）"),
            param!("translate", Array, false, "平移 [dx, dy]（像素），缺省 [0, 0]"),
            param!("scale", Array, false, "轴对齐缩放 [sx, sy]，缺省 [1, 1]"),
            param!("rotate_deg", Number, false, "旋转角度（度，逆时针为正），缺省 0"),
            param!("pivot", Array, false, "旋转/缩放中心 [px, py]，缺省为选区包围盒中心"),
        ],
    },
    ToolSpec {
        name: "select_opaque_pixels",
        profile: Profile::Structure,
        summary: "按图层不透明像素建选区（Photoshop Ctrl+点击图层缩略图语义）：Alpha 通道直接映射为 8-bit 覆盖率，不做硬二值化，无白边/黑边。",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "图层 id（必填）"),
            param!("selection_id", String, false, "选区 id，缺省自动生成"),
            param!("min_alpha", Number, false, "过滤阈值 0..1：alpha 低于此值的像素视为 0，用于滤飞白脏点，缺省 0"),
            param!("binarize", Boolean, false, "是否硬二值化（alpha>0 → 255），缺省 false（不推荐，会产生 1px 边缘误差）"),
            param!("close_pinholes", Boolean, false, "是否闭合 ≤8px² 内部微孔（复用 smart_select 算法），缺省 true"),
            param!("mode", String, false, "组合模式 new/add/subtract/intersect，缺省 new"),
            param!("feather", Number, false, "羽化半径（像素），缺省 0"),
        ],
    },
    ToolSpec {
        name: "swatch_brush_strokes",
        profile: Profile::Core,
        mutating: false,
        // **需求文档附录五（P0）**：画家术语「swatch」＝试色试笔。与 `brush_preview` 同一条落笔
        // 实现，但**批量**（笔刷 × 颜色 × 路径）且**不写文档、不生成 atom** => 可以放心试。
        summary: "试笔触（swatch）：笔刷 × 颜色 × 路径**批量**试画，一次返回多张对比图；**不写文档、不生成 atom** => 满意了再用 brush_stroke 真画。                  **★ 工作流（必读）★**：**每次落笔前 —— 尤其切换笔刷或改 `brush_params` 时 —— 先 swatch 试 3–5 支，选最好的再真画；不要只试一次就开画** ✗（实测教训：只在开画前试过一次 => 之后全是盲选；真画一笔 = 写文档 + 生成 atom + 画坏要 undo，而 **swatch 试 10 支也不心疼** ✓）。                  批量用法：**一次给 N 支笔刷 × M 组参数**（例：3 支 × opaque 0.5/0.9 = 6 张对比图）=> 一次看完再选 ✓。                  **⚠️ 小图好看 ≠ 大面积好用** ✗（实测：某水彩笔在小预览上轻盈好看，大面积平涂 6 笔几乎看不见 ✓）：铺大色前**多试几笔叠加**，或先在小区域真画一笔看覆盖力 ✓。                  **⚠️ 涂抹类笔刷（如 `oil-mop`）要求画布上先有颜色** => 空画布上试会报「没有可抹的像素」✓：先铺底色再试 ✓。                  顶层 `ok` 即全成（与 `all_ok` 一致 ✓，A6 方案 ①）：有试笔失败 ⇒ `ok:false`，逐项的错误在 `swatches[].error` ✓。",
        params: &[
            param!("brushes", Array, true, "要试的笔刷名（.myb，可省扩展名；也可给单个字符串）"),
            param!("brush_params", Object, false, "覆盖 .myb 字段（与 `brush_stroke` 同一套 ✓）：**批量试「同一支笔刷的不同参数」就用它** ✓（例：`brushes: [\"Classic_Paint\"]` ＋ `brush_params: {\"opaque\": 0.5}`，再换成 0.9 各试一次 ✓） ★**这里没有 `load`／`wetness`／`color_mixing`！** 要载墨量／湿度／光谱混色 ⇒ 用 **`medium_stroke`** 的 `load`／`wetness`／`color_mixing=pigment`（清单见 `list_engine_capabilities.medium_params` ✓）；这些是 `.myb`（MyPaint 动力学）的，见 `docs/design/brush-params.md` ✓。"),
            param!("preview_size", String, false, "预览形态：**single**（缺省 = 单笔触，看「这支笔长什么样」✓）/ **area**（**同一区域来回 5 趟** ⇒ 看**覆盖力 / 叠加 / 透明度累积** ✓ —— 铺大色前用它，避免「小图好看、大面积太透明」✗）。给了 `paths` 时以 `paths` 为准 ✓"),
            param!("colors", Array, false, "要试的颜色（#RRGGBB 或 {r,g,b,a}；缺省用当前/默认色）"),
            param!("paths", Array, false, "要试的路径（每条是 [[x,y,pressure],…]；缺省用内置试笔路径）"),
            param!("size", Number, false, "笔尖直径（像素，缺省 24，上限 512）"),
            // **涂抹类笔刷要先有颜色可抹** ⇒ 试笔这一层也要能铺底/读画布 ✓
            //   （第 1132 轮补：此前给涂抹笔刷的错误消息写着"先铺底色再试"，而这里没有入口 ✗）
            param!("background", Any, false, "预览底色（写法同 color）：**先把预览区铺满它再落笔** ⇒ 涂抹类笔刷（如 `oil-mop`）在试笔时也有东西可抹 ✓；缺省纯白底（与以前逐字节相同 ✓）"),
            param!("read_canvas", Boolean, false, "true ⇒ 预览读**文档已有的可见像素**（同 brush_preview.read_canvas ✓）⇒ 涂抹/混色笔刷能在「已画的画面」上试 ✓；缺省 false"),
            param!("opacity", Number, false, "不透明度覆盖 0–1（同 brush_stroke ✓）"),
            param!("hardness", Number, false, "硬度覆盖 0–1（同 brush_stroke ✓）"),
            param!("color_to", Any, false, "末端色（同 brush_stroke 的 color_to ✓）⇒ 预览里也能试一笔多色"),
            param!("smooth", Boolean, false, "把路径当 Catmull-Rom 平滑（同 brush_stroke.smooth）"),
            param!("include_image", Boolean, false, "true ⇒ 每个 swatch 额外给一个 `image` 块（base64 PNG ✓）⇒ **MCP Agent 能直接看到图** ✓（外部复测 QY-G01；**最多 8 张**，超了给 `image_skipped` ✓）"),
            param!("output_path", String, false, "给了（**目录**，须在导出目录或系统临时目录内，守卫同 export_png）⇒ 每支试笔的 PNG 落盘为 swatch_<序号>.png（仍受 8 张上限），每条给 `image_file` 而非内嵌 base64；与 include_image 同给时文件形态优先"),
        ],
    },
    ToolSpec {
        name: "brush_preview",
        profile: Profile::Core,
        // **不改文档 ⇒ mutating: false** ✓（它只画一张小图并存进 blob CAS ✓）；
        // 但**要读笔刷资产** ✓ ⇒ 需要工作区能解析 `assets/brushes` ✓。
        summary: "**选笔刷之前先看它长什么样** ✓：用同一支 .myb 笔刷真画一小笔，回一张小 PNG（thumb_url）；                 与 brush_stroke 共用同一条落笔实现，所以预览就是真实笔触，不是示意图。                  两支不同的笔刷 ⇒ 两张预览不同 ✓；同一支两次 ⇒ 逐字节相同 ✓。",
        mutating: false,
        params: &[
            param!("brush", String, true, "笔刷名（assets/brushes 或工作区缓存里的 .myb；可省 .myb）"),
            param!("size", Number, false, "笔尖直径像素（缺省 24，上限 512）"),
            param!("color", Any, false, "试色：写法同 brush_stroke 的 color ✓（**任一分量 > 1 即按 0..255 字节解释** ✓；不给则用 .myb 自带色）"),
            param!("points", Array, false, "自定义采样笔迹 [[x,y,pressure],…]；不给则用一条固定的缓 S 形 ✓（同一支笔刷 ⇒ 可复现 ✓）"),
            param!("color_to", Any, false, "末端颜色（**写法同 color** ✓ ⇒ 任一分量 > 1 即按 0..255 字节解释 ✓）⇒ 预览里也能看到**一笔多色** ✓（与落笔同一条实现 ✓）"),
            param!("smooth", Boolean, false, "true ⇒ 与 brush_stroke 的 smooth 同一条平滑 ✓（预览所见 = 落笔所得）"),
            // **这两个实现里一直在读、参数面里却没有** ✗ —— 实测报错原话：
            // "brush_preview 不接受参数 hardness（拼写错误？）；可用参数：brush, si…" ✓
            //（第 82 轮实测 ✓）⇒ 调用方**无从知道**能覆盖它们 ✓，只能猜 ✗。
            // 现在补上 ✓ —— 与 `brush_stroke` 的语义**完全一致** ✓（都是"这一笔用多少不透明度 / 多硬的边"✓）。
            param!("opacity", Number, false, "不透明度覆盖 0–1（同 brush_stroke ✓；不给则用 .myb 自带 ✓）"),
            param!("hardness", Number, false, "硬度覆盖 0–1（同 brush_stroke ✓；不给则用 .myb 自带 ✓）"),
            param!("background", Any, false, "预览的底色（同 `color` 的写法 ✓）：**给了就先把它铺满预览区** ✓ ⇒ 能看「落在深色底上是什么样」✓（不给 ⇒ 纯白底，与以前逐字节相同 ✓）"),
            param!("read_canvas", Boolean, false, "true ⇒ **预览会读文档已有的可见像素** ✓（同 `brush_stroke` 的 `feed_base`）⇒ 能看「落在**已画的画面**上是什么样」✓（**外部复测 G02 的第三项** ✓；缺省 false ⇒ 与以前逐字节相同 ✓）"),
            // ★ **`brush_params` 必须声明 ✗** ★**（外部复测 QY-G02 ✓）
            //
            // **∴ 而 handler **早就**转发它了**✗**（**∴ `:17396` 的 `args.get("brush_params")` ✓，
            //   **∴ 第 254 轮修过的真 bug ✓**）
            //   ⇒ **∴ 但** spec **没声明**✗** ⇒ **∴ schema 校验**直接拒绝**✗**
            //   ⇒ **∴ 于是**"用预览试 `brush_params`"✗ **做不到**✗ ✓
            // **∴ 所以**只是**补个声明**✗ ⇒ **∴ 实现与落笔完全同一套**✓**（**∴ 天然一致 ✓**）✓
            param!("brush_params", Object, false, "覆盖 .myb 字段（与 `brush_stroke` **同一套** ✓）：**批量试「同一支笔刷的不同参数」就用它** ✓（例：`{\"opaque\": 0.2}` ✓；不给 ⇒ 用 .myb 自带 ✓）"),
            param!(
                "include_image",
                Boolean,
                false,
                "true ⇒ 额外内嵌图像 ⇒ **图片以 MCP 的 `image` 内容块返回**（base64 在那里 ✓，不在 text 里 ✗）；\\n\
                 text 块里只有 thumb_url（`yanshi://blob/…`）✓。批处理客户端可加 `--no-inline-images` 关掉 ✓。"
            ),
            param!("output_path", String, false, "给了就把预览 PNG 落盘到该路径（导出目录或系统临时目录内，守卫同 export_png），响应给 `image_file` 元数据而非内嵌 base64；与 include_image 同给时文件形态优先"),
        ],
    },
    ToolSpec {
        name: "list_assets",
        profile: Profile::Core,
        summary: "列出某类资产（**笔刷/纹理/调色板都从这里查 ✓**）：kind 取 brush ⇒ 201 支 .myb，                  texture ⇒ PNG 纹理，palette ⇒ 调色板。没有 list_brushes / list_textures 这类单独的工具 ✗。",
        mutating: false,
        params: &[
            param!("kind", String, true, "brush / texture / palette"),
            param!(
                "tag",
                String,
                false,
                "只保留带该用途标签的资产（仅 brush 有标签）。标签**由名字派生**（启发式，粗）\
                 ⇒ 返回值带 `tags` 与 `tag_source`，最终仍建议用 `brush_preview` 确认。\n\
                 常用标签：fur / feather / ink / pencil / paint / marker / texture / airbrush / pattern / eraser"
            ),
        ],
    },
    ToolSpec {
        name: "import_asset",
        profile: Profile::Core,
        // **不改文档 ⇒ mutating: false** ✓（它只往工作区缓存里写文件 ✓）。
        // 但**仍需落盘工作区** ✓ —— 纯内存模式会**明确拒绝** ✓（而不是假装成功 ✗）。
        summary: "导入资产到工作区缓存：笔刷 .myb / 纹理 .png / 调色板 .json .kpl .gpl .txt",
        mutating: false,
        params: &[
            param!("kind", String, true, "brush / texture / palette"),
            param!("name", String, true, "保存的文件名（扩展名必须与种类相符）"),
            param!("path", String, false, "服务器本地文件路径（MCP 常用）"),
            param!("blob", Object, false, "{blob_hash,size,mime_type}（Web 先把文件上传成 blob 再用）"),
            param!("overwrite", Boolean, false, "已存在时是否替换（缺省 false ⇒ 报冲突，不静默覆盖）"),
            param!("data_base64", String, false, "与 path / blob 三选一（同给报错）：资产字节的 base64 正文（**不带 `data:` 前缀**）；解码与体积上限与 put_blob 同源"),
        ],
    },
    ToolSpec {
        name: "list_textures",
        profile: Profile::Core,
        summary: "列出可用纹理（内置 assets/textures + 工作区缓存 <root>/textures）",
        mutating: false,
        // **参数表就是契约** ✗：此前这里挂着一个 `dir` ✓，而实现**忽略**它 ✗
        // ⇒ 调用方传了也不会变 ✓ ⇒ 正是"**接受了却没用**" ✓ ⇒ 去掉它 ✓
        //（要按种类看资产用 `list_assets` ✓）。
        params: &[],
    },
    ToolSpec {
        name: "export_png",
        profile: Profile::Core,
        summary: "把整幅（或指定区域）渲染成 PNG 落盘：任意尺寸、不经 base64、不受 512px 限制",
        mutating: false,
        params: &[
            param!(
                "path",
                String,
                true,
                "输出文件路径（含 .png）。**只能写进导出目录**（缺省 ./exports，可用 YANSHI_EXPORT_DIR 改）\n                 或系统临时目录 ✓；也可以给相对路径 ✓；**不接受其它绝对路径** ✗（安全沙箱：见 0b862a4 审计 P0#1 \n                 —— 原先可写任意路径，等于一个文件写漏洞 ✗）。要放到别处请自己 `cp` ✓，\n                 推荐设 `YANSHI_EXPORT_DIR` 指向你的交付目录 ✓（那样就不必再 cp ✓）"
            ),
            param!("region", Any, false, "只导出该区域 {x,y,w,h}；缺省整幅"),
            param!("width", Integer, false, "输出宽（与 height 一起给；不给就按原尺寸）"),
            param!("height", Integer, false, "输出高"),
            param!("max_edge", Integer, false, "限制最长边（按比例缩放；与 width/height 二选一）"),
            param!("filter", String, false, "nearest | bilinear（缺省 bilinear）"),
            // **★ 第 1199 轮**：外部 §164 报「连续两次导出 43194 像素不同」⇒
            //   而 render_region 的 CAS 哈希相同 ⇒ 非确定源在导出后半段（缩放/编码）
            //   ⇒ 本参数**禁止缩放 + 强制 Nearest** ⇒ 让「改一处测指标」可复现
            param!("deterministic", Boolean, false, "true ⇒ 禁止缩放并强制 nearest，保证同一文档两次导出逐像素相同（外部 §164）"),
            param!("layer_id", String, false, "只导出这一层（**忽略它的可见性** ✓；缺省导出整幅合成）"),
        ],
    },
    ToolSpec {
        name: "export_svg",
        profile: Profile::Core,
        summary: "把文档导出成 SVG 落盘：能转矢量的笔触转成可编辑路径（路线乙），转不动的（纹理/软边/调整等）按紧凑包围盒嵌 PNG 保 z 序；返回值逐项计数每笔去向",
        mutating: false,
        params: &[
            param!(
                "path",
                String,
                true,
                "输出文件路径（含 .svg）。**只能写进导出目录**（缺省 ./exports，可用 YANSHI_EXPORT_DIR 改）\\n                 或系统临时目录 ✓；也可以给相对路径 ✓（与 export_png 同一沙箱规矩）"
            ),
            param!("validate", Boolean, false, "true ⇒ 逐笔跑边缘容忍度量验收（E_interior ≤ 0.12 且 E_geom ≤ 0.15），不过线的笔自动回退位图并计入 validate_failed；缺省 false（快）"),
            param!("flatten_opacity", Boolean, false, "true ⇒ 把半透明路径在**线性空间**压平到背景后 emit 不透明色 ⇒ 与 PNG 逐像素一致（代价：该路径不再保留可编辑的 α）；要求背景不透明，否则不生效（第 1409 轮）"),
        ],
    },
    ToolSpec {
        name: "export_book_pngs",
        profile: Profile::Core,
        summary: "把多个文档**按给定顺序**各导出一页 PNG（书册／连环画的一步出口；PDF 由它拼）",
        mutating: false,
        params: &[
            param!(
                "doc_ids",
                Any,
                true,
                "页序：文档 id 的数组（**顺序就是页序** ✓）⇒ 按它逐页导出"
            ),
            param!(
                "dir",
                String,
                true,
                "输出目录（**只能写进导出目录或系统临时目录** ✓）；文件名是 p00.png／p01.png…"
            ),
        ],
    },
    ToolSpec {
        name: "export_timelapse",
        profile: Profile::Core,
        summary: "把绘画过程导出成 MP4 视频：服务端单趟增量重放 atom 日志抓帧 ⇒ ffmpeg（libx264）编码落盘；以 job 登记进度，可用 get_job 看 progress",
        // **不改文档 ⇒ mutating: false** ✓（与 export_png 同口径：它写文件，但不动文档 ✓）。
        mutating: false,
        params: &[
            param!(
                "path",
                String,
                true,
                "输出文件路径（含 .mp4）。**只能写进导出目录**（缺省 ./exports，可用 YANSHI_EXPORT_DIR 改）\n                 或系统临时目录 ✓（与 export_png 同一条沙箱规矩 ✓）"
            ),
            param!("doc_id", String, false, "导出哪个文档（缺省当前文档）"),
            param!("fps", Integer, false, "帧率（缺省 30，钳到 1~60）"),
            param!("frame_every", Integer, false, "每 N 个 atom 抓一帧（缺省 10，与 Web 端过程视频同一观感）；给了它就不看 duration_sec"),
            param!("duration_sec", Number, false, "目标总时长（秒）：按 fps 反推步长，让成片接近这个时长；与 frame_every 二选一"),
            param!("width", Integer, false, "输出宽度上限（缺省 2048）：等比缩小、从不放大、两边取偶（yuv420p 要求）"),
            param!("crf", Integer, false, "libx264 质量（缺省 20，0~51，越小越清晰、文件越大）"),
            param!("max_frames", Integer, false, "帧数上限（缺省 1800）：帧数将超时自动加大步长（过程仍完整、只是更粗），绝不截断过程"),
        ],
    },
    ToolSpec {
        name: "try_stroke",
        profile: Profile::Core,
        summary: "试一笔：算出这一笔的效果但**不写入文档**（不产生原子、不改 seq、不改文件）⇒ 看一眼满意再 commit",
        mutating: false,
        params: &[
            param!("layer_id", String, false, "落在哪个图层（只用于取该图的上下文，不写入）；无 `based_on` 时必填"),
            param!("brush", String, false, "笔刷名"),
            param!("points", Any, false, "控制点 [[x, y, pressure?], …]（文档像素）"),
            param!("size", Number, false, "笔尖直径（像素；缺省用 .myb 自带）"),
            param!("color", Any, false, "笔尖颜色 {r,g,b,a} 或 [r,g,b,(a)]"),
            param!("opacity", Number, false, "整笔不透明度 0..1"),
            param!("hardness", Number, false, "笔尖硬度 0..1"),
            // **`sensors` 之前漏声明** ✓（第 719 轮；外部审计 S-02 第 ② 组）
            //   ⇒ handler 一直在读它 ✓（压感／倾斜／速度，见 `brush_sensors_from_value` ✓）
            //   而 `validate_args` 只放行**声明过**的键 ✗ ⇒ 传它就 `invalid_argument`
            //   ⇒ **这条能力（真实压感）从上线起就不可达** ✗ —— 声明与实现对不上，谁也发现不了 ✓。
            param!("sensors", Any, false, "压感／倾斜／速度：{\"pressure\":0..1,\"tilt_x\":…,\"tilt_y\":…,\"velocity\":…}（缺省全 0 = 无压感）"),
            param!("want_image", Boolean, false, "缺省 true = 返回区域图（base64 PNG）；false = 只返回包围盒与像素数"),
            param!("output_path", String, false, "给了就把区域图 PNG 落盘到该路径（导出目录或系统临时目录内，守卫同 export_png），响应给 image_file 元数据；它本身即「要图」，不必再传 want_image"),
            param!("margin", Integer, false, "区域图在包围盒外扩的像素数（缺省 16）"),
            param!("label", String, false, "笔迹标签 ⇒ 提交时写进原子，供 list_atoms(label=…) 筛"),
            param!("based_on", String, false, "基于某次试画：只覆盖本次传入的字段，其余全部继承"),
        ],
    },
    ToolSpec {
        name: "list_attempts",
        profile: Profile::Core,
        summary: "列出试画记录（按 label／状态筛，新的在前）⇒ 回看「我试过哪几版」",
        mutating: false,
        params: &[
            param!("limit", Integer, false, "最多返回几条（缺省 20）"),
            param!("label", String, false, "只列该标签的尝试"),
            param!("status", String, false, "只列该状态的尝试：pending／committed／discarded"),
        ],
    },
    ToolSpec {
        name: "discard_stroke",
        profile: Profile::Core,
        summary: "丢掉某次试画（状态转 discarded，之后不可再提交）⇒ 「试了不要」",
        mutating: false,
        params: &[
            param!("attempt_id", String, true, "try_stroke 返回的尝试编号"),
        ],
    },
    ToolSpec {
        name: "try_batch",
        profile: Profile::Core,
        summary: "试一组笔（一个部位通常 5~8 笔）⇒ 逐笔算、不落笔，返回每笔的 bbox 与像素数",
        mutating: false,
        params: &[
            param!("layer_id", String, true, "落在哪个图层（只取上下文，不写入）"),
            param!("strokes", Any, true, "笔数组：每项 {brush, points, size?, color?, opacity?, hardness?}"),
            param!("label", String, false, "整组的标签 ⇒ 提交时每笔都带上"),
        ],
    },
    ToolSpec {
        name: "commit_batch",
        profile: Profile::Core,
        summary: "把 try_batch 的一组试画按顺序贴回文档 ⇒ 返回 atom_ids，可整组撤销",
        mutating: true,
        params: &[
            param!("attempt_ids", Any, false, "try_batch 返回的 attempt_ids 数组（批量）"),
            param!("attempt_id", String, false, "单笔模式：只提交这一笔"),
        ],
    },
    ToolSpec {

        name: "commit_stroke",
        profile: Profile::Core,
        summary: "把某次 try_stroke 的试画原样贴回文档（位图方案 ⇒ 与试画逐像素一致）",
        mutating: true,
        params: &[
            param!("attempt_id", String, true, "try_stroke 返回的尝试编号"),
        ],
    },
    ToolSpec {
        name: "list_atoms",
        profile: Profile::Core,
        summary: "按 label／layer_id／brush／since_seq 筛出笔迹原子（带 seq 与 label）⇒ 先找到「哪一笔坏了」，再 revert；**每条带 `origin`**（drawn／imported／generated／system）—— ⚠️ `kind` 说的是**像素存在哪**（存储形态），**不是来源**：笔触原子在库里恒为 `import_image`，拿 `kind` 当来源证明会把「画出来的笔」误判成「导入了一张图」",
        mutating: false,
        params: &[
            param!("limit", Integer, false, "最多返回几条（缺省 20，上限 5000）"),
            param!("offset", Integer, false, "跳过前面几条匹配（翻页用；回执里有 `offset`/`truncated`/`has_more`）"),
            param!("label", String, false, "只列该标签的原子（brush_stroke 的 label 参数写入）"),
            param!("layer_id", String, false, "只列该图层的原子"),
            param!("brush", String, false, "只列该笔刷画的原子（名字含该子串）"),
            param!("since_seq", Integer, false, "只列 seq 大于该值的原子"),
            param!("group_id", String, false, "只列该组的原子（配合 revert_group 与 brush_stroke 的 group_id）"),
            param!(
                "bbox",
                Any,
                false,
                "只列笔迹区域与该框相交的原子：[x, y, w, h]（文档像素）⇒ 「在画面这个区域里」怎么查"
            ),
            param!(
                "thumb_size",
                Integer,
                false,
                "缩略图长边像素：缺省 0 = 保持 1:1；给 128 则**最近邻**缩小 ⇒ 传输量按边长平方下降"
            ),
            param!(
                "want_thumb",
                Boolean,
                false,
                "缺省 true = 每条带一张 128px 缩略图（外部复测方靠看图认笔）；单次最多 20 张，超出截断并注明"
            ),
            param!(
                "include_non_stroke",
                Boolean,
                false,
                "缺省 false = 只列笔迹（有 source 的原子）；true = 连建文档／建图层等一起列（它们没有笔刷名，brush 会是 null）"
            ),
        ],
    },
    ToolSpec {
        name: "get_atom",
        profile: Profile::Core,
        summary: "读取一条原子的完整记录（含净荷）",
        mutating: false,
        params: &[param!("atom_id", String, true, "原子 id")],
    },
    ToolSpec {
        name: "list_comments",
        profile: Profile::Collab,
        summary: "列出评论（协作通道的可读一侧）",
        mutating: false,
        params: &[
            param!("limit", Integer, false, "本页最多返回条数（缺省 50，上限 500 ✓；满页时 `truncated:true` ⇒ 立刻再问一次）"),
            param!("since_seq", Integer, false, "只返回 seq 大于该值的评论（增量轮询用 ✓）。**分页语义**：交付的是「游标之后最早的 `limit` 条」（页内仍是新的在前 ✓），`next_since` 只推进到**已交付**的最大 seq ⇒ 照着它轮询**一条不漏**（旧版交付最新一页却把游标推到全集最大 ⇒ 更早的评论永久消失 ✗）"),
            param!("actor", String, false, "按作者过滤"),
            param!("object_id", String, false, "按被评论的对象过滤"),
        ],
    },
    ToolSpec {
        name: "list_suggestions",
        profile: Profile::Collab,
        summary: "列出建议及其状态（pending / accepted / rejected，由 accept/reject 原子推导）",
        mutating: false,
        params: &[
            param!("status", String, false, "按状态过滤：pending/accepted/rejected"),
            param!("since_seq", Integer, false, "只返回 seq 大于该值的建议（AI 轮询增量用）"),
            param!("limit", Integer, false, "最多返回多少条（缺省 50，上限 500）"),
            param!("offset", Integer, false, "跳过前 N 条（分页）"),
        ],
    },
    ToolSpec {
        name: "comment",
        profile: Profile::Collab,
        summary: "发表评论（协作原子，不产生状态效果）",
        mutating: true,
        params: &[
            param!("text", String, true, "评论内容"),
            param!("target_atom", String, false, "被评论的原子"),
            param!("object_id", String, false, "被评论的对象"),
        ],
    },
    // ---- 扩展：structure ----
    ToolSpec {
        name: "set_brush_dynamics",
        profile: Profile::Core,
        // **AI 画家需求 P1-7**：默认 pressure→size 是线性的 ⇒ 画不出"轻入重出"。
        // **行业口径（查过权威实现 ✓）**：MyPaint/.myb 里曲线是 `settings[设置].inputs[输入] = [[输入值, 偏移]]`，
        // 而**偏移在对数域**（`radius_logarithmic` 的 `base_value` 就是 `ln(半径)`）⇒
        // ⇒ **本工具收的是"倍率"，写进去的是 `ln(倍率)`** ✓（写在这里，别让调用方猜 ✗）。
        // **改完落回工作区缓存里的同名 `.myb`** ✓（= MyPaint 的"预设覆盖内置" ✓；内置资产**一个字节不动** ✓）
        // ⇒ 后续 `brush_stroke` **自然读到** ✓（这条正是"持久化"的关键 ✓）。
        summary: "设置笔刷动力学曲线：curve 里的值是**倍率**（如 1.5 = 粗一半）⇒ 按 ln(倍率) 写进 .myb 的对数偏移；改完存回工作区缓存（覆盖内置）",
        mutating: false,
        params: &[
            param!("brush", String, true, "笔刷名（同 brush_stroke，可省 .myb）"),
            param!(
                "curve",
                Object,
                true,
                "{size_pressure|opacity_pressure|tilt_size: [[输入值, 倍率], …]}；只替换给到的输入，其余保留"
            ),
        ],
    },
    ToolSpec {
        name: "get_brush_dynamics",
        profile: Profile::Core,
        // **为什么需要它**（C1 现状清单短名单 #2）：`get_brush_settings` 只给 base_value，
        // 曲线「写得进、读不回」⇒ 调参盲写。本工具把 66 项的 base + 全部 inputs 曲线
        // （含未知输入/未知设置）一次读全；format=myb 时给完整 .myb 文本。
        summary: "读出一支笔刷的全部动力学曲线：66 项设置的 base_value ＋ 每条 inputs 映射曲线（原始偏移）；format=myb 时返回完整 .myb 文本",
        mutating: false,
        params: &[
            param!("brush", String, true, "笔刷名（同 brush_stroke，可省 .myb）"),
            param!("format", String, false, "json（缺省）= 结构化读出；myb = 完整 .myb 文本"),
        ],
    },
    ToolSpec {
        name: "set_brush_curve",
        profile: Profile::Core,
        // **修 C1 短名单 #1**：老 set_brush_dynamics 的任意键分支只能改写已有 input、
        // 且写出的 {"points": …} 与解析器不兼容（写坏整支笔，已实测）。
        // 本工具：规范名校验（未识别报错）、input 不存在就新建、写裸数组（解析器原生格式）、
        // 收**原始偏移**（与倍率口径的 set_brush_dynamics 分工，summary 里写清）。
        summary: "写任意一条笔刷动力学曲线（原始偏移口径）：setting/input 用 .myb 规范名，points 是 [[输入值, 偏移], …]；input 不存在会自动新建，名字写错会报错",
        mutating: false,
        params: &[
            param!("brush", String, true, "笔刷名（同 brush_stroke，可省 .myb）"),
            param!("setting", String, true, "设置的规范名（如 radius_logarithmic / opaque / hardness）"),
            param!("input", String, true, "输入的规范名（如 pressure / speed1 / tilt_declination / stroke）"),
            param!("points", Array, true, "点列 [[输入值, 偏移], …]（至少 1 点；x 自动升序）—— 注意是原始偏移，不是倍率"),
        ],
    },
    ToolSpec {
        name: "apply_curve_preset",
        profile: Profile::Core,
        // **曲线形状快速通道**（C1 短名单 #5）：此前只有 Web 查看器里 3 档写死的
        // size_pressure 形状，MCP 侧没有。这里给 6 个命名形状（倍率点列在实现里写死、
        // README/工具说明可查），经 set_brush_dynamics 的语义键通道落盘。
        summary: "一键套用命名曲线形状（倍率口径）：shape ∈ linear/soft/hard/ease-in/ease-out/s-curve，target 缺省 size_pressure（限七个语义键）",
        mutating: false,
        params: &[
            param!("brush", String, true, "笔刷名（同 brush_stroke，可省 .myb）"),
            param!("shape", String, true, "形状名：linear / soft / hard / ease-in / ease-out / s-curve（ease-in-out 是 s-curve 的别名）"),
            param!("target", String, false, "套到哪条语义曲线（缺省 size_pressure）：size_pressure/opacity_pressure/tilt_size/size_speed/opacity_speed/opacity_multiply_pressure/smudge_stroke"),
        ],
    },
    ToolSpec {
        name: "list_engine_capabilities",
        profile: Profile::Core,
        // **为什么要它** ✗：能力面此前**散落各处** —— 滤镜/调整/混合各有 `NAMES` 常量 ✓，
        // 修图/液化只是渲染器里的字符串比较 ✗（本轮才提成 `RETOUCH_KINDS`/`LIQUIFY_KINDS` ✓），
        // 笔刷动力学要靠 `get_brush_settings` 读**某一支笔**才知道 ✓，介质又只写在参数说明里 ✗
        // ⇒ **∴ agent 无法"先看有什么、再决定画什么"** ✗。
        // **本工具一处导出全部 ✓，且每个列表都**引用引擎自己的常量**✗**（不是硬编码 ✗）⇒ **∴ 加一种只改一处 ✓**。
        summary: "列出引擎的全部能力面：混合模式／滤镜／调整／修图种类／液化模式／介质／笔刷动力学名字",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "get_brush_settings",
        profile: Profile::Core,
        // **为什么需要它**：引擎（`hokusai`）实现了全部 MyPaint 动力学 ✓，
        // 而 MCP 侧此前只有 `size`/`opacity`/`hardness`/`color` 四个旋钮 ✗
        // ⇒ agent 无法逐项调 `speed1_gamma`／`slow_tracking`／`change_color_*`／`smudge_length` 等
        // ⇒ 只能从 199 支固定预设里挑 ⇒ **画不出真实笔速、手抖与一笔内变色** ✗
        // **本工具把全部规范名读出来** ✓，并**单独列出引擎不认识的项** ✓
        //（`hokusai` 会保留未知项做无损往返 ✓ ⇒ 那一栏就是「引擎忽略了什么」的实测证据 ✓）。
        summary: "读出一支笔刷的全部 MyPaint 动力学（逐项读 `base_value`），并单独列出引擎不认识的设置",
        mutating: false,
        params: &[param!("brush", String, true, "笔刷名（同 brush_stroke，可省 .myb）")],
    },
    ToolSpec {
        name: "set_brush_setting",
        profile: Profile::Core,
        // **只写 `base_value`**（基准值 ✓），**不动 `inputs` 动力学曲线** ✗ —— 曲线归 `set_brush_dynamics` ✓。
        // 名字用 `BrushSetting::from_cname` 校验 ✓（那是 `.myb` 的官方标识符 ✓）⇒ **未知名报错并给候选** ✓。
        // 落盘沿用 `set_brush_dynamics` 的手法 ✓（`persist_brush_json` ⇒ `import_asset(overwrite)` ✓）
        // ⇒ 覆盖工作区缓存里的同名笔刷 ✓（内置资产一个字节不动 ✓）。
        // **第 1440 轮**：那一手法原先要「写临时文件再交 `path`」✗，现在正文直接走 `data_base64` ✓
        //（`path` 通道带工作区沙箱 ⇒ 服务端自己走它会被拒 ✗；临时文件也从不删 ✗）。
        summary: "设一笔刷的单项 MyPaint 动力学基准值（名字用 .myb 的规范名，如 smudge / speed1_gamma / change_color_h）",
        mutating: false,
        params: &[
            param!("brush", String, true, "笔刷名（同 brush_stroke，可省 .myb）"),
            param!("setting", String, true, "设置的规范名（.myb 里的键，如 radius_logarithmic / smudge / slow_tracking）"),
            param!("value", Number, true, "基准值（原样写入 settings[设置].base_value）"),
        ],
    },
    ToolSpec {
        name: "set_layer_blend",
        profile: Profile::Core,
        // **AI 画家需求 P1-4**：`multiply` 是**罩染**的基础 ✓（报告："现在只能调 opacity 硬叠 ⇒ 颜色发脏"✗）。
        // **查过现状** ✓：渲染器的 `BlendMode`（`blend.rs:17` ✓）与合成（`render.rs:294` ✓）**早就有** ✓，
        // 图层字段 `blend_mode` 也**早就被解析** ✓（`fold.rs:1348` ✓）⇒ 缺的**只是这个友好名字** ✓。
        // ⇒ 本工具是**薄包装** ✓：转发给既有的 `set_property` ✓（不另有状态 ✗、不与它分叉 ✓）。
        summary: "设置图层混合模式（normal/multiply/screen/overlay/darken/lighten/add）—— 罩染用 multiply",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("mode", String, true, "normal / multiply / screen / overlay / darken / lighten / add"),
        ],
    },
    ToolSpec {
        name: "set_property",
        profile: Profile::Structure,
        summary: "设置对象/图层属性（低层原子 set_property）",
        mutating: true,
        params: &[
            param!("key", String, true, "属性名"),
            param!("value", Any, false, "属性值（类型随 key 变化：布尔/字符串/对象）"),
            param!("object_id", String, false, "对象 id"),
            param!("layer_id", String, false, "图层 id"),
        ],
    },
    ToolSpec {
        name: "lock_layer",
        profile: Profile::Structure,
        summary: "锁定图层",
        mutating: true,
        params: LAYER_ID,
    },
    ToolSpec {
        name: "unlock_layer",
        profile: Profile::Structure,
        summary: "解锁图层",
        mutating: true,
        params: LAYER_ID,
    },
    ToolSpec {
        name: "list_brushes",
        profile: Profile::Core,
        // **P1 #9（第 254 轮）**：原来写死 4 支内置笔刷、而且看不到工作区 => AI 不知道有哪 201 支
        // `.myb` 可挑，也不知道 `brush_params` 的字段叫什么。现在枚举走仓库已有的资产枚举，
        // 并附上**规范字段名全表**（与 P0 #4 的 `brush_params` 配对）。
        summary: "列出可用 .myb 笔刷（含各自参数摘要 ＋ 规范字段名全表）=> 先看有哪些笔刷，再决定 brush 与 brush_params",
        mutating: false,
        params: &[
            param!(
                "filter",
                String,
                false,
                "只列名字里含该子串的笔刷（如 oil／watercolor／pencil／Clouds）"
            ),
            param!(
                "medium",
                String,
                false,
                "按介质过滤：oil／watercolor／sketch／ink／pastel ⇒ 只返回该画法常用的笔刷白名单（与 filter 可叠加）"
            ),
        ],
    },
];

// ---------------------------------------------------------------------------
// workflow 预设白名单（Photo / Svg，设计 §7 F 族）
// ---------------------------------------------------------------------------

/// **Photo（修图）预设的工具白名单** ✓ —— 与 `ALL_TOOLS` 同源同文件 ✓。
///
/// **它是什么** ✗：`Profile::Photo` 不展开为「加层」✗ ⇒ 它贡献这份名单 ✓，
/// `ToolRegistry::with_profiles` 在**显式子集请求**时用它与分层展开集求交 ✓
/// （**触发口径**见 `tools.rs` 的 `with_profiles` ✓：ALL 全集组成与缺省会话
/// **不**触发收窄 ✓ —— Wang 2026-10-24 裁定「ALL 要包含所有的」✓）。
///
/// **维护纪律** ✓：名单里每个名字**必须**存在于 `ALL_TOOLS` 且分层 ∈ Photo
/// 基底集 {Core, History, Changeset, Retouch, Structure} ✓ ⇒ 由测试
/// `photo_allowlist_names_all_registered` 守 ✗（塞不存在的名字 ⇒ 红 ✓）。
/// 新工具进预设 = **只改这一处** ✓。
pub const PHOTO_TOOLS: &[&str] = &[
    // ---- 文档与会话 ----
    "new_document",
    "list_documents",
    "get_document",
    "delete_document",
    "get_state",
    "get_log",
    "list_atoms",
    "get_atom",
    "export_project",
    "import_project",
    "verify_package",
    "get_job",
    "get_render_status",
    "cancel_job",
    "batch",
    "collect_diagnostics",
    "explain_tool",
    "list_engine_capabilities",
    // ---- 导入与资产 ----
    "put_blob",
    "import_image",
    "import_asset",
    "list_assets",
    // ---- 图层与对象 ----
    "create_layer",
    "update_layer",
    "delete_layer",
    "reorder_layers",
    "list_layers",
    "duplicate_layer",
    "lock_layer",
    "unlock_layer",
    "set_layer_blend",
    "list_objects",
    "get_object",
    "update_object",
    "delete_object",
    "restore_object",
    "move_object",
    "replace_object_data",
    "transform_object",
    "set_property",
    // ---- 修图（Retouch 全层） ----
    "resample",
    "add_adjustment",
    "apply_adjustments",
    "add_filter",
    "update_adjustment",
    "update_filter",
    "list_effects",
    "list_presets",
    "get_preset",
    "apply_preset",
    "preview_preset",
    "estimate_dehaze",
    "frequency_separation",
    "dodge_burn_setup",
    "auto_retouch",
    "liquify_push",
    "liquify_twirl",
    "liquify_pinch",
    "patch",
    "smudge",
    "heal_stamp",
    "clone_stamp",
    // ---- 蒙版与选区 ----
    "create_mask",
    "paint_mask",
    "delete_mask",
    "create_selection",
    "delete_selection",
    "list_selections",
    "transform_selection",
    "select_opaque_pixels",
    // ---- 分析取样 ----
    "get_histogram",
    "sample_color",
    "sample_colors",
    "analyze_region",
    "render_region",
    "describe_canvas",
    // ---- 历史与变更集 ----
    "undo_last",
    "redo_last",
    "get_undo_status",
    "revert",
    "revert_atoms",
    "revert_to_seq",
    "begin_changeset",
    "commit_changeset",
    "abort_changeset",
    "begin_transaction",
    "commit_transaction",
    "get_changesets",
    "revert_changeset",
    // ---- 导出 ----
    "export_png",
    "export_book_pngs",
    // ---- 精修最小绘画 ----
    "draw_stroke",
    "fill_region",
    "erase",
    "list_brushes",
    "get_brush_settings",
    "set_brush_setting",
];

/// **Svg（矢量）预设的工具白名单** ✓ —— 维护纪律同 [`PHOTO_TOOLS`] ✓：
/// 每个名字必须存在于 `ALL_TOOLS` 且分层 ∈ Svg 基底集
/// {Core, History, Changeset, Structure} ✓（**不含任何 Retouch 照片面** ✓），
/// 由测试 `svg_allowlist_names_all_registered` 守 ✓。
pub const SVG_TOOLS: &[&str] = &[
    // ---- 文档与会话（同 Photo） ----
    "new_document",
    "list_documents",
    "get_document",
    "delete_document",
    "get_state",
    "get_log",
    "list_atoms",
    "get_atom",
    "export_project",
    "import_project",
    "verify_package",
    "get_job",
    "get_render_status",
    "cancel_job",
    "batch",
    "collect_diagnostics",
    "explain_tool",
    "list_engine_capabilities",
    // ---- 导入与资产（同 Photo） ----
    "put_blob",
    "import_image",
    "import_asset",
    "list_assets",
    // ---- 图层与对象（Photo 同组去 replace_object_data，加组操作） ----
    "create_layer",
    "update_layer",
    "delete_layer",
    "reorder_layers",
    "list_layers",
    "duplicate_layer",
    "lock_layer",
    "unlock_layer",
    "set_layer_blend",
    "list_objects",
    "get_object",
    "update_object",
    "delete_object",
    "restore_object",
    "move_object",
    "transform_object",
    "set_property",
    "create_group",
    "add_to_group",
    "remove_from_group",
    "set_group_transform",
    "revert_group",
    // ---- 矢量著录 ----
    "get_path",
    "edit_path",
    "path_edit",
    "draw_shape",
    "stroke_path",
    "combine_shapes",
    "convert_to_shape",
    "convert_to_path",
    "vector_erase",
    "vector_hit_test",
    "draw_stroke",
    "fill",
    "fill_region",
    "erase",
    "gradient_fill",
    "gradient_blend",
    "create_selection",
    "delete_selection",
    "list_selections",
    "transform_selection",
    "select_opaque_pixels",
    // ---- 文本 ----
    "draw_text",
    "list_fonts",
    // ---- 实例 ----
    "create_instance",
    "detach_instance",
    "link_to_master",
    "get_dependency_graph",
    "get_resolved_state",
    "update_override",
    "update_sync_policy",
    // ---- 历史与变更集（同 Photo） ----
    "undo_last",
    "redo_last",
    "get_undo_status",
    "revert",
    "revert_atoms",
    "revert_to_seq",
    "begin_changeset",
    "commit_changeset",
    "abort_changeset",
    "begin_transaction",
    "commit_transaction",
    "get_changesets",
    "revert_changeset",
    // ---- 导出 ----
    "export_svg",
    "export_png",
    "export_book_pngs",
];

// ---------------------------------------------------------------------------
// 分发
// ---------------------------------------------------------------------------

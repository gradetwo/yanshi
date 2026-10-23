---
name: yanshi-photo-styling
description: 用 yanshi 的 MCP 工具修照片、调风格 —— 导入 RAW、先校正到中性基线、再用风格预设系统（list_presets / get_preset / preview_preset / apply_preset）套风格并用 strength 与语义旋钮微调。适用于处理 RAW/照片类素材、复现某种修图风格、或诊断「套了风格却发绿/发灰/过曝」的问题。不适用于绘画创作（那是 yanshi-painting）。
---

# 先校正，后风格；微调只动一个旋钮

## 这份 skill 解决什么

给一张照片套上某种风格（港风、莫兰迪、小清新……），最常见的三种失败：

1. **在一张没校正的原片上直接施加风格** —— 原片的欠曝、偏色被风格放大，最后分不清哪一层出了问题。
2. **每次微调都重写一整串调整参数** —— 迭代慢、不可复现、改完不知道自己动了什么。
3. **照着 Lightroom 数值硬抄参数** —— 两边的参数名、量程、口径都不同，抄过来必然翻车（本仓曝光键叫 `ev`，不是 `exposure`，更不是 LR 的滑杆数）。

**∴ 正道是走预设系统**：风格配方已经沉淀成数据（22 套内置预设），agent 的工作是
**选对预设 → 看清展开 → 一次落图 → 用 strength / knobs 做小步微调**。
配方本身不在这份 skill 里——配方是数据，在 `crates/yanshi-server/data/style-presets.json`，
以 `list_presets` / `get_preset` 读到的是唯一口径，skill 里抄一份只会过期。

## 为什么目录叫 yanshi-photo-styling

yanshi 会有很多个方向的 skill（已有 `yanshi-painting` 管绘画笔触）。
这个目录名把三件事说清：对象是**照片**（photo，区别于 painting）、动作是**修图与调风格**（styling，
与设计文档 `docs/design/style-preset-system.md` 的 style preset 术语一致）、归属是 yanshi 的 MCP 工具面。
下次再有新方向（如视频、拼版），照这个粒度另起目录，不要往这份里塞。

## 前置条件（先确认，再动手）

| 条件 | 怎么确认 | 不满足的表现 |
|---|---|---|
| 服务端是 **raw feature 构建** | 导入 RAW 时 `put_blob` 能把返回的 `mime_type` 改写成 `image/x-yanshi-rawsrc` | 关构建**不能入库 rawsrc、也不能展开它**，错误会明说「本构建未启用 raw feature」⇒ 用开了 `raw` feature 的构建（`yanshi-http/raw` → `yanshi-server/raw` → `yanshi-render/raw` 同开同关，缺省关） |
| 文档已打开/新建 | 新文档**自带 `layer_default` 图层** | 图层不存在 ⇒ `apply_preset` 报「图层 … 不存在」，不会替你新建 |
| 大图先掂量内存 | 见第一步的大图警告与 `references/anti-patterns.md` 的诊断表 | 约 3300 万像素级 RAW 在小内存机器上导入展开会被系统杀掉 |

## 四个预设工具（签名以注册表为准，这里只记语义）

| 工具 | 干什么 | 必填 | 可选（缺省） |
|---|---|---|---|
| `list_presets` | 列预设摘要：id / 双语名 / 状态 / 标签 / 适用与慎用 / 旋钮 | — | `tag`、`subject`（适用题材子串）、`status` |
| `get_preset` | 读一套完整定义：两段配方、中性值、旋钮映射、实测 notes，并附 strength=1 的展开预览 | `preset_id` | — |
| `preview_preset` | **只展开不落成、不渲染**：算出将要落的全部操作（含确定性 object_id 与实际参数），展开结果先过一遍真实参数校验 | `preset_id` | `strength`（1）、`knobs`、`include_correction`（true） |
| `apply_preset` | 展开并落到图层上：先全量校验，任一非法**整组不落**；二次应用走更新不叠加 | `preset_id`、`layer_id` | `strength`（1）、`knobs`、`include_correction`（true） |

响应里要读的字段：`apply_preset` 回 `created_object_ids` / `updated_object_ids` 与计数、
实际 `strength`、`op_count`；`list_presets` 回 `count` 与每套的 `correction_baseline`（full/light）、
`source`（builtin/external）。看到 `ok=false` **先读 error 原文**——本系统的报错都会写明
是哪套预设、哪个旋钮、收到什么值（如未知预设会把 22 个可用 id 全列出来）。

## 核心语义（五条，记牢）

1. **两段结构**：每套预设 = 修正段 + 风格段。修正段把照片拉回中性基线，风格段才是风格本体。
2. **strength 只管风格段**：风格段每个可调参数都声明了中性值，实际值 = 中性值 +（配方值 − 中性值）× strength。
   修正段**不参与插值**——strength=0 关掉的是风格，不是校正。
   没有声明中性值的键（seed、size、channel、`split_toning` 的 `amount` 这类）是**固定参数**，strength 不动它。
3. **knobs 在 strength 之后生效**：旋钮值按它自己的 [min, max] 归一后，映射到声明的目标参数上，
   **覆盖**插值结果。所以 strength=0 又给了旋钮，不再是恒等——做对照时别同时给旋钮。
   **没给的旋钮不生效**：`default` 只是建议值，不是缺省覆盖（只有一处真相：配方值）。
4. **二次 apply = 更新，不是叠加**：落成对象用确定性 id `preset_<预设id>_<c|s>_<序号>_<类型>`
   （c = 修正段、s = 风格段）。同一预设再 apply，同 id 命中更新路径（Supersede 合并该笔
   全部参数——同一笔的参数键集恒定，所以是全量替换）。想换一套风格是给**另一个** preset_id
   再 apply，图层上会多出一组对象，不会自动撤掉上一套。
5. **全量校验、整组落成**：图层不存在、任一笔参数非法、已存在的同 id 对象类型/效果名对不上，
   都会在落成前整组拒绝，并且**一个原子都不落**。报错后先读 error，不要换个参数盲重试。

## 工作流（六步，按顺序）

### 第一步：导入 RAW

- `put_blob {"path": "<服务端本地路径>"}` 入库——它会**探测字节**：真是 RAW 就把返回的
  `mime_type` 改写成 `image/x-yanshi-rawsrc`。把返回值 `{blob_hash, size, mime_type}` **原样**
  作为下一步的 `bitmap`，不要自己拼 mime。
- `import_image {"layer_id": "layer_default", "bitmap": {…上一步返回值…}, "region": {x,y,w,h}, "raw_options": {…}}` 落层。
  `raw_options` 只对 RAW 源有效，给到普通图会**响亮报错**（不是静默忽略）。常用键：
  `wb`（as_shot/auto）、`demosaic`（malvar/bilinear，缺省 malvar）、`pre_exposure_ev`（−4..4，越界报错不钳制）、
  `lens_correction`（off/auto，缺省 off）、`lens_override`（点名镜头库中的镜头，查无此镜报错）、
  `highlight_recovery`（chroma/floor，缺省 chroma）。导入响应会回显 `raw_meta`——相机、镜头、
  实际生效的展开参数都在里面，**先读它**再往下走。
- 大图警告：约 3300 万像素的 RAW 导入展开有内存峰值，小机器上服务端会被杀。
  先确认机器内存，或先降尺寸/换机器，**不要反复硬撞同一个文件**（撞一次记一次，没用）。
- 格式注记：Panasonic RW2、Olympus ORF 已能被入库探针正确识别（旧版探针的 magic
  白名单缺这两家签名，会把真 RAW 判成非 RAW、连带拒绝 `raw_options`）。RW2 竖拍
  文件的方向拾取有已知缺口（EXIF Orientation 可能记错），导入后核对 `raw_meta` 的
  方向与画面是否一致，处置见诊断表。

### 第二步：校正（多数情况已含在预设里，但要知道自己在用哪种基线）

- 预设的修正段有两种基线：`full`（全量）与 `light`（轻校正）。
  **用 light 的是曾蒙灰纱的批次**：小清新、韩系、法式、糖水、田园奶油——它们修正段只有 2 笔
  温和调整；全量基线的提影量和这些风格段叠加会蒙灰，这是实测教训。
- `list_presets` 的摘要里有 `correction_baseline` 字段，apply 前看一眼就知道这套用哪种。
- 照片本身有预设基线盖不住的问题（严重欠曝、混合光源色偏）时，先用既有工具手工校正
  （`get_histogram` 看分布、`add_adjustment` / `apply_adjustments` 落笔），再考虑
  `include_correction: false` 只落风格段，避免两份校正打架。
- 注意 `exposure` 的参数名是 **`ev`**；且导入期的 `pre_exposure_ev` 与调整层的 `exposure.ev`
  是两个阶段的量，别重复补同一笔曝光。

### 第三步：选风格（先 list，不要盲选）

- 先按题材缩小范围：`list_presets {"tag": "夜景"}` 或 `{"subject": "霓虹"}`。
- 读候选的 `suitable` / `caution`：风格与题材冲突是头号失败源（港风的 caution 明写
  「白天须降强度」「肤色发绿是头号失败模式」）。主体色与风格染色方向打架时，换风格或降 strength，
  不要硬套后到处补救。
- 读 `status`，三种状态的纪律不同：
  - `established`（7 套）：真实照片目检成立，可放心用；
  - `fixed_v2`（5 套）：第一版翻车后按实测修正的配方（港风/国风/电影/HDR/胶片复古），就是要用这一版；
  - `needs_review`（10 套）：**还没在真实照片上目检过**。可以用，但必须向用户提示「未目检」，
    出图后认真看、把结论记下来——不要把 needs_review 的出图当成已验证效果交付。

### 第四步：preview 后再 apply

- 先 `preview_preset` 看展开清单：笔数、每笔的实际参数、确定性 object_id。
  一套预设总笔数 = 修正段 + 风格段（如港风 4 + 9 = 13 笔），数对得上才往下走。
- 再 `apply_preset {"preset_id": …, "layer_id": …}` 落图。批量驱动时走静默：
  把多步调用放进 `batch` 且 `silent: true`，子调用不产预览、不编码 PNG，省掉每步的预览开销；
  需要中途看进展时用 `preview_every_n_strokes` 放行几次即可。
- 渲染与落图**串行**推进：一张接一张，不要并发猛灌大图——内存是这一路的硬约束。

### 第五步：看图（迭代的依据只有眼睛 + 数值）

- 小步看法：`render_region` 出局部、`export_png` 出整幅（路径只能落在导出目录
  `YANSHI_EXPORT_DIR` 或系统临时目录，这是沙箱口径，不是故障）。
  `export_png` 是**同步重渲**：响应里的 `head_seq` / `rendered_seq` 只是预览水位，
  不管导出完整性——导出的图恒是当前全部原子的结果，不必「等渲染跟上」再导。
  判断一轮微调到底有没有生效，最硬的证据是比两次导出响应的 `sha256`：改了参数哈希还不变，就是没落上。
- 对照法：需要判断「这个风格到底动了什么」时，在**另一文档/图层**上以同预设
  `strength: 0` 再 apply 一次——风格段逐参数等于中性值，得到的正是「只有修正段」的基线图，
  与 styled 版并排比，差异就是风格本体。注意别在同一图层上这样做：那会把已落的风格**更新成 0**。
- 肤色类问题先取样再动手（`sample_color` / `get_histogram`），不要凭观感全局乱推。

### 第六步：微调循环（一次只动一个量）

| 症状 | 先动什么 | 不要动什么 |
|---|---|---|
| 整体太重/太假 | 降 `strength`（如 1 → 0.7 → 0.5）再 apply | 不要逐笔去改展开参数 |
| 某个味道不对（太青、不够暖、颗粒太粗） | 动对应的**语义旋钮**（先在 `get_preset` 里查这套的 knobs 与它映射的目标） | 不要动没声明旋钮的参数 |
| 肤色发绿（港风/国风类高发） | 降 strength，或动保护红/橙的旋钮（如国风 `zhuhong`、港风 `teal` 往低调） | **不要动全局色温救肤色**——那会把整幅的冷调根基一起掀掉；根因是全局色偏与染色，不是白平衡错了 |
| 太灰/蒙纱 | 查是不是 light 基线风格又被手工叠了全量校正；或 strength 过低 | 不要先加 dehaze/clarity 硬拉 |
| 个别地方仍不对 | 预设之外用 `add_adjustment` 手工补一两笔，并记下补了什么 | 不要为了局部问题把整套 strength 拉满 |

- 每轮只改一个量、重新 apply、再看图。响应里的 `updated_object_ids` 非空说明走的是更新路径；
  若 `created_object_ids` 每次都在涨，说明图层或文档换了（确定性 id 只在同一文档同一图层内命中）。
- **再 apply 会把这组预设对象的 opacity 重置为 1.0**——手工改过某个预设对象不透明度的，
  微调后要重新设。
- 糖水 `sweet` 特例：配方默认**不带辉光**（glow 那笔的 intensity 配方值就是 0），
  只有显式给 `glow` 旋钮才会打开。打开后必须目检边缘——「斜切边缘是否辉光所致」的对照实验
  还没有定论（见该套 notes），发现边缘异常先把 glow 关回去再判断。

## 数据著录口径（读/写配方前必读，设计文档 §9.1）

内置配方已经按**内核的线性空间语义**校准完毕——`get_preset` 读到的就是内核坐标，
**不要**按显示空间（Lightroom）直觉去「纠正」它们。反过来，自己写新配方（外挂或升内置）
时必须先换算，否则会原样复现展示册制作时抓到的三轮翻车：

- **`split_toning` 必须带 `amount`**：三元组是线性空间的加法偏移，显示空间的「轻染」
  量级会被放大 2–4 倍。现行定档：亮场景约 0.35；**暗场景必须更小**——forest 0.05、
  cinema 0.06、hong_kong 0.08、guofeng / retro_film 0.12（规律：amount 随场景亮度缩放）。
  `amount` 是固定参数，strength 不动它；染色还重先查 amount 与场景是否匹配，别只降 strength。
- **`curves` 控制点与 `levels.black` 是位置型参数**：写新值先把显示直觉值逐点过
  sRGB EOTF 换成线性坐标（显示 0.07 的 fade 黑点不转换，等于在线性里把黑位抬到约
  0.30；levels.black 不转换，有效显示黑点会被放大到意图的 3.5–7 倍）。现行数据里
  的点列已是转换后坐标（如小清新 curves 首点 y≈0.0049、且已按显示空间曲线形重采样
  到 17 点保形）。`levels.white` / `gamma` 跨空间近似不变，直通。
- **`brightness_contrast` 是无枢轴线性**（绕黑缩放 + 线性加法）：brightness 在黑端的
  实效约为著录值的 5–6 倍，只给 0–0.03 量级；fade 类组合（curves 抬黑 + bc 亮度）
  必须在 style 段末尾自带 `levels` 黑点回锚，否则成图黑位全浮（retro_film 实测教训）。
- **`color_balance` 别写进新配方**：它与 split_toning 同根（线性空间分档加法偏移），
  目前无预设使用、且没有 amount 可收敛——属潜伏项，要用先补校准注记。
- 其余常用项（`exposure.ev`、`shadows_highlights`、`saturation` 等）与显示口径同阶，
  直通著录即可；完整审计结论与清白名单见展示册 AUDIT 与设计文档 §9/§9.1。

## 外挂预设（YANSHI_PRESET_DIR）

- 想试新配方或临时改一套：把同格式的 `style-presets.json` 放进一个目录，
  启动服务端时设 `YANSHI_PRESET_DIR=<该目录>`。合并规则：同 id 覆盖内置、新 id 追加，
  `list_presets` 的 `source` 会标成 `external`。
- **报错行为要知道**：变量已设置但目录里缺文件、文件解析失败，都是**直接报错**，
  不会静默退回纯内置库——看到这类报错先去查外挂文件，不要怀疑内置数据坏了。
- 外挂实验配方在真照片上验证成立后，才升进内置 JSON（并同步更新 status 与设计文档附录）。
  升内置是改数据文件 + 重跑预设测试门，不是改代码逻辑。

## 与研究材料的关系（一句话版）

风格的判断框架是**四轴**（影调 / 色彩 / 质感 / 题材适配），配方构建遵循**八步法**
（先校正、后风格；曝光与白平衡不进风格段）——细节已经落进每套预设的元数据与配方里，
完整出处是研究文档《修片风格体系速查与配方》与设计文档 `docs/design/style-preset-system.md`
（含 LR → Yanshi 换算表与三期边界：像素级低清预览、B&W Mix、逐通道 HSL 尚未实现，
预设 notes 里标了 unsupported 的技法就是真没有，不要假装能调）。

## 完整示例（港风夜景，从导入到定稿）

```jsonc
// 1) RAW 入库（返回的 mime_type 是 rawsrc 才说明探测命中；bitmap 原样透传）
put_blob        {"path": "/srv/photos/night_street.arw"}
// → {"blob_hash": "…", "size": …, "mime_type": "image/x-yanshi-rawsrc"}

// 2) 展开落层（raw_options 只给需要的键：lens_correction 缺省 off、pre_exposure_ev 只在原片明显欠曝时给）
import_image    {"layer_id": "layer_default",
                 "bitmap": {"blob_hash": "…", "size": …, "mime_type": "image/x-yanshi-rawsrc"},
                 "region": {"x": 0, "y": 0, "w": 6000, "h": 4000},
                 "raw_options": {"wb": "as_shot", "highlight_recovery": "chroma"}}

// 3) 选题材 → 看候选的 suitable / caution / status
list_presets    {"tag": "夜景"}
// → 选中 hong_kong（港风，fixed_v2）：适用夜街/招牌/茶餐厅，caution 提示肤色发绿风险

// 4) 先看 strength=1 的完整定义与展开，再预览要试的 strength
get_preset      {"preset_id": "hong_kong"}
preview_preset  {"preset_id": "hong_kong", "strength": 0.8}
// → ops 共 13 笔（修正 4 + 风格 9），object_id 形如 preset_hong_kong_s_02_split_toning

// 5) 落图
apply_preset    {"preset_id": "hong_kong", "layer_id": "layer_default", "strength": 0.8}
// → created_object_ids 13 个，updated 0

// 6) 看图后觉得青绿阴影还是重：只动 teal 旋钮（strength 不变），再 apply = 原地更新
apply_preset    {"preset_id": "hong_kong", "layer_id": "layer_default",
                 "strength": 0.8, "knobs": {"teal": 0.3}}
// → created 0，updated_object_ids 13 个（同一组 id，对象总数不增）

// 7) 定稿导出（同步重渲，导出恒完整；记下响应的 sha256 作本轮定稿指纹）
export_png      {"path": "night_street_hk.png"}
```

## 反模式（明写，别做）

- ✗ 不看 `list_presets` 的适用/慎用与 status，凭风格名盲选。
- ✗ 把 needs_review 的出图当已验证效果，不提示、不目检。
- ✗ 跳过 preview 直接 apply，然后靠反复 apply 猜参数。
- ✗ 微调时同时改 strength + 多个旋钮 + 手工补笔，改完说不清哪一项起了作用。
- ✗ 在同一图层用 strength=0 做对照——那不是对照，是把风格更新没了。
- ✗ 用全局色温救被染色污染的肤色。
- ✗ 照抄 Lightroom 滑杆数值手搓 ops 代替预设（量程口径全不同；真要手搓，先读 `get_preset` 的展开值）。
- ✗ 对约 3300 万像素的大 RAW 反复硬撞导入，看着服务端一次次被杀。
- ✗ 并发灌多张大图渲染（内存峰值叠加）；串行 + 静默批处理才是这一路的正道。
- ✗ 给普通 JPEG/PNG 的 import 带 `raw_options`，或给 RAW 漏掉 feature 构建问题去查参数。

诊断表见 `references/anti-patterns.md`；22 套预设的 id / 状态 / 基线速查见 `references/preset-index.md`。

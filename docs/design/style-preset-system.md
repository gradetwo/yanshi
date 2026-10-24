# Yanshi 风格预设系统设计文档

> 状态：已实现一期+二期主体（展开器、四个工具、22 套内置数据、外挂目录合并），
> 见文末「实现落点」。本文按能直接转给开发的规格写；所有「默认」项均已采用、可改，
> 改时须同步数据与测试。
> 数据来源：《修片风格体系速查与配方》（下称「速查」）的 22 套 Lightroom 参数签名、
> 2026-10-23 实测批次结论（chatlog）、《Lightroom 与 Photoshop 精修技巧调研》（修正段依据）。

## 1. 目标与非目标

**目标**：让 agent 下次调风格时不必重写一整串 ops——按名取一套已验证的配方，
一次调用落图；看后不满意，用一个 `strength` 或一两个语义旋钮重出，而不是重写配方。

**非目标**：

- 不做渲染队列/批处理调度（渲染仍走既有路径）；
- 不做 PDF 展示册（素材与版式是另一项工作）；
- 不模拟没有对应能力的技法：无对应工具的参数一律写进该预设的 `notes.unsupported`，
  **不许编造参数硬凑**（见附录各套 notes）；
- 不做天空替换/合成（与 LR/PS 调研的范围口径一致：只做忠实修复与风格化）。

## 2. 核心决定：预设是数据，不是代码

每个预设是一份 JSON 数据（id、双语名、状态、题材标签、适用/慎用、说明、实测 notes、
修正段 + 风格段、旋钮）。新增/修订风格 = 改数据文件，不改代码、不重新编译逻辑
（内置数据经 `include_str!` 嵌入，改数据仍需重新构建二进制——这是分发形态，不是逻辑耦合）。

存放两层，仿镜头库先例：

| 层 | 位置 | 规则 |
|---|---|---|
| 内置 | `crates/yanshi-server/data/style-presets.json` | 随程序走的种子库（首批 22 套） |
| 外挂 | 环境变量 `YANSHI_PRESET_DIR` 指向的目录里的 `style-presets.json` | 同 id 覆盖、新 id 追加，来源标 `external`；**变量已设置但文件缺失/解析失败 = 报错**，不许静默当作没设置（仓规：不许静默降级） |

## 3. 两段结构

每套预设 = **修正段 + 风格段**（速查「配方构建八步」的口径：先校正、后风格；
曝光/白平衡这类逐张校正**不进风格段**，只以温和基线出现在修正段里供 agent 逐张再调）。

- 修正段声明 `baseline`：`full`（全量基线）或 `light`（轻校正基线）。
  用 light 的是曾蒙灰纱的批次（小清新、韩系、法式、糖水、田园奶油）——全量基线
  的阴影提量与它们的风格段叠加会蒙灰，这是实测教训，不是审美偏好。
- 修正段**不参与 strength 插值**（校正不是风格的一部分，strength=0 不该把校正也关掉）。
- 风格段是一串操作：`{kind: adjustment|filter, type, params, neutral}`，
  类型与参数格式以仓内既有口径为准（`validate_effect` 的参数表、
  `yanshi-render` 的 `ADJUSTMENT_NAMES` / `FILTER_NAMES`）。

## 4. 数据模型

```jsonc
{
  "id": "hong_kong",                 // 唯一，ASCII 小写下划线
  "name_zh": "港风", "name_en": "Hong Kong",
  "status": "fixed_v2",              // established | fixed_v2 | needs_review，如实标注
  "tags": ["夜景", "街景", "霓虹"],
  "suitable": ["夜街", "招牌"], "caution": ["白天须降强度"],
  "description": "…", "notes": "实测结论 + 硬约束 + unsupported 技法",
  "correction": { "baseline": "full", "ops": [ /* PresetOp */ ] },
  "style_ops": [ /* PresetOp */ ],
  "knobs": [ /* 每套 2 个语义旋钮 */ ]
}
// PresetOp: { "kind": "adjustment"|"filter", "type": "split_toning",
//             "params": { …配方值（strength=1）… },
//             "neutral": { …可插值参数的中性值，键必须是 params 的子集… } }
// Knob: { "id": "teal", "label": "青绿阴影", "description": "…",
//         "min": 0, "max": 1, "default": 0.5,
//         "targets": [ { "op": 2, "param": "shadows", "range": [低端值, 高端值] } ] }
//   op 是风格段下标；range 两端形状与目标参数一致（数字/三元组/对象均可）。
```

**状态口径**：`established` = 已在真实照片上目检成立；`fixed_v2` = v1 翻车后按
修正口径出 v2 并入册；`needs_review` = 未目检——**不许装成成品**，`list_presets`
会如实返回状态，agent 选用前必须先读。

## 5. strength 插值（微调的核心语义）

### 5.1 公式

对风格段每个参数键：

- 声明了中性值：`实际值 = 中性值 + (配方值 − 中性值) × strength`
- 未声明中性值：**固定参数**，strength 不动它（seed、size、radius、threshold、
  channel 等——它们决定「质感的种类」而不是「风格的量」）

插值递归定义：数字线性；数组逐分量（两端长度必须一致，否则展开报错）；
对象逐键（中性对象必须含配方对象的全部键）；`curves` 的 `points` 按此规则
逐点插值——中性点列在数据里写成 `[[x, x], …]`（恒等线），配方点列与中性点列
x 相同、只插 y。布尔/字符串等非数值出现在 neutral 里且两端不等 ⇒ 报错，
不许静默取一端。

### 5.2 中性值表（本系统用到者，逐项对着内核实现核过）

| 效果 | 参数 | 中性值 | 依据（内核缺省/恒等条件） |
|---|---|---|---|
| exposure | ev | 0 | 缺省 0 |
| brightness_contrast | brightness / contrast | 0 / 1 | 缺省 0 / 1 |
| saturation | amount | 1 | 缺省 1，amount 是乘法倍率 |
| vibrance | amount | 0 | amount ≤ 0 直接 return |
| white_balance | temperature / tint | 0 / 0 | 缺省 0 |
| shadows_highlights | shadows / highlights | 0 / 0 | 缺省 0 |
| levels | black / white / gamma | 0 / 1 / 1 | 缺省 0/1/1 |
| hsl | hue / saturation / lightness | 0 / 1 / 0 | 缺省 0/1/0 |
| color_balance | shadows/midtones/highlights | [0,0,0] | 三元组缺省零 |
| split_toning | shadows/highlights / balance | [0,0,0] / 0 | 同上；amount 不写（缺省 1，只插三元组，避免二次缩放） |
| clarity / texture / dehaze（调整版） | amount | 0 | 文档明示 0 = 恒等 |
| selective_color | adjust 各色类 [c,m,y,k] | [0,0,0,0] | 缺省全零 |
| channel_mixer | matrix / constants | 单位阵 / [0,0,0] | 缺省单位阵 |
| film_grain（滤镜） | amount | 0 | amount ≤ 0 直接 return |
| noise（滤镜） | amount | 0 | 同上口径 |
| vignette（滤镜） | strength | 0 | strength ≤ 0 直接 return |
| glow（滤镜） | intensity | 0 | 无强度即无辉光 |
| sharpen（滤镜） | amount | 0 | 同上口径 |
| curves | points | [[x, x], …] 恒等线 | 缺省恒等曲线 |

**没有中性态的效果不许进风格段**：`invert`、`posterize`、`gradient_map`、
`color_lut` 等没有「量」的效果，strength 插值对它们无定义。
需要去色时用 saturation 插值到 0（见黑白预设的 notes）。
注：`black_and_white` 自期 4 起有 `amount` 中性值（0 = 恒等），已不在此列；
`black_white` 仍用 saturation 0.0 是期 5 像素等价 QA 的结论，非不得已。

### 5.3 钳制

展开（插值 + 旋钮）之后、校验之前，全部数值按 `validate_effect` 的范围表钳制
（钳制表在展开器内与校验同口径维护）。钳制只处理浮点尾差与旋钮极端组合，
**不许用钳制掩盖数据错误**：数据本身的配方值越界会在测试里被 `validate_effect` 抓红。

### 5.4 可测性质（测试门守着，必须能变红）

1. **strength=0 恒等**：风格段展开的每个可插值参数恰等于其中性值（展开器单元测试，
   逐套逐参数断言）；像素层：strength=0 整套渲染与只施修正段逐像素一致（集成测试）。
2. **全量可校验**：22 套在 strength ∈ {0, 0.5, 1} 的展开全部过 `validate_effect`。
3. **二次应用不叠加**：见 §7。

## 6. knobs 数据模型

每套 2 个**语义**旋钮（不是把某个滑杆直接暴露）：旋钮给一个标量，归一位置
`t = (值 − min) / (max − min)`，目标参数 = 在 `range` 两端之间按 t 插值
（数字/三元组/对象同 §5.1 的递归规则），在 strength 插值**之后**施加、再钳制。

- 调用方**没给**的旋钮不覆盖——配方值原样生效。`default` 只是建议值，不许拿它
  覆盖配方（否则 strength=1 的「配方」会被缺省旋钮改掉，两处真相）。
- 未知旋钮、非数字、越界值：一律报错，错误文本含**旋钮名与收到的值**
  （仓规：报错要说清缺什么/错在哪）。

## 7. 落成与二次应用（第一核对项的结论）

**核对结论**：原设计曾打算把「二次应用 = 更新实例」挂在 core 的 `CreateStyle` /
`Supersede` 原子上，并把「CreateStyle 载荷到底是不是同一物」列为实现期第一核对项。
已对着源码核清（`crates/yanshi-core/src/fold.rs` 的 CreateStyle 分支）：
**CreateStyle 的载荷是文档样式实体**（name/category/parent_style/brushes/palette/
textures/render_params/seed_policy），是绘画侧的笔触样式，**不是照片调整配方，
两者不是同一物**。⇒ 不硬塞 CreateStyle。

**最终方案**（全部走既有路径，不造新原子）：

- 每笔展开操作有**确定性 object_id**：`preset_<预设id>_<c|s>_<两位序号>_<类型>`。
- 首次应用：走 `add_adjustment` / `add_filter` 的实现（`write_add_effect`）新建。
- 二次应用：同 id 已存在 ⇒ 走 `update_adjustment` / `update_filter` 的实现
  （`write_update_effect`，Supersede 提交）。响应分列 `created_object_ids` /
  `updated_object_ids`，调用方能核对。
- 更新是**合并**语义（patch 覆盖同名键）：预设保证「同一预设同一序号笔的参数键集
  在所有 strength/knobs 组合下恒定」（键集由配方决定，插值与旋钮只改值不增删键），
  ⇒ 对预设托管对象，合并恰等于全量替换该笔参数，不会残留上一轮的键。
  这一条由数据结构保证：neutral 与 knobs.targets 引用的键都在配方键集内（加载时校验）。
- 落成前先**全量校验**：图层存在、每笔过 `validate_effect`、已存在同 id 对象的
  类型与效果名与将落的一致（不一致拒绝覆盖并说清）。任一不过整组不落成。
  提交期失败（并发撞 id 等）口径同 `apply_adjustments`：在错误里说清已落几笔
  的纪律由既有 add/update 路径各自负责，本层不另造事务。
- `include_correction=false` 的二次应用只更新风格段对象；此前落的修正段对象
  不动（不删、不更新），调用方要改修正请整套重 apply 或逐个 update。

## 8. 工具规格（均在 Retouch 组；总数 196 → 200）

| 工具 | 变更性 | 参数 | 返回要点 |
|---|---|---|---|
| `list_presets` | 只读 | `tag?` `subject?` `status?` | 摘要数组 + count；**先读它再选风格** |
| `get_preset` | 只读 | `preset_id` | 完整定义 + strength=1 展开预览 |
| `preview_preset` | 只读 | `preset_id` `strength?` `knobs?` `include_correction?` | 展开清单（object_id/段/序号/类型/实际参数）+ op_count；**不落成、不渲染**；展开结果先过一遍 `validate_effect`，非法在预览期就报 |
| `apply_preset` | 变更 | `preset_id` `layer_id` `strength?` `knobs?` `include_correction?` | 既有变更响应 + `preset_id/strength/created_object_ids/updated_object_ids/op_count` |

设计说明：原设计的「低清快速预览」在本期由 `preview_preset` 的**清单预览**承担
（零渲染成本、确定性）；像素级小图预览列入三期（见 §10），不假装已有。

## 9. Lightroom → Yanshi 换算规则（数据是怎么从速查来的）

速查的数值是 Lightroom/ACR 口径，落 Yanshi 时按下面规则换算，并在每套 notes 里
标注近似与 unsupported。换算只求**方向与量级**对，逐张适配仍是 agent 的事。

| LR 口径 | Yanshi 落法 |
|---|---|
| 曝光 ±EV | `exposure.ev` 直通（Yanshi 是精确 2^ev，量级同阶） |
| 对比滑杆 −100..+100 | `brightness_contrast.contrast = 1 + v/100`（期 3 起内核为显示枢轴口径，映射更贴切；`brightness` 著录值 = 目标显示位移 ×2，因 brightness ±0.5 ⇔ 显示 ±0.25） |
| 饱和滑杆 | `saturation.amount = 1 + v/100` |
| 自然饱和度 | `vibrance.amount ≈ v/50`（Yanshi 量程 0..2，0 为中性） |
| 高光/阴影滑杆 | `shadows_highlights` ÷100（−1..1） |
| 色温偏移（K 差） | `white_balance.temperature ≈ ΔK/3000` 钳到 [−1,1]；正 = 暖（内核：R 增、B 减） |
| 色调（绿↔品红） | `white_balance.tint`，正 = 品红 |
| 曲线点 (输入→输出，0–255) | `curves.points` ÷255 归一后**逐点过 sRGB EOTF 换成线性坐标**（内核曲线直接作用于线性 RGB，不转换量级会被放大，见 §9.1 续；点列须含首尾） |
| 抬黑点 fade（输出 5–15/255） | 曲线首点 y ÷255 后同样过 EOTF 换线性（**只能走曲线**：`levels.black` 是输入黑点、方向相反，写它只会压暗部——2026-10-23 修订，原稿把两者并列是错的） |
| 色阶输入黑点（0–255） | `levels.black` ÷255 后**过 EOTF 换线性**（内核 black 是线性输入黑点位置，不转换有效显示黑点会被放大到 0.15–0.35，见 §9.1 续；white / gamma 跨空间近似不变，直通） |
| 分离色调/色彩分级（H/S 两轮） | `split_toning` 三元组：按目标色的 RGB 偏移方向近似，量级 ≤0.15/分量（速查通则：分级宁小勿大，两轮之和 ≤25 的口径映射为三元组小量）；v2 四套再减半 |
| HSL 逐通道（H/S/L） | **无逐通道 HSL**：全局 `hsl` 只动整体；逐通道意图改用 `selective_color` 的 CMYK 偏移近似，或写入 unsupported（期 3 起 selective 为 PS 差值权重 + sRGB→ProPhoto 往返，偏移量级约旧线性硬扇区口径的 ×1.8，期 3 试点推导，期 5 复核） |
| 校准面板移色 | `channel_mixer` 矩阵近似（赛博朋克），或 unsupported |
| 清晰度/纹理/去朦胧 | 调整版 `clarity` / `texture` / `dehaze` 的 amount = LR 值 ÷100（期 3 起 clarity 为 sRGB Y' 平面 + 中间调掩模 + 亮度比回写，量级同阶；dehaze 正向约 ×1.3，因 γ_t 重映射后有效强度下降，期 3 试点推导，期 5 复核） |
| 颗粒（量/大小/粗糙度） | `film_grain.amount` ≈ 量÷40 钳到 0..1；size 取 2–3；**粗糙度无对应**（unsupported 标注） |
| 数字噪点（CCD） | 滤镜 `noise.amount` 小量 + 关降噪类操作；彩噪的红绿斑块质地无精确对应（notes 标注） |
| 暗角（量） | 滤镜 `vignette.strength` ≈ |v|/100×0.5（期 3 起显示乘算，旧线性强度 → 新显示强度 ≈ ×0.8，故因子 0.6 → 0.5；期 3 试点推导，期 5 复核），radius/softness 给固定常用值 |
| 柔焦/glow | 滤镜 `glow.intensity` 小量；或 clarity/texture 负值（糖水的辉光教训：默认关，见其 notes） |
| 锐化（量/半径） | 滤镜 `sharpen` amount ≈ 量÷80、radius 取整 |

**修正段**依据 LR/PS 调研：镜头校正在导入期（raw_options）完成，不进预设；
修正段只落温和的影调基线（小曝光、阴影/高光回收、微去雾/微清晰），白平衡只以
中性占位出现——逐张的白平衡与曝光收尾不进预设，这是速查「专业与业余预设的分水岭」。

### 9.1 量纲校准注记：`split_toning` 在线性空间施加（2026-10-23 展示册实测）

**结论先行：写预设时 `split_toning` 的三元组不要按显示空间（sRGB）直觉取值。**
渲染管线在线性光下施加调整，该 op 是原始加法偏移（`color + amount × tint`，
权重按线性明度平滑过渡）。同样的偏移量在线性空间被显示 gamma 展开放大，
暗部尤甚；且线性中灰的明度只有约 0.18，「阴影」权重实际罩住阴影加中间调。

实测（`western_cool`，shadows [−0.08, 0.02, 0.14]、amount 缺省 1.0，单 op 隔离）：
按显示明度带统计的实际染色 ΔB 在最暗带达 **+0.334**（参数仅 0.14，约 2.4 倍），
ΔR 在中带达 **−0.244**（参数仅 −0.08）；模型拟合证明显著优于 sRGB 空间加法模型
（MAE 0.023 vs 0.078）。宏观后果：15 套含 split_toning 的预设在真实照片上全部呈
「全图染色罩」（森系青绿罩、欧美清冷青蓝罩、LOMO 青色奶雾），与各自意图不符。

**现行修法（数据侧，不动内核语义）**：15 套的 split_toning 一律补
`amount: 0.35`（展开器中作固定参数，strength 不动它；strength=0 时三元组归零、
恒等不受影响）。0.35 的定值依据：最强三元组（western_cool 0.14）在此值下罩感
消失且冷调意图保留；再大强三元组预设（cinema 0.14、cyberpunk 0.16）有返罩风险，
再小弱三元组预设（morandi 0.03 级）的分色调意图会消失。

**逐预设例外（forest，2026-10-23 二轮隔离实测）**：uniform 0.35 只是起点，
个别预设可按实测再收。forest 在 0.35 下逐 op 隔离（Leica SL 草甸栅栏图）：
split_toning 单独仍有全图青绿染色（平均 ΔG +0.034），且 dehaze −0.08 单独即
产生重灰白雾（平均 ΔLuma +0.068、抬黑压平）——残余雾感是两者叠加。故 forest
定版为逐预设值：**split_toning amount 0.15、dehaze −0.08 → −0.03**（雾气旋钮
仍可逐张加回；预设默认值不再预置重雾）。其余 14 套当时维持 0.35；
2026-10-24 暗场景实测后 forest 再降至 0.05，cinema / hong_kong /
guofeng 三套暗场景预设同步定档，见 §9.1 续三。

**给后来写预设的人**：
- 要「LR 分离色调 15–25 的轻染」效果 ⇒ 三元组量级仍按 §9 取 ≤0.15，但**必须**
  同时给 `amount ≈ 0.3–0.4` 收敛；或等价地三元组直接取目标值的约 1/3 且 amount 留 1。
- 同理警惕一切「加法型」参数在线性空间的放大效应；判断量级时以线性值为准，
  别用显示器上看到的增量反推。
- 内核侧是否应改 split_toning 的施加空间/权重曲线，留待后续评审（会改变所有
  既有调用方的语义，本次不动）。

**续：`curves` 是同根问题（2026-10-23 同日续测，morandi 暗调静物上现形）。**
内核 `curves` 的控制点**直接作用于线性 RGB**（`filter.rs` 的 `monotone_curve`
单调三次插值），而数据里的点是按显示空间 / LR 直觉著录的——与 split_toning
同根，只是形态从「染色罩」变成「奶雾抬黑」：显示 fade 黑点 (0, 0.07) 若直用作
线性坐标点，等于把黑位抬到线性 0.07（编码回显示约 0.29），暗部被整体掀起。
morandi 逐 op 隔离（基线黑位 p2 0.084、明度 std 0.131、均值 0.315）：curves
单独 p2 → **0.311**、std → 0.086【主犯】；全套实测 p2 0.365 与隔离叠加一致。
此前在亮调素材（汉堡港）上不现形，是暗部像素太少把 bug 掩盖了，不是无 bug。

**修法（数据侧，与 split_toning 先例一致）**：style 段含 `curves` 的全部预设
（数它用 `grep -c '"type": "curves"' crates/yanshi-server/data/style-presets.json`），
其 `params.points` 与 `neutral.points` 逐点做 sRGB EOTF 转换
(x, y) → (lin x, lin y)，lin 为 sRGB 解码（c ≤ 0.04045 时 c/12.92，否则
((c+0.055)/1.055)^2.4），结果保留 4 位小数。转换后内核曲线在每个控制点上
精确复现显示空间著录意图，段内由三次插值在线性空间近似。两点配套：
- `neutral.points` 同法转换后仍落在对角线 y = x 上（EOTF 保对角），
  恒等曲线不变、strength = 0 恒等不受影响；
- 以 `points` 为目标的旋钮（`japanese_airy` 的 fade、`cinema` 的 matte）
  其 `range` 两端点列同法转换——旋钮存的也是显示空间曲线值，不转换就会
  在覆盖瞬间把旧量级带回来；转换保单调，方向性单测（旋钮越大首点 y 越高）
  仍成立。

**同根问题三：`levels.black` 是位置型参数（2026-10-24 审计实锤）。**
内核 `levels` 的 black / white 是**线性**输入黑白点位置
（`out = ((v−black)/(white−black))^(1/gamma)`），而 6 套预设的著录值
0.02–0.10 是按显示空间输入黑点直觉落的（§9 换算表此前没有 levels 一行，
是文档缺口，已补）。有效显示黑点 = enc(black)：

| 预设 | 著录 black | 有效显示黑点（转换前） | 转换后 black（线性） |
|---|---|---|---|
| cinema | 0.02 | 0.15 | 0.0015 |
| ins_minimal | 0.03 | 0.19 | 0.0023 |
| hdr_landscape | 0.03 | 0.19 | 0.0023 |
| japanese_airy | 0.04 | 0.22 | 0.0031 |
| sweet | 0.06 | 0.27 | 0.0049 |
| korean | 0.10 | 0.35 | 0.01 |

即暗部压黑的位置是著录意图的约 3.5–7 倍（korean 最重：显示 0.35 以下
全压成黑）。修法与 curves 同：black_kernel = lin(black_著录）逐套转换。
连带核对：white（0.99 的有效显示白点 0.9956）与 gamma（幂运算跨空间
近似不变，仅深影分段区有二阶偏差）清白，不动；neutral 的 black 恒为 0，
strength = 0 恒等不受影响。本条与旋钮归属问题（22290c4）相互独立：
那次确认的是 black 的**方向**（输入侧、与抬升相反），这次是同一参数的
**位置量纲**。注意 japanese_airy 是 established，其现行观感包含这 0.22
的有效黑点——转换后暗部会明显变开，须以重渲目检定版（同 morandi
金丝雀口径）；本册另三行 cinema / ins_minimal / hdr_landscape 赶在
渲染前落数据，省整行重渲。

**`brightness_contrast` 只点名、本次不动数据（2026-10-24 审计数据补全）**：
它是无枢轴线性运算（`out = clamp(v·contrast + brightness)`，对比绕
黑点 0 缩放、亮度为线性加法），与 §9 的著录直觉（显示空间绕 0.5 枢轴
对比 + 显示加法亮度）**几何与空间双错位**。审计解析对照（显示探针值）：
黑端 +brightness 的线性加法实效为著录值的 **5–6 倍**（著录 0.03 →
黑端抬到显示 0.19 量级；morandi 隔离实测 bc 单独 p2 → 0.178 与此吻合）；
contrast > 1 的一组（ccd 1.30 / y2k 1.38 / western_cool 1.20 /
dark_moody 1.28 / hong_kong 1.28）内核并不压黑，代之以高光硬剪切，
剪切点 = 显示 enc(1/c) ≈ **0.87–0.92**，即显示 0.87 以上并入纯白；
中灰偏差 −0.09..+0.10 显示单位。**处置：不动已验收行的数据**——
western_cool / lomo / forest / morandi 的现行观感已经包含此行为，
改数据等于推翻验收；needs_review 的 10 套将来逐套评审时，按上述
真实语义复核各自的 (brightness, contrast) 对；给内核加枢轴 / 显示
空间模式是内核语义变更，留 gradetwo 评审。

**`color_balance`（期 3 A2 已修复）**：原为线性空间分档加法偏移、无
amount 的潜伏项；期 3 已原地改为显示口径（Y' 分档 + 显示空间施加 +
`amount` 缺省 1.0），可按显示直觉正常著录（偏移刻度 ±0.25 显示单位）。

修正段只有 exposure / shadows_highlights / dehaze / clarity，不含
curves、levels 与 brightness_contrast，已渲的原图 / 修正图不受影响，
历次量纲修复重渲都只需补 style 格。

**续二：`curves` 稀疏点重采样 + 两预设 bc 校准（2026-10-24 展示册 QA）。**
展示册逐图 QA 打回 japanese_airy / ins_minimal 两行（同一亮调城堡
素材）：奶雾罩、两行趋同。处置分两半，都有实测：
- **curves 重采样**：稀疏控制点（5 点）经 EOTF 转换后，内核单调三次
  在线性空间的段内形状与显示空间著录意图可有偏离；稳妥做法是转换
  **之前**先在显示空间把曲线 PCHIP 重采样为 17 点再逐点转换（本次
  对 japanese_airy 的 params / neutral / fade 旋钮两端一致执行）。
  本例实测稀疏与重采样的成图度量差极小（黑位 p2 0.292 vs 0.294）——
  重采样是保形状的保险，不是本行奶雾的主因；后来者遇到更弯的曲线
  （点更稀、斜率更大）时偏离会放大，著录新曲线一律先重采样。
- **奶雾主因是抬升栈叠加**：两预设同为「exposure +0.4 档 +
  brightness_contrast brightness +0.02 + 暗部提亮」主导，bc 的无枢轴
  亮度在线性空间对黑端的实效放大（§9.1 前文）使 +0.02 在亮调素材上
  再掀一层。校准：**仅这两套**的 style 段 brightness 改 0（contrast
  不动）；airy 全套黑位 p2 0.412 → 0.362、明度 std 0.106 → 0.131，
  ins std → 0.152，两行观感重新拉开（柔亮哑光 vs 干净明快）。这不
  推翻「bc 数据不动」的既定口径——那条针对已验收行的观感背书；
  被打回的行按实测逐行校准，本就是展示册的迭代规矩。

**续三：`split_toning` 的 amount 必须随场景亮度缩放（2026-10-24）。**
0.35 是在亮场景上定的全局值；暗场景上同一 amount 的暗带染色会被
QA 判为污染（内核的阴影权重窗在线性亮度上取，暗场景里大半画面
落在窗内）。逐预设按 split_toning 单独隔离的暗带染色实测（最暗四
分位平均 B−R，相对修正图的变化量 Δ）定档：

| 预设 | 场景 | amount 旧 → 新 | Δ（新档 split 单独） |
|---|---|---|---|
| forest | 暗（欠曝峡谷） | 0.15 → **0.05** | +0.035 |
| cinema | 暗（夜街） | 0.35 → **0.06** | +0.083 |
| hong_kong | 暗（夜街） | 0.35 → **0.08** | +0.087 |
| guofeng | 暗（夜庙） | 0.35 → **0.12** | +0.050 |

旧档全套成图的暗带 Δ 分别为 +0.060 / +0.273 / +0.205 / +0.112。
规律：著录新预设时先问素材亮度——**亮场景可用 0.35，暗场景按三元
组强度降到 0.05–0.12**，且以暗带 Δ ≲ +0.09 为验收线；三元组本身
（色相方向）不动，变的只是 amount。

**续四：抬升栈要有回锚、s_h 双向量级要有预算（2026-10-24 QA 二轮）。**
retro_film（暮色剪影）与 hdr_landscape（大光比风光）被逐图 QA 打回，
病因都是「只抬不锚」：
- retro_film 旧配方五个 op 全部在抬黑（逐 op 隔离，黑位 p2 增量：
  s_h +0.063、curves +0.048、bc +0.030、exposure +0.018、split 色
  偏另计），叠加后 p2 0.162 → 0.293、剪影全浮，split 暖三元组又把
  抬起的黑染成棕（暗带 B−R Δ −0.111）。修法：curves 抬黑偏差减半、
  s_h shadows 0.2 → 0.05、bc brightness → 0、exposure 0.3 → 0.1、
  split amount 按续三降到 0.12，并在 style 段**末尾追加 levels 黑
  位回锚**（black 0.02 线性）——新配方 p2 0.118、std 0.125（反超修
  正图），剪影沉回、红霞透出。著录规范：**fade/matte 类预设必须自带
  黑位回锚或等效的下压项，只抬不锚的配方在暗场景必翻**。
- hdr_landscape 的白雾罩主犯是 shadows_highlights 单 op：旧值
  {+0.55, −0.55} 单独即 p2 0.346 / std 0.094（修正图 0.218 / 0.164）；
  全段除 s_h 反而健康（std 0.174）。修法：s_h 收至 {+0.22, −0.3}
  （lift 旋钮 range 同步平移为 [0.04, 0.4]，中点对齐新配方值），
  新配方 p2 0.268 / std 0.142。著录规范：**s_h 双向同时大开（|值|
  ≳ 0.5）等于亲手把动态范围压平，HDR 类预设的 s_h 预算以 ±0.3
  为限，扩张动态范围靠 levels/curves 的端点而非 s_h 的中段抬升**。

**续五：A 族语义原地变更 + 22 套重著录（2026-10-24，期 3 甲案）。**
§9.1 前文「`brightness_contrast` 只点名、本次不动数据」一条作废——
期 3 已按 Wang 甲案裁定（原地改语义、一步到位、该重录的就重录）
改了内核并全量重著录，存量文档回放会变样（接受的代价，发布说明
明示）。内核定版（`filter.rs` / `color.rs`，提交序列见期 3 包说明）：

- **通则**：显示空间 ops 一律扩展（保符号）sRGB 传递，编码前不钳
  [0,1]；HDR 编码 >1.7 不钳、负值镜像；[0,1] 内与旧钳制版逐位一致。
- **A1 `brightness_contrast`**：编码 → brightness 平移（±0.5 ⇔ 显示
  ±0.25）→ contrast 绕显示 0.5 枢轴 → 线性亮度比回写（灰像素与逐
  通道显示映射恰等价、彩色标量保色相、HDR 成立）；neutral 快速返回。
- **A2 `color_balance`**：权重改读显示 Y'（二次式形状保留）、偏移按
  ±0.25 显示刻度 × amount 加在显示值上、扩展解码；新增 `amount`
 （分发缺省 1.0，工具层键表已启用未知键拒绝）。
- **A3 `selective_color`**：全面重写——分类用 PS 差值权重法（排序通道
  权重，禁用色相角硬切；明度三档以显示 0.25/0.75 为锚；色度门控
  span/0.08 交叉淡化），为对齐 LR 来源数据加内部 sRGB→ProPhoto
  矩阵往返（不做强度偏大约 35–50%），CMYK 剩余量（room）模型施加、
  本 op 专用不钳输出；零有效偏移先行短路逐位恒等。
- **A4-1 `clarity`**（仅 `guided_detail_adjust`）：Y' 平面（逐通道扩展
  编码加权和、不钳）+ 掩模中心回显示中灰 + 线性亮度比回写；平坦区
  逐位不动（引导滤波常数平面浮点残差零守卫）；半径/epsilon 数值不变。
- **A4-2 `vignette`**：显示空间乘算 `out_d = d·(1 − strength·smoothstep(t))`
  （逐通道扩展编解码、HDR 不钳；几何不变；乘子 = 1 中心区逐位不动）。
- **A4-3 `dehaze`**：保留线性 DCP + 四修正（大气光色度钳 [0.95,1.05]、
  γ_t=0.5 重映射 + t0 动态底 0.20–0.25、β=0.35 亮度权重混合、前景
  中性再锚定增益钳 [0.90,1.10]）。
- **不动项**：`curves` 内核不改（期 0 的 EOTF 换算线性坐标值继续有效，
  本节 curves / levels.black 的转换规则全部沿用）；`split_toning`
  内核不改（本节 split 量纲注记与 amount 随场景缩放规则全部沿用；
  设计 §9.2 第 1 项未决，待 Wang 确认是否后续纳入）；`shadows_highlights`
  不动；滤镜版 `clarity()` 不在本期。

**重著录迁移通则（试点验证，3 套试点 + 12 展示行逐行渲染判定）**：
bc 数值大体保留（仅过暗者下调，如 lomo 1.25→1.15）；vignette 强值
约 ×0.8（lomo 0.38→0.30）；selective 偏移 ×~1.8–1.9（权重稀释 ×
ProPhoto 阻尼）；brightness 旧线性值 ×1.7（中调等效解析推导，期 5
复核 morandi/guofeng 两行）；dehaze 正向 ×~1.35（hdr 0.22→0.30）；
split_toning 仅 STYLE_SPEC 点名弱者上调（cinema 0.06→0.13、forest
0.05→0.12、hong_kong 0.08→0.16、guofeng 0.12→0.14、retro_film
0.12→0.10 暗场暖染收一档）；其余 op 数值沿用。册外 10 套按同一通则
定值（无展示行渲染，期 5 三格重渲时复核）。试点与批量逐套改动见
期 3 包说明；对照渲染（旧内核成图 vs 新内核成图，12 行）随包。

**续六：期 5 复核与 split_toning 内核落地（2026-10-24）。**
- **split_toning 内核改 Q1-c**：通道先扩展 sRGB 编码、权重在显示 Y'
  （Rec709 加权和）上取（mid ± 0.25 smoothstep，mid = 0.5 + balance × 0.25）、
  着色在显示空间做（d + amount × tint）、解码回线性后按线性亮度比回写
  （黑像素仍为黑）。实测门（灰阶 10 点 + 5 色块 × 7 套，显示空间 Δ）：
  7 套现行参数 mean / max 全部过门（mean ≥ 0.004 或 max ≥ 0.01），故落地；
  变体 A（显示空间）与变体 B（线性着色）互差亦大，选 A 的依据是 A 族
  显示空间通则而非数值大小。设计 §9.2 第 1 项至此关闭。
  14 套受影响预设的数据回调经 12 展示行 + 册外行目检否决：0.35 量组
  （western_cool、lomo）未过强，其余行观感皆正，数据不动。
- **brightness ×1.7 复核**：morandi / guofeng 两行中调皆落舒适带，无需改；
  续五迁移通则中的「期 5 复核」句至此兑现。
- **§9 新因子复核**：vignette ×0.5、dehaze ×1.3 皆不应用（行目检已接受现行值，
  期 3 已兑现过同量级放大，再乘即超调）；selective ×1.8 不应用（期 3
  重著录已按 ×~1.8–1.9 兑现，再乘为二次叠加）。

## 10. 测试门与三期划分

**测试门**（`crates/yanshi-server/tests/style_preset_package.rs` + 展开器内单元测试）：

1. 22 套全部展开并过 `validate_effect`，在合成文档上 apply 成功且落成对象数 = 展开笔数，
   strength=1 对测试图非恒等；
2. strength=0 恒等（参数层逐参数 = 中性值；像素层与只施修正段逐像素一致）；
3. 二次应用改 strength：object_id 集合不变、对象总数不增、像素变；
4. 未知预设/未知旋钮/strength 越界/旋钮越界：报错且文本含具体名字/值，且零落成；
5. knob 覆盖只改其目标参数（展开结果对比）。

**三期划分**：

- 一期（已完成）：数据模型 + 内置 22 套 + 展开器（strength/knobs/钳制）+
  list/get/preview 三个只读工具 + 测试门 2/4/5 的展开层部分。
- 二期（已完成主体）：`apply_preset` 落成与二次更新、外挂目录合并、
  测试门 1/3 与像素层恒等。
- 三期（未做，如实标注）：像素级低清快速预览（固定长边渲染的 preview_preset 扩展）、
  agent 实验配方从外挂升内置的流程化工具、黑白 B&W Mix 的 amount 化（需内核改动）、
  逐通道 HSL（需内核新调整类型）。

## 11. 原 5 个待定问题（默认建议，已采用，可改）

| # | 问题 | 默认（已采用） |
|---|---|---|
| 1 | 预置放内置还是全走外挂 | **内置种子 + 外挂目录并存**（§2）：内置保证开箱可用，外挂保证不改代码加新风格 |
| 2 | strength 线性插值够不够，还是要分段控制 | **先线性**：语义简单、性质可测（§5.4）；分段控制等有实测证据说线性不够再议 |
| 3 | 快速预览的尺寸/速度目标 | **一期用清单预览（零渲染）**；像素预览列三期，目标到时再定（不预先承诺数字） |
| 4 | 二次应用的语义 | **更新实例不叠加**：确定性 id + 既有 update 路径（§7）；CreateStyle 经核对不是同一物，不用 |
| 5 | 未目检预设可否进内置库 | **可进，但 status 必须标 `needs_review`**，且 list/get 如实返回；升 `established` 必须有真实照片目检结论 |

## 12. 实现落点（与本文的对账）

| 设计 | 实现 |
|---|---|
| 数据 | `crates/yanshi-server/data/style-presets.json`（22 套） |
| 展开器 + 工具处理器 | `crates/yanshi-server/src/presets.rs`（tools 的子模块，复用 `validate_effect` 与 add/update 实现，不另起校验口径） |
| 注册/分发/示例 | `tools_registry.rs` +4（Retouch 组，总数 200）、`tools.rs` 分发 +4、`tools_examples.rs` +4 |
| 文档 | `docs/tools.md` 工具总数 200 + Style presets 一节 |
| 测试 | `crates/yanshi-server/tests/style_preset_package.rs` + `presets.rs` 内单元测试 |

**与原设计不一致处**：① 二次应用不挂 CreateStyle——Supersede
仍是更新的提交原子（经 update 工具），但入口是既有 update 工具而非 CreateStyle，
原因见 §7 核对结论；② 「低清快速预览」一期降级为清单预览，像素版列三期（§8）；
③ 外挂文件名与内置同名同格式（原设计只说外挂目录），合并规则见 §2。

---

## 附录：22 套预设目录

图例：状态按 2026-10-23 实测与找回的截图证据如实标注。修正段基线：
full = 全量基线、light = 轻校正基线（配方均以 strength=1 记，参数细节以
`style-presets.json` 为单一事实来源，本附录记要点与旋钮）。
**期 3 注（2026-10-24）**：22 套数据已按新显示空间语义全量重著录
（§9.1 续五）；本附录各条的配方要点文字是期 0–期 2 的著录，数值以
JSON 为准，status 字段未随重著录改动（评审工作流状态归 gradetwo）。

### 1. 小清新 `japanese_airy` — established（已验证）
- 题材：晴日户外人像、海边、白建筑、校园；慎：夜景、肤色易被青污染。
- 实测：以「JP LIGHT FINAL」在三素材（圣托里尼、罗弗敦雾天变体 JP CLEAN、树荫人像）
  深挖验证；此前蒙灰纱，换轻校正基线。
- 配方要点（light）：曝光 +0.45、低对比、抬黑曲线、高光回收、微暖白平衡、柔 texture。
- 旋钮：`fade` 空气感（curves.points 首点抬升）、`softness` 柔和度（texture.amount）。
- unsupported：逐通道 HSL 用全局 + selective 近似；眼睛局部锐化属蒙版活。

### 2. 模拟 CCD `ccd` — established（已验证）
- 题材：直闪夜拍、日常抓拍、聚会；慎：大光比风光。
- 实测：以「CCD FINAL」在同三素材深挖验证（与小清新是当时深挖的两套）。
- 配方要点（full）：硬 S 曲线（两端不抬不压）、对比 1.3、冷偏白平衡、noise 数字噪点、暗角。
- 旋钮：`noise` 数字噪点、`cold` 冷闪偏色。
- unsupported：直闪光效（拍摄端）、真实色差（导入期处理）、PS 高光 glow 复制层法。

### 3. 日系胶片 `japanese_film` — needs_review
- 题材：阴天、窗光、校园；慎：强顺光金色时刻。清淡略青绿（Fuji 400H 系）。
- 旋钮：`fade` 褪色（阴影青绿）、`grain` 颗粒。unsupported：颗粒粗糙度无对应参数。

### 4. 胶片复古 `retro_film` — fixed_v2
- 题材：婚礼、金色时刻人像；慎：神社夜景（天空仍偏棕橙，未完全修透）、钨丝灯未校白。
- 实测：v2 暖度减半，白天定版；夜景对暖偏置格外敏感。曝光键是 `ev`（曾写错，已修正）。
- 旋钮：`warmth` 暖度、`grain` 颗粒。

### 5. 港风 `hong_kong` — fixed_v2
- 题材：夜街、招牌、茶餐厅、地铁；慎：白天须降强度、肤色发绿是头号失败。
- 实测：v1 全局色偏开满（与国风同病）；v2 色偏减半、selective_color 保红、要真黑不 fade。
- 旋钮：`neon` 霓虹浓艳（saturation）、`teal` 青绿阴影（split_toning.shadows）。

### 6. 韩系 `korean` — needs_review
- 题材：棚拍、探店、情侣/闺蜜；慎：硬光正午、男性/街头。奶油粉彩、柔焦敢下手。
- 实测：蒙灰纱批次，换轻校正基线重渲，待最终目检。基线 light。
- 旋钮：`cream` 奶油柔焦（texture）、`bright` 明亮（exposure）。

### 7. 欧美清冷 `western_cool` — established
- 题材：建筑、汽车、健身、男性人像、雪景、港口；慎：柔情题材、肤色变青。
- 实测：本批最好——日落暖天配冷蓝阴影对撞干净；汉堡港蓝灰高级感成立。
  （研究名「欧美冷调」，同一套。）
- 旋钮：`cold` 冷感（白平衡）、`hard` 硬朗（clarity）。

### 8. 电影感 `cinema` — fixed_v2
- 题材：旅行、街头、叙事人像、夜景灯火；慎：无青/橙素材硬套、分级过饱和卡通化。
- 实测：v1 全局泼青（威尼斯暖灯被灭、罗弗敦海面成青汤）；v2 青只压阴影、橙留亮部
  （只用 split_toning 分段，全局饱和不推高）。
- 旋钮：`teal_orange` 青橙对峙、`matte` 哑光（curves.points 首点抬升）。

### 9. 法式 `french` — needs_review
- 题材：田园、家居、窗光人像、婚礼细节；慎：都市夜景、双暖变黄墙。琥珀奶油橄榄绿。
- 实测：轻校正基线重渲，待目检。基线 light。旋钮：`warmth`、`grain`。

### 10. 森系 `forest` — established
- 题材：森林、溪流、雾日；慎：城市建筑脏绿、林下人像肤色。
- 实测：台湾苔藓森林合格（绿更润更深、质感在、无伪影）；**雾林素材已剔除**
  （九成白雾，任何风格都测不出，降级为解码验证，不进展示与评测）。
- 配方要点：压曝光造林下暗、绿压暗不提亮（selective greens/yellows）、负 dehaze 留雾。
- 旋钮：`depth` 幽深（exposure）、`mist` 雾气（dehaze 负值）。

### 11. 莫兰迪 `morandi` — established
- 题材：静物、极简室内、服装平铺、产品、港口建筑；慎：运动/广告/夜景。
- 实测：汉堡港教科书式（灰米、低饱和、哑光）。
- 配方要点：全色一起降饱和、明度向中高靠、曲线两端灰化。旋钮：`gray` 灰度、`matte` 哑光。

### 12. 暗黑情绪 `dark_moody` — needs_review
- 题材：黄昏/阴天人像、咖啡馆、静物、男性肖像；慎：明快题材、左端撞墙丢层次。
- 旋钮：`dark` 压暗、`vignette` 暗角。

### 13. 黑白 `black_white` — established（已验证）
- 题材：建筑、街头、纹理人像、形态强的风光；慎：靠色彩叙事的题材。
- 配方要点：saturation 插值到 0 去色 + 强 S + clarity + 银盐颗粒。
- 期 5 目检（d700 桌游货架真照片三格行）：全幅真黑白、灰阶分离优秀；
  saturation 0.0 与真 `black_and_white` op（amount 1.0、缺省 Rec709 权重）
  双配方对照像素等价（mean_abs_diff 0.0001），故保留 saturation 实现。
  逐色 B&W Mix 权重仍为未来选项。

### 14. 赛博朋克 `cyberpunk` — needs_review
- 题材：雨夜霓虹（原片须真有光源）；慎：日光硬套 = 紫天毒绿。
- 实测：chatlog 点名未目检，须补目检才能升库内 established。
- 配方要点：channel_mixer 近似校准移色、分级青/品红、glow 造霓虹晕。
- 旋钮：`neon_glow` 霓虹晕、`magenta` 品红。

### 15. Y2K `y2k` — needs_review
- 题材：派对、镜面自拍、夜出合影；慎：严肃/商业场合、做成暖胶片（黑是硬的）。
- 配方要点：CCD 链再推一档——高对比、高饱和冷色、硬黑边、noise。
- unsupported：日期戳与 JPEG 压缩感属导出层，不在预设内。旋钮：`pop`、`noise`。

### 16. 国风 `guofeng` — fixed_v2
- 题材：汉服、古建、园林、茶空间、神社/寺庙；慎：现代玻璃建筑、红绿同拉满变影楼。
- 实测：v1 青绿过头、神社朱红大门被压发闷；v2 硬约束**朱红必须保住**
  （selective reds 减青保红，青只进阴影）。研究名「古风/国潮」，同一套。
- 旋钮：`daiqing` 黛青、`zhuhong` 朱红（selective_color.adjust）。

### 17. Ins 简约 `ins_minimal` — established
- 题材：探店、好物、家居、留白构图、港口；慎：杂乱多色场景、白色过曝、暗部压太狠。
- 实测：一半成立——汉堡港干净简约成立；圣托里尼暗部压太狠（悬崖成一团黑），
  本配方阴影只轻提不压以避开该失败模式。研究名「ins 极简」，同一套。
- 旋钮：`white` 通透白（exposure）、`clean` 低饱和（saturation）。

### 18. 糖水 `sweet` — needs_review
- 题材：少女写真、闺蜜、樱花/花海人像；慎：男性、纪实、老人。
- 实测：轻校正基线重渲；**去辉光对照实验未跑完**（斜切边缘是否辉光所致未定），
  故本配方 glow 默认关、只作旋钮可选项并注明开启须目检边缘。基线 light。
- 旋钮：`glow` 辉光（默认 0）、`sweet` 甜亮（exposure）。

### 19. 商业 `commercial` — needs_review
- 题材：产品、广告、形象照；慎：情绪风格带进交付会被退片（黑位不准/颗粒=噪点）。
- 实测：未目检。配方要点：风格即没有风格——只纠偏、材质与锐利小量。
- unsupported：美妆频率分离/中性灰 D&B 是 PS 像素级技法，无对应工具，不进预设。
- 旋钮：`material` 材质感（texture）、`sharp` 锐利（sharpen）。

### 20. HDR 风光 `hdr_landscape` — fixed_v2
- 题材：大光比风光、建筑；慎：雾景、人像禁用此拉法、过量出光晕。
- 实测：v1 两张都糊（日落阴影硬抬成绿雾、天空橙溢出；罗弗敦灰泥 + 左边缘可疑青斑待验）；
  v2 拉量约减半、保真黑（levels），青只进阴影、暖只进高光。
  注：找回的修正对照截图显示罗弗敦原片高光粉边/青斑在**修正段**处理后消失，
  与「HDR v1 的青斑」是两回事，勿混。
- unsupported：真 HDR 的包围曝光合并无对应工具，此为单张伪 HDR。
- 旋钮：`drama` 戏剧化（clarity）、`lift` 暗部拉量。

### 21. LOMO `lomo` — established
- 题材：生活抓拍、聚会、旅行手帐；慎：色彩准确场合。
- 实测：成立——下坡路糖果色与饱和就是 LOMO 本人；圣托里尼暗角偏重、吃左下细节，
  但在风格本性范围内（旋钮可收）。
- 配方要点：高饱和、高对比、重暗角、分级染色、颗粒。
- unsupported：宝丽来 milky fade 是另一支（速查同卡），未混入本预设；
  手动桶形畸变属逐张 lens 操作，不进预设。旋钮：`vignette` 暗角、`candy` 糖果色。

### 22. 田园奶油 `pastoral_cream` — needs_review
- 题材：田野、野餐、亲子、花市、田园婚纱；慎：都市/夜景、黄提过头白衣泛奶黄。
- 实测：待补图目检。基线 light（与法式/韩系同族，避灰纱）。
- 配方要点：暖白平衡、麦色提亮靠曲线、高光奶油分级、细颗粒。
- 旋钮：`cream` 奶油暖（白平衡）、`glow_soft` 柔光（clarity 负值）。

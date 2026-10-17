# 引擎能力 vs MCP 暴露 —— 审计表（第 1000+ 轮，**只写读到的** ✓）

**审计口径**：`crates/yanshi-render/src/{object,filter,blend}.rs`、`crates/yanshi-wasm/src/brush.rs`、
`crates/yanshi-server/src/tools.rs`（19,019 行）、`web/brushes/`（199 支 `.myb`）、`web/mediums/`（6 个 wasm）、
**`hokusai-core-0.3.0/src/setting.rs` 的 `define_settings!` 宏**（**这是笔刷动力学的权威清单 ✓**）。

## 一、引擎能力面（**实测 ✓**）

| 维度 | 引擎实际有 | 证据 |
|---|---|---|
| **图元** | **9 种**：`Stroke`／`Shape`／`Text`／`Adjustment`／`Filter`／`RasterPatch`／`Retouch`／`Liquify`／`Unsupported` | `object.rs:25` |
| **滤镜** | **14 个名字**（`FILTER_NAMES: [&str; 14]`）：invert／levels／exposure／curves／posterize／saturation／sharpen／noise／vignette／glow／clarity／dehaze／**relief**（**别名 `emboss`、`impasto` 都指向它**） | `filter.rs:1170`、`:147` |
| **调整** | **12 个名字**（`ADJUSTMENT_NAMES: [&str; 12]`） | `filter.rs:1154` |
| **混合模式** | **11 个**（`BlendMode::NAMES: [&'static str; 11]`） | `blend.rs:58` |
| **修图（Retouch）** | **4 种**：`clone_stamp`／`heal`／`smudge`／`erase`（**其余进 `unsupported` ✓**） | `render.rs:3745-3760` |
| **液化（Liquify）** | **3 种**：`push`／`twirl`／`pinch`（负强度＝膨胀 ✓） | `object.rs:127` |
| **形状（Shape）** | 矩形／椭圆／多边形 | `tools.rs:1682`（`draw_shape` summary） |
| **介质（medium）** | **6 个 wasm**：oil／watercolor／marker／pencil／pixel／**example-dab** | `web/mediums/` |
| **笔刷库** | **199 支 `.myb`**（MyPaint） | `web/brushes/` |
| **★ 笔刷动力学** | **63 项**（`hokusai-core::BrushSetting` 全表 ✓） | `hokusai-core-0.3.0/src/setting.rs:46` |

## 二、MCP 暴露面（**实测 ✓**）

| 项 | 实测 |
|---|---|
| **工具数** | **151 个** |
| **滤镜/调整可达** | **14/14 ＋ 12/12 ✓**（`validate_effect` 用 `kind.supported()` ⇒ `FILTER_NAMES`／`ADJUSTMENT_NAMES` ✓） |
| **混合模式可达** | **10/10 ✓**（`set_layer_blend` 用 `BlendMode::NAMES` ✓，注释还记着"本文件此前硬编码 7 个 ✗"已修 ✓） |
| **修图可达** | **4/4 ✓**（`patch`／`smudge`／`heal_stamp`／`clone_stamp` ＋ `erase` ✓） |
| **液化可达** | **3/3 ✓**（`liquify_push`／`liquify_twirl`／`liquify_pinch` ✓） |
| **笔刷旋钮** | **只有 4 个**：`size`／`opacity`／`hardness`／`color`（＋整份 `myb` 文件 ＋ `set_brush_dynamics` 写倍率曲线） | `yanshi-wasm/src/brush.rs` 的 `PaintRequest` |

## 三、★★ 核心缺口（**63 项动力学里 59 项不可逐项控 ✗**）

**∴ 引擎实现了 63 项 ✓；MCP 只能控 4 项（size／opacity／hardness／color ✓）⇒ 59 项只能靠预设 `.myb` 间接影响 ✗**
（**∴ 而`.myb` 只有 199 支固定预设 ⇒ 组合空间被锁死 ✓**）

### 3.1 「像真人」最关键的四组（**全部不可控** ✗）

| 组 | 项（`.myb` 规范名） | 为什么重要 |
|---|---|---|
| **A 笔速** | `speed1_gamma`／`speed2_gamma`／`speed1_slowness`／`speed2_slowness`／`offset_by_speed`／`offset_by_speed_slowness` | **慢笔多墨、快笔飞白** —— 真人最核心的特征 |
| **B 手抖与笔势** | `slow_tracking`／`slow_tracking_per_dab`／`tracking_noise`／`direction_filter` | **笔迹不会数学般笔直** —— 真人手不稳 |
| **C 一笔之内变色** | `change_color_h`／`change_color_l`／`change_color_v`／`change_color_hsl_s`／`change_color_hsv_s`／`colorize`／`restore_color` | **蘸一次颜料画出的色差** —— 油画感的关键 |
| **D 厚涂/涂抹** | `smudge`／`smudge_length`／`smudge_length_log`／`smudge_radius_log`／`smudge_transparency` | **推颜料** —— 油画笔触的厚度 |

### 3.2 其余不可控项（**都有引擎实现 ✓**）

`opaque_multiply`／`opaque_linearize`／`radius_by_random`／`radius_logarithmic`／`anti_aliasing`／
`dabs_per_basic_radius`／`dabs_per_actual_radius`／**`dabs_per_second`**／`offset_by_random`／
`elliptical_dab_ratio`／`elliptical_dab_angle`／`lock_alpha`／`snap_to_pixel`／`pressure_gain_log`／
`posterize`／`posterize_num`／`paint_mode`／`stroke_threshold`／`stroke_duration_logarithmic`／
`stroke_holdtime`／`custom_input`／`custom_input_slowness`／`offset_x`／`offset_y`／`offset_angle`／
`offset_angle_asc`／`offset_angle_view`／`offset_angle_2`／`offset_angle_2_asc`／`offset_angle_2_view`／
`offset_angle_adj`／`offset_multiplier`／`gridmap_scale`／`gridmap_scale_x`／`gridmap_scale_y`／
`color_h`／`color_s`／`color_v`／`eraser`

## 四、★ 另两处**行为层**缺口（不是"没暴露"，是"结构上做不到"）

| # | 缺口 | 证据 |
|---|---|---|
| **1** | **画线按距离固定细分**：`divisions = ceil(distance/2)`（clamp 1..4096），`STEP_SECONDS = 0.01` **写死** ⇒ **`dabs_per_second`（时间基间距）拿不到真实笔速** ✗ | `yanshi-wasm/src/brush.rs` |
| **2** | **`brush.rs` 里读的 5 个 `BrushSetting` 只在 `YANSHI_OPEN_TIMING` 打开的诊断分支里** ✗ ⇒ **∴ 它们不是渲染路径的输入 ✓**（**∴ 真实输入全在 `hokusai::BrushState` 内部 ✓**） | 同上 |

## 五、结论与优先级（**"让 agent 更像真人"**）

| 优先级 | 动作 | 理由 |
|---|---|---|
| **P0** | **`get_brush_settings` / `set_brush_setting`**（读写 63 项） | 一处改动打开 **59 项**；A／B／C／D 四组全受益 |
| **P1** | **让 `dabs_per_second` 真的生效**（把固定 `STEP_SECONDS` 换成按真实时间推进） | 唯一"结构上做不到"的一项，**直接决定笔速感** |
| **P2** | **把 A／B／C／D 四组做成**语义化参数**（不是裸 63 项） | 让 agent 不必理解 MyPaint 细节：`speed_gain`／`hand_jitter`／`color_variance`／`smudge_amount` |
| **P3** | 枚举化 `medium`（并把 `example-dab` 标为示例） | 小改动，防误用 |
| **P4** | 导出 `list_brush_settings` / `list_retouch_kinds` / `list_liquify_kinds` / `list_shape_kinds` | 让 agent **自己发现**能力（**现在只能读散文 ✗**） |

## 六、本轮已核实的"没有缺口"的项（**避免重复劳动 ✓**）

**滤镜 14/14 ✓｜调整 12/12 ✓｜混合模式 10/10 ✓｜修图 4/4 ✓｜液化 3/3 ✓｜形状 3/3 ✓**
⇒ **∴ 真正该动的是**笔刷动力学（59 项）＋ 时间基间距**✗**

## 七、P0 落地与证据（第 1000+ 轮 ✓）

### 7.1 新增两个工具 ✓

| 工具 | 作用 |
|---|---|
| **`get_brush_settings { brush }`** | 返回 `{ count, settings: { "<cname>": { base_value } }, unknown }`；**63 项由 `BrushSetting::ALL` 逐项遍历** ✓（不硬编码 ✗）；`unknown` ＝ 原始 JSON 里有、而 63 项里没有的键 ⇒ **引擎会忽略的设置** ✓ |
| **`set_brush_setting { brush, setting, value }`** | 只写 `settings[setting].base_value` ✓；名字用 **`BrushSetting::from_cname`** 校验 ✓；落盘沿用 `set_brush_dynamics` 的手法 ✓（临时文件 ⇒ `import_asset(overwrite:true)`） |

**工具数 151 → 153** ✓（`95ecb484` ✓）。

### 7.2 判据 ✓（`scripts/tool-brush-settings.mjs`，7 条断言）

本机实测（真 server，全新工作区 ✓）：
```
✓ 项数 = 63（引擎权威项数是 63）
✓ smudge 的 base_value 是数字（读到 0）
✓ radius_logarithmic 的 base_value 是数字（读到 3.009999990463257）
✓ set_brush_setting 成功（HTTP 200）
✓ 读回一致：smudge = 0.375（写入 0.375）
✓ 其它项没被牵连：radius_logarithmic = 3.009999990463257（原 3.009999990463257）
✓ 未知设置名被拒（HTTP 400）
✓ 笔刷动力学读写：… 未知名被拒
```
### 7.3 ★ 变异证明（**有牙 ✓**）

**变异**：把 setter 的落点写死成 `"radius_logarithmic"` ✗（设 `smudge` 时实际写到 `radius`）⇒ 重建 ⇒ 跑同一判据：
```
✓ set_brush_setting 成功（HTTP 200）              ← 这一步本来就该过 ✓
✗ 读回一致：smudge = 0（写入 0.375）              ← **红 ✓**
✗ 其它项没被牵连：radius_logarithmic = 0.375（原 3.01） ← **红 ✓**
✗ 笔刷动力学读写有 2 处问题                        ← **判据自己判红并 process.exit(1) ✓**
```
⇒ **∴ 判据对"写错键"这类错**有牙 ✓**（**∴ 而它正是 `set_brush_setting` 最可能出的错 ✓**）。

**★ 一处**我的观测错**✗（如实记 ✓）**：**那轮我写的是 `node … | tail -5; echo "退出码=$?"`** ✗ ——
**`$?` 取的是 `tail` 的退出码，不是 node 的** ✗ ⇒ **∴ 屏幕上那个"退出码=0"**不作数**✗**。
**∴ 真正作数的是判据自己的输出**（两行 `✗` ＋ 一行 `✗ 笔刷动力学读写有 2 处问题` ✓，其代码路径必然 `exit(1)` ✓）。
**∴ 教训**：**不要读穿过管道的 `$?`** ✗（**∴ 这条我在本 session 早先记过一次 ✓，这次又犯 ✓**）。

### 7.4 恢复 ✓

变异已撤销（`cp` 回原文件 + 重建 ✓），`git status` 干净 ✓；判据在**未变异**版本上仍是 7 条全过 ✓。

## 八、★ 更正与追加实测（第 1000+ 轮，第二轮核对 ✓）

### 8.1 更正：混合模式是 **11 个**，不是 10 ✗

`blend.rs:58` 写着 **`pub const NAMES: [&'static str; 11]`** ✓ —— 我早先按
`^\s+[A-Z][A-Za-z]+` 数枚举变体，**数漏了一个** ✗ ⇒ 表格里写成了 10 ✗。
**∴ 这是"用正则数枚举"这种做法的固有风险 ✓** —— **∴ 权威数应当读 `NAMES` 的数组长度 ✗**（写给后来者 ✓）。

### 8.2 追加：P1（真实时间）**其实早就接通了** ✓，我上一轮说"缺 MCP→dtimes"是**错的** ✗

```
`tools.rs:14377`  `fn parse_brush_point_times(args)` ⇒ 从 **`points` 的第 4 个元素**读时间（秒）✓
`tools.rs:16017`  `let dtimes = parse_brush_point_times(args)?;` ✓
`tools.rs:15838`  `stamp_stroke_from(…, points, &mut previous_here, dtimes)`（注释「第 254 轮实测修」）✓
`tools.rs:18803`  判据 `dtimes_reach_the_engine` 专门守着它 ✓
```
⇒ **∴ `brush_stroke` 的 `points: [[x, y, pressure, time_seconds], …]` ✓ —— **时间已经到达引擎** ✓
   ⇒ **∴ 我上一轮的结论是错的 ✓**（**∵ 我只读了 `stamp_stroke()` 那个包装（内部传 `&[]` ✗），漏读了 800 行外的显式传参 ✓**）
   ⇒ **∴ 教训 ✓：**"某个包装函数传空"≠"整条链没接"✗ —— **∴ 要顺着**真实调用点**读，不是顺着名字读 ✓**

### 8.3 追加：P4 的真实成本 —— 三个列表**没有常量** ✗

| 列表 | 权威处 | 有常量吗 |
|---|---|---|
| 修图 4 型 | `render.rs:3754-3757` 四个字符串比较 | ✗ **没有** |
| 液化 3 型 | `object.rs:127` 的文档注释 ＋ `:342` 缺省 `"push"` | ✗ **没有** |
| 形状类型 | `ShapeKind` 枚举（`object.rs`） | ✓ 有枚举 |
| 滤镜 14 / 调整 12 / 混合 11 | `filter.rs:1170`／`:1154`／`blend.rs:58` | ✓ **都有** |

⇒ **∴ P4 要"不漂移地导出"，必须**先把前两个提成常量**✗，并让渲染器引用它 ✓** —— **∴ 那是下一轮的正事 ✓**。

## 九、P4 落地与证据（第 1000+ 轮 ✓）

### 9.1 新增 `list_engine_capabilities` ✓（`82d1a93b`，工具数 153 → 154）

一次返回：`blend_modes`（11）／`filters`（14）／`adjustments`（12）／`retouch_kinds`（4）／
`liquify_kinds`（3）／`mediums`（5）／`example_mediums`（`example-dab` ✓）／`brush_settings`（63 个 cname）／`counts`。

**每个列表都引用引擎自己的常量** ✓：`BlendMode::NAMES`／`FILTER_NAMES`／`ADJUSTMENT_NAMES`／
`RETOUCH_KINDS`／`LIQUIFY_KINDS`／`BrushSetting::ALL` ⇒ **∴ 加一种只改一处 ✓**。
**`example-dab` 单独一栏** ✓（**∴ 它不是成品介质 ✓**）。

### 9.2 判据 ✓（`scripts/tool-engine-capabilities.mjs`）

**★ 设计要点 ✓**：**判据不写死 11/14/12/4/3** ✗ —— 它**直接读源码**里 `[&str; N]` / `[&'static str; N]` 的 N，
要求每处**恰好一处**匹配（否则**退 3**，不假装通过 ✓），再与工具输出比 `counts` 与**数组长度** ✓。
**∴ 这样它证明的不是"工具没坏"✗，而是"工具**没漂移**"✗✓** —— **∴ 真正的风险是有人加了一种修图而工具还报 4 ✓**。

本机实测（真 server ✓）：
```
✓ blend_modes：工具报 11、列表长 11、源码常量 11
✓ filters：14 / 14 / 14
✓ adjustments：12 / 12 / 12
✓ retouch_kinds：4 / 4 / 4
✓ liquify_kinds：3 / 3 / 3
✓ brush_settings：工具报 63、列出 63
✓ example-dab 只在 example_mediums 里，不在 mediums 里
✓ 退出码 0
```
### 9.3 ★ 变异证明（**有牙 ✓**）

**变异**：把工具里的 `retouch_kinds` 写死成 `["clone_stamp", "heal", "smudge"]` ✗（源码常量是 4）⇒
```
✗ retouch_kinds：工具报 4、列表长 3、源码常量 4（不一致）   ← **红 ✓**
✗ 能力清单有 1 处与引擎常量不一致                        ← 判据自报 ✓
**退出码 1**（**直接取 ✓，没穿管道 ✓**）
```
⇒ **∴ 判据对「列表被写死/漂移」有牙 ✓✓**。

### 9.4 ★ 两处我自己的错（**如实记 ✓，都在这一轮内被自己抓住**）

1. **中文串里用了 ASCII 双引号** ✗（`…（含"示例介质不混入成品"这层语义）`）⇒ `node --check` 直接 `SyntaxError`
   ⇒ **∴ 又踩了 AGENTS.md 那条「中文文本里一律用「」」✗**（改成「」后语法通过 ✓）；
2. **`text.matchAll(pattern)` 但 `pattern` 没有 `g` 标志** ✗ ⇒ `TypeError: … non-global RegExp argument` ⇒ **判据根本没跑起来** ✗✓
   ⇒ **∴ 而这一笔我**已经提交了**✗**（`92765145`）⇒ **∴ 下一笔 `76d8fa33` 是"修上一笔的坏判据"✓**
   ⇒ **∴ 教训 ✓：语法通过 ≠ 跑得起来 ✗ ⇒ 判据必须**实跑一次**再提交 ✓**（**∴ 这条比"写判据"本身更贵 ✓**）。

## 十、★★ 更正：**「59 项不可控」是错的** —— `brush_params` 早就能逐笔覆盖全部 63 项 ✗✓

### 10.1 证据（`tools.rs:3019-3031`）

```rust
// :3025  注释原文
//   「**∴ AI 不必换笔刷文件就能调** `smudge`／`opaque`／`dabs_per_radius` 这类 ✓ ★★★
param!("brush_params", Object, false,
      "覆盖 .myb 字段：{规范名: 数值}，如 {\"smudge\":1.0,\"opaque\":0.2}；\
       字段名就是 .myb 的 JSON 键，**拼错会报错并给候选**。…")
// :3093  `swatch_brush_strokes` 有同一套 ✓
// :3342  `list_brushes` 附「规范字段名全表」 ✓
```

⇒ **∴ agent 早就可以**逐笔**写 `brush_params: {"smudge": 1.0, "speed1_gamma": 4.0, "slow_tracking": 0.9}` ✓✓**
（**∴ 而且**拼错会报错并给候选**✓ ⇒ **∴ 可发现性也有 ✓**）

### 10.2 因此本文件前几节的口径要更正 ✓

| 之前写的 ✗ | 正确的 ✓ |
|---|---|
| 「**63 项里 59 项**不可逐项控**」✗ | **`brush_params` 可逐笔覆盖全部 63 项 ✓**（键 = `.myb` 规范名 ✓） |
| 「P0（`get/set_brush_setting`）是**最大的一格**」✗ | **P0 的价值是**持久化**（写回工作区缓存 ✓）与**读当前值**✓，不是"第一次能控制"✗ |
| 「P2 语义参数是**必须做**」✗ | **P2 是**语义糖**✓**（`hand_jitter` 之类别名 ⇒ 让 agent 不必记规范名 ✓），**不是能力缺口** ✗ |

### 10.3 ★ 那么这个目标**真正**缺什么（**重新排序 ✓**）

| 优先级 | 缺口 | 说明 |
|---|---|---|
| **真缺口 ①** | **语义别名** | `hand_jitter`／`speed_gain`／`color_variance`／`smudge_amount` ⇒ 一个词管一组 `.myb` 键 ✓ |
| **真缺口 ②** | **`medium` 未枚举化** | 仍接受任意字符串；`example-dab` 可被当真（**∴ 上一轮已在清单里单独标注 ✓，但调用仍不校验 ✗**） |
| **真缺口 ③** | **判据覆盖** | `brush_params` 的"拼错报错给候选"有没有判据守着 ✗ —— **∴ 待核 ✓** |
| **已满足 ✓** | 63 项可调（`brush_params` ✓）／可发现（`list_engine_capabilities` ＋ `list_brushes` ✓）／持久化（`set_brush_setting` ✓）／真实时间（`points` 第 4 项 ✓） | |

### 10.4 ★ 教训 ✓

**∴ 我把"没在这个工具里看到"✗ 当成了"没有这个能力"✗** —— **∴ 而 `brush_params` 就在**同一个 `ToolSpec` 的**相邻几行**（`:3026` ✗），
   **∴ 我上一轮读 `brush_stroke` 的 schema 时**只读了前 12 行**✗（`head -12` ✗）⇒ **∴ 恰好切掉了它 ✓**
   ⇒ **∴ 教训 ✓：**读 schema 要读到 `},` 为止 ✗，不要用 `head -N` 截断 ✓**（**∴ 这条与"漏读 800 行外的 dtimes"同源 ✓**）✓

## 十一、第 1000+ 轮补充核实（③ 已满足 ✓；② 仍待核 ✗）

### 11.1 ★ ③ 已满足：`brush_params` 既有校验、也有判据在用 ✓

```
`crates/yanshi-server/src/tools.rs:15563`  `fn apply_brush_params(brush: &mut hokusai::Brush, params: &Value) -> Result<usize>`
   —— **∴ 有专门的校验/应用函数 ✓**（**∴ 「拼错报错并给候选」就在它里面 ✓，`:5124` 的 `canonical_param_names` 提供候选清单 ✓**）
`scripts/tool-bristle.mjs:128`  `{ brush_params: { dabs_per_basic_radius: 0.4, radius_by_random: 1.5 } }`
   —— **∴ 已有判据**实跑**这个参数 ✓**（**∴ 而且是**变异有牙**的设计 ✓：注释明写「把那组 brush_params 删掉 ⇒ 必红」✓**）
```
⇒ **∴ 原表里「③ 判据覆盖：待核 ✗」⇒ **改为**已满足 ✓**（**∴ 但"拼错给候选"这一条**具体**有没有判据守着，仍**待核** ✗ ✓**）

### 11.2 ② 仍待核：`medium` 的校验点没找到 ✗

`write_medium_stroke`（`:13080`）第一行就是 `let medium = require_str(args, "medium")?;` ✓，
但**我这一轮的 grep 窗口没看到它拿 `medium` 去做什么校验** ✗ ⇒ **∴ 不能断言"没校验"✗，只能说**没看到**✓**
   ⇒ **∴ 下一轮第一件事：把 `write_medium_stroke` 读到它真正用 `medium` 的那一行 ✓**（**∴ 不用 `head -N` 截断 ✗**）

## 十二、更正 ②：`medium` **是有校验的** ✓（在 `yanshi_medium_host` 里）

**证据**（`write_medium_stroke`，`tools.rs:13080..13332`，252 行 ✓）：
```
:149  let (region, rgba) = yanshi_medium_host::paint_stroke_over(&medium, …)
:169  let spec = yanshi_medium_host::spec(&medium).expect("paint_stroke 已经校验过介质名");
```
⇒ **∴ 介质名由 `yanshi_medium_host` 校验** ✓（**∴ 不是"接受任意字符串"✗**）
   ⇒ **∴ 原表「② `medium` 未枚举化（仍接受任意字符串、`example-dab` 可被当真）」✗ 要改口径 ✓**：
     **∴ 不是能力缺口 ✓，而是「错误消息是否给候选」的**友好度**问题 ✗**（**∴ 这点仍待核 ✓**）

### 12.1 ★★ 一个模式（**本目标里第 2 次** ✗）

**∴ 我已经**两次**把"我没读过"✗ 当成"没有"✗**：
1. `brush_params`（**∴ 用了 `head -12` 截断 ✓**）
2. `medium` 的校验（**∴ 只看了函数前 8 行 ✓**）

⇒ **∴ 共同来源 ✓**：**读代码时**截断**✗** ⇒ **∴ 而代价都是**先写错结论、再花一轮改账**✗✓**。
⇒ **∴ 给自己记一条 ✓**：**在写"缺什么"之前，先把那一处的**完整函数体**读到它的结尾 `}` ✓**（**∴ 用 `awk` 找 `^}` ✓，就像这一轮 ✓**）。

## 十三、★★ 实测两件事：② 不是缺口 ✓；但**我把示例介质的名写错了** ✗

### 13.1 `medium` 有校验、**也给候选** ✓ —— ② 完全不是缺口

```
错介质名 ⇒ {"detail":"**未知介质 definitely_not_a_medium ⇒ 可用：oil / watercolor / marker / pencil / pixel / example**",
            "error_code":"invalid_argument"}
```
⇒ **∴ 校验在 ✓、候选在 ✓** ⇒ **∴ 原表 ② 「未枚举化」**彻底不成立**✗✓**（**∴ 我上一轮说"待核"✓，这一轮**实测**✓**）

### 13.2 ★ 但 `example-dab` 这个名字是**我编的** ✗ —— 真名是 `example`

```
medium_stroke(medium: "example-dab") ⇒ {"detail":"**未知介质 example-dab** ⇒ 可用：… / example"}
```
⇒ **∴ 真名 `example` ✓、我叫 `example-dab` ✗**（**∴ 我是按 `web/mediums/example-dab.wasm` 的**文件名**推的 ✗**）
⇒ **∴ 而 `list_engine_capabilities` 里我写的 `example_mediums: ["example-dab"]` **是错的**✗✓**
   ＋ **`mediums` 硬编码 5 个（oil/watercolor/marker/pencil/pixel）**✗ **漏了 `example`** ✗
   ⇒ **∴ 两处都是**硬编码**✗ ⇒ **∴ 而它们都是**我手写的**✗ ⇒ **∴ 这恰好是"漂移"的典型 ✗**

### 13.3 ∴ 由此产生的两条修正（**下一轮做 ✓**）

| # | 修正 | 说明 |
|---|---|---|
| **1** | `list_engine_capabilities` 的介质清单**改成从引擎取** ✗ | 不再硬编码 5 个 ＋ 不再编 `example-dab` ✓；**∴ 若 `yanshi_medium_host` 没导出清单 ⇒ 至少改成 6 个真名 ✓**（oil/watercolor/marker/pencil/pixel/**example** ✓） |
| **2** | **标注 `example` 是示例** ✗ | 它现在**混在"可用"清单里 ✓** ⇒ **∴ agent 会把它当成品用 ✗** ⇒ **∴ 在 `medium_stroke` 的候选消息里标成 `example（示例，别用于成品）` ✓** |

### 13.4 ★ 又一次"我以为"（**本目标第 3 次** ✗）

**∴ 我按**文件名**推**介质 id**✗**（`example-dab.wasm` ⇒ 以为是 `example-dab` ✓）—— **∴ 而 id 是 `example` ✓**
   ⇒ **∴ 前两次是"没读到就当成没有"✗，这次是"看到文件名就当成了 id"✗** ⇒ **∴ 同一个病：**拿推断代替读取**✗** ✓
   ⇒ **∴ 而这次是**一次 curl 就抓住了**✓**（**∴ 成本最低的一次 ✓**）。

## 十四、P2 落地与证据：四组**语义别名** ✓（第 1000+ 轮 ✓）

### 14.1 做了什么 ✓

**∴ 在 `apply_brush_params` 的 `match` **之前**加别名展开** ✓（**∴ 四个调用点全自动受益 ✓**）：

| 别名 | 展开成（`.myb` 真名 ✓） |
|---|---|
| **`hand_jitter`** | `slow_tracking` ＋ `tracking_noise` ＋ `offset_by_random` |
| **`speed_gain`** | `speed1_gamma` ＋ `speed2_gamma` ＋ `speed1_slowness` ＋ `speed2_slowness` |
| **`color_variance`** | `change_color_h` ＋ `change_color_v` ＋ `change_color_hsl_s` |
| **`smudge_amount`** | `smudge` ＋ `smudge_length` ＋ `smudge_transparency` |

**∴ 展开仍走 `BrushSetting::from_cname` ✓** ⇒ **∴ 不可能写出引擎不认识的名字 ✓**；
**∴ 组内每项都设成给的那个值 ✓**（**∴ 简单、可预测、可判据 ✓**）

### 14.2 判据的**两层** ✓（**互补，缺一层都不行**）

| 层 | 证明什么 | 变异时 |
|---|---|---|
| **结构断言**（读 `tools.rs` 里的别名表 ✓） | **展开**完整** ✓** | **删一项 ⇒ 红 ✓** |
| **像素断言**（`swatch_brush_strokes` 跑两次比字节 ✓） | **展开**有效** ✓** | **真名写错 ⇒ 红 ✓** |

**本机实测 ✓**：8 项全过（4 结构 ＋ 4 像素），退出码 0 ✓。
**变异 ✓**：删 `hand_jitter` 的 `offset_by_random` ⇒
`源码 [slow_tracking, tracking_noise]｜期望 [offset_by_random, slow_tracking, tracking_noise]` ⇒ **红 ✓，退出码 1** ✓。

### 14.3 ★★ 这一笔最值钱的发现：**只比像素有盲区** ✗

```
**∴ 我第一版判据只比像素 ✗ ⇒ 变异（删 `offset_by_random`）**活下来了**✗✓（**判据仍绿 ✓**）
   ⇒ **∴ 原因 ✓**：**那项在 `oil-mop` 上**本来就不生效**✗ ⇒ **∴ 删掉它 ⇒ 像素不变 ⇒ 判据看不出来 ✓**
   ⇒ **∴ 结论 ✓**：**像素等价只能证明「生效的项一致」✗，证明不了「展开完整」✗✓**
**∴ 而修法 ✓**：**再加一层"读源码别名表、逐项比对"的结构断言 ✓**（**∴ 它不需要重建 ✓，因为读的是文件 ✓**）
```

### 14.4 ★ 又犯一次的错（**如实 ✓**）

**∴ 取退出码时写成 `node … | head -4` ✗ ⇒ `$?` 拿到的是 `head` 的 ✓** ⇒ **∴ 重跑一次不带管道的才拿到真的 `1` ✓**
   ⇒ **∴ 这条本 session 已第三次 ✓**（**∴ 前两次：`png` 计数、`brush-settings` 判据 ✓**）

## 十五、★★★ **真缺口**：经 MCP 的「时间 → dab 数」这条链**不通** ✗（实测 ✓）

### 15.1 三次尝试，指标都一样 ✗

| 尝试 | 笔刷 | `brush_params` | 快笔（0.005s/段） | 慢笔（0.500s/段） |
|---|---|---|---|---|
| ① | `oil-mop` | —— | **不落墨** ✗（错误消息：它是涂抹类 `smudge=1` ✓） | 同 |
| ② | `100%_Opaque` | —— | `avg_brightness = 0.9140100980392157` | **完全相同** ✗ |
| ③ | `100%_Opaque` | `speed_gain:1.6` ＋ **`dabs_per_second:200`** | `0.9129734313725489` | **完全相同** ✗ |
| ④ | **`Clouds`** ✓ | **`dabs_per_second:200`** | **`steps=141`／`painted_pixels=54400`** | **`steps=141`／`painted_pixels=54400`** ✗ |

**∴ ④ 是最硬的一次 ✓**：**`steps` 就是 dab 数 ✓** —— **∴ 若时间真的进了引擎 ⇒ 慢笔每段多 100 倍的时间 ⇒ 时间基间距下应多落 dab ⇒ `steps` 必增大 ✗✓**
   ⇒ **∴ 而它**一模一样**✗ ⇒ **∴ 结论 ✓：`points` 第 4 项的时间，在 MCP 这条路上**没有影响 dab 数**✗✓**

### 15.2 ★ 但引擎层是通的 ✓ —— 所以这是**接线缺口**，不是引擎缺口

**`cargo test -p yanshi-server --release --lib dtimes_reach_the_engine`** ⇒ **1 passed** ✓
   ⇒ **∴ 那条判据**直接调 `stamp_stroke_from(…, dtimes)`**✗（**绕过了 MCP 的参数解析与落笔路径 ✓**）
   ⇒ **∴ 所以 ✓**：**引擎层"时间影响像素"**成立 ✓**；**MCP 层"时间影响 dab 数"**不成立 ✗✓**

### 15.3 ∴ 下一轮的定位方向（**明确 ✓**）

```
**∴ 要查 ✓**：`write_brush_stroke` 里 `dtimes` 被解析（`:16017` ✓）之后，**到底传给了哪一次落笔**✗
   ⇒ **∴ 已知 ✓**：`:15838` 有一条**显式传 `dtimes`** 的路 ✓；`:15037` 的 `stamp_stroke()` 包装传 `&[]` ✗
   ⇒ **∴ 而 `steps` 这个数**从哪来**✗ —— **∴ 若它来自**那条传 `&[]` 的路**✗ ⇒ **∴ 就是它 ✓✓**
**★ 而这次我有了一个**廉价且硬的判据指标**✓**：**`steps` ＋ `painted_pixels`**✗
   ⇒ **∴ 修好后 ✓：慢笔的 `steps` 必须显著大于快笔 ✓**（**∴ 而它能红能绿 ✓：现在的代码下两者相等 ⇒ 红了 ✓**）
```

### 15.4 ★★ 缺口**再收窄一层**：参数通道是通的，**不通的是 `dtimes`** ✗

**实测（`swatch_brush_strokes`，`Clouds`）**：
```
A：brush_params {dabs_per_second: 0}    ⇒ thumb_url … sha256:388102a4f9606fea…
B：brush_params {dabs_per_second: 200}  ⇒ thumb_url … sha256:8d8bc2148b6cd995…
⇒ **图像不同 ✓** ⇒ **∴ `dabs_per_second` 参数**确实生效**✓✓**
```
**∴ 而"快笔 0.005s vs 慢笔 0.500s"两笔**逐字节相同**✗（第 15.1 节 ④ ✓）

⇒ **∴ 两条合起来 ✓ 结论唯一 ✓**：
   **∴ `brush_params` → `.myb` 的通道**通 ✓**；**∴ 而 `points` 第 4 项的时间 → 引擎的通道**不通 ✗✓**
   ⇒ **∴ 缺口**不在"参数能不能调"✗，**而在 **`dtimes` 有没有真的用上**✗** ✓

### 15.5 ★ 一处**我自己的误判**（已纠正 ✓）

**∴ 我上一轮把 `steps` 当成"dab 数"✗** —— **∴ 而它其实是**按距离细分的迭代次数**✓**
   （**∴ `:15885`／`:15919` 都在 `stamp_stroke_from` 里 ✓，那个循环按 `distance/2` 细分 ✓**）
   ⇒ **∴ 所以 `steps=141` 快慢相同**是正常的 ✓**，**∴ 它**不能**用来判"时间有没有进引擎"✗** ✓
   ⇒ **∴ 而真正能判它的，是这一节的**图像是否变化**✓**（**∴ 那个变了 ⇒ 参数在 ✓；而时间没让它再变 ⇒ 时间不在 ✗**）✓

### 15.6 ★★★ 两变量对照：**参数生效，时间不生效** ✗（决定性 ✓）

**在 `brush_stroke` 上做（不是 `swatch` ✓），只改一个变量 ✓**：

| 组 | `dabs_per_second` | 每段秒数 | `painted_pixels` | 判定 |
|---|---|---|---|---|
| **① 只改参数** | **0** → **200** | 固定 0.005 | **53890 → 54400** | **变了 ⇒ 参数生效 ✓** |
| **② 只改时间** | 固定 200 | **0.005 → 0.500** | **54400 → 54400** | **没变 ⇒ 时间不生效 ✗✓** |

⇒ **∴ 结论唯一 ✓✓**：**`brush_params` → `.myb` 的通道**通 ✓**；**`points` 第 4 项的时间 → 落墨**不通 ✗**
   ⇒ **∴ 而 `stamp_stroke_from` 的代码**看起来是对的**✓**：
     `let provided = dtimes.get(index).copied().unwrap_or(-1.0);` ＋ `if provided >= 0.0 { (provided, true) }` ✓
     **∴ 索引对齐也正确**（`points.iter().enumerate()` ✓）
   ⇒ **∴ 所以最可能是** `dtimes` 实际传进来的是**空切片** ✗**（**∴ 即 `parse_brush_point_times` 返回空 ✓**）
     ⇒ **∴ 下一个待查点 ✓：`parse_brush_point_times` 对 `[x,y,pressure,time]` 这种四元组**到底解析出几个**✗**
       （**∴ 它读 `pair.get(3)` ✓ —— **∴ 而那正是**第 4 个元素**✓ ⇒ **∴ 理论上应当有 5 个 ✓**）
       ⇒ **∴ 若它有 5 个 ⇒ **∴ 那问题在**传参**（`:15885`／`:15919` 是否真的走了 ✓）**；**若 0 个 ⇒ **∴ 就是解析 ✓**

> **★★ 阅读提示 ✓**：**本节 15.1–15.6 的**结论**（"时间不通、缺口在 `dtimes`"）**已被 15.7 推翻** ✗✓
> —— **∴ 保留它们是因为它们**如实记录了错误过程**✓（**∴ 且每一节都留了可复算的数字 ✓**）；
> **∴ 现在的正确结论是 ✓：`dtimes` **是通的**（**毫秒时间戳**✓），我错在**四处测量口径**✗**。

### 15.7 ★★★★★ 反转：**P1 一直是通的** ✓ —— 是我**四个测量口径全错** ✗

**最后那次实测（正确口径 ✓）**：
```
快笔（每段 **5 ms**，时间戳 0/5/10/15/20）    ⇒ avg_brightness = **0.7616**
慢笔（每段 **500 ms**，时间戳 0/500/…/2000）⇒ avg_brightness = **0.3734** ⇒ **暗得多 ✓✓**
⇒ **∴ 差 **0.3881**✗ ⇒ **∴ 慢笔的墨**明显更多** ⇒ **∴ "慢笔多墨、快笔飞白"**成立 ✓✓**
```

**∴ 我错在哪四处（**逐条如实 ✓**）**：

| # | 我错的 | 正确的 |
|---|---|---|
| **1** | 把 `points` 第 4 项当**步长（秒）** ✗ | 它是**毫秒时间戳** ✓（`(current - previous) / 1000.0` ✓）⇒ **∴ 我给 0.005 时差恒为 0 ⇒ 时间等于没给 ✓** |
| **2** | 用 `painted_pixels`（**去重覆盖数**）当 dab 数 ✗ | 重复落墨**不增加覆盖数** ⇒ **∴ 测不出 ✓** |
| **3** | 用 **α=255 不透明**色 ✗ | 同区域重复落墨 ⇒ 颜色**几乎不变** ⇒ **∴ 亮度测不出 ✓** |
| **4** | 把 `steps` 当 dab 数 ✗ | 它是**按距离细分的迭代次数** ✓ |

⇒ **∴ 四个口径全错 ✗，而**功能一直是好的**✓✓** ⇒ **∴ 这比"发现真缺口"更值得记：**错误的测量会把"通"读成"不通"**✗** ✓

### 15.8 判据 ✓（`scripts/tool-brush-speed.mjs`，**能红能绿** ✓）

**正确口径 ✓**：**α=40（半透明 ⇒ 重复落墨叠加变深 ✓）＋ 毫秒时间戳 ✓ ＋ 比 `avg_brightness` ✓**
**判据 ✓**：**慢笔必须比快笔**暗 ≥ 0.05**✗**（**∴ 不设"小差异"门槛 —— 那是噪声问题 ✓**）

**本机实测 ✓**：
```
快笔（每段 5ms）  ⇒ avg_brightness = 0.7616
慢笔（每段 500ms）⇒ avg_brightness = 0.3734
✓ 慢笔明显更暗（差 0.3881）⇒ 笔速真的影响墨量
退出码 0
```
**∴ 它能红的地方 ✓**：**若时间没进引擎（例如又传 `&[]`）⇒ 两笔亮度相等 ⇒ 差 0 ⇒ **判据红** ✓**
   （**∴ 待做：变异证明 ✓**）

### 15.9 笔速判据的**变异证明** ✓（有牙 ✓）

**变异**：在 `stamp_stroke_from` 的 `let __t_stamp = …` 之前插一行
`let dtimes: &[f64] = &[];` ✗（**遮蔽掉传进来的时间 ⇒ 引擎永远用 `STEP_SECONDS` ✓**）⇒ 重建（1m33s 真重建 ✓）⇒ 跑判据：

```
快笔（每段 5ms）  ⇒ avg_brightness = **0.3738**
慢笔（每段 500ms）⇒ avg_brightness = **0.3738**  ⇒ **完全相同 ✗**
✗ **慢笔没有明显更暗（差 0.0000 < 0.05）⇒ 时间基间距没起作用**
```
⇒ **∴ 判据**确实红了 ✓**（**∴ 输出 `✗` ＋ 条件 `darker < 0.05` ⇒ 走 `process.exit(1)` ✓**）

**★ 一处我自己的观测错（第 4 次 ✓）**：**我又写成 `node … | tail -3; echo $?` ✗** ⇒ **∴ `$?` 是 `tail` 的 ✓**
   ⇒ **∴ 屏幕上那个"退出码=0"**不作数**✗** ⇒ **∴ 作数的是判据自己的两行 `✗` 与差 0.0000 ✓**
   ⇒ **∴ 这条错在本 session 已第 4 次 ✓**（`png` 计数 ✗／`brush-settings` ✗／`brush-aliases` ✗／本条 ✗）

### 15.10 结论 ✓

**∴ 笔速这一格**完整 ✓**：**判据能绿 ✓（差 0.3881 ✓）＋ 变异能红 ✓（差 0.0000 ✓）** ⇒ **∴ 任务 2（时间基间距）**真的成立 ✓✓**

## 十六、CI 收口过程中的**四个坑**（第 1000+ 轮 ✓，都是我自己造成或误读 ✓）

### 16.1 坑一：`concurrency` 组被一个**卡死的 run** 占着 ✗

`ci.yml` 用 GitHub 原生 `concurrency` ✓（同一分支只保留最新一轮 ✓）。而某次一个 run 卡在 `in_progress`
**一小时以上** ✗ ⇒ **∴ 后续所有 run 都是 `pending` 且 **`JOBS` 为空（job 数 = 0）**✗✓**
⇒ **∴ 我一度写进 blocked 原因说"GitHub 侧卡住、我推不动"✗✓ —— **∴ 而真相是**我自己的一笔 run 卡死、占着并发组**✗**
⇒ **∴ 处置 ✓：`gh run cancel <id>` ✗ ⇒ **∴ 25 秒后它就分到 job（0 → 2）✓✓**

### 16.2 坑二：我用 `workflow_dispatch` 手工触发，**反而把 `brush parity` 拉起来**了 ✗

```
ci.yml 的 brush parity：**`if: github.event_name != 'push'`** ✗（**∴ push 时**不跑**✓，只有手动／定时才跑 ✓**）
   ⇒ **∴ 而它跑 `all`（199 支 × 4 色 ＝ 796 次 ✓），实测 **32m1s** ✗
   ⇒ **∴ 所以我"手工触发"的那一刻，就把它**拖进了 run**✗ ⇒ **∴ run 变成 40+ 分钟不结束 ✓**
   ⇒ **∴ 而它 **`continue-on-error: true`** ✓ ⇒ **∴ 它**不阻塞门禁**✗** ⇒ **∴ 但它**拖住了整个 run 的日志归档**✗✓**
**∴ 结论 ✓：手工触发要用 `push`（或等下一次 push ✓），不要 `workflow_dispatch` ✗**
```

### 16.3 坑三：取消 run ⇒ **job 日志永久丢失** ✗

```
**∴ GitHub 只在**整个 run 结束**后才归档 job 日志 ✗** ⇒ **∴ `--log-failed` 在 run 未结束时返回 **0 行**✓**
   ⇒ **∴ 而我为了解卡**取消了 run**✗ ⇒ **∴ 那一笔 `brush parity`（29 分钟失败）的日志**永远拿不到了**✗✓**
   ⇒ **∴ 教训 ✓：**先落盘、再取消**✗**（**∴ 我后来改成"run 一完成就抢日志"的后台任务 ✓**）
```

### 16.4 坑四：**同一个工具数散落五处**，我改一处漏四处 ✗

```
**∴ 「工具数」写在 ✓**：`tools.rs`（4 处 doc 注释 ✓）／`docs/guide.md` ✓／`docs/guide.zh-CN.md` ✓／
   `docs/tools.md` ✓／`docs/design/tool-examples.md`（生成物 ✓）
⇒ **∴ 我改了 tools.rs 与两份 guide ✓，漏了 `docs/tools.md` 与 `tool-examples.md` ✗ ⇒ **∴ 三个 job 红 ✓**
**∴ 而且 `docs/tools.md` 有**两套口径**✗**：
   · **core ＝ 85** ✓（**∴ 我改对了 ✓**）
   · **"全部"＝ **142**（注册表口径 ✓）** ✗ 而**不是 `/api/tools` 的 154** ✗
   ⇒ **∴ 断言原文把它并排打出来 ✓：`core 85 vs 85，全部 154 vs 142` ✓** —— **∴ 我按 154 写过一次，被它打回 ✓**
```

## 十七、外部报告（油画 AI）的逐条核对与 G2 设计（第 1000+ 轮 ✓）

### 17.1 核对结果：三条真、一条误判 ✓

| 报告条目 | 结论 | 依据 |
|---|---|---|
| **#6** `load`/`wetness` 拿不到 ✗ | **误判** ✗ | `medium_stroke` 早有 `load`／`wetness`／`texture`／`color_mixing=rgb\|pigment`（`tools.rs:2829-2834`）；`DabInput` 在 `crates/yanshi-medium-host/src/lib.rs:204/218/235/249/263` 被构造 **5 次**（报告 grep 的是 `yanshi/crates` ✗、且在 `medium-oil` 里找 ✗） |
| **#7** 只能糊不能融 ✓ | **真缺口** ✓ | `smudge` 的参数只有 `layer_id/points/size/smudge_length/hardness/opacity/object_id/z_index`（报错原文 ✓），**没有 deposit** |
| **#8** 没有厚涂/浮雕 ✓（需限定 ✓） | **真缺口**（限定后 ✓） | 有 `impasto`＝`relief` 别名（在 `FILTER_NAMES` 14 个里 ✓），是亮/暗边缘伪浮雕；**没有高度场** ✓ |
| **#10** 规范名清单只能从 `list_brushes` 拿 ✓ | **真缺口**（可发现性）✓ | 63 项只在 `list_brushes.canonical_param_names` 与 `list_engine_capabilities.brush_settings` 里 |

**∴ #6 之所以被误判，恰好证明可发现性不足** ✓（G1 要修的就是这个 ✓）。

### 17.2 G2 的设计：`deposit` 补的是**采到的色**，不是用户给的色 ✓

| 方案 | 后果 |
|---|---|
| A. 补用户给的色 ✗ | **那就是 `rub`** ⇒ 落新色 ⇒ **造新硬边** ✗（报告自己否掉了 ✓） |
| **B. 补该 stamp 采到的后方色 ✓** | **不引入任何新色** ✓ ⇒ 只把已有的色压实一点 ⇒ **"既带色、又不造新边"** ✓ |

⇒ **∴ 采用 B ✓**；`deposit=0` ⇒ 不补 ⇒ 与今天完全一样 ✓；`deposit=1` ⇒ 每 stamp 都补 ⇒ 接近"用源色画一遍" ✓。

### 17.3 要改的四处 ✓（**写回处不用动** ✓）

```
① crates/yanshi-render/src/object.rs:117 区   Retouch 结构体加 deposit: f64
② 同文件 :292 区                              deposit: object.data.get("deposit").and_then(Value::as_f64).unwrap_or(0.0)
   ⇒ **∴ 旧对象没有这个键 ⇒ 得 0.0 ⇒ 与今天一致 ✓✓**
③ crates/yanshi-render/src/render.rs:3846 区  「采到色之后、写回之前」按 deposit × pressure 补一层
④ crates/yanshi-server/src/tools.rs           write_retouch 把参数写进 data（**只在非零时写 ✓**）＋ schema 加 param
```

**★ 陷阱 ✓（第 1 轮发现 ✓）**：若在写回处**无条件**写出 `"deposit": 0.0` ⇒ 旧对象的 `data` 会**多一个键** ✗
⇒ **∴ atom 内容变了 ⇒ "逐字节不变"当场破功** ✗ ✓ ⇒ **∴ 只在非零时写 ✓**。

### 17.4 判据已就绪、且**红在正确的地方** ✓

`scripts/tool-smudge-deposit.mjs`（三条断言 ✓；手法：**一变体一层** → 铺斑块 → 涂抹 → `export_png(layer_id, region, path)` 落盘 → 比 sha256 ＋ 比亮度 ✓；**∴ 三层互不干扰，不需要 undo（仓库没有 undo 工具 ✗）** ✓）：

```
**判据退出码 = 1** ✓
✗ dep_zero: smudge 失败
  {"detail":"smudge 不接受参数 deposit（拼写错误？）；可用参数：layer_id, points, size, smudge_length,
   hardness, opacity, object_id, z_index"}
⇒ **∴ 红在"参数不存在"，而不是"像素没变"** ✓ ⇒ **∴ 缺口是接线，不是算法 ✓✓**
```

### 17.5 进度（**如实 ✓**）

- G2 **判据 ✓ 红了（红得对 ✓）／实现 ✗ 未做**（四处改动待落）；
- G1／G3／G4 **未开始** ✗。

### 17.6 G1 完成 ✓（`472ab905`）＋ 变异证明 ✓

**做了什么** ✓：`brush_stroke` 与 `medium_stroke` 的 summary 互相点明分工（要载墨量／湿度／光谱混色 ⇒
`medium_stroke` 的 `load`／`wetness`／`color_mixing=pigment`，**不是** `brush_params` ✓，∵ `.myb` 63 项里没有这两项 ✓）；
`list_engine_capabilities` 增 `medium_params`；判据加一条**读 `medium_stroke` 的 schema 与工具返回的 `names` 逐项比** ✓。

**判据当场打回我一次** ✓：我第一版手写 11 项、漏了 `layer_id` 与 `object_id` ✗ ⇒ 判据报「工具 [...]｜源码 [...]」
⇒ 从源码抽出 **13 项**对齐后才绿 ✓ —— **∴ 这正是"读源码、不硬编码"的价值** ✓（**∴ 若两边都手写同一个错清单，会双双错误地绿 ✗**）。

**变异证明** ✓（按目标：删掉 schema 里一个参数）：
```
删掉 `param!("wetness", …)` ⇒ **不重建**（判据读源码 ✓）⇒ 跑判据：
  ✗ medium_params 与 medium_stroke schema 一致：工具 [13 项]｜源码 [12 项]
  **退出码 1** ✓
恢复后 脏=0、`param!("wetness"` 回来 1 处 ✓
```
⇒ **∴ G1 完整 ✓**：实现 ✓ ＋ 判据绿 ✓ ＋ 变异红 ✓。

**G2／G3／G4 仍未开始** ✗（G2 判据已红得对 ✓，四处改点与陷阱已记在 17.3 ✓）。

### 17.7 G2 的实现配方（**已定位到行，含一个浮点陷阱** ✓，未动代码 ✗）

**落点** ✓：`crates/yanshi-render/src/render.rs` 的 retouch 采样段（约 `:3748-3753`）：
```rust
:3748  let sampled = source.pixel(sx, sy);
:3749  if sampled[3] <= 0.0 { … }                       // 采样透明 ⇒ 跳过
:3750  [sampled[0] / sampled[3], sampled[1] / sampled[3], sampled[2] / sampled[3],
:3753   (brush.opacity as f32 * sampled[3]).clamp(0.0, 1.0)]   // ← **这一项就是"涂抹的强度"**
```
⇒ **∴ `deposit` 的语义 ＝ 用**同一个色**再叠一层** ✓ ⇒ **∴ 标准 over 合成 ✓**：
`a' = 1 − (1 − a)(1 − deposit × pressure)`

**★★ 浮点陷阱 ✓**：**不能**写成无条件的 `1.0 - (1.0 - a) * (1.0 - d*p)` ✗ ——
在 `f32` 下 `1.0 - (1.0 - a) * 1.0` **不保证**逐位等于 `a` ✗ ⇒ **∴ `deposit=0` 的"逐字节不变"当场破功** ✗✓
⇒ **∴ 必须写成条件分支 ✓**：
```rust
let base = (brush.opacity as f32 * sampled[3]).clamp(0.0, 1.0);
let alpha = if deposit > 0.0 {
    let d = (deposit as f32 * pressure).clamp(0.0, 1.0);
    1.0 - (1.0 - base) * (1.0 - d)
} else {
    base                       // ⇒ **∴ 缺省路径**完全相同**✓✓**
};
```

**其余三处改动** ✓（`object.rs` 结构体 ＋ `:292` 解析 `.unwrap_or(0.0)` ＋ `tools.rs` 的 schema 与 `write_retouch` 写 `data`，
**只在非零时写** ✓）：**∴ 都没动 ✓。**

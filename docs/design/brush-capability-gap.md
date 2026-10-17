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

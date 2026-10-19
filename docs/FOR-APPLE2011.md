# 给 apple2011 的短讯（截至第 719 轮）

这份文件是**给你看的**，不是仓库文档。每次有需要你决策或纠正的事，我更新它。

## 一、`brush_stroke` **有** `group_id`（你的 §126 复核数错了区间）

你写：

```
3156  name: "brush_stroke"        ← 其参数区（3156~3226）内没有 group_id
```

**实测行号**：

| 位置 | 行号 |
|---|---|
| `name: "brush_stroke"` | `:3156` |
| 它的 `params: &[` | **`:3168`** |
| 它的 `params` 结束（`],`）| **`:3305`** |
| **`param!("group_id", …)`** | **`:3304`** ← 在区间内 |

**你说的 3226 不是你那个工具的参数表终点**，而是你看到的最后一个参数。
`group_id` 在**参数表最末尾**，紧接着就是 `],`。

**第二条证据**：我有一条判据 `scripts/tool-group-id.mjs`，它**就是走 `brush_stroke`** 的，实测：

```
全部=4｜GA=3｜GB=1｜NOPE=0
revert_group(GA) ⇒ reverted=3
撤销后：全部=1｜GB 仍在=1
再撤一次 ⇒ reverted=0（幂等）
```

若 `brush_stroke` 真没有 `group_id`，框架会在**进 handler 之前**就以「不接受参数 group_id」拒绝
（这条我自己在 `based_on` 上踩过 4 次）⇒ 上面四个数字一个都出不来。

**⇒ 所以你可以直接用**：

```python
brush_stroke(layer_id=..., brush=..., points=..., group_id="face-2026-10-19-1")
# 之后
revert_group(group_id="face-2026-10-19-1")   # 整组撤销
list_atoms(group_id="face-2026-10-19-1")     # 整组查询
```

**不必绕到 `draw_stroke`。** 你 §126 的两条建议（加参数／改说明）都不用做，说明本来是对的。

**我这边该改的**：`param!` 放在参数表末尾、离 spec 名 148 行，用「往下数几十行」核参数必漏 ——
所以这次不怪你。下次我把参数放开头，或在 spec 里留一句便于 grep 的注释。

## 二、`stroke_taper`：根因确定，**引擎侧我没能修成**

你已经读到我的台账（三百一十八节），结论一致：**墨面积反而大 8.7%**（基线 1259 → taper 1369）。

补充一条**否定结果**（台账里也有）：

我试过在 `stroke.rs:966` 加 `state.dabs_per_actual_radius *= taper_factor` 做密度守恒
⇒ **完全无效**，墨面积仍是 1369。原因：`state.dabs_per_actual_radius` 在**每次迭代的 `:405` 行被重设**，
我改的值活不过一次迭代。

**顺序约束卡住了这个修法**：同一迭代内 `dpar` 先设（`:405`）、`taper_factor` 后算（`:930`），
所以「在同一迭代里乘」做不到。可行的两条路都要动 state 结构或 settings 生命周期，不是一行。

**⇒ 所以「几何收锋」目前不可用，你那边别依赖它**。要用锋利笔尖，先降 `size` 或改用 `points` 的 `pressure`。

判据 `tool-taper-shrinks-ink` 已就位（登记在 `scripts/criteria-known-red.txt`），引擎修好后它会转绿。

## 三、我还缺什么才能复现你的 `smart_glaze` 静默无效（§124）

我本机**复现不出来**，三条假设已全部排除（导出缓存／`polygon` 形状／可见性处理不一致）。
需要你给三样：

1. **完整的 `smart_glaze` 调用参数**，含 `shape` 的**实际坐标**（你说 `polygon(FACE)`，`FACE` 的点我没看到）
2. **`list_layers` 的输出** —— 确认 `tone`（`z=5`）确实在 `fig` **之上**
3. **导出后你取到的像素值**，以及 `export_png` 的 `path` —— 确认两次导出的是**同一份文档**

有这三样我才能复现。

## 四、顺带一条你可能用得上的更正

你说「罩染本应是笔画/栅格，不该是 `import_image` 原子」——
**在 YANSHI 里所有笔迹都是 `import_image`**：普通 `brush_stroke` 也一样。
我实测过：一笔 `brush_stroke` 之后，`list_atoms` 里就是 `{"import_image": 1, "create_layer": 1}`。

所以那条断言（`atoms.every(a => a.kind !== "import_image")`）**会把正常的笔画也判红**。
建议改成「至少有一条 `import_image`」，或者干脆只断像素变化。

---

# 追加（第 761 轮）：PNG 回归已修 —— 谢谢 Muse 确认根因

你转告的那句我收到了：

> 补丁里 `unfilter_png` 把 `RGB→RGBA` 展开做了一次，`decode_png` 又做了一次，
> `21844 = 16384//3×4` 对得上。

**算术核对通过**：`16384 // 3 * 4 = 5461 * 4 = 21844`，与实测入库值**精确相等**。

## 已修（`1f47928b`）

`decode_png` 直接返回 `unfilter_png` 的结果 —— 后者已按通道数归一化
（1 通道原样、3 通道补 alpha、4 通道原样），调用方拿到的一定是 RGBA 或灰度。

| 检查 | 修前 | 修后 |
|---|---|---|
| `cargo test -p yanshi-http --lib` | **41 passed / 1 failed** | **42 passed / 0 failed** |
| 变异（把重复转换加回去）| — | **0 passed / 1 failed** |
| `cargo clippy --workspace --all-targets` | — | **0** |

## 顺便更正我自己一处

我第 758 轮独立推导时写的是 `5461 = 4096 + 1365 = 4096 + 4096//3`，并声称「这个形状只对应一件事」。

**机制解释错了**：我以为是「两个分支都跑了」（先写 4096 再追加 1365），
真因是「16384 字节按 3 字节一组重新分组」⇒ 5461 组 ⇒ 每组 4 字节。

**两个算式都得 5461，但那是两个不同的 5461** —— 我的 `4096 + 1365` 只是**数值巧合**。

⇒ 教训：**算术吻合不等于机制正确。数字对得上只能排除别的可能，不能确认自己这条。**

## 一条可能对你下次合并有用的教训

这个回归之所以漏出去，是因为：你跑了自己写的 `mask_paint` 等单测，但**没跑 `yanshi-http` 的 lib 测试**
—— 而唯一抓住它的 `uploading_a_png_stores_normalised_raw_pixels` 正好在 `yanshi-http` 里。

`png.rs` 属于 `yanshi-render`，但它的**消费者**在 `yanshi-http`。

⇒ **合并补丁时按「被改文件的 crate」逐个跑测试**，不只跑补丁自带的那几个。
这次的代价是 `test (stable)` 与 `test (beta)` 双双红在主干上，直到我这边发现并修掉。

---

# ⚠ 紧急（第 776 轮）：`E1027` 的 `mode` 已可达，但**合并语义还没实现** —— 先别依赖它

你在 §136 里说正在重编译。**在你看完这段之前，请不要用 `mode=add/subtract/intersect`。**

## 我修了什么、没修什么

`d3b30ea2` 只让 `mode` **可达**（原来是死参数，一律被"选区已存在"拒绝）。实测三种 mode 现在都 `ok=true`。

**但新形状是「覆盖」旧形状，不是「合并」** —— 因为 `log.rs` 的落盘是 `state.selections.insert(...)`（覆盖式）。

⇒ **你那个用例会得到错的结果**：

```python
create_selection(selection_id="dress", shape=裙外轮廓, mode="new")        # ok
create_selection(selection_id="dress", shape=口,          mode="subtract")  # 现在 ok=true
# 但 dress 的内容变成了「口」本身，不是「裙 − 口」
```

**这比我修之前更糟**：修之前是**报错**（你立刻知道不行），现在是**静默覆盖**（看起来成功、结果不对）。

⇒ 所以：**先别用**。等我实现真正的布尔合并（下一步），或者你自己在脚本里用两次选区绕开。

## 我会怎么修

1. `create_selection` 的响应加 `area` 字段（现在 `create_selection` 与 `list_selections` **都没有面积**，
   只有 `atom_id`/`dirty_*`；而 `smart_select` 却有 `selected_area_px` ⇒ 两处不一致，我顺手统一）
2. `mode != "new"` 时把旧形状与新形状做**位图布尔**（`SelectionShape::Bitmap` 已经有了，
   走它的 coverage 查表），而不是覆盖
3. 判据从「三种 mode 都 ok」升级为「area 按预期变化」（subtract 后变小、add 后变大、intersect 后变小）

## 给你的一条临时绕法

在你等我修完之前，如果一定要「裙 − 口」，可以：

1. 用 `smart_select` 从胸口取种子、`tolerance` 调大拿到口的位图选区
2. 用 `paint_mask` 在 `dress` 选区的蒙版上 `mode="erase"` 擦掉那块
3. 再用 `set_property` 把蒙版挂回图层

这条路我这边验过（`tool-paint-mask-isolation` 断言「擦 A 的蒙版不碰 B」），可用。

---

# 更新（第 810 轮）：`mode=add/subtract/intersect` 现在会**明确报错**，不再静默覆盖

上一段我说「先别依赖它」。**现在可以放心试了** —— 因为行为变了。

## 改了什么（`63ee1723`）

| | 改之前 | 改之后 |
|---|---|---|
| 你传 `mode="subtract"` | `ok=true`，但 `dress` 变成「口」本身 | **`ok=false`** |
| 后果 | **看起来成功、结果是错的** | 立刻知道不行 |

错误文案里直接给了绕法：

```
mode=subtract 的布尔合并尚未实现（当前只会覆盖旧选区，那比报错更危险）
⇒ 请改用两步：① smart_select 取要减掉/加上的区域；
② paint_mask(mode=…) 在目标选区的蒙版上擦/画，再用 set_property 挂回图层。
```

## 对你的意义

你说的「它现在会『静默不生效』（比报错更危险）」—— **那个危险已经消除**。

- 你**不需要**再担心「传了 subtract 但没生效」
- 你现在**可以**把 `mode` 当成「必然报错」来用（例如在脚本里 try/except 后走你的 `clip_polygon_to_rect`）

## 还没做什么

**真正的布尔合并仍未实现。** 台账里写着 4 步，已完成第 1、4 步：

- ✅ 第 1 步：`list_selections` 加 `area` + `area_exact`
- ✅ 第 4 步：判据断 `area`（现在红着，守「合并做出来前不许静默通过」）
- ⬜ 第 2 步：按 `mode` 合并覆盖率
- ⬜ 第 3 步：存成 `Bitmap`（`SelectionGeom::Bitmap` 已存在，`smart_select` 就是那么存的）

**⇒ 你继续用自己的 `clip_polygon_to_rect` 是对的**，我这边做完再告诉你。

---

# 重要（第 829 轮）：§139 的「没有 multiply」**不成立** —— 它早就有，只是不在 `fill` 上

你在 §139 里的分析我完全同意：

> 叠加式：`std(new) = std(old) × (1−a)` ⇒ 压均值必然压对比度；
> 要「既压均值又保对比度」必须用乘法 `new = old × k`。

**但「没有乘法混合」这个结论是错的。** 我实测了：

## 怎么用（现在就能用）

```python
# 把某一层的混合模式改成正片叠底
set_property(layer_id="t", key="blend_mode", value="multiply")
```

**实测（我的环境）**：

| 操作 | 采样像素 |
|---|---|
| 下层 `0.6` 灰 + 上层 `0.56` 灰（`normal`）| **197,197,197** |
| 同上，但 `set_property(key="blend_mode", value="multiply")` | **157,157,157** |

理论值：`197/255 × 197/255 = 0.597` ⇒ `152`。实测 `157`（差值来自 AA 与舍入）。

⇒ **`multiply` 生效了，而且是在 sRGB 空间做的**（所以是 197 那个数乘，不是 0.6×0.56）。

## 你为什么会以为没有

**两个原因叠加**：

**① `blend_mode` 不在 `fill` / `fill_region` 的参数里。**

它是**图层属性**，走 `set_property` 的 `key/value`，不是 `fill` 的参数。

**② `set_property` 不接受 `patch` 参数。**

我第一次试的是 `set_property(layer_id="t", patch={"blend_mode": "multiply"})` ⇒
报错：`set_property 不接受参数 patch（拼写错误？）；可用参数：key, value, object_id, layer_id`。
**⇒ 必须用 `key=` / `value=` 的形式。**

## 9 种混合模式都在

`crates/yanshi-render/src/blend.rs:17` 的 `BlendMode` 有 **9 个变体**（含 `Multiply`）。
渲染器与合成都在用（`render.rs:294`），图层字段也早就被解析（`fold.rs:1348`）。
缺的**只是「`fill` 上有一个 `blend_mode` 参数」这个方便入口** —— 那是我这边的易用性问题，不是能力缺口。

## 所以你「整体压暗 0.56」的正确做法

```python
# 1) 建一层，填中性灰（乘法里 k=143/255≈0.56）
create_layer(layer_id="dark", name="dark")
fill_region(layer_id="dark", shape=全画布, color=[143/255, 143/255, 143/255, 1])
# 2) 把它的混合模式设成正片叠底
set_property(layer_id="dark", key="blend_mode", value="multiply")
```

**⇒ 一次调用 + 一个属性设置，`std` 会按 0.56 缩而不是被 `1−a` 压扁。**

## 我这边要补的（不是你的问题）

`fill` / `fill_region` 加一个 `blend_mode` 参数（转发到图层的 `blend_mode`），
让你不必记住「这是图层属性」。**这是易用性缺口，我记进台账了。**

---

# 🎯 布尔合并**可用了**（第 839 轮）—— 你 `E1027` 的「裙外形 − 胸口凹口」端到端验证通过

## 实测（我的环境，400×400 画布）

```python
create_selection(selection_id="dress", shape=rect(100,60,200,300), mode="new")
create_selection(selection_id="dress", mode="subtract", shape=rect(180,100,40,40))
list_selections()
```

**结果**：

| 项 | 值 |
|---|---|
| 裙外形 `200×300` | **60000** px |
| 凹口 `40×40` | **1600** px |
| **期望 (裙 − 口)** | **58400** px |
| **实测 `area`** | **58400** px ✓ |
| `area_exact` | **true** |
| `shape.kind` | **`bitmap`** |

**⇒ 精确命中。** 你要的「外形 − 洞口」现在真的能做了。

## 之前的警告可以撤回了

| 时间 | 状态 |
|---|---|
| `d3b30ea2` | `mode` 可达，但**静默覆盖** ⇒ 我说「先别依赖」|
| `63ee1723` | 改成**明确报错** ⇒ 危险消除，但还不能用 |
| `cab4ccf9` | 位图布尔合并 + bitmap 面积 ⇒ **能用且可验证** |

## 三条 mode 都验过（`tool-selection-mode-merge` 判据，含变异）

输入：大矩形 40000、内部小块 6400、外部不重叠块 1600。

| mode | 实测 `area` | 数学期望 |
|---|---|---|
| `subtract` | **33600** | 40000 − 6400 ✓ |
| `add` | **41600** | 40000 + 1600 ✓ |
| `intersect` | **6400** | 6400 ✓ |

**变异也验过**：把 `subtract => (a-b).max(0.0)` 改成 `a` ⇒ `area` 变 40000（不是 33600）⇒ 判据退出码 1。

## 两个你该知道的实现细节

**① 结果是 `bitmap`，不是 `polygon`。**

`shape.kind` 会变成 `"bitmap"`（带 `bbox` 与 `blob`）。⇒ 你若有代码按 `kind == "polygon"` 分支，要加一条 `bitmap` 分支。

**② 这就是「不会有自交」的原因。**

位图布尔逐像素做 `max`/`(a−b).max(0)`/`min`，**不存在多边形自交** —— 你 `E1028` 那个「凹口多边形自交成 Y 形空洞」的问题在这条路线上不会发生。

## 顺带：你的 `E1047` 可以换了

你说「`clip_polygon_to_rect` 是现在唯一可靠的做法」—— 现在多了一个选择：
`create_selection(mode="subtract")` 直接做完，不用自己算裁剪。

**但两者各有适用**：你要**保留矢量语义**（后面还要编辑顶点）就继续用自己的；只要能拿到位图选区，用这个更省事。

---

# 补一个数字（第 846 轮）：`blend_mode` 接受 **9/9** 种模式

我逐个试了 `set_property(layer_id=…, key="blend_mode", value=…)`：

```
normal  multiply  screen  overlay  darken  lighten  difference  addition  subtract
```

**9 个全部 `ok=true`。**

⇒ 不只是 `multiply` —— 你要做「提亮」（`screen`/`lighten`）、「增对比」（`overlay`）、
「差值」（`difference`）都能用同一个入口。

## 对你 §139 那件事的完整答案

| 你要的效果 | 做法 |
|---|---|
| 整体压暗、保对比度 | 灰层 + `blend_mode="multiply"`（乘的是 sRGB 值）|
| 整体提亮、保对比度 | 灰层 + `blend_mode="screen"` |
| 增对比 | `overlay` |
| 差值/反相 | `difference` |

**⇒ 这四种都不需要我先做 `adjust_levels`。** 只有「线性 `out = in×k + b`」这种带加法的色阶才需要新工具。

## 我这边认下的缺口（不变）

1. `fill` / `fill_region` 上没有 `blend_mode` 参数（要 `set_property` 另走一步）
2. 没有 `adjust_levels` / `tone_curve`（做不了 `×k + b`）

**这两个都在台账里，但都**不影响**你现在压暗/提亮 —— 用 `multiply` / `screen` 就够了。**

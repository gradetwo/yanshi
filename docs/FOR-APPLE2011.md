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

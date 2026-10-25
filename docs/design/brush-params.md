# 笔刷参数清单

**这份文件是生成的** ✗ —— `node scripts/brush-params-doc.mjs write <base> <doc> <token>` ✓；
**别手改** ✗，改了会和 `BrushSetting::ALL` 对不上，CI 的 `brush-params-doc` 判据会红 ✓。

## 两条路，两套参数

| 路径 | 参数来自 | 数量 |
|---|---|---|
| `brush_stroke` 的 `brush_params` | `.myb`（MyPaint 动力学） | **66** |
| `medium_stroke` | 介质插件（油画／水彩／…） | **14**（含 `load`／`wetness`／`color_mixing`） |

**∴ 要载墨量／湿度／光谱混色 ⇒ 走 `medium_stroke`** ✓（**∵ `.myb` 的清单里没有它们** ✗）。

## 四组语义别名（`brush_params` 里可以直接写）

| 别名 | 展开成 |
|---|---|
| `oil` | `oil-` ＋ `oil_` ＋ `flat` ＋ `mop` ＋ `knife` ＋ `smudge` ＋ `smear` ＋ `detail_brush` ＋ `imp_details` ＋ `classic-knife` |
| `watercolor` | `watercolor` ＋ `water-` ＋ `water_` ＋ `wet_` ＋ `wet-` ＋ `thin_watercolor` ＋ `wateryflat` ＋ `acrylic` |
| `sketch` | `pencil` ＋ `2b` ＋ `4h` ＋ `8b` ＋ `charcoal` ＋ `sketch` ＋ `chalk` |
| `ink` | `calligraphy` ＋ `ink` ＋ `textured_ink` ＋ `sumi` ＋ `dip-pen` ＋ `brushpen` ＋ `kabura` ＋ `penbrush` |
| `pastel` | `pastel` ＋ `chalk` ＋ `smudge` ＋ `smear` ＋ `charcoal-blur` |
| `marker` | `marker` ＋ `felt` ＋ `chisel` ＋ `brushpen` |
| `pencil` | `pencil` ＋ `graphite` ＋ `charcoal` ＋ `crayon` ＋ `2b` ＋ `4h` ＋ `8b` |
| `pixel` | `pixel` ＋ `block` ＋ `hard-edge` ＋ `square` |
| `hand_jitter` | `slow_tracking` ＋ `tracking_noise` ＋ `offset_by_random` |
| `speed_gain` | `speed1_gamma` ＋ `speed2_gamma` ＋ `speed1_slowness` ＋ `speed2_slowness` |
| `color_variance` | `change_color_h` ＋ `change_color_v` ＋ `change_color_hsl_s` |
| `smudge_amount` | `smudge` ＋ `smudge_length` ＋ `smudge_transparency` |

## 66 个规范名（`BrushSetting::ALL`，按字母序）

- `opaque`
- `opaque_multiply`
- `opaque_linearize`
- `radius_logarithmic`
- `hardness`
- `anti_aliasing`
- `dabs_per_basic_radius`
- `dabs_per_actual_radius`
- `dabs_per_second`
- `radius_by_random`
- `speed1_slowness`
- `speed2_slowness`
- `speed1_gamma`
- `speed2_gamma`
- `speed_ink`
- `pressure_shape`
- `stroke_taper`
- `offset_by_random`
- `offset_by_speed`
- `offset_by_speed_slowness`
- `slow_tracking`
- `slow_tracking_per_dab`
- `tracking_noise`
- `color_h`
- `color_s`
- `color_v`
- `restore_color`
- `change_color_h`
- `change_color_l`
- `change_color_hsl_s`
- `change_color_v`
- `change_color_hsv_s`
- `smudge`
- `smudge_length`
- `smudge_length_log`
- `smudge_radius_log`
- `eraser`
- `stroke_threshold`
- `stroke_duration_logarithmic`
- `stroke_holdtime`
- `custom_input`
- `custom_input_slowness`
- `elliptical_dab_ratio`
- `elliptical_dab_angle`
- `direction_filter`
- `lock_alpha`
- `colorize`
- `snap_to_pixel`
- `pressure_gain_log`
- `posterize`
- `posterize_num`
- `paint_mode`
- `smudge_transparency`
- `offset_y`
- `offset_x`
- `offset_angle`
- `offset_angle_asc`
- `offset_angle_view`
- `offset_angle_2`
- `offset_angle_2_asc`
- `offset_angle_2_view`
- `offset_angle_adj`
- `offset_multiplier`
- `gridmap_scale`
- `gridmap_scale_x`
- `gridmap_scale_y`

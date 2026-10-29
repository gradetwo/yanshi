# 水彩 skill 第 1 轮证据附录（2026-10-28）

- 本文件是水彩 skill v3 通用版的证据附录。
- 本文件收第 1 轮的具体数字，供查证。
- 数字只取第 1 轮磁盘证据。
- 数字不编造。
- 证据里找不到的数字不写。
- 找不到的数字写明无记录。
- 本文件按规则号组织。
- 每节对应 v3 的一条规则。
- 全文用中文加 ASD-STE100 格式。
- 每句话只写一个意思。
- 句子要短。
- 一件事写一行。

## 证据来源总表

- 底本：skills/watercolor-v2-detailed.md。
- 轮次报告：REPORT.md。
- 诊断报告：DIAGNOSIS-ROUND1.md。
- 进度日志：watercolor/progress.log。
- 计时文件：watercolor/timer.json。
- 笔记：watercolor/NOTES-watercolor.md。
- 缺口记录：watercolor/GAPS-watercolor.md。
- 探针报告：probe/PROBE.md。
- 独立复核：eye-check/INDEPENDENT-CHECK.md。
- 执行脚本与规划文件：watercolor/*.py 与 watercolor/work/*。
- 本附录的相对路径以 paint-copy/round1-20261028/ 为根。

## §A-0 画布、背景与包

- 画布尺寸是 3475×3840 像素。
- 画布宽是 3475 像素。
- 画布高是 3840 像素。
- 来源是 watercolor/restart.py 第 13 行。
- 背景色是 [228, 219, 200, 255]。
- 来源是 watercolor/restart.py 第 40 行。
- 同一背景色也见 watercolor/phase1.py 第 62 行。
- 同一背景色也见 watercolor/swatch.py 第 30 行。
- 终版文档名是 hare-watercolor3。
- 终版成画文件是 watercolor/hare-watercolor-final.png。
- 成画尺寸是 3475×3840 像素。
- 成画大小是 5,282,746 字节。
- 成画 sha256 是 a524bf52 开头。
- 来源是 eye-check/INDEPENDENT-CHECK.md 第 7 行。
- 工程包是 watercolor/hare-watercolor3.yanshi。
- 工程包大小是 45,622,784 字节。
- 工程包 sha256 是 b3e50cb744314a0646f686c88589fea8027874da61ce6189af418b4553d07bd7。
- 工程包大小与哈希已用 sha256sum 核对一致。
- 落笔层标识是 layer_01M606QXCRW06QAQYS7ZY10001。
- 来源是 watercolor/apply_stage.py。

## §A-1 版本与自检

- 二进制是 yanshi-mcp 0.3.0。
- 提交是 48f3190。
- 发布包是 yanshi-mcp-release-48f3190.tar.gz。
- 包 sha256 是 78dd09efa53d1509fd89176017b3a7be06a129422693cf90fd0519200ff209d0。
- 自检命令是 python3 scripts/mcp.py --selftest。
- 自检 10 项，失败 0 项。
- tools/list 共 206 个工具。
- 正常一笔的 painted_pixels 是 10,914。
- 正常一笔的 ink_footprint_pixels 是 10,909。
- 正常一笔的 max_channel_delta 是 131。
- 来源是 watercolor/NOTES-watercolor.md 头部。
- 来源是 watercolor/progress.log 2026-10-28 11:54 CST 行。
- 来源是 probe/PROBE.md §1 与 §2。

## §A-2 模糊阶梯笔数

- 五档笔数见下表。
- 表头值已用规划文件核对一致。

| 档 | b | 笔刷 | size（像素） | opacity | 笔数 |
|---|---|---|---|---|---|
| 1 | 30 | 100%_Opaque.myb | 130 | 0.55 | 419 |
| 2 | 12 | 100%_Opaque.myb | 60 | 0.60 | 1690 |
| 3 | 5.4 | 100%_Opaque.myb | 30 | 0.65 | 2500 |
| 4 | 2.8 | long_grass | 16 | 0.75 | 2000 |
| 5 | 1.15 | long_grass | 9 | 0.80 | 3000 |

- 五档笔数合计是 9609 笔。
- 修补档 1 是 b 2.8，long_grass，size 12，opacity 0.80，1000 笔。
- 修补档 1 的文件是 work/strokes-halo.json。
- 修补档 2 是 b 1.15，long_grass，size 7，opacity 0.85，300 笔。
- 修补档 2 的文件是 work/strokes-ear.json。
- 起稿是 17 笔。
- 细节是 37 笔。
- 纸面修复是 3 笔。
- 画笔总数是 10,986 笔。
- 总数以 get_stroke_log 分页核数为准。
- 总数含 20 个渐变步。
- 总数全是 kind=brush。
- atoms.jsonl 共 11,024 行。
- 来源是 watercolor/NOTES-watercolor.md 分阶段表。
- 来源是 watercolor/GAPS-watercolor.md G-W10。
- 废弃文档 b=30 是 747 笔。
- 废弃文档 b=12 是 2500 笔。
- 废弃文档值不可用。
- 来源是 watercolor/progress.log 2026-10-28 12:40 CST 行。

## §A-3 残差取点实测

- 默认掩膜是 [150:3400, 250:3400]。
- 默认掩膜是行 150 至 3400，列 250 至 3400。
- 来源是 watercolor/plan_stage.py 第 27 行。
- 换算成分数是行 0.039 至 0.885，列 0.072 至 0.978。
- b=12 档主体区 mean|d| 是 10.60。
- b=12 档全图 mean|d| 是 7.69。
- 该残差只作诊断，不作验收依据。
- 来源是 watercolor/NOTES-watercolor.md 每阶段目检结论一节。
- 废弃文档 b=30 档残差是 21.50 变 6.08。
- 废弃文档 b=12 档残差是 8.07 变 4.34。
- 废弃文档 b=5.4 档残差是 9.48 变 6.12。
- 来源是 watercolor/progress.log 2026-10-28 12:40 CST 行。
- 来源是 watercolor/progress.log 2026-10-28 12:47 CST 行。

## §A-4 笔触几何实测

- 规划笔首笔示例是半长 117 像素。
- 该示例对应 size 130。
- 来源是 work/strokes3-b30.json 首笔。
- humanize 开与关逐像素 0 差异。
- pressure_curve auto 与 flat 逐像素 0 差异。
- 两项差异结论来自探针 15 项全测。
- 来源是 probe/PROBE.md §3 与 §4。

## §A-5 方向角实测

- b=2.8 档首笔旋转前角度是 86.82 度。
- b=2.8 档首笔旋转后角度是 176.82 度。
- b=1.15 档首笔旋转前角度是 95.19 度。
- b=1.15 档首笔旋转后角度是 185.19 度。
- 修补档 1 首笔旋转后角度是 225.0 度。
- 修补档 2 首笔旋转后角度是 228.81 度。
- 来源是 work/strokes3-b28.json 首笔。
- 来源是 work/strokes3-b28r.json 首笔。
- 来源是 work/strokes3-b115.json 首笔。
- 来源是 work/strokes3-b115r.json 首笔。
- 来源是 work/strokes-halor.json 首笔。
- 来源是 work/strokes-earr.json 首笔。
- 来源是 watercolor/final_pass.py 第 49 行。
- 旋转后文件只覆盖 b=2.8 档与 b=1.15 档。
- b=30、b=12、b=5.4 三档无旋转后文件。

## §A-6 尺寸阶梯实测

- 试笔文档尺寸是 3475×3840 像素。
- 试笔颜色是 [120, 90, 60]。
- 试笔尺寸档是 36、100、300、480。
- 试笔 medium 档是 size 100 与 size 420。
- size 36 一笔 0.8 秒。
- size 100 一笔 5.5 秒。
- size 300 一笔 47.3 秒。
- size 480 一笔 117.8 秒。
- 同尺寸 100%_Opaque.myb 一笔 0.1 秒。
- ladder2 带 mean|d| 是 wc-dense 23.5。
- ladder2 带 mean|d| 是 opaque-half 133.5。
- medium_stroke 的足迹约 144k 像素。
- medium_stroke 的线宽约 20 像素。
- 来源是 watercolor/size_test.py 全文。
- 来源是 watercolor/ladder2.py 全文。
- 来源是 watercolor/GAPS-watercolor.md G-W1。
- 来源是 watercolor/GAPS-watercolor.md G-W2。
- 来源是 watercolor/progress.log 2026-10-28 12:20 CST 行。
- 来源是 watercolor/progress.log 2026-10-28 12:35 CST 行。

## §A-7 掩膜与纸面

- 剪影掩膜文件占画布 46.8%。
- 腐蚀后掩膜文件占画布 40.5%。
- 另一掩膜文件占画布 43.2%。
- 43.2% 是实测文件均值。
- 来源是 watercolor/NOTES-watercolor.md。
- 来源是 watercolor/progress.log 2026-10-28 13:02 CST 行。
- 来源是 watercolor/progress.log 2026-10-28 14:10 CST 行。
- 废弃文档共 6,148 个原子。
- 废弃文档纸面布满淡米色胶囊。
- 废弃文档整版废弃。
- 终版文档纸面干净。
- 纸面污渍 |d| 可达约 40。
- 来源是 DIAGNOSIS-ROUND1.md W-1。
- 掩膜文件清单如下。
- 剪影掩膜文件是 work/hare-mask.npy。
- 腐蚀后掩膜文件是 work/hare-mask3.npy。
- 另一掩膜文件是 work/hare-mask26.npy。
- 边缘光晕带文件是 work/halo-band.npy。
- 局部细节区掩膜文件是 work/ear-mask.npy。
- 各阶段用哪个掩膜文件无记录。
- 掩膜构造无构造脚本。
- 来源是 skills/watercolor-v2-detailed.md §20.1 补充说明。

## §A-8 锚点表

- 画布宽是 3475 像素。
- 画布高是 3840 像素。
- 换算公式是 x 等于 round(fx×3475)。
- 换算公式是 y 等于 round(fy×3840)。
- 分数坐标表如下（A01 至 A15）。

| 编号 | fx | fy | x（像素） | y（像素） |
|---|---|---|---|---|
| A01 | 0.335 | 0.125 | 1164 | 480 |
| A02 | 0.615 | 0.075 | 2137 | 288 |
| A03 | 0.470 | 0.300 | 1633 | 1152 |
| A04 | 0.580 | 0.360 | 2015 | 1382 |
| A05 | 0.603 | 0.530 | 2095 | 2035 |
| A06 | 0.710 | 0.609 | 2467 | 2339 |
| A07 | 0.720 | 0.630 | 2502 | 2419 |
| A08 | 0.300 | 0.300 | 1042 | 1152 |
| A09 | 0.120 | 0.480 | 417 | 1843 |
| A10 | 0.350 | 0.680 | 1216 | 2611 |
| A11 | 0.550 | 0.700 | 1911 | 2688 |
| A12 | 0.720 | 0.830 | 2502 | 3187 |
| A13 | 0.520 | 0.820 | 1807 | 3149 |
| A14 | 0.880 | 0.720 | 3058 | 2765 |
| A15 | 0.550 | 0.900 | 1911 | 3456 |

- 像素坐标由分数坐标按换算公式算出。
- 像素坐标已用起稿脚本核对一致。
- 来源是 watercolor/restart.py 第 15 至 22 行。
- 本轮 15 个锚点，块面位置对。
- 来源是 watercolor/NOTES-watercolor.md skill 方法一节。

## §A-9 取证采样点

- 起稿 sample_colors 采 3 点。
- 起稿采样点是 [973, 1200]、[1216, 2381]、[173, 192]。
- 来源是 watercolor/restart.py 第 125 至 128 行。
- 首版 sample_colors 采 2 点。
- 首版采样点是画内点与空白角点。
- 空白角点是分数 (0.05, 0.05) 换算的像素点。
- 起稿收据通过数见 restart.py 输出。
- 来源是 watercolor/phase1.py 第 50 至 56 行。
- 来源是 watercolor/phase1.py 第 120 至 122 行。

## §A-10 颜色与透明度实测

- 背景色对象是 [228, 219, 200, 255]。
- 来源是 watercolor/restart.py 第 40 行。
- 起稿颜色示例是 [174, 163, 149]。
- 起稿颜色示例是 [113, 96, 78]。
- 来源是 watercolor/restart.py 第 47 至 49 行。
- selftest 的正常一笔带 4 分量颜色。
- 该笔触发一条 warnings。
- warnings 文本是 color.a 目前被忽略，透明度用 opacity。
- 该 warnings 不阻止落笔。
- opacity 0.05 使 delta 从 235 压到 86。
- 来源是 probe/PROBE.md §3。
- 来源是 probe/PROBE.md §4。

## §A-11 会话环境值

- 内存预算是 YANSHI_DOC_MEMORY_BUDGET_MB=6144。
- 导出目录是 YANSHI_EXPORT_DIR=/data/home/hare-work/exports。
- 该导出目录值是盒上路径。
- 二进制路径是 /data/home/yanshi-mcp-48f3190/yanshi-mcp-release-48f3190/bin/yanshi-mcp。
- 工作根目录是 /data/home/hare-work。
- 来源是 watercolor/apply_stage.py 第 16 至 24 行。
- 新会话直接发 batch 时，3 笔全失败。
- 失败时 all_ok 是 False。
- 失败时原子数不变。
- 先 get_document 后，同一批成功。
- 重复发 initialize 后，batch 全部失败。
- 重复发 initialize 的报错只有 unknown。
- 重复发 initialize 的失败数等于调用数。
- 默认活动文档是 1024×1024。
- 来源是 watercolor/GAPS-watercolor.md G-W3。
- 来源是 watercolor/GAPS-watercolor.md G-W6。
- 来源是 probe/PROBE.md §8 第 1 项。
- 来源是 probe/PROBE.md §8 第 2 项。

## §A-12 批次耗时

- 落笔层标识是 layer_01M606QXCRW06QAQYS7ZY10001。
- 来源是 watercolor/apply_stage.py。
- b=1.15 档一批约 59 秒。
- 该批耗时见 progress.log 14:26 行。
- b=1.15 档在 N 约 9600 时一批约 85 秒。
- 该批耗时见 NOTES-watercolor.md 分阶段表后注。
- 来源是 watercolor/progress.log 2026-10-28 14:26 CST 行。

## §A-13 画笔数口径

- get_stroke_log 总数是 10,986。
- 总数全是 kind=brush。
- 总数含 20 个渐变步。
- list_atoms total 同为 10,986。
- list_atoms total 只计画笔原子。
- atoms.jsonl 共 11,024 行。
- 11,024 行等于 10,986 加 38。
- 38 行是 15 锚点加 1 建文档加 1 建图层加 11 选区加 10 draw_shape。
- 来源是 watercolor/count_strokes.py 全文。
- 来源是 watercolor/NOTES-watercolor.md 分阶段表后注。
- 来源是 watercolor/GAPS-watercolor.md G-W10。

## §A-14 导出与核验

- 导出路径是 hare-watercolor3.yanshi。
- 重导路径是 hare-watercolor3b.yanshi。
- 重导包文件不在磁盘目录中。
- 重导逐字节相同的结论取自进度日志。
- 来源是 watercolor/package_final.py 全文。
- 来源是 watercolor/reexport.py 全文。
- 首版包大小是 45,622,784 字节。
- 首版包 blob_count 是 10,933。
- 首版包 sha256 是 b3e50cb744314a0646f686c88589fea8027874da61ce6189af418b4553d07bd7。
- 核验 referenced 是 15,640。
- 核验 present 是 10,933。
- 核验 missing 是 4,707。
- 核验 missing_body 是 45。
- 核验 missing_tile 是 4,662。
- 核验 atoms 是 11,024。
- 45 个缺本体涉 53 个原子。
- 53 个原子是 4 根画框线加 20 个渐变步加 31 根细节笔。
- 重导包与首版逐字节相同。
- 重导包大小仍是 45,622,784 字节。
- 重导后 missing_body 仍为 45。
- 来源是 watercolor/NOTES-watercolor.md 工程包一节。
- 来源是 watercolor/GAPS-watercolor.md G-W9。
- 来源是 watercolor/progress.log 2026-10-28 17:15 CST 行。
- 来源是 watercolor/progress.log 2026-10-28 06:40 UTC 行。
- 来源是 DIAGNOSIS-ROUND1.md W-2。

## §A-15 纸面修复实测

- 种子共 6 个。
- 种子文件是 work/fix-seeds.json。
- 种子坐标与颜色见下表。

| 序号 | 种子坐标 | 像素数 | 颜色 |
|---|---|---|---|
| 1 | [520, 1270] | 172088 | [228, 218, 199] |
| 2 | [958, 910] | 149156 | [233, 224, 209] |
| 3 | [1972, 790] | 60084 | [230, 219, 204] |
| 4 | [2328, 646] | 26816 | [230, 220, 203] |
| 5 | [2204, 3302] | 18800 | [230, 218, 199] |
| 6 | [2654, 2162] | 11120 | [212, 202, 186] |

- 种子表已用 fix-seeds.json 核对一致。
- 容差 0.25 时只选中 38% 至 74%。
- 容差 0.45 时选区面积是 5,045,626 像素。
- 容差 0.45 时选区面积是 11,457,113 像素。
- 容差 0.45 时选区面积是 13,334,281 像素。
- 容差 0.45 时一个选区漫过约 67% 画布。
- 6 个种子中只有 1 个选区可用。
- 可用选区的面积是 53,984 像素。
- effective_delta_e 恒为 0.1165。
- effective_delta_e 与容差无关。
- 一遍盖后笔迹由 [78, 68, 58] 变 [约 200, 190, 175]。
- 一遍盖后笔迹仍可见。
- 二遍盖后笔迹到 [217–226, 208–216, 189–198]。
- 二遍盖后笔迹接近纸色。
- 一遍只盖约 75%。
- 第三步多边形颜色示例是 [229, 215, 191]。
- 第三步多边形颜色示例是 [230, 220, 203]。
- 多边形文件是 work/fix-polys.json。
- 多边形共 2 个。
- 来源是 watercolor/fix_select.py 全文。
- 来源是 watercolor/fix_paint.py 全文。
- 来源是 watercolor/fix_poly.py 全文。
- 来源是 watercolor/work/fix-seeds.json 全文。
- 来源是 watercolor/work/fix-polys.json 前两项。
- 来源是 watercolor/GAPS-watercolor.md G-W7。
- 来源是 watercolor/GAPS-watercolor.md G-W8。
- 来源是 DIAGNOSIS-ROUND1.md W-8。
- 来源是 DIAGNOSIS-ROUND1.md W-9。

## §A-16 细部实测

- 细部共 37 笔。
- rigger 笔共 29 笔。
- opaque 笔共 8 笔。
- 本轮细部 37 笔，落笔 14.0 秒的记录无原始日志。
- 该秒数不作规则。
- 来源是 watercolor/detail_pass.py 全文。
- 来源是 watercolor/work/strokes-detail.json 表头。
- 来源是 watercolor/final_pass.py 第 52 行。
- 符号画错的形状是 "////" 加三角。
- 符号画错后，纸色多边形盖两遍才盖净。
- 来源是 DIAGNOSIS-ROUND1.md W-3。
- 来源是 DIAGNOSIS-ROUND1.md W-4。

## §A-17 试笔实测

- swatch 文档是 1400×720 像素。
- swatch 背景色是 [228, 219, 200, 255]。
- swatch 共 8 行。
- swatch 每行三点，x 从 80 到 1200。
- swatch 行距是 80 像素。
- swatch 尺寸全是 36 像素。
- swatch 颜色全是 [120, 90, 60]。
- swatch 三点压力是 [0.35, 0.9, 0.35]。
- swatch 三点时序是 0、150、300。
- swatch medium 的 load 是 0.8。
- swatch 导出图是 swatch.png。
- swatch 8 笔全落笔，全带收据。
- 试笔后选定的笔刷是 5 支。
- 选定的 5 支是 watercolor-02-paint、watercolor_expressive、long_grass、rigger_brush_thin、medium watercolor。
- 来源是 watercolor/swatch.py 全文。
- 来源是 watercolor/size_test.py 全文。
- 来源是 watercolor/progress.log 2026-10-28 11:54 CST 行。
- 来源是 watercolor/NOTES-watercolor.md 试笔结论一节。

## §A-18 成本模型实测

- 计时起点是 2026-10-28T03:49:49Z。
- 计时起点 epoch 是 1793159389。
- 计时终点 epoch 是 1793168706。
- T(10000) 拟合值是 1628 秒。
- T(10000) 实测值是 1598.1 秒。
- 拟合误差是 1.9%。
- RSS 在 10,000 笔时峰值是 2009.9 MB。
- RSS 公式在 10,000 笔时给 1905.7 MB。
- 两值相差约 104 MB。
- 相差原因未解释。
- 50,000 笔外推时间是 12.2 小时。
- 50,000 笔外推 RSS 约 9475 MB。
- 盒内存是 7964 MB。
- 水彩冷启动总时间是 9317 秒。
- 水彩冷启动总时间是 155.3 分钟。
- 水彩实测批耗时与模型之比约 1.3 至 1.9 倍。
- 水彩 long_grass 批耗时最高，约为模型 1.9 倍。
- 该比值来自诊断报告 C-14。
- 默认预算是 2048 MB。
- 默认预算拒绝重开 10,000 笔文档。
- 重开估算内存是 2930 MB。
- 内存预算 6144 MB 通过重开。
- 重开估算 0.293 MB/笔无测量出处。
- 来源是 probe/PROBE.md §5.1。
- 来源是 probe/PROBE.md §6。
- 来源是 probe/PROBE.md §8 第 4 项。
- 来源是 watercolor/timer.json 全文。
- 来源是 watercolor/work/timer_start.txt。
- 来源是 watercolor/work/timer_start_epoch.txt。
- 来源是 DIAGNOSIS-ROUND1.md C-14。
- 来源是 REPORT.md §2 计时备注。

## §A-19 长活事故实测

- 子进程 PID 是 415362。
- 子进程存活时 CPU 占 99%。
- 子进程继续提交，600 笔不是 400 笔。
- 废弃文档共 6,148 个原子。
- 废弃后重启为新文档。
- 重启后用剪影掩膜。
- 来源是 watercolor/GAPS-watercolor.md G-W4。
- 来源是 watercolor/NOTES-watercolor.md 事故与修复一节。
- 来源是 watercolor/apply_stage.py 第 41 至 44 行。
- 来源是 watercolor/progress.log 2026-10-28 13:02 CST 行。
- 来源是 watercolor/progress.log 2026-10-28 13:25 CST 行。
- 来源是 DIAGNOSIS-ROUND1.md W-10。
- 来源是 DIAGNOSIS-ROUND1.md C-4。

## §A-20 目检验收实测

- 本轮 1:1 一眼通过。
- 本轮 3x 三处不通过。
- 本轮整图不通过。
- 三处不通过是一处扁平椭圆。
- 三处不通过是一处扁平暗块加两根越界直线。
- 三处不通过是一处胶囊马赛克无单根线。
- 三处部位的映射见 DIAGNOSIS-ROUND1.md W-4。
- 3x 裁切图文件如下。
- crop3x-eye.png 是 2420×1200 像素。
- crop3x-ear.png 是 2420×1200 像素。
- crop3x-ear-tip.png 是 2030×1060 像素。
- crop3x-back.png 是 2420×1200 像素。
- crop3x-back-fur.png 是 2030×1060 像素。
- 裁切图尺寸已用图像文件核对一致。
- 裁切矩形的源坐标无脚本记录。
- 源坐标不写。
- 三联对照图是 watercolor/triptych-final.png。
- 三联对照图是 1838×1050 像素。
- b=12 档主体区 mean|d| 是 10.60。
- b=12 档全图 mean|d| 是 7.69。
- 残差只作诊断，不作验收依据。
- 来源是 watercolor/NOTES-watercolor.md 每阶段目检结论一节。
- 来源是 REPORT.md §2 水彩行。
- 来源是 DIAGNOSIS-ROUND1.md W-4。
- 来源是 eye-check/INDEPENDENT-CHECK.md 水彩一节。

## §A-21 补测条目计数说明

- v3 §20 的补测条目是 46 项。
- 46 项是水彩 19 项加共性 27 项。
- 底本文件信息写补测 44 项。
- 底本写水彩 19 项加共性 25 项。
- 底本 §20.2 实列共性 27 项。
- v3 按实列数写 46 项。
- 底本文件信息与实列数的差已记在本节。
- 底本未改。

## 文件信息

- 文件路径是 skills/watercolor-evidence-round1.md。
- 文件版本是第 1 轮证据附录。
- 文件日期是 2026-10-28。
- 附录节数是 22 节。
- 附录节是 §A-0 至 §A-21。
- 本文件未碰 git。
- 本文件未碰上游。


## 第 2 轮证据（2026-10-29，binary 9c481d63，修复程序期 1–7）

- 画作：水彩《野兔》。画布 3475×3840。时间 138.7 分钟（第 1 轮 155.3 分钟，省 16.6 分钟）。笔数 20,318（全是 brush；第 1 轮 10,986 笔）。
- 目检：1:1 通过；3x 眼部分通过（推翻执行 agent 的“眼通过”：眼形在，周围仍是胶囊拼贴，眼上方有不明条纹扇形伪影）、耳尖部分通过、背毛部分通过；整图不通过。
- 单根毛：rigger size 2–3 五点弧线画出干净单根毛，顺毛流；首版因 stride bug（np.where 网格下标当画布坐标）全部毛发落左上角，已修 stride、正位重画 2,570 笔。
- 胶囊底：单根毛现已可见，但胶囊底仍透出，毛量不足。
- 越界：规划点在掩膜内，但笔触半长（size×0.9）越出轮廓；起稿块（size 260–420）越出达约 380px；纸面留淡胶囊圈与新月形残留。
- 擦除边：横向纸色胶囊沿曲轮廓拼出扇贝状白边（纸环 1 size 20、纸环 2 size 12 后，3x 轮廓白扇贝边）。
- 探针：medium_stroke size 有效（size 100 足迹 52,245、size_used=100；size 420 足迹 281,774、size_used=420）；小尺寸（≤36）均匀淡洗 0.07 秒；size 100 条纹状 0.41 秒；size 300 条纹状淡洗 3.29 秒；gradient_blend 10 步阶梯圆盘、40 步平滑。
- smart_select：容差 0.25/0.35/0.45 选区面积同约 0.72%，effective_delta_e 随容差走 0.146/0.232/0.335，无断崖。
- missing_body：导出包 missing_body=0（第 1 轮的 45 已消除；referenced=22,704，present=17,224，missing_tile=5,480）。
- 预算：20,318 笔逼近 6,144 MB 预算（约剩 190 MB 余量）。
- fill_region：本轮矩形纸盖一律盖两遍，沿用第 1 轮“一遍约 75%”结论，未在本 binary 上量测；纸色笔一遍即达纸色（不透明度 1.0）。
- 导出：export_png 1–2 秒、export_project 3.1–4.6 秒。
- 出处：REPORT.md §2；watercolor/NOTES-watercolor.md；watercolor/GAPS-watercolor.md。

## 第 3 轮证据（2026-10-29，binary 9045faeb，通用写法）

- 本节通用写法，不写画名与画内部件名；§8 方法发现只作证据记录，不升为正文规则（须更多实画验证）。
- 第 3 轮：本风格 1:1 通过、整图不通过；时间约 45 分钟，笔数 12,284；单批请求行 ≤54,975 字节。
- 方法发现：细部笔须以渲染图实测位置复核；题识须逐字对照字形；长弧须用多点折线拟合。
- 方法发现 W-1：毛团判据在本轮笔数下未达成（5,056 笔草笔仍见胶囊底，加密遍数与草笔尺寸的量级关系未定；须更多实画验证）。
- 方法发现（通用）：X-2 防高估规则本轮执行有效（执行方自判均未高估，总协调采信全部自判）。
- 方法发现（通用）：X-1 落位先行验证成立，但细结构端点量法有缺口（本轮为目读 + 3x 图估算，非像素量测）。
- 成本硬约束全守：单批请求行 <60,000 字节，0 挂起；水位无跳闸。
- 量测 D-4b：密集重叠下湿笔环形涂抹伪影复现确认（程序侧现象，已进修复分期；不作 skill 规则）。
- 量测 U-1：旧“一遍约 75%”作废（feather 0 一遍全净 100%；feather 25 一遍留约 4 级淡痕、第二遍清净；条件见正文 v5 更新）。
- 量测 glazing 回执 254：回执口径与导出光栅口径不一致已定位（回执取 dab 证据缓冲、导出是复合后光栅；D 期修导出一侧、回执口径未动；已进修复分期；不作 skill 规则）。
- 量测项 2：changed_pixels 关闭成立（单毛对照 changed=110、导出恰 110 像素，与 d8e66638 对照点一致；见正文 v5 更新）。
- 防作弊：本风格包内源图命中 0 个（按 kind 与 blob 内容判定）。
- 出处：REPORT.md §8、§9；MEASUREMENTS-ROUND3.md。

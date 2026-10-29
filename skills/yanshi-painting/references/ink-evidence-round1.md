# 水墨第 1 轮证据附录（对应 ink-v3-generic.md，2026-10-28）

## 说明

- 本附录收第 1 轮的具体数字。供查证。
- 数字只取第 1 轮磁盘证据。不编数字。证据里找不到的数字不写。
- 磁盘根目录是 paint-copy/round1-20261028/。
- 每节先写数字，再写磁盘出处。
- v3 正文不写这些数字。正文只写相对定位法与通用参数。

## §0 画布、源图、缩放比、文档名

- 画布是 1637×3840 像素。
- 源图是 2569×6027 像素。
- 缩放比是 0.6372。
- 首试文档：shitao-landscape-r1、r1b、r1c。
- 重做文档：shitao-landscape-r1-redo。
- 纸色：首试 (211,193,163)。重做 (215,198,168)。
- 出处：ink-redo/paint-ink-redo.py 第 7 行与第 21 行。ink-redo/NOTES-ink-redo.md 基本信息一节。

## §规则 1 探针文档尺寸

- 探针文档一：1200×1050 像素。
- 探针文档二：900×700 像素。
- 出处：ink-redo/probe2.py。ink/brush-test.py。

## §规则 2 中景多边形、三峰几何、崖壁多边形

- 中景多边形 MID_POLY，10 个顶点：(446,1080)、(600,1000)、(850,985)、(1050,1010)、(1250,1060)、(1338,1180)、(1380,1400)、(1360,1657)、(446,1657)、(430,1300)。
- 峰 A：顶点 (920,700)。底 y 1051。半宽 130。40 行。lean 18。尺寸 70–170。
- 峰 B：顶点 (1085,860)。底 y 1051。半宽 62。18 行。lean 8。尺寸 60–130。
- 峰 C：顶点 (1300,640)。底 y 1083。半宽 200。44 行。lean -25。尺寸 70–180。
- 崖壁多边形 CLIFF_C：(382,1274)、(560,1300)、(650,1450)、(700,1650)、(680,1848)、(500,1848)、(420,1600)。
- 崖壁多边形 CLIFF_L：(38,1657)、(180,1680)、(223,1900)、(200,2167)、(60,2167)、(30,1900)。
- 崖壁多边形 CLIFF_R：(1211,1720)、(1350,1700)、(1450,1800)、(1529,1950)、(1500,2103)、(1250,2103)、(1211,1900)。
- 远脊调用：ridge(0,446,980,1115)，行数 8。ridge(446,900,940,1083)，行数 8。
- 雾带调用：mist_band(0,1637,980,130)，n=8，op=0.32。mist_band(1000,1637,2080,120)，n=6，op=0.3。mist_band(1100,1637,320,140)，n=4，op=0.25。
- 竹丛调用：bamboo_grove(605,1115,1466,1848,16)。bamboo_grove(620,760,1500,1800,6)。
- 出处：ink-redo/paint-ink-redo.py 第 185–240 行（peak、ridge、mist_band）。第 387–405 行（stage_S2 与 MID_POLY）。第 432–437 行（CLIFF 多边形）。第 242–292 行（branch、pine）。

## §规则 3 采样点绝对坐标

- 淡墨采样点：(400,900)。采样值 [213,197,170]。
- 出处：ink/NOTES-ink.md 诊断一节。ink-redo/paint-ink-redo.py 第 29–47 行。

## §规则 4 出处清单

- 出处：ink/GAPS-ink.md GAP-INK-6。ink-redo/GAPS-ink-redo.md G-1。ink-redo/paint-ink-redo.py 第 26 行。ink/brush-test.py 第 60–63 行。probe/probe_params-48f3190.log。
- 订正说明出处：本轮目录中无 probe-params.py 文件。顶层被拒的原始报错日志无保存文件。

## §规则 5 出处清单

- 出处：ink-redo/probe2.py。ink/brush-test.py。ink/brush-test3-log.json。ink-redo/NOTES-ink-redo.md 试笔结论一节。ink/GAPS-ink.md GAP-INK-1。ink-redo/GAPS-ink-redo.md G-9、G-10、G-11。DIAGNOSIS-ROUND1.md I-6。

## §规则 6 出处清单

- 出处：ink-redo/paint-ink-redo.py 第 123–163 行（paint_mass）。第 164–184 行（wash_mass）。第 406–425 行（S3、S3b）。第 802–803 行（S10）。第 659–707 行（S13）。ink-redo/NOTES-ink-redo.md 每阶段目检结论一节。

## §规则 7 阶段表出处

- 阶段表数字见 v3 规则 7。
- 出处：ink-redo/paint-ink-redo.py STAGES 与 ORDER（第 884–890 行）。ink-redo/NOTES-ink-redo.md 分阶段笔数与时间表。ink-redo/paint-redo.log 全文。ink-redo/state-redo.json。

## §规则 8 锚点绝对坐标

- 重做 12 个锚点：inscription (200,60)。peakA (920,720)。peakC (1300,650)。midridge (900,1000)。pavilion (790,1560)。waterfall (130,1660)。bigtree (450,1980)。house (400,3250)。willow (930,3440)。fground (400,3650)。leftpine (180,3200)。rightpine (1250,2400)。
- 首试 16 个锚点：inscription_top (327,77)。inscription_seal (164,346)。peak_right_top (1310,730)。peak_mid (982,845)。peak_left_far (131,1075)。ridge_mid_left (688,1152)。pavilion_roof (900,1670)。pavilion_base (900,1805)。cliff_left (262,1613)。tree_tall_left (196,2381)。tree_big_center (540,2611)。house_roof (540,3072)。willow_right (1179,3187)。rock_fg_left (360,3571)。rock_fg_right (1179,3648)。water_bottom (1391,3725)。
- 出处：ink-redo/paint-ink-redo.py main() S0 分支。ink-redo/fix-s2.py。ink-redo/paint-redo.log 07:32:37。ink/stage1.py ANCHORS。ink/setup-r1c.py 第 61–68 行。ink-redo/GAPS-ink-redo.md G-2。

## §规则 9 采样点绝对坐标

- 采样落点：(1300,730)。采样值 [215,199,172]。
- 空白角：(200,200)。(1617,100)。采样值均为纸色 [211,193,163]。
- 出处：ink/stage1-log.json blocks。ink-redo/paint-ink-redo.py 第 894–912 行。ink/NOTES-ink.md 第一阶段一节。ink/GAPS-ink.md GAP-INK-4。DIAGNOSIS-ROUND1.md I-3。

## §规则 10 出处清单

- 出处：ink-redo/paint-ink-redo.py 第 82–101 行。probe/probe_params-48f3190.log。probe/PROBE.md §4。ink/residual-ink.py 取点一节。

## §规则 11 出处清单

- 出处：probe/probe_params-48f3190.log。probe/PROBE.md §4。ink/stage1.py 第 28–30 行。

## §规则 12 采样点绝对坐标与导出路径

- 采样点：(920,880)。(100,100)。(790,1600)。(400,3600)。
- 终版采样值（m14）：(920,880)=[176,163,138]。(100,100)=[215,198,168]。(790,1600)=[148,135,113]。第 4 点值在日志中被截断，未记录。
- 导出路径：exports/redo-<名>.png。
- 出处：ink-redo/paint-ink-redo.py 第 890–893 行与第 925–933 行。ink-redo/paint-redo.log 08:21:12 SAMPLE 行。

## §规则 13 三联图尺寸与裁切矩形

- 三联图：3274×3840 像素。中间留白 20 像素。
- 裁切边长：400×400 像素。放大 3 倍。
- 裁切 4 处：远景晕染 (900,600)-(1300,1000)。山石皴法 (600,1900)-(1000,2300)。建筑竹丛区 (700,1500)-(1100,1900)。文字印块区 (100,0)-(500,400)。
- 出处：ink/make-compare.py 全文。ink/compare3x-*.png。

## §规则 14 出处清单

- 出处：ink-redo/fix-s2.py。ink-redo/fix-s34.py。ink-redo/fix-s4.py。ink-redo/paint-redo.log 07:44:26 与 07:47:37。ink-redo/paint-ink-redo.py 第 22 行。ink-redo/NOTES-ink-redo.md 诚实记录一节。

## §规则 15 失败笔迹范围

- S7 第一批失败 23 笔。笔迹范围示例：x -107..-39、y 2264..2303。x -136..-70、y 2235..2268。
- 首试 B1 笔迹范围：x 938..1734、y 402..1134。部分出界被裁剪，仍落笔。
- 首试 B3 笔迹范围：x 0..604、y 810..1354。部分出界被裁剪，仍落笔。
- 画布：1637×3840 像素。见 §0。
- 出处：ink-redo/paint-ink-redo.py stage_S7b。ink-redo/paint-redo.log 07:48:32。ink/stage1-log.json blocks B1 与 B3。ink-redo/GAPS-ink-redo.md G-6。

## §规则 16 出处清单

- 出处：ink-redo/paint-redo.log 07:36:58、07:48:32、07:48:49、07:50:56。ink-redo/paint-ink-redo.py 第 331–342 行。ink-redo/GAPS-ink-redo.md G-5。
- K6 出处：ink-redo/paint-redo.log 07:48:32 至 07:48:49。ink-redo/NOTES-ink-redo.md 同一处写 23 笔出画布加 2 笔短路径，也错。同 log。
- K9 出处：ink-redo/paint-redo.log 07:36:58 与 07:48:32 报错文。

## §规则 17 出处清单

- 出处：ink-redo/finalize.py。ink-redo/probe-count.py。ink-redo/paint-redo.log 08:21:15。ink-redo/NOTES-ink-redo.md 基本信息与诚实记录两节。

## §规则 18 探针画布尺寸

- 探针画布：3840×2160 像素。
- 出处：probe/PROBE.md §5.1、§6、§7。ink-redo/paint-redo.log。DIAGNOSIS-ROUND1.md C-14。ink-redo/GAPS-ink-redo.md G-7。

## §规则 19 垂枝起笔坐标范围

- S9 垂枝树木：130 根垂枝。起笔 x 800–1465。起笔 y 3060–3220。
- S13(c) 垂枝树木第二层：150 根垂枝。起笔 x 790–1470。起笔 y 3000–3300。
- 出处：ink-redo/paint-ink-redo.py stage_S9。stage_S13(b)(c)。第 270–292 行（pine）。ink-redo/NOTES-ink-redo.md 最终验收一节。

## §规则 20 字列与印块绝对坐标

- 字列：3 列。x=150、215、285。y 从 48 起。步 30 像素。至 420 前止。每列 13 个字。
- 红色印块：2 枚。(175,200) 与 (175,320)。尺寸 26。每枚 4 笔。
- K7 出处：ink-redo/paint-ink-redo.py stage_S11。y 从 48 起，步 30 像素，至 420 前止，共 13 格。
- 出处：ink-redo/paint-ink-redo.py stage_S11。ink-redo/NOTES-ink-redo.md 诚实记录一节。

## §规则 21 出处清单

- 出处：ink/residual-ink.py 全文。ink/timer.json stage2 一节。ink/NOTES-ink.md 接手记录一节。ink/progress.log。

## §规则 22 出处清单

- 出处：ink/send-ink.py 全文。ink-redo/paint-ink-redo.py 第 894–912 行。ink/GAPS-ink.md GAP-INK-5、GAP-INK-8。probe/PROBE.md §8。

## §规则 23 包与成品哈希出处

- 重做包：shitao-landscape-r1-redo.yanshi。54,034,944 字节。sha256 a826ccfd24c187d98405247371dfae59eeecb110828a8ac76c68d4ab2aace430。
- 成品 PNG：shitao-landscape-r1-redo.png。3,437,848 字节。sha256 e73c48221e5b9516b69ba87200604ba4bd80aaf45ed8a6102c722650a48ad82c。
- 出处：skill-src/yanshi-paint/scripts/check_package.py 第 131–139 行与第 174–213 行。ink-redo/NOTES-ink-redo.md 工程包核验一节。ink-redo/finalize.py。

## §规则 24 计时出处

- 重做计时：起点 2026-10-28 15:24:00 CST。终点 2026-10-28 16:21:15 CST。总时长 57.25 分钟。活笔 10,857 笔。
- 首试计时：起点 2026-10-28 11:48:53 CST。无终点。无总时长。
- 出处：ink-redo/timer.json。ink/timer.json。ink/progress.log。REPORT.md §1、§2、§8。


## 第 2 轮证据（2026-10-29，binary 9c481d63，修复程序期 1–7）

- 画作：水墨石涛《Landscape》。画布 1637×3840。时间 33.6 分钟（第 1 轮 57.25 分钟）。活笔 11,518 笔（import_image 11,587 − revert 69；第 1 轮 10,857 笔）。
- 目检：1:1 不通过；3x 远峰不通过、山石皴法不通过、亭子竹林不通过、题款印章位置通过；整图不通过。
- 落位（Wang 2026-10-29 ~06:10 定论）：水墨最差、复原度最低；亭子位置错：原作亭子在宽 58%、高 49.8%，临摹在宽 75.5%、高 86%，右移约 17.5 个百分点、下移约 36.2 个百分点；房屋本体基本在位（原作 34%、83.6% → 临摹 32.4%、89.3%）。执行 agent 自检称“位置全对”被推翻。
- 轮廓：单根长直线轮廓读成硬线框（多边形底边、崖壁边、岩石十边形轮廓、屋顶三角）；REPAIR 纸色淡化 + 跨边纹理修补效果有限。
- 层理：大面积散布 dry_brush 与 2B_pencil 颗粒笔读成均匀噪点/划痕，不成层理；3x 下是离散黑点簇。
- 松树：轮生枝等距横条读成梳子；逐棵变角度与层数未能破除重复感；REPAIR2 加不规则暗团与竖向连接笔后仍以横条为主。
- 垂柳：垂枝 260 + 200 根，太直、太密、太暗，读成均匀黑帘，拱形主枝被盖住。
- 墙面：房屋只有屋顶斜纹 + 两根墙线 + 门窗小线，墙面大面积空白，读成空轮廓；REPAIR2 加 20 笔淡墙洗（op 0.3）后仍近纸色。
- 洗染：浅色 op 0.25–0.35 时远峰洗染几乎看不见；改用深色 op 0.55–0.75 后才有体量。
- 短路径：大尺寸笔短路径被拒（size 88.8 路径 13.2 对间距 14.8；size 121.9 路径 14.8 对间距 20.3）；驱动内修段长下限 size×0.35 + 通用短路径拉伸守卫后未再出现；batch 局部失败 1 笔（all_ok=False、fail_count=1、69 个 atom_ids，无逐笔明细）。
- 笔刷行为：大尺寸反白笔画出白色扇形弧（delta 254/255）；大尺寸纹理笔出豹纹气泡团；小尺寸水彩笔 delta=3 几乎看不见；极淡笔 delta 12–21；颜色分量 >1 被接受、无警告、渲染不可预期。
- 题识：题款印章位置通过，字形抽象、不可读；两枚红块印章位置与原作一致。
- 探针：color.a 生效（a=10 delta=70、a=250 delta=235）；humanize / pressure_curve:"auto" 仍逐像素 0 差异。
- D-4b：40 笔同几何 A/B 对照未复现环形伪影；但未重跑 r1b 密集重叠条件，仍悬。
- 成本：1 千笔约 123 秒、5 千笔约 441 秒、1 万笔约 1,349 秒（旧模型拟合 1,628 秒，快约 17%）；RSS 370 MB → 1,162 MB → 2,289 MB；export_png 1.6 秒、export_project 2.3 秒；11,518 笔文档在 6,144 MB 预算下 0.8 秒重开成功。
- 包：export_project 自报 missing_derived=0，独立口径缺 4,402 个派生 tile（全是派生 tile，本体缺 0；C1/C2/C3 全过）。
- 出处：REPORT.md §4；ink/NOTES-ink.md；ink/GAPS-ink.md。

## 第 3 轮证据（2026-10-29，binary 9045faeb，通用写法）

- 本节通用写法，不写画名与画内部件名；§8 方法发现只作证据记录，不升为正文规则（须更多实画验证）。
- 第 3 轮：本风格 1:1 不通过、整图不通过；落位通过（关键结构偏差均在 10 点内）；时间 20.5 分钟，活笔 7,824；单批请求行 ≤52,000 字节。
- 方法发现：行向铺底法在 4K 竖幅山水上易读成横条席纹（规则 6 需补充“破横条”条款：斜向交叉笔占比、行间错位量；本轮加密遍斜笔未达破条阈值，阈值未量出）。
- 方法发现：叶团画法缺“叶簇成团、留空见干”的量化画法（须更多实画验证）。
- 方法发现：枯树缺主枝—次枝—细枝三级长度与角度递减画法（须更多实画验证）。
- 方法发现：小结构保护规则缺失（加密遍淹没小结构是实例；须更多实画验证）。
- 方法发现：落位先行 + 锚点 + 复核流程成立（两项关键结构偏差均在 10 点内）。
- 方法发现：续画必须复用同一 root（新 root 下同名文档静默新建是误判源）。
- 方法发现：印块实块画法成立（密排短笔，行距小于笔尺）。
- 方法发现（通用）：X-2 防高估规则本轮执行有效（执行方自判均未高估，总协调采信全部自判）。
- 方法发现（通用）：X-1 落位先行验证成立，但细结构端点量法有缺口（本轮为目读 + 3x 图估算，非像素量测）。
- 成本硬约束全守：单批请求行 <60,000 字节，0 挂起；水位无跳闸。
- 量测 D-4b：密集重叠下湿笔环形涂抹伪影复现确认（程序侧现象，已进修复分期；不作 skill 规则）。
- 量测 U-1：旧“一遍约 75%”作废（feather 0 一遍全净 100%；feather 25 一遍留约 4 级淡痕、第二遍清净；条件见正文 v5 更新）。
- 量测 glazing 回执 254：回执口径与导出光栅口径不一致已定位（回执取 dab 证据缓冲、导出是复合后光栅；D 期修导出一侧、回执口径未动；已进修复分期；不作 skill 规则）。
- 量测项 2：changed_pixels 关闭成立（单毛对照 changed=110、导出恰 110 像素，与 d8e66638 对照点一致；见正文 v5 更新）。
- 防作弊：本风格包内源图命中 0 个（按 kind 与 blob 内容判定）。
- 出处：REPORT.md §8、§9；MEASUREMENTS-ROUND3.md。

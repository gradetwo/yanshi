# 素描 skill v3 证据附录（第 1 轮）

- 本附录收第 1 轮的具体数字。
- 本附录按规则号组织。
- 本附录供查证用。
- 数字只取第 1 轮磁盘证据。
- 数字来源是 v2 引用的来源、REPORT.md、DIAGNOSIS-ROUND1.md、probe、timer.json、progress.log。
- 证据里找不到的数字不写。
- 全文中文加 ASD-STE100 格式。
- 一句话只写一个意思。
- 句子要短。
- 一件事写一行。

## §0 总览

- 画布 2824×3840 像素。
- 源图 2258×3070 像素。
- 缩放系数 1.2507。
- 二进制是 yanshi-mcp 0.3.0。
- 二进制 commit 是 48f3190。
- 二进制构建于 2026-10-27 10:27 UTC。
- 本轮工具数 206 个。
- 本轮自检 10 项，失败 0 项。
- 文档文件是 vitruvian-r1-final.yanshi。
- 总笔数 20,000 笔整。
- 冷启动 161 分钟。
- 冷启动起止是 03:48:53Z 至 06:29:36Z。
- 最终导出在 06:27:45Z。
- 到最终输出约 159 分钟。
- 161 分钟含约 2 分钟收尾。
- 落笔 96.1 分钟（5,768.6 秒）。
- 其余时间是导出、残差计算、传文件与等待。
- 最终包 56,026,624 字节。
- 最终 PNG 6,395,937 字节。
- 最终 PNG 的 sha256 前 16 位是 594634dde770dd4e。
- 包完整 sha 无磁盘记录，不写。
- 阶段笔数与落笔秒数见下表。
- 阶段表来源是 sketch/timer.json。
- b30 是 1,200 笔，20.8 秒。
- b12 是 2,000 笔，107.6 秒。
- b5.4 是 2,500 笔，294.2 秒。
- b2.8 是 2,200 笔，400.8 秒。
- b1.15 是 2,090 笔，502.7 秒。
- b1.15-2 是 5,000 笔，1,972.2 秒。
- b1.15-3 是 5,000 笔，2,470.3 秒。
- 第一阶段是 10 笔，10 秒。
- 3x 对照图三份。
- 三份是 compare3x-circle-square.png、compare3x-head-hands.png、compare3x-legs-feet.png。
- 裁切矩形无磁盘记录，不写。

## §R-01 源图坐标换算到画布坐标

- 源图尺寸 2258×3070 像素。
- 画布尺寸 2824×3840 像素。
- 缩放系数 1.2507。
- 圆心高于方形中心约 106 像素（源图坐标）。
- 同一量在画布坐标下是 133 像素。
- 106 × 1.2507 = 132.6，取整 133。
- 方形源图坐标左 330、右 1933、上 655、下 2223。
- 方形画布坐标左 413、右 2418、上 819、下 2780。
- 圆心源图坐标 (1116.5, 1332.5)，半径 909.4 像素。
- 圆心画布坐标 (1396, 1667)，半径 1137 像素。
- 位置偏差量是 27 像素。
- 来源：sketch/NOTES-sketch.md 原作量测节。
- 来源：skills/audit-work/audit-sketch-ink.md S-03。
- 来源：skills/METHOD-DETAIL-AUDIT.md §5 S-03。

## §R-02 几何锚点坐标

- 锚点共 14 个。
- 锚点原名与画布坐标见下表。
- 原名取自 drive.py 的 ANCHORS 列表。
- head_top (1438, 757)。
- circle_top (1396, 530)。
- navel_circle_center (1396, 1667)。
- square_TL (413, 819)。
- square_TR (2418, 819)。
- square_BL (413, 2780)。
- square_BR (2418, 2780)。
- raised_hand_L (483, 857)。
- raised_hand_R (2295, 857)。
- horiz_fingertip_L (413, 1162)。
- horiz_fingertip_R (2418, 1162)。
- foot_L (1340, 2780)。
- foot_R (1560, 2780)。
- chin (1438, 1130)。
- set_anchor 钉 14 个点。
- get_anchors 回读 count=14，坐标一致。
- 圆 97 点，size 7.0。
- 方形四边各 13 点，size 7.0。
- 线色 [0.43, 0.24, 0.06, 1.0]。
- 几何线显式压力起 0.55、中 0.85、收 0.55。
- 几何线点间时序步进 40。
- 来源：sketch/drive.py ANCHORS 列表。
- 来源：sketch/drive.py circle_points 与 line_points 函数。
- 来源：sketch/NOTES-sketch.md 锚点节。
- 来源：skills/audit-work/audit-sketch-ink.md S-04。

## §R-03 sRGB 到 linear 分段换算

- 第一阶段发 [0.78, 0.66, 0.45]。
- 渲染后采样 (224, 204, 166)。
- 纸面是 (210, 180, 118)。
- 洗色比纸面亮。
- 0.78 ^ (1/2.2) = 0.894，对应 sRGB 约 228，与实测一致。
- 第二阶段换算后颜色画准。
- 来源：sketch/residual.py srgb_to_linear 函数。
- 来源：sketch/GAPS-sketch.md G2。
- 来源：sketch/NOTES-sketch.md 第一阶段执行记录。
- 来源：skills/audit-work/audit-sketch-ink.md S-08。

## §R-04 glaze 罩染参数与纸面底色

- 文档 background 是 (210, 180, 118)（0..255 对象）。
- 纸面大笔 glaze_passes=3，opacity 0.35。
- 主体淡洗 glaze_passes=3，opacity 0.30。
- 纸面大笔 size 900.0。
- 主体淡洗 size 420.0 至 520.0。
- 淡洗色 [0.78, 0.66, 0.45, 1.0]。
- opacity 0.35 走 3 遍，有效不透明度 1-(1-0.35)^3 = 0.725。
- opacity 0.30 走 3 遍，有效不透明度 1-(1-0.30)^3 = 0.657。
- 第一阶段 5 笔洗笔 painted_pixels_clipped 在 584,187 至 2,618,616 之间。
- 第一阶段 5 笔洗笔 max_channel_delta 在 41 至 66 之间。
- 未落笔区采样 (210, 180, 118)，背景色准确。
- 来源：sketch/drive.py stage_block1。
- 来源：sketch/NOTES-sketch.md 第一阶段执行记录。
- 来源：skills/audit-work/audit-sketch-ink.md S-09。

## §R-05 后期残差笔盖不掉串珠

- 大笔 size 420 至 900。
- 大笔只给 3 个点。
- 3 点大笔的点列见 sketch/drive.py stage_block1。
- 第一阶段 3 点大笔留串珠。
- 上部亮带留扇贝边。
- 主体区留一串圆点。
- 20,000 笔后主体区仍有串珠柱残留。
- 第二阶段 20,000 笔残差重铺仍未完全盖掉串珠痕迹。
- 来源：sketch/NOTES-sketch.md 最终目检。
- 来源：sketch/GAPS-sketch.md G3。
- 来源：skills/audit-work/audit-sketch-ink.md S-11。
- 来源：DIAGNOSIS-ROUND1.md S-4。

## §R-06 主体掩码构造

- 圆心 (1396, 1667)，半径 1137 像素，外放 20 像素。
- 方形 x 在 413 至 2418 之间，y 在 819 至 2780 之间，各边外放 20 像素。
- 方形掩码区是 x 在 393 至 2438 之间，y 在 799 至 2800 之间。
- 圆掩码区是圆心距小于等于 1157 像素。
- 候选点在圆内或方形内才保留。
- b=2.8 全局取点 2,200 笔，主体轮廓线一笔未得。
- b=1.15 加主体掩码后轮廓虚线出现。
- 掩码是本轮最有效的改法。
- 来源：sketch/residual.py --mask figure 分支。
- 来源：sketch/NOTES-sketch.md 第二阶段执行记录。
- 来源：sketch/GAPS-sketch.md G8。
- 来源：skills/audit-work/audit-sketch-ink.md S-12。
- 来源：DIAGNOSIS-ROUND1.md S-2。

## §R-07 掩码只在 b=1.15 证明有效

- 掩码从 b=1.15 起用。
- b=1.15 共三遍，笔数 2,090 加 5,000 加 5,000。
- b=2.8 阶段是全局取点，笔数 2,200。
- b=2.8 全局取点失败，主体轮廓线一笔未得。
- b=1.15 加掩码后轮廓虚线出现。
- 掩码版 b=2.8 从未跑过。
- 来源：sketch/NOTES-sketch.md 阶段表。
- 来源：sketch/progress.log 13:35 目检记录。
- 来源：skills/audit-work/audit-sketch-ink.md S-13。

## §R-08 细阶段笔数订正

- b=2.8 阶段 2,200 笔。
- b=1.15 第一遍 2,090 笔。
- b=1.15 第二遍 5,000 笔。
- b=1.15 第三遍 5,000 笔。
- 合计 2,200 加 2,090 加 5,000 加 5,000 等于 14,290 笔。
- 14,290 笔占 20,000 笔的 71.5%。
- 原草案 12,490 笔少算 1,800 笔。
- 来源：sketch/timer.json stage_strokes。
- 来源：skills/METHOD-DETAIL-AUDIT.md §4 K4。
- 来源：skills/audit-work/audit-sketch-ink.md S-14。

## §R-09 等照度线方向与笔触几何

- b=30 时长度 120.0，笔宽 39.0。
- b=12 时长度 48.0，笔宽 15.6。
- b=5.4 时长度 21.6，笔宽 7.0。
- b=2.8 时长度 14.0，笔宽 5.0。
- b=1.15 时长度 14.0，笔宽 5.0。
- 首批 b=30 batch_000 第一笔 size 39.0，opacity 0.55。
- 末批 b=1.15-3 batch_024 第一笔 size 5.0，opacity 0.8。
- 探针 15 项全测，humanize 与 pressure_curve:"auto" 仍逐像素 0 差异。
- 来源：sketch/residual.py 笔触生成段。
- 来源：sketch/batches-b30/batch_000.json 第一笔。
- 来源：sketch/batches-b1.15-3/batch_024.json 第一笔。
- 来源：probe/PROBE.md §4。
- 来源：skills/audit-work/audit-sketch-ink.md S-16。

## §R-10 暗部换笔

- b=2.8 与 b=1.15 三遍共 14,290 笔按此规则分笔。
- 粗阶段 b=30、b=12、b=5.4 全用 2B_pencil。
- 来源：sketch/residual.py 笔刷选择段。
- 来源：sketch/NOTES-sketch.md 第二阶段方法。
- 来源：skills/audit-work/audit-sketch-ink.md S-17。

## §R-11 阶段残差均值

- b=30 均值 24.04。
- b=12 均值 19.07。
- b=5.4 均值 18.57。
- b=2.8 均值 19.45。
- b=1.15 均值 21.73。
- b=1.15 第二遍均值 21.41。
- b=1.15 第三遍均值 20.89。
- 来源：sketch/batches-b30/manifest.json score_mean 24.04。
- 来源：sketch/batches-b12/manifest.json score_mean 19.07。
- 来源：sketch/batches-b5.4/manifest.json score_mean 18.57。
- 来源：sketch/batches-b2.8/manifest.json score_mean 19.45。
- 来源：sketch/batches-b1.15/manifest.json score_mean 21.73。
- 来源：sketch/batches-b1.15-2/manifest.json score_mean 21.41。
- 来源：sketch/batches-b1.15-3/manifest.json score_mean 20.89。
- 来源：skills/METHOD-DETAIL-AUDIT.md §4 K5。
- 来源：skills/audit-work/audit-sketch-ink.md S-18。

## §R-12 批量发送无法执行每批取证

- 第二阶段 19,990 笔分 101 批发出。
- 101 批全部 ok=true，fail_count=0。
- b=1.15 第二遍 batch_022 一批 279.0 秒。
- 其余同阶段约 85 秒。
- batch_022 无失败，applied=200，fail=0。
- batch_022 异常原因未查。
- 来源：sketch/send.py state 行记录。
- 来源：sketch/state-b1.15-2.json batch_022 行。
- 来源：sketch/GAPS-sketch.md G6。
- 来源：skills/audit-work/audit-sketch-ink.md S-22。
- 来源：DIAGNOSIS-ROUND1.md S-3。

## §R-13 draw_stroke 回执无取证字段

- 圆顶 (1396, 530) 采样 (175, 134, 69)。
- 方角 (413, 819) 采样 (175, 134, 69)。
- 空白角采样 (230, 209, 167)。
- 未落笔区采样 (210, 180, 118)。
- 落点与空白角不同色，线落上。
- 来源：sketch/NOTES-sketch.md 第一阶段取证。
- 来源：sketch/GAPS-sketch.md G1。
- 来源：sketch/drive.py stage_block1 采样段。
- 来源：DIAGNOSIS-ROUND1.md S-7。

## §R-14 选笔试笔做法

- list_brushes 共 201 条。
- 201 条含 2 条非笔刷。
- 2 条非笔刷是 CASE-NOTES.md 与 LICENSE-CC0.txt。
- 2 条非笔刷的 reads_canvas 与 pressure_sensitive 是 null。
- brush_preview 试 2 支笔。
- 2 支笔是 2B_pencil.myb 与 100%_Opaque.myb。
- brush_preview size 40.0。
- brush_preview 点列 [[100, 100, 0.8, 0], [300, 140, 0.9, 60]]。
- brush_preview 颜色 [0.43, 0.24, 0.06, 1.0]。
- 2B_pencil 试笔 blob 16,255 字节。
- 100%_Opaque 试笔 blob 1,497 字节。
- 块面用 100%_Opaque。
- 排线后备 2B_pencil、8B_Pencil#1、PenBrush。
- 来源：sketch/drive.py stage_brushes。
- 来源：sketch/NOTES-sketch.md 第一阶段执行记录。
- 来源：sketch/GAPS-sketch.md G7。
- 来源：skills/audit-work/audit-sketch-ink.md S-24。

## §R-15 成本模型与素描实测对照

- 探针 10,000 笔落笔 1,598.1 秒。
- 拟合值 1,628 秒。
- 误差 1.9%。
- 素描 20,000 笔落笔 5,768.6 秒（96.1 分钟）。
- 素描批耗时从 1.2 秒/200 笔（N=26）涨到 120 秒/200 笔（N=20,000）。
- 素描 b=30 第一批 1.21 秒。
- 素描 b=1.15-3 末批 119.98 秒。
- 拟合用干净文档 45 批数据（N 大于等于 1,000，log-log 回归）。
- 50,000 笔外推落笔 12.2 小时。
- 来源：probe/PROBE.md §6。
- 来源：probe/PROBE.md §5.1。
- 来源：sketch/timer.json stage_stroke_seconds。
- 来源：sketch/state-b30.json 第一批。
- 来源：sketch/state-b1.15-3.json 末批。
- 来源：skills/METHOD-DETAIL-AUDIT.md §5 X-09。

## §R-16 内存模型与素描实测对照

- 探针 10,000 笔 RSS 峰值 2,009.9 MB。
- 公式值 1,905.7 MB。
- 两者相差约 104 MB，未解释。
- 素描 20,000 笔 RSS 峰值 3,719 MB。
- 素描各阶段 RSS 峰值 280、568、1,037、1,455、1,815、2,769、3,719 MB。
- 素描 b=2.8 阶段 RSS 峰值 1,455 MB（state 文件值）。
- progress.log 13:25 写 b=2.8 RSS 约 1,281 MB，与 state 文件不符。
- 以 state 文件为准。
- 实测上限约 21,000 笔（可重开）。
- 盒内存 7,964 MB。
- 连续会话 RSS 在约 36,000 至 42,000 笔触到盒内存上限。
- 来源：probe/PROBE.md §6。
- 来源：sketch/NOTES-sketch.md 阶段表。
- 来源：sketch/state-b30.json max_rss_mb 279.6。
- 来源：sketch/state-b2.8.json max_rss_mb 1455.1。
- 来源：sketch/state-b1.15-3.json max_rss_mb 3718.6。
- 来源：sketch/progress.log 13:25 行。
- 来源：skills/METHOD-DETAIL-AUDIT.md §5 X-11。

## §R-17 模型与素描实测批耗时之比

- 素描各阶段批耗时见 sketch/state-*.json。
- 素描 b=30 阶段 6 批，tier_sec 20.8。
- 素描 b=1.15-3 阶段 25 批，tier_sec 2,470.3。
- 来源：DIAGNOSIS-ROUND1.md C-14。
- 来源：sketch/state-*.json。
- 来源：sketch/timer.json。
- 来源：skills/METHOD-DETAIL-AUDIT.md §5 X-15。

## §R-18 包核验判据 missing_body 与三项控制

- 第一阶段包 referenced=10，present=5，missing=5，missing_body=0。
- 最终包 referenced=20,665，present=18,317，missing=2,348，missing_body=0。
- 最终包 56,026,624 字节，blob_count=18,317。
- 最终 PNG 6,395,937 字节。
- head_seq=20,016，原子 20,016。
- 非笔画原子 16（文档 1 加层 1 加锚点 14）。
- 笔画等于 20,016 减 16 等于 20,000。
- 来源：skill-src/yanshi-paint/scripts/check_package.py 第 174 至 213 行。
- 来源：sketch/NOTES-sketch.md 第一阶段执行记录。
- 来源：sketch/NOTES-sketch.md 最终目检一节。
- 来源：skills/METHOD-DETAIL-AUDIT.md §5 X-27。

## §R-19 残差格子边长

- b=30、d=4 时 size_full 39.0，cell 10。
- b=12、d=2 时 size_full 15.6，cell 8。
- b=5.4、d=1 时 size_full 7.0，cell 7。
- b=2.8、d=1 时 size_full 5.0，cell 5。
- b=1.15、d=1 时 size_full 5.0，cell 5。
- 格边约等于笔宽。
- 间距按笔宽取，不按笔长取。
- 来源：sketch/residual.py 贪心取点段。
- 来源：sketch/residual.py STAGES 表。
- 来源：skills/METHOD-DETAIL-AUDIT.md §5 X-32。

## §R-20 取点规则：格心取点，无抖动，分数下限 2.0

- b=30 阶段 score_p90 46.33，score_p99 152.33。
- b=1.15-3 阶段 score_p90 42.33，score_p99 87.33。
- 各阶段 score_mean 见 §R-11。
- 分数小于 2.0 的点被跳过。
- 笔数不够时打 WARNING。
- 来源：sketch/residual.py 贪心取点段。
- 来源：sketch/residual.py 笔触生成段。
- 来源：sketch/batches-b30/manifest.json。
- 来源：sketch/batches-b1.15-3/manifest.json。
- 来源：skills/METHOD-DETAIL-AUDIT.md §5 X-34。

## §R-21 批量文件核对

- 素描 batch 文件 101 个。
- brush_stroke 调用 19,990 个。
- import_image 调用 0 个。
- 101 个文件分 7 个阶段目录。
- b30 6 个，b12 10 个，b5.4 13 个，b2.8 11 个，b1.15 11 个，b1.15-2 25 个，b1.15-3 25 个。
- 19,990 笔加第一阶段 10 笔等于 20,000 笔整。
- 来源：sketch/batches-b30/ 至 sketch/batches-b1.15-3/ 共 101 个 batch 文件。
- 来源：sketch/batches-b30/manifest.json 至 sketch/batches-b1.15-3/manifest.json。
- 来源：eye-check/ANTI-CHEAT-CHECK.md 判定方法一节。
- 来源：skills/METHOD-DETAIL-AUDIT.md §5 X-50。

## §待办

- smart_hatch_follow 本轮未用。
- smart_hatch_follow 调用方法与参数磁盘上无记录。
- 来源：skills/METHOD-DETAIL-AUDIT.md §5 S-19。
- 来源：skills/sketch.md 规则 7。

## §补测数字

- 本节收补测条目里泛化掉的具体数字。
- S-01：取点坐标共 17 个。
- S-02：残差中位上限是 10px。
- X-10：5,000 笔档 317 秒的原始批次记录。
- X-12：重开估算系数 0.293 MB/笔。
- X-22：字节守卫阈值 55 与 60。
- X-28：导出回执 kept_mismatch=19,910。
- 来源：skills/METHOD-DETAIL-AUDIT.md §6。


## 第 2 轮证据（2026-10-29，binary 9c481d63，修复程序期 1–7）

- 画作：素描《维特鲁威人》。画布 2824×3840。时间 30.7 分钟（探针 540 秒单独计；第 1 轮 161 分钟）。笔数 19,540（目标 20,000，差 460，未硬凑）。
- 目检：1:1 通过（第 1 轮不通过）；3x 头部部分通过、手部分通过、文字部分通过、圆方几何通过；整图不通过。
- 方法：沿线追踪取点（tracer.py，lum<160 骨架 + DP 化简，22,985 条折线）：S1 线稿 8,544 笔；S2a 短线补密 8,235 笔；S3 面部/手部暗 blob 轮廓 223 笔；S2b 残差排线（b=1.15）2,500 笔；S4 纸面修复 30 笔 + S5 补角 8 笔。
- 纸面噪声：S1 渲染 1:1 下纸面满布细碎短线/斑点；S4/S5 纸面色不透明笔重盖 38 笔后终版边幅干净，但框内人体周围仍有残留细碎。
- 面部：头部区 lum<160 骨架化，暗核（lum<115）骨架化成网（每像素都是交点，碎段 1–2 像素，85,694 条原始折线中可保留仅 23 条）；面部无可辨特征。
- 手指：指缝有暗示，手指呈波状平行线，未分成独立指。
- 文字：字行、字块、密度与原作一致，单字不可读，字母断裂多于原作连笔。
- 几何接头：圆方几何交点、长线交叉处 3x 下有小缺口或错位；拟合最长单条仅 1031 画布像素；接头压力两端 0.55。
- 特征定位：头部区 lum<110 连通域 344 个，暗斑与特征同量级；最暗行不能对应到双眼中心；未落笔。
- 探针：draw_stroke 单笔回执五项齐全（97 点圆单笔 painted=ink=46450、delta=167、warnings=[]）；list_brushes 199 条全 .myb；掩码只在 b=1.15 用（b≤2.8 适用性未验证）。
- D-4：未复现。探针 50 批 + 实画 199 批，最大 19.3 秒，无异常。D-4 关闭。
- 包：终版 missing=17（全是派生 tile，missing_body=0），exit 0；PNG 2,351,666 B；export_png 0.8 秒、export_project 1.0 秒。
- 出处：REPORT.md §3；sketch/NOTES-sketch.md；sketch/GAPS-sketch.md。

## 第 3 轮证据（2026-10-29，binary 9045faeb，通用写法）

- 本节通用写法，不写画名与画内部件名；§8 方法发现只作证据记录，不升为正文规则（须更多实画验证）。
- 第 3 轮：本风格 1:1 不通过（第 2 轮通过，退步）、整图不通过；时间约 47 分钟，笔数 16,138；单批请求行 ≤44,624 字节。
- 方法发现：淡线稿总墨量控制法缺失（各档阈值叠加致过暗，无总暗占比预算与校准步骤；须更多实画验证）。
- 方法发现：纸面取样方法缺失（边框中位会取到老化边缘，须取画面内浅色纸区）。
- 方法发现：单独立项描摹法只有原则、无可执行步骤（须更多实画验证）。
- 方法发现（通用）：X-2 防高估规则本轮执行有效（执行方自判均未高估，总协调采信全部自判）。
- 方法发现（通用）：X-1 落位先行验证成立，但细结构端点量法有缺口（本轮为目读 + 3x 图估算，非像素量测）。
- 成本硬约束全守：单批请求行 <60,000 字节，0 挂起；水位无跳闸。
- 量测 D-4b：密集重叠下湿笔环形涂抹伪影复现确认（程序侧现象，已进修复分期；不作 skill 规则）。
- 量测 U-1：旧“一遍约 75%”作废（feather 0 一遍全净 100%；feather 25 一遍留约 4 级淡痕、第二遍清净；条件见正文 v5 更新）。
- 量测 glazing 回执 254：回执口径与导出光栅口径不一致已定位（回执取 dab 证据缓冲、导出是复合后光栅；D 期修导出一侧、回执口径未动；已进修复分期；不作 skill 规则）。
- 量测项 2：changed_pixels 关闭成立（单毛对照 changed=110、导出恰 110 像素，与 d8e66638 对照点一致；见正文 v5 更新）。
- 防作弊：本风格包内源图命中 0 个（按 kind 与 blob 内容判定）。
- 出处：REPORT.md §8、§9；MEASUREMENTS-ROUND3.md。

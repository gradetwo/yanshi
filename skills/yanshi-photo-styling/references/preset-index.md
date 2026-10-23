# 22 套内置预设速查（索引，不是配方）

> 本表只做**定位**：凭 id、状态、基线、标签找到候选。配方数值、中性值、旋钮映射以
> `get_preset` 的返回与 `crates/yanshi-server/data/style-presets.json` 为准——不在这里复制，
> 数据改了这张表也不会跟着改数值（表里的笔数/旋钮名若与 `list_presets` 对不上，以工具返回为准并回来修表）。
> 状态口径：`established` = 真实照片目检成立；`fixed_v2` = 按实测结论修正过的第二版；
> `needs_review` = 未目检，套用须提示、出图须复看。基线：`full` = 全量修正，`light` = 轻校正。

| # | preset_id | 名称 | status | 基线 | 标签 | 旋钮 |
|---|---|---|---|---|---|---|
| 1 | `japanese_airy` | 小清新 | established | light | 人像 / 日光 / 高调 | `fade` 空气感、`softness` 柔和度 |
| 2 | `ccd` | 模拟 CCD | established | full | 夜景 / 抓拍 / 直闪 | `noise` 数字噪点、`cold` 冷闪偏色 |
| 3 | `japanese_film` | 日系胶片 | needs_review | full | 人像 / 胶片 / 阴天 | `fade`、`grain` |
| 4 | `retro_film` | 胶片复古 | fixed_v2 | full | 人像 / 胶片 / 金色时刻 | `warmth`、`grain` |
| 5 | `hong_kong` | 港风 | fixed_v2 | full | 夜景 / 街景 / 霓虹 | `neon` 霓虹浓艳、`teal` 青绿阴影 |
| 6 | `korean` | 韩系 | needs_review | light | 人像 / 高调 / 奶油 | `cream`、`bright` |
| 7 | `western_cool` | 欧美清冷 | established | full | 风光 / 建筑 / 人像 | `cold`、`hard` |
| 8 | `cinema` | 电影感 | fixed_v2 | full | 夜景 / 街景 / 叙事 | `teal_orange`、`matte` |
| 9 | `french` | 法式 | needs_review | light | 人像 / 窗光 / 田园 | `warmth`、`grain` |
| 10 | `forest` | 森系 | established | full | 风光 / 森林 / 雾气 | `depth`、`mist` |
| 11 | `morandi` | 莫兰迪 | established | full | 静物 / 室内 / 产品 | `gray`、`matte` |
| 12 | `dark_moody` | 暗黑情绪 | needs_review | full | 人像 / 低调 / 静物 | `dark`、`vignette` |
| 13 | `black_white` | 黑白 | needs_review | full | 街头 / 建筑 / 风光 | `contrast`、`grain` |
| 14 | `cyberpunk` | 赛博朋克 | needs_review | full | 夜景 / 霓虹 / 都市 | `neon_glow`、`magenta` |
| 15 | `y2k` | Y2K | needs_review | full | 夜景 / 派对 / 抓拍 | `pop`、`noise` |
| 16 | `guofeng` | 国风 | fixed_v2 | full | 古建 / 人像 / 园林 | `daiqing` 黛青、`zhuhong` 朱红 |
| 17 | `ins_minimal` | Ins 简约 | established | full | 探店 / 家居 / 建筑 | `white`、`clean` |
| 18 | `sweet` | 糖水 | needs_review | light | 人像 / 高调 / 粉嫩 | `glow` 辉光（默认关）、`sweet` 甜亮 |
| 19 | `commercial` | 商业 | needs_review | full | 产品 / 广告 / 形象照 | `material`、`sharp` |
| 20 | `hdr_landscape` | HDR 风光 | fixed_v2 | full | 风光 / 建筑 / 大光比 | `drama`、`lift` |
| 21 | `lomo` | LOMO | established | full | 抓拍 / 旅行 / 生活 | `vignette`、`candy` |
| 22 | `pastoral_cream` | 田园奶油 | needs_review | light | 田园 / 人像 / 亲子 | `cream`、`glow_soft` |

命名对版：研究速查里的「欧美冷调 / 古风·国潮 / ins 极简」就是这里的
`western_cool` 欧美清冷 / `guofeng` 国风 / `ins_minimal` Ins 简约，同一套，不要当成两套找。

外挂预设（`YANSHI_PRESET_DIR`）会让实际可用集合多于或不同于这 22 套，
且同 id 时内容以 `external` 为准——动手前永远先跑一次 `list_presets` 看当前真实库。

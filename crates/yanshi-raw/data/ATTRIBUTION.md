# 数据出处与授权：打包镜头子集 `lensfun-subset.xml`

- **来源**：Lensfun 数据库（https://lensfun.github.io/），上游仓库
  `lensfun/lensfun` 的 `data/db/` 目录，数据库时间戳 **1577948414**。
- **授权**：Creative Commons Attribution-ShareAlike 3.0 Unported
  （CC-BY-SA 3.0）——Lensfun 数据库以此授权发布；本子集是其摘录，
  以相同授权随本仓库分发，署名：**Lensfun contributors**。
- **范围**：12 支镜头整条摘录（标定条目一条不删、不改数值），
  仅作离线保底；完整数据库经环境变量 `YANSHI_LENSFUN_DIR` 外挂，
  同 ID 条目覆盖本子集（见设计 5.9 定版：打包子集保底 + 外挂优先）。
- **增补（2026-10-25，photo-bugfix-p1）**：6 支 v5 实拍镜头整条摘录——
  Nikkor Z 24-70mm f/4 S、Nikkor Z 24-120mm f/4 S、XF23mmF2 R WR、
  Ricoh GR III & compatibles + GW-4、M.Zuiko Digital ED 14-42mm
  f/3.5-5.6 EZ、Lumix G Vario 14-140mm f/3.5-5.6；取自上游
  `lensfun/lensfun` master（db timestamp 仍为 1577948414，未变）。
  Sigma APO 170-500mm F5-6.3 Aspherical RF 上游无此条目，未收录；
  各支的实拍匹配结果见本期包说明。
- **边界**：本仓库只使用 Lensfun 的**数据**。Lensfun 的程序代码是
  LGPL，本项目不链接、不翻译其代码；XML 解析与全部校正数学为
  本项目自写（`src/lens_db.rs` / `src/lens.rs`），模型公式按
  Lensfun 公开文档与 DNG 规范口径实现。

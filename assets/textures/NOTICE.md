# 纹理来源与许可 —— **CC0 1.0** ✓（公共领域贡献 ✓，无需署名 ✓、可商用 ✓）

这些纹理由 `scripts/fetch-textures.sh` 从 **ambientCG** 抓取：
<https://ambientcg.com/> ✓，许可证说明见 <https://ambientcg.com/license/> ✓。
抓的是各资产的 **1K-PNG** 变体里的**颜色贴图**（Color ✓）。

**本目录不在 git 里** ✓ —— 它是**运行时缓存** ✓，每个工作区各自一份 ✓。

**为什么只收 PNG** ✓：本项目的像素解码器是**自己写的** ✓、只解 PNG ✗
（JPEG / WebP 会被**明确拒绝**并说明原因 ✓）⇒ 抓 PNG 变体是为了"抓到就能用" ✓。

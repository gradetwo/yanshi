# 纹理来源与许可 —— **CC0 1.0** ✓（公共领域贡献 ✓，无需署名 ✓、可商用 ✓）

这些纹理由 `scripts/fetch-textures.sh` 从 **ambientCG** 抓取：
<https://ambientcg.com/> ✓，许可证说明见 <https://ambientcg.com/license/> ✓。
抓的是各资产的 **1K-PNG** 变体里的**颜色贴图**（Color ✓）。

**本目录**在 git 里** ✓（实测 `git ls-files assets/textures/` = **11 个** ✓）—— 所以
**克隆下来就有** ✓、**发布包也带着** ✓（`scripts/package-release.sh` 会把它装进
`share/yanshi/textures/` ✓，而且**源目录不在就打包失败** ✗ ⇒ 这是**有意**的 ✓）。

**那 `scripts/fetch-textures.sh` 是干什么的** ✓：需要**更多/别的**纹理时用它联网抓 ✓ ——
它写进**同一个目录** ✓，但本地新增的那些**不会**自动进 git ✗（与上面那 11 个**不同** ✓）。

**★ 2026-10-15 更正 ✗→✓ ★**：这里原来写着「**本目录不在 git 里** ✗ ⇒ 它是运行时缓存」✓ ——
**那句话与仓库不符** ✗（git 里就有这 11 个 ✓）。我（AI）曾据此在验收档里推出
"离线拿不到纹理" ✗，**也是错的** ✓ ⇒ 两处都已按实测改正 ✓（见
`docs/design/vermeer-acceptance.md` §33 ✓）。

**为什么只收 PNG** ✓：本项目的像素解码器是**自己写的** ✓、只解 PNG ✗
（JPEG / WebP 会被**明确拒绝**并说明原因 ✓）⇒ 抓 PNG 变体是为了"抓到就能用" ✓。

# 工作交接（截至 2026-10-25 16:05 Asia/Shanghai）

**仓库**：`/home/crow/yanshi`（分支 `main`）｜**HEAD**：`ca58be70`｜推送：`git push git@github.com:gradetwo/yanshi.git main`

## 一、每轮第一件事：比两份外部报告

| 报告 | 远端路径 | 本地基线 | 比法 |
|---|---|---|---|
| `art`（apple2011）| `apple2011:/home/crow/work/art/YANSHI_ISSUES.md` | `~/.local/share/yanshi-external/issues_seen.md` | `scp` 下来后 `md5sum` **两侧都打印**，相同才说「未变」 |
| `qoder` | `apple2011:~/work/qoder/yanshi_issue.md` | `~/.local/share/yanshi-external/qoder_issue_seen.md` | 同上 |

**⇒ 2026-10-25 16:00 实测（两侧 md5 相同 ✓，基线已更新）**：
- `art`：`42a993d78aafabfd279b9db47e3da1c3`（740,964 B）⇒ **只多了一条定时检查**（他复述 `c7739c89`／`2e70615b`／`1788080d`，无新缺口）。
- `qoder`：`7dcd42c3a6e79e94eabc15b94b8e0bcb`（141,575 B）⇒ **未变**。

**⇒ 规矩（我犯过）**：报「未变」**必须附两侧 md5**。此前 `qoder` 根本没被真正比对过（旧基线是 33 KB 的**截断抓取**），我报了十几轮"未变"，全是**断言恒真**。

## 二、当前状态

```
CI：bf0c6e5a **success ✓**（run 40176610440；12 个可跑的 job 全 success，brush parity skipped）
    ⇒ D172 闭环 ＋ `HANDOFF.md` 白名单修正**都在这一跑里验过** ✓
    40175183140（ca58be70）failure ⇒ 唯一意外失败是 `tool-repo-root-clean`（已在 bf0c6e5a 修）
    2e70615b success ✓（dab ×3 注释；**补丁线收工**）
    26ba9483 success ✓（F 的 Default 派生修正）
```

**已落地（都过了 CI 或本机验证）**：
1. **0.3.0 发布完整**：版本号、注解标签 `v0.3.0`、5 个资产（4 平台包 ＋ `SHA256SUMS`）。三层验证：哈希重下载一致 → 包内 509 条目 → 解包执行 `--version`。
2. **发布流水线缺陷已修**：`checksums` 作业没 checkout 却跑 `gh` ⇒ 加 `--repo`（`9ddb92d3`）＋ 判据 `check-release-workflow-gh-repo`（变异可红、已进 `pre-commit`）。
3. **外部补丁包 F ＋ H 已合并**（`61a7e64a`，CI success）：F 的资产根解析一次（仓库外启动不再 404）；H 的退化点改闭合等弧长环（32×61 → 46×46），相位哈希改 `to_bits()` 混列。A2 未合（作者自述"未证明不伤 below 缓存"）。
4. **D172 完整闭环**（`ca58be70`，本轮）：
   - 完整包参数**早就有**，名字是 **`include_bitmaps`**（交接上一版写的 `include_blobs` 是错的；`tools_registry.rs:1971`、`service.rs:3219`）。
   - `yanshi-timelapse`：缺**必需** blob ⇒ **`exit(2)`**（不再产出一条"看起来正常的空影片"）；**`tiles` 解码索引缺失不致命**（渲染侧会退回整幅，`render.rs:4350`）。
   - `real_package_shrinks`：导出传 `include_bitmaps: true`，断 `omitted == 0`、`blob_count > 0`、回放侧「缺 blob 0 个」。
   - 新判据 `a_missing_tile_index_alone_is_not_fatal`：只缺 tiles ⇒ `rc=0`，但 stderr 仍要看见「N 个 tiles 索引缺失」（豁免 ≠ 静默）。
   - 三条变异都实测过（见 §五-3）。
5. **判据补强（前几轮）**：台账引用 430 处（棘轮 333→0）、注释里的断言不算、临时目录守卫必须被绑定、示例 205→203＋静态生成、`omitted` 可见判据、D12 knife 判据。

## 三、下一步（按优先序，都有精确入口）

1. **qoder 剩余条目**：`D139`（静默零覆盖，需他给完整参数；我 6 点网格未复现）、`D188` 的量级说明（差随原子数增长：200 图块 ≈0.6 lv/通道、22,926 原子 ≈16 lv ⇒ 写进那条 `⚠️` 警告）。
2. `shrink` blit（`art` 第 12 项）：最小包上对照判据实测 均值 1.89／>30 占 0.22% ⇒ 无缺口；要他用同判据跑 0.30 GB 的 `test1full`。
3. （可选，第二口径）`scripts/tool-export-bitmaps.mjs` 加一段：`include_bitmaps: true` 导出的包直接喂 `target/release/yanshi-timelapse` ⇒ `rc=0`。

## 四、纪律与环境（别重犯）

- **CI 一条并发**：`main` 上只跑一个 run；**一次 push 就够，别连推**。推前先查 `gh run list --limit 3 --json status --jq '[.[] | select(.status=="in_progress" or .status=="pending")] | length'` ⇒ 0 才派发。
- **推前必跑** `bash scripts/pre-commit.sh`（fmt／workspace clippy／台账引用／三条自测／发布工作流形态）。本轮已跑，全绿。
- **改 `.rs` 后本机**：`cargo clippy -p <crate> --all-targets -- -D warnings` ＋ 定向 `cargo test -p <crate> --test <name>`；全量交 CI。
- **二进制新鲜度**：跑数字前 `ls -l --time-style=full-iso <binary> <source>` ⇒ **源码不得比二进制新**（我拿变异版二进制量出「6.5% 纯黑」，真值 0.00%）。
- **`target` 体积**：本机 `target` 现约 **4.8 G**（16:00 清过一次 `target/debug` 2.8 G ＋ `/tmp` 里 10 个陈旧 chrome profile 1.45 G；清完磁盘 70% → 69%）。脚本是 `/home/crow/cleanup-dev-space.sh`，本轮给它补了 ③b 段（只删 **crow 所有且 >6 h 且没有 chromium 在跑**的 `*_chrome_*`）。
- **`paths-ignore`**：只改 `docs/**`、`**/*.md`、`scripts/criteria-known-red.txt` 的提交**不触发 CI**。
- **已知红**：权威名单是 `scripts/criteria-known-red.txt`（用 `grep -c` 数，别写死）；「CI 绿」＝红都在名单里。
- 中文用「」；中文不许插进 `param!(…)` 参数之间；含 `\"` 的字符串里不许用 `[^\\\"]*`。

## 五、我这场的坑（供新 session 少走）

1. **报告基线是截断抓取** ⇒ 十几轮"qoder 未变"无依据（已修基线）。
2. **`head -c 400` 截断 JSON** ⇒ 误报「没有 `total` 字段」（键按字母序，`total` 在最后）。
3. **D172 的两条新坑（`ca58be70`）**：
   - **"缺 blob"的审计不能直接用全文 64-hex 判致命**：`import_image` 的 `payload.tiles` 是**分块解码索引**，不在 `refs.blobs` 里、导出**故意不装**，而渲染侧取不到就退回整幅 ⇒ 把它算成缺 = **拒绝自己的包**（实测 6 个引用里 3 个是 tiles ⇒ `rc=2`）。修法：只把**非 tiles** 的缺失当致命，但仍打印 tiles 缺失数。
   - **断言的输入必须能触发它**：原测试用 `8B_Pencil#1`，这支笔**不可省** ⇒ `omitted == 0` 恒真 ⇒ 把 `include_bitmaps: true` 删掉测试**照样绿** ✗。换成 `2B_pencil.myb`（`export_small` 已证可省）后，删参数 ⇒ `omitted: 3` ⇒ 红 ✓。
   - **顺序**：`exit(2)` 必须在"导出侧能装全"之后（第 1089 轮先加 `exit(2)` ⇒ 拒收自产包 ⇒ 回归过）。现在的顺序是 导出侧 `include_bitmaps` ✓ → 测试/文档传 `true` ✓ → 才 `exit(2)` ✓。
4. **改行为前先 grep 谁依赖它**（这条欠账的代价是一轮红）。
5. **新增仓库**顶层**条目（含 `.md`）要同步加进 `scripts/tool-repo-root-clean.mjs` 的 `ALLOWED`**：
   `HANDOFF.md` 在 `ab7a1114` 提交后**第一次**跑 CI 才红（`2e70615b` 的 run 早于它）
   ⇒ 这类红**不在产品侧**，但会阻塞"CI 绿" ✓。判据自己会指出"有意的就加白名单"。

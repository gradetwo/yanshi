# 工作交接（截至 2026-10-25 11:40 Asia/Shanghai）

**仓库**：`/home/crow/yanshi`（分支 `main`）｜**HEAD**：`2e70615b`｜推送：`git push git@github.com:gradetwo/yanshi.git main`

## 一、每轮第一件事：比两份外部报告

| 报告 | 远端路径 | 本地基线 | 比法 |
|---|---|---|---|
| `art`（apple2011）| `apple2011:/home/crow/work/art/YANSHI_ISSUES.md` | `~/.local/share/yanshi-external/issues_seen.md` | `scp` 下来后 `md5sum` **两侧都打印**，相同才说「未变」 |
| `qoder` | `apple2011:~/work/qoder/yanshi_issue.md`（**141,575 字节**）| `~/.local/share/yanshi-external/qoder_issue_seen.md` | 同上 |

**⇒ 规矩（我犯过）**：报「未变」**必须附两侧 md5**。此前 `qoder` 根本没被真正比对过（旧基线是 33 KB 的**截断抓取**），我报了十几轮"未变"，全是**断言恒真**。

## 二、当前状态（本机全绿，CI 见下）

```
CI：26ba9483 success ✓（F 的 Default 派生修正）
    2e70615b in_progress（dab ×3 注释；覆盖 c7739c89 的相位修正）
    c7739c89 cancelled（被后代顶掉 ⇒ 无损）
```

**已落地（都过了 CI 或本机验证）**：
1. **0.3.0 发布完整**：版本号（workspace ＋ 25 处依赖 ＋ lock 21 包）、注解标签 `v0.3.0`、**5 个资产**（4 平台包 ＋ `SHA256SUMS` 4 行）。三层验证：哈希重下载一致 → 包内 **509 条目** → **解包执行 `--version` 报 `yanshi 0.3.0 (commit a720cad…)`**。
2. **发布流水线缺陷已修**：`checksums` 作业没 checkout 却跑 `gh` ⇒ `failed to run git` ⇒ 加 `--repo`（`9ddb92d3`）＋ 新判据 `check-release-workflow-gh-repo`（变异可红、已进 `pre-commit`）。
3. **外部补丁包 F ＋ H 已合并**（`61a7e64a`，CI success）：
   - **F**：`serve()` 解析一次资产根并写回 `options` ⇒ 仓库外启动不再 404；新增 `asset_paths_survive_foreign_cwd`。**我另改**：缺省值比较从字面量改成 `HttpOptions::default()` 派生（`26ba9483`）。
   - **H**：退化点从竖线改**闭合等弧长环**（8 顶点＋回起点，相位由坐标哈希）⇒ 点从 32×61（1.91）变 46×46（1.00）。**我另改**：相位哈希改 `to_bits()` 混列（旧写法 `as i64` 在坐标 >1.2e11 时**饱和**），加 2 条单测（变异可红）。
   - **A2 未合**（作者自述"未证明不伤 below 缓存"）。
4. **判据补强**：台账引用 430 处（棘轮 333→0）、注释里的断言不算、临时目录守卫必须被绑定、示例 205→203＋静态生成、`omitted` 可见判据、D12 knife 判据（6494 px vs 3237 px）。

## 三、下一步（按优先序，都有精确入口）

1. **读 `2e70615b` 的 CI 结论**（覆盖相位修正；绿 ⇒ 补丁线收工）。
2. **D172 完整闭环**（导出侧）：`export_project` **故意省掉**笔触位图（`document.rs:402`），但响应里**有 `omitted`**（`tools_io.rs`）。顺序：给 `export_project` 加 `include_blobs`（默认 false ＝现行为；走 8 处清单：spec／handler／dispatch／`TOOL_EXAMPLES`／`docs/tools.md` 两数字／`tools.rs` 注释／两份 guide／重新生成 `docs/design/tool-examples.md`）→ timelapse 的测试传 `true` → 再把 `yanshi-timelapse` 缺 blob 的**两行警告**换成 `exit(2)` ＋ 判据。
3. **qoder 报告剩余条目**：`D139`（静默零覆盖，需他给完整参数；我 6 点网格未复现）、`D188` 的量级说明（差随原子数增长：200 图块 ≈0.6 lv/通道、22,926 原子 ≈16 lv ⇒ 要写进那条 `⚠️` 警告）。
4. `shrink` blit（`art` 第 12 项）：我给的对照判据「开/不开 SHRINK 末帧逐像素比对」在最小包上 **均值 1.89／>30 占 0.22%** ⇒ 无缺口；要他用同判据跑 0.30 GB 的 `test1full`。

## 四、纪律与环境（别重犯）

- **CI 一条并发**：`main` 上只跑一个 run；**一次 push 就够，别连推**（我因此被顶掉过三次证据）。
- **推前必跑** `bash scripts/pre-commit.sh`（fmt／clippy／台账引用／三条自测／发布工作流形态）。
- **改 `.rs` 后本机**：`cargo clippy -p <crate> --all-targets -- -D warnings` ＋ 定向 `cargo test -p <crate> --test <name>`；全量交 CI。
- **二进制新鲜度**：跑数字前 `ls -l --time-style=full-iso <binary> <source>` ⇒ **源码不得比二进制新**（我拿变异版二进制量出「6.5% 纯黑」，真值 0.00%）。
- **`target/debug` > 5 G** ⇒ `rm -rf target/debug/{deps,build,incremental}`（`target/release` 保留）。
- **`paths-ignore`**：只改 `docs/**`、`**/*.md`、`scripts/criteria-known-red.txt` 的提交**不触发 CI** ⇒ 改这些是"无判据区间"。
- **已知红 18 条**（权威：`scripts/criteria-known-red.txt`）；「CI 绿」＝红都在名单里。
- 中文用「」；中文不许插进 `param!(…)` 参数之间；含 `\"` 的字符串里不许用 `[^\\\"]*`。

## 五、我这场的坑（供新 session 少走）

1. **报告基线是截断抓取** ⇒ 十几轮"qoder 未变"无依据（已修基线）。
2. **`head -c 400` 截断 JSON** ⇒ 误报「没有 `total` 字段」（键按字母序，`total` 在最后）。
3. **先收紧再发现自家用例红**：`yanshi-timelapse` 缺 blob 改 `exit(2)` ⇒ `real_package_shrinks` 变红（因为我们自己的 `export_project` 省位图）⇒ 已改回**大声警告**，顺序写进 `main.rs` 注释。
4. **改行为前先 grep 谁依赖它**（这条欠账的代价是一轮红）。

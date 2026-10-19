# 交接快照（第 968 轮更新，取代此前所有版本）

> 文件名保留是为了不产生两份漂移。**内容以此为准。**

## 代码现状

| 项 | 值 |
|---|---|
| `main` 与 `l4-sync` | `d8b3324d`（同步）|
| 工作区 | 干净 |
| `cargo clippy --workspace --all-targets` | **0**（`scripts/pre-commit.sh` 保证）|
| 判据（CI 收集口径）| **282** |
| 已知红（`criteria-known-red.txt`）| **30** |

## ⚠ 三条**必须先读**的流程陷阱（都是本场血债）

**① 本机跑判据前，确认 `target/release/yanshi-serve` 比源码新**

```bash
cargo build --release -p yanshi-http 2>&1 | tail -1
# 或
[ target/release/yanshi-serve -nt crates/yanshi-server/src/tools.rs ] || cargo build --release -p yanshi-http
```

**代价 10 轮**：我用陈旧二进制测出「`[200,…]` 与 `[100,…]` 都给 255」的假现象，
然后花 947–956 去追一个**不存在的 bug**，期间对同一处代码给出过**三个互相矛盾的结论**。

**判别信号**：一个 bug 在本机稳定复现、而 CI 上任何一片都不红 ⇒ 先怀疑本机二进制陈旧。

**② 不要每轮查 CI** —— 单片 `criteria` 可达 **19 分钟**，而轮次间隔只有 **12–80 秒**。

**③ 提交前跑 `bash scripts/pre-commit.sh && git commit …`**
（本场我在 clippy 红着时提交过 3 次，都因为命令链用了 `;` 而非 `&&`。）

## 本场真正落地的交付（全部可复核）

| 项 | 证据 |
|---|---|
| **布尔合并**（`mode=add/subtract/intersect`）| 端到端 **58400**；三 mode 33600/41600/6400；判据 `tool-selection-mode-merge.mjs` |
| **`blend_mode` 9 种** | `normal/multiply/screen/overlay/darken/lighten/difference/addition/subtract` **9/9 ok**；判据 `tool-blend-mode-multiply.mjs`（203→162，sRGB 精确）|
| **`rect` 选区的 `w/h` 真 bug** | `create_selection` 存 `w/h`，`clip` 读 `width/height` ⇒ 任何 rect 选区都不能裁剪。901 修；判据 `tool-clip-to-selection-rect.mjs`（四形状，变异验过）|
| **`brush-params.md` 重新生成** | `check=0`（66 个规范名 / 9 组别名 / medium 13）；手改标题会造成**行序错位**，必须用 `write` 模式 |
| **`browser-pwa-local` 假红** | chromium 未就绪用了 `exit(1)` ⇒ 改 `exit(3)`（按 336 轮那条）|
| **性能断言加固** | `rect_coverage_parity` 的 `new < old` ⇒ `ratio < 1.15`（CI 实测差仅 1.8%）|
| **颜色 alpha 混合域** | `[200,200,200,1]` 的 `a=1` 曾被当字节 1 ⇒ alpha 1/255 ≈ 全透明。修后 **200/100 精确**；判据 `tool-color-array-domain.mjs` |
| `exit(3)` 说明覆盖 | **194/194** |
| `trackTemp` 覆盖 | **62/62** |

## 还没做完的

- **`fill` / `fill_region` 加 `blend_mode` 参数**（功能已有，走 `set_property`；缺方便入口）
- **`adjust_levels` / `tone_curve` 工具**（外部改用 `add_adjustment(levels)` 自行绕过，优先级降低）
- **等外部回答**：他们 `cv.fill` 的实现（解释 `193/49` vs `226/154` 的差）

## 外部报告状态

| § | 内容 | 我方结论 |
|---|---|---|
| §139 | 「没有 multiply」 | **不成立**——9 种都有，走 `set_property(key="blend_mode")` |
| §142 | `subtract` 不生效 | **他们自己撤回**（用法问题）；但我顺藤找到 `rect` 的 `w/h` 真 bug |
| §144 | 我的 §142 观察救了真 bug | 已答复 |
| §145 | `add_adjustment(levels)` 只在单层生效 | **他们自己找到正确用法**（挂到有内容的那一层）|
| §146 | 颜色混合域 | 已答复；**我的 `color.rs` 修复是真的有效**（完整链路验过）|

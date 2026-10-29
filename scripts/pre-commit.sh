#!/usr/bin/env bash
# **提交前的门**（第 834 轮）：我本场有 **3 次**在 clippy 红着的时候照样 commit
# （第 254 轮两次、第 833 轮两次），根因都是命令链里写了 `cargo clippy …; echo $?`
# 却**没让 commit 依赖它的结果**。
#
# ⇒ 用这个脚本代替「靠记得」：
#     bash scripts/pre-commit.sh && git commit -m "…"
#
# 它跑三件事，任一失败就 exit 1（于是 && 后面的 commit 不会执行）。
set -uo pipefail
fail=0

echo "  ① cargo fmt --all -- --check"
if ! cargo fmt --all -- --check > /tmp/precommit-fmt.log 2>&1; then
  echo "     ✗ fmt 不干净（跑 cargo fmt --all 再提交）"
  tail -5 /tmp/precommit-fmt.log
  fail=1
else
  echo "     ✓"
fi

echo "  ② cargo clippy --workspace --all-targets -- -D warnings"
if ! cargo clippy --workspace --all-targets -- -D warnings > /tmp/precommit-clippy.log 2>&1; then
  echo "     ✗ clippy 有错"
  grep -E "^(error|warning)" /tmp/precommit-clippy.log | head -6
  fail=1
else
  echo "     ✓"
fi

# ③ **台账引用**（第 998 轮加）—— **我三次把轮号写成「第 N 节」⇒ 三次都是 CI 先抓到** ✗
#   **⇒ 判据**：`check-ledger-section-numbering.mjs`（悬空引用／撞号／棘轮 ✓）
#   **⇒ 为什么放到提交前**：它**只读 docs**（毫秒级 ✓）而**CI 抓到要等 20–40 分钟** ✗
#     ⇒ 三次的代价一共是 **三个 run** ✗ ⇒ 挪到提交前就**不可绕过** ✓
if node scripts/check-ledger-section-numbering.mjs > /tmp/precommit-ledger.log 2>&1; then
  echo "     ✓"
else
  echo "     ✗ 台账引用有问题（把轮号写成节号了？）"
  grep -E "✗|悬空" /tmp/precommit-ledger.log | head -4
  fail=1
fi

# ④ **判据自己的自测**（第 1024 轮加）—— **毫秒级** ＋ **不触碰仓库** ✓
#   **⇒ 为什么**：这两条判据的谓词现在都有 `--self-test`（小字符串 ⇒ 期望值 ⇒ 变异可红 ✓）
#     ⇒ 放进提交前 ⇒ **它们不会退化成装饰** ✓（谁改坏了谓词，提交就被拦 ✓）
for probe in check-tests-clean-temp check-tests-have-asserts check-release-workflow-gh-repo check-blank-overwrite-guard; do
  if node "scripts/${probe}.mjs" --self-test > "/tmp/precommit-${probe}.log" 2>&1; then
    echo "     ✓ ${probe}"
  else
    echo "     ✗ ${probe} 自测失败（谓词被改坏了？）"
    tail -4 "/tmp/precommit-${probe}.log"
    fail=1
  fi
done

# ⑤ **发布工作流的 `gh` 命令形态**（第 1051 轮加）—— **毫秒级、只读 yml** ✓
#   **⇒ 为什么**：v0.3.0 的发布红在「`checksums` 作业没 checkout 却跑 `gh`」✗
#     而那条错误**只能在发布时暴露**（要等 40 分钟 + 一个 tag ✗）⇒ 挪到提交前 ✓
if node scripts/check-release-workflow-gh-repo.mjs > /tmp/precommit-relwf.log 2>&1; then
  echo "     ✓"
else
  echo "     ✗ 发布工作流里有「无 checkout 却没给 --repo」的 gh 命令"
  tail -3 /tmp/precommit-relwf.log
  fail=1
fi

# ⑥ **脚本卫生测试**（第 1059 轮加）—— **只在 scripts/*.sh 有改动时跑** ✓
#   **∴ 为什么**：本仓已有两组测试专门查脚本卫生：
#     · shell_scripts        —— \$VAR 被引用但没赋值（1057 轮抓到我的 watch-ci.sh）
#     · script_portability   —— \$VAR 紧跟非 ASCII（1014）+ bash 4+ 写法（1018）
#   ⇒ 前三次（1014/1018/1057）都是 **CI 先发现**我在脚本里犯错 ✗
#   ⇒ **∴ 所以**把它们挪到提交前，让「脚本卫生」和 fmt/clippy 一样**不可绕过** ✓
#   **∴ 为什么加条件**：`cargo test` 属「重计算」（AGENTS.md 说交给 CI）
#     ⇒ 但这两个文件很小、且只在改脚本时才需要
#     ⇒ **∴ 所以**用 `git diff --name-only` 判断，**没改脚本就跳过** ✓
if git diff --name-only HEAD 2>/dev/null | grep -q '^scripts/.*\.sh$' \
   || git diff --name-only --cached 2>/dev/null | grep -q '^scripts/.*\.sh$'; then
  echo "  ③ cargo test --test shell_scripts --test script_portability"
  if cargo test -p yanshi-server --test shell_scripts --test script_portability > /tmp/pre-commit-shell.log 2>&1; then
    echo "     ✓"
  else
    echo "     ✗（详见 /tmp/pre-commit-shell.log）"
    grep -aE "被引用但从未赋值|紧跟非 ASCII|不支持的写法|panicked" /tmp/pre-commit-shell.log | head -3 | sed 's/^/       /'
    fail=1
  fi
fi

# ⑤ **改工具注册表 ⇒ 自动跑"新增工具的八处清单"那两条检查**（第 1562 轮）
#   **⇒ 为什么加**：我加 `explain_tool` 时**记得七处、漏了 `TOOL_EXAMPLES`** ✗
#     ⇒ CI 的 `every_registered_tool_has_a_documented_example` 当场红了 ✓
#   **⇒ ⇒ 而那正说明**：**清单是有效的，失效的是我的记性** ✓
#     ⇒ 所以把它**挂到改注册表的那一刻**，而不是靠我记住 ✓
#   **⇒ ⇒ ⇒ 只在 `tools_registry.rs` / `tools_examples.rs` 变了时才跑**（约 30 秒）
#     ⇒ 其余提交不受影响 ✓
if git diff --name-only HEAD 2>/dev/null | grep -qE 'tools_(registry|examples)\.rs$' \
   || git diff --name-only --cached 2>/dev/null | grep -qE 'tools_(registry|examples)\.rs$'; then
  echo "  ④ 新增工具清单（注册表变了 ⇒ 跑两条机械检查）"
  if cargo test -p yanshi-server --lib every_registered_tool_has_a_documented_example > /tmp/pre-commit-tools.log 2>&1 \
     && cargo test -p yanshi-server --test tool_inventory >> /tmp/pre-commit-tools.log 2>&1; then
    echo "     ✓（示例齐全、文档计数与注册表一致）"
  else
    echo "     ✗（详见 /tmp/pre-commit-tools.log）"
    grep -aE "没有可照抄的示例|声明的工具数量与注册表不符|panicked" /tmp/pre-commit-tools.log | head -3 | sed 's/^/       /'
    fail=1
  fi
fi

# ⑥ **磁盘卫生提醒**（第 1038 轮加）—— **只提醒，不阻塞** ✓
#   **∴ 为什么**：`AGENTS.md` 里有「build 后清 target/debug/{deps,build,incremental}」那条，
#     而本场我**读了但没做** ⇒ `target/debug` 涨到 **9.5 G**、磁盘 **63%** ✗
#     ⇒ 清完 **614 M**、磁盘 **60%** ✓（省 8.9 G）
#   ⇒ **∴ 而**它**没有脚本** ⇒ 没人执行（包括我）⇒ **∴ 所以**加在这里 ✓
DBG_KB=$(du -sk target/debug 2>/dev/null | awk '{print $1}')
if [ -n "$DBG_KB" ] && [ "$DBG_KB" -gt 5242880 ]; then   # > 5 GiB
  echo "  ⚠ target/debug 已 $(du -sh target/debug 2>/dev/null | awk '{print $1}')（> 5 G）"
  echo "     建议：rm -rf target/debug/{deps,build,incremental}"
  echo "     （release 保留；本场实测 9.5 G → 614 M、磁盘 63% → 60%）"
fi

# ⑦ **PWA 内联工具表**（2026-10-29 加）—— **只在工具面文件变了时跑** ✓
#   **⇒ 为什么加**：四支分支（newdoc／f53／batch-wait／f56）先后倒在同一个判据
#     tool-pwa-suggestion-accept 上，根因全是「改了工具面却没重生成
#     web/api-local.js 里嵌的内联工具表」✗ ⇒ 每次都是 CI 排队半小时后才暴露 ✗
#     ⇒ 所以把它**挂到改工具面的那一刻**，而不是靠人记住 ✓
#   **⇒ 触发集合（已核实：表内容只由这五处决定）**：
#     · crates/yanshi-server/src/tools_registry.rs（ToolSpec 清单与参数表 ✓）
#     · crates/yanshi-server/src/tools.rs（ParamSpec 常量、清单函数 ✓）
#       （grep 核实：ToolSpec 块与 ParamSpec 常量只在这两个文件里，别处没有 ✓；
#        GET /api/tools 就是 registry.tools_list_json() 的直出，无过滤 ✓）
#     · web/api-local.js（表文件本身：手改也要验 ✓）
#     · crates/yanshi-http/assets/api-local.js（第二份表：服务端 include_str! 内嵌、
#       实际 serve 的那份；只改 web/ 会让页内比对继续红，2026-10-29 batch-wait
#       新 run 实测：web/ 已新、embedded 仍旧 ⇒ CI 仍倒在同一判据 ✓）
#     · scripts/gen-pwa-tool-specs.mjs（生成器变了 ⇒ 表可能要重算 ✓）
#     ⇒ 其余提交**零成本**（grep 一下就跳过，不起服务、不构建 ✓）
#   **⇒ 检查本身不写回**：regen 脚本的 --check 只比对，表过期才拦提交 ✓
#     （钩子绝不在提交前夜里改工作区：写回是开发者的事，跑一条命令就行 ✓）
#   **⇒ 退出码 3（不可判）不拦**：服务起不来／构建失败 ⇒ 警告放行，判据同规
#     （不可判 ≠ 产品失败；且本步骤跑在 clippy 之后，树已通过编译，构建失败极罕见 ✓）
if git diff --name-only HEAD 2>/dev/null | grep -qE '^crates/yanshi-server/src/tools(_registry)?\.rs$|^web/api-local\.js$|^crates/yanshi-http/assets/api-local\.js$|^scripts/gen-pwa-tool-specs\.mjs$' \
   || git diff --name-only --cached 2>/dev/null | grep -qE '^crates/yanshi-server/src/tools(_registry)?\.rs$|^web/api-local\.js$|^crates/yanshi-http/assets/api-local\.js$|^scripts/gen-pwa-tool-specs\.mjs$'; then
  echo "  ⑦ PWA 内联工具表（工具面变了 ⇒ 核对表是否过期）"
  if bash scripts/regen-pwa-table.sh --check > /tmp/precommit-pwa-table.log 2>&1; then
    echo "     ✓（表与服务端一致）"
  else
    RC=$?
    if [ "$RC" -eq 3 ]; then
      echo "     ⚠ 表无法判定（服务起不来或构建失败），不阻塞提交——请手动跑重生成确认"
      tail -3 /tmp/precommit-pwa-table.log | sed 's/^/       /'
    else
      echo "     ✗ 表过期（改了工具面却没重生成）——提交被拦"
      grep -aE "✗|⇒" /tmp/precommit-pwa-table.log | head -6 | sed 's/^/       /'
      echo "       重生成：bash scripts/regen-pwa-table.sh"
      fail=1
    fi
  fi
fi

if [ "$fail" -ne 0 ]; then
  echo "  ⇒ **不许提交**（上面有红）"
  exit 1
fi
echo "  ✓ 可以提交"

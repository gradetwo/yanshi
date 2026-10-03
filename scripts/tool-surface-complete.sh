#!/usr/bin/env bash
# **「默认就能看到全部已实现工具」判据** ✓（用户要求：已实现的功能若 MCP 没提供，就要提供 ✓）。
#
# 现状（第 86 轮普查 ✓）：规格 126 个 ✓，但**默认只暴露 core 59 个** ✗ ⇒ 67 个已实现功能
# （选区 / 检查点 / 事务 / 协作 / 批注 / 冲突解决 / 结构编辑 …）**默认拿不到** ✗ ——
# 画师实测时就撞上了它 ✓（"未知工具 … 所属的组没有启用 ✓ 当前启用的组：core, history, …" ✗）。
#
# 判据：**不带任何 profile 参数**启动服务 ⇒ `GET /api/tools` 的条数必须等于**规格总数** ✓
# （规格总数从 `tools.rs` 现数 ✓ ⇒ 不写死某个观测 ✓）。
# 用法：bash scripts/tool-surface-complete.sh <base-url> <tools.rs 路径>
set -uo pipefail
base="${1:?用法: tool-surface-complete.sh <base-url> <tools.rs>}"
src="${2:?需要 tools.rs 路径}"
[[ -f "${src}" ]] || { echo "  找不到 ${src}"; exit 1; }
# 规格总数 = `ToolSpec { name: …` 的出现次数 ✓（与 profile 无关 ✓）
# **规格总数要按"ToolSpec 块里的第一个 name"数** ✗ —— 我先前的 `grep -c 'ToolSpec {'` **多算 1 个** ✗
# （注释/文档里提到过这个字样 ✓）⇒ 判据于是**误报"少一个"** ✗。教训 ✓：**数数也要问"我数的是不是那个东西"** ✓。
want=$(python3 - "${src}" <<'COUNT'
import re, sys
src = open(sys.argv[1], encoding="utf-8").read()
names = [m.group(1) for m in
         (re.search(r'name:\s*"([a-z0-9_]+)"', block) for block in src.split("ToolSpec {")[1:])
         if m]
print(len(set(names)))
COUNT
)
have=$(curl -s "${base}/api/tools" | python3 -c '
import json,sys
value=json.load(sys.stdin)
tools=value.get("tools") or value.get("data",{}).get("tools") or []
print(len(tools))
' 2>/dev/null)
echo "  规格总数=${want}｜默认暴露=${have:-（取不到）}"
if [[ -z "${have}" ]]; then echo "  ✗ 取不到工具清单 ⇒ 判据无效"; exit 1; fi
if [[ "${have}" != "${want}" ]]; then
  echo "  ✗ 默认只暴露 ${have} 个，而有 ${want} 个已实现 ⇒ 有 $((want - have)) 个功能默认拿不到"
  exit 1
fi
echo "  ✓ 默认暴露全部 ${want} 个已实现工具"

#!/usr/bin/env bash
# Phase 4b 端到端演示：人类标注 → AI 解析并给出建议（含 patch）→ 人工接受/拒绝 →
# patch 被真正应用（像素变化）→ 标注状态更新。
#
# 用法：
#   scripts/phase4b-demo.sh                     # 默认打到 127.0.0.1:8110
#   BASE_URL=http://127.0.0.1:8080 scripts/phase4b-demo.sh
#
# 依赖：curl、python3（只用于解析 JSON）。脚本只创建**新文档**，不动已有文档。

set -euo pipefail

BASE="${BASE_URL:-http://127.0.0.1:8110}"
DOC="${DOC:-phase4b-demo-$$}"

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
pick() { python3 -c "import sys,json;d=json.load(sys.stdin);print(d$1)"; }

step "打开文档 $DOC"
TOKEN="$(curl -s -X POST "$BASE/api/documents" \
  -d "{\"doc_id\":\"$DOC\",\"width\":256,\"height\":256}" | pick "['token']")"
tool() { curl -s -X POST "$BASE/api/tools/$1?doc=$DOC&token=$TOKEN" -d "$2"; }

# 渲染整幅并输出 PNG 的 sha256（作为像素指纹）。
fingerprint() {
  local url
  url="$(tool render_region '{"region":{"x":0,"y":0,"w":256,"h":256}}' | pick "['thumb_url']")"
  # **macOS 没有 `sha256sum`** ✗（那边是 `shasum -a 256` ✓）⇒ 选一个存在的 ✓。
  curl -s "$BASE${url#yanshi://blob}" | { command -v sha256sum >/dev/null 2>&1 && sha256sum || shasum -a 256; } | cut -c1-16
}

step "铺一层中灰底"
tool create_layer '{"layer_id":"L"}' >/dev/null
tool draw_shape '{"layer_id":"L","object_id":"base","data":{"geometry":{"kind":"rect","bbox":{"x":0,"y":0,"w":256,"h":256}},"color":{"r":120,"g":120,"b":120,"a":255}}}' >/dev/null
BEFORE="$(fingerprint)"
echo "基线像素指纹：$BEFORE"

step "① 人类标注"
ANNOTATION="$(tool create_annotation '{"type":"region","content":"整幅太暗，提亮一点","intent":"modify","target":{"type":"region","bbox":{"x":0,"y":0,"w":256,"h":256}}}' | pick "['annotation_id']")"
PENDING="$(tool list_annotations '{"status":"pending"}' | pick "['pending']")"
echo "标注 id：${ANNOTATION}（待处理 ${PENDING}）"

step "② AI 解析并给出建议（patch = 工具调用序列）"
SUGGESTION="$(tool suggest "{\"annotation_id\":\"$ANNOTATION\",\"summary\":\"曝光 +0.8EV 后反相\",\"patch\":[{\"tool\":\"add_adjustment\",\"arguments\":{\"layer_id\":\"L\",\"adjustment_type\":\"exposure\",\"params\":{\"ev\":0.8}}},{\"tool\":\"add_adjustment\",\"arguments\":{\"layer_id\":\"L\",\"adjustment_type\":\"invert\"}}]}" | pick "['suggestion_id']")"
tool list_suggestions '{"status":"pending"}' | pick "['pending']" | xargs -I{} echo "待处理建议：{}（建议 id ${SUGGESTION}）"

step "③ 再建一条标注并关联该建议（追踪）"
tool create_annotation "{\"type\":\"region\",\"content\":\"跟随建议\",\"intent\":\"style\",\"target\":{\"type\":\"region\",\"bbox\":{\"x\":0,\"y\":0,\"w\":64,\"h\":64}},\"suggestion_id\":\"$SUGGESTION\"}" >/dev/null
tool list_annotations '{"status":"pending"}' | pick "['pending']" | xargs -I{} echo "待处理标注：{}"

step "④a 只读步骤必须被拒（patch 只能包含会产生状态效果的步骤）"
READONLY="$(tool suggest '{"patch":[{"tool":"get_document","arguments":{}}]}' | pick "['suggestion_id']")"
tool accept_suggestion "{\"suggestion_id\":\"$READONLY\"}" | pick "['error_code']" | xargs -I{} echo "拒绝原因：{}"

step "④b 人工接受建议 → 重放 patch"
tool accept_suggestion "{\"suggestion_id\":\"$SUGGESTION\"}" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print('应用原子数：%d；解决标注数：%d；剩余待处理标注：%d' % (
    len(d['applied_atom_ids']), len(d['resolved_annotations']), d['pending_annotations']))
"
AFTER="$(fingerprint)"
echo "接受后像素指纹：$AFTER"
if [ "$BEFORE" = "$AFTER" ]; then
  echo "❌ 像素未改变：patch 没有被真正应用" >&2
  exit 1
fi
echo "✅ 像素已改变，patch 被真正应用"

step "⑤ 另一条建议被拒绝（记录原因）"
REJECT="$(tool suggest '{"patch":[{"tool":"add_adjustment","arguments":{"layer_id":"L","adjustment_type":"saturation","params":{"amount":0.5}}}]}' | pick "['suggestion_id']")"
tool reject_suggestion "{\"suggestion_id\":\"$REJECT\",\"reason\":\"方向不对\"}" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print('拒绝建议 %s：原因「%s」，联动标注 %d 条' % (d['suggestion_id'], d['reason'], len(d['rejected_annotations'])))
"
step "⑥ 补丁内先建图层再绘制（协作原子豁免嵌套引用校验后打通）"
LAYERS_BEFORE="$(tool get_document '{}' | pick "['layers']")"
CREATE_USE="$(tool suggest '{"summary":"新建图层并画方块","patch":[{"tool":"create_layer","arguments":{"layer_id":"fresh"}},{"tool":"draw_shape","arguments":{"layer_id":"fresh","data":{"geometry":{"kind":"rect","bbox":{"x":32,"y":32,"w":48,"h":48}},"color":{"r":255,"g":0,"b":0,"a":255}}}}]}' | pick "['suggestion_id']")"
if [ -z "$CREATE_USE" ] || [ "$CREATE_USE" = "None" ]; then
  echo "❌ 先建后画的补丁无法记录（嵌套引用校验回归）" >&2
  exit 1
fi
BEFORE_CREATE="$(fingerprint)"
tool accept_suggestion "{\"suggestion_id\":\"$CREATE_USE\"}" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print('应用原子数：%d' % len(d['applied_atom_ids']))
assert d['ok'], d
"
AFTER_CREATE="$(fingerprint)"
LAYERS_AFTER="$(tool get_document '{}' | pick "['layers']")"
if [ "$BEFORE_CREATE" = "$AFTER_CREATE" ]; then
  echo "❌ 新建图层上的绘制没有改变像素" >&2
  exit 1
fi
if [ "$LAYERS_AFTER" -le "$LAYERS_BEFORE" ]; then
  echo "❌ 图层数未增加（$LAYERS_BEFORE → ${LAYERS_AFTER}）：create_layer 步骤未生效" >&2
  exit 1
fi
echo "✅ 补丁内建层+绘制已落地（图层 $LAYERS_BEFORE → ${LAYERS_AFTER}；像素指纹 $BEFORE_CREATE → ${AFTER_CREATE}）"

echo
echo "建议状态："
tool list_suggestions '{}' | python3 -c "
import sys, json
d = json.load(sys.stdin)
for item in d['suggestions']:
    print('  %s  %-8s  %s' % (item['suggestion_id'], item['status'], item.get('summary') or ''))
print('  pending =', d['pending'])
"
echo
echo "✅ Phase 4b 环路演示完成（文档 ${DOC}）"

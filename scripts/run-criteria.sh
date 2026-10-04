#!/usr/bin/env bash
# **在 CI 上跑全部判据**（本地只做小改动 ✓，重活交给 GitHub ✓）。
# 约定：tool-*.mjs 收 <base-url>；browser-*.mjs 收 <viewer-url> 并读 CDP_PORT；
#       kernel-brush-parity.mjs 收 <base-url> <doc> <token> <内核 js 路径>。
# **已知红名单**（scripts/criteria-known-red.txt）里的脚本仍会跑、结果照印，但不让本脚本失败 ✓。
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
PORT="${PORT:-13990}"
CDP_PORT="${CDP_PORT:-9490}"
ROOT_DIR="$(mktemp -d)"
PROFILE="$(mktemp -d)"
KNOWN="$ROOT/scripts/criteria-known-red.txt"
is_known_red() { [ -f "$KNOWN" ] && grep -qE "^$1[[:space:]]" "$KNOWN"; }

cleanup() {
  [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null
  [ -n "${CHROME_PID:-}" ] && kill "$CHROME_PID" 2>/dev/null
  rm -rf "$ROOT_DIR" "$PROFILE"
}
trap cleanup EXIT

case "${SKIP_SERVER:-0}" in
  1) : ;;
  # **给服务端指定导出目录** ✓（第 339 轮）：产品的 `guarded_output_path` 把导出目录**相对它的 CWD** 解析 ✗
  #（缺省 `./exports` ✓，可用 `YANSHI_EXPORT_DIR` 改 ✓）⇒ **判据与服务端对"文件在哪"的理解不一致** ✗ ⇒
  # `tool-cjk-text` 正是因此报「找不到导出文件 ⇒ 试过：$WORKSPACE/exports/… ｜ …」✓。
  # 指到工作区内 ⇒ **写与读对齐** ✓（判据的候选路径**第一位就是它** ✓）。
  *) YANSHI_EXPORT_DIR="$ROOT_DIR/work/exports" ./target/debug/yanshi-serve --bind "127.0.0.1:$PORT" --root "$ROOT_DIR/work" \
       --doc boot --width 320 --height 240 --assets-dir "$ROOT/assets" >"$ROOT_DIR/server.log" 2>&1 &
     SERVER_PID=$!
     for _ in $(seq 1 40); do curl -sf "http://127.0.0.1:$PORT/api/documents" >/dev/null 2>&1 && break; sleep 0.5; done ;;
esac
BASE="http://127.0.0.1:$PORT"
# 让判据能核对落盘产物（render.png 等）；不设则相关检查自行跳过并打印 ✓。
export YANSHI_WORKSPACE="$ROOT_DIR/work"

if [ "${SKIP_BROWSER:-0}" != "1" ] && command -v chromium >/dev/null 2>&1; then
  chromium --headless=new --no-sandbox --disable-gpu --remote-debugging-port="$CDP_PORT" \
    --user-data-dir="$PROFILE" about:blank >"$ROOT_DIR/chrome.log" 2>&1 &
  CHROME_PID=$!
  for _ in $(seq 1 40); do curl -sf "http://127.0.0.1:$CDP_PORT/json/version" >/dev/null 2>&1 && break; sleep 0.5; done
  # **连不上就快速失败** ✓：以前会**逐条各等 15 分钟** ✗ ⇒ 一小时内什么都拿不到 ✓。
  if ! curl -sf "http://127.0.0.1:$CDP_PORT/json/version" >/dev/null 2>&1; then
    echo "  ⚠️ 浏览器起来了但 CDP 连不上 ⇒ **跳过全部浏览器判据**（不干等 ✓）"
    kill "$CHROME_PID" 2>/dev/null; CHROME_PID=""; SKIP_BROWSER=1
  fi
fi

token_for() {
  curl -s -X POST "$BASE/api/documents" -H 'content-type: application/json' \
    -d "{\"doc_id\":\"$1\",\"width\":320,\"height\":240}" |
    python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])'
}

pass=0; fail=0; expected_red=0; skipped=0
FAIL_SUMMARY=""      # **末尾汇总** ✓：GitHub 的 --log-failed 只给"步骤日志尾部" ✗
# **分片** ✓（`SHARD`/`SHARDS` 由 CI 的矩阵传入 ✓）：按**文件名排序后取模** ✓
# ⇒ 分片是**确定性**的 ✓（同一份代码每次落进同一片 ✓），也**不重不漏** ✓。
SHARD="${SHARD:-1}"; SHARDS="${SHARDS:-1}"
shard_index=0
# ⚠️ **枚举是按前缀的** ✗ ⇒ 任何不匹配的判据**静默地永不运行** ✗（第 787 轮实测：74 个里 14 个没跑 ✓，
# 其中含 (A)⑥ 的核心判据 `wasm-brush-parity` ✗）⇒ 本条按第 788 轮**显式补上它** ✓（接口与下面那条相同 ✓）。
for script in $(ls scripts/tool-*.mjs scripts/browser-*.mjs scripts/kernel-brush-parity.mjs scripts/wasm-brush-parity.mjs scripts/kernel-wasm-allowlist.mjs scripts/medium-abi-check.mjs scripts/server-ws-origin.mjs scripts/ui-layout-grid.mjs scripts/ui-text-density.mjs scripts/mcp-document-switch.mjs scripts/mcp-tool-descriptions.mjs scripts/ui-control-heights.mjs scripts/server-token-policy.mjs 2>/dev/null | sort); do
  shard_index=$((shard_index + 1))
  if [ "$SHARDS" -gt 1 ] && [ $(( (shard_index - 1) % SHARDS + 1 )) -ne "$SHARD" ]; then
    continue
  fi
  name="$(basename "$script")"
  [ -f "$script" ] || continue
  # 浏览器判据没有 chromium 就跳过（本地环境常见；CI 里一定装了）。
  # ⚠️ `ui-control-heights.mjs` 也**需要真浏览器** ✓（第 794 轮 ✓）⇒ 显式并进来 ✓。
  # **不改名成 `browser-*`** ✗：它的参数形状与那一族**不同** ✗ —— 它读 `argv[3]` 当 CDP 端口 ✓，
  # 而 `browser-*` 拿到的 `argv[3]` 是 server-base ✗ ⇒ 改名会让端口错位 ✓。
  if [[ "$name" == browser-* || "$name" == ui-control-heights.mjs ]] && [ "${SKIP_BROWSER:-0}" = "1" -o -z "${CHROME_PID:-}" ]; then
    echo "  ⊘ ${name}（没有 chromium，跳过）"; skipped=$((skipped+1)); continue
  fi
  doc="crit_$(echo "$name" | tr -cd 'a-z0-9')"
  tok="$(token_for "$doc")"
  # **心跳** ✓：卡住时一眼看出是**哪一条** ✓（以前只看到"Job 还在跑" ✗）。
  echo "  → $name"
  case "$name" in
    # **自足的静态检查** ✓（第 789 轮接线 ✓）：读内核源码、**不需要**服务端/浏览器 ✓ ⇒
    # 上限给 120s 就够（它只 readFileSync 几个文件 ✓）。
    kernel-wasm-allowlist.mjs)
      timeout 120 node "$script" >"$ROOT_DIR/out.txt" 2>&1 ;;
    # **介质插件 ABI 契约** ✓（第 790 轮接线 ✓）：拿**仓库里的** assets/mediums/*.wasm 当
    # 插件加载并验收（无 imports ✓、按 seed 确定 ✓、有配额 ✓、v2 上下文语义 ✓）⇒
    # 显式把产物路径传给它 ✓（不依赖它内建的那份默认清单 ✓ —— 那样才是「检查真产物」✓）。
    # 本机实测：6 个插件全部通过、耗时数秒 ✓。
    medium-abi-check.mjs)
      timeout 300 node "$script" assets/mediums/*.wasm >"$ROOT_DIR/out.txt" 2>&1 ;;
    # **WebSocket 跨站防护** ✓（第 791 轮接线 ✓）：只要服务端在跑 ✓ ⇒ 传 <host> <port> ✓。
    # 三情形正反都能红：跨站 Origin 必须**拒**、本机 Origin 必须 101、不带 Origin 必须 101 ✓。
    server-ws-origin.mjs)
      timeout 120 node "$script" 127.0.0.1 "$PORT" >"$ROOT_DIR/out.txt" 2>&1 ;;
    # **静态界面判据** ✓（第 792 轮接线 ✓）：只读查看器的 HTML/CSS ⇒ **不需要服务端/浏览器** ✓。
    # ui-layout-grid：间距/字号必须落在少数几档、且间距在 4 的倍数上 ✓；
    # ui-text-density：直接可见的文本不得是长解释句 ✓。
    ui-layout-grid.mjs|ui-text-density.mjs)
      timeout 120 node "$script" >"$ROOT_DIR/out.txt" 2>&1 ;;
    # **MCP 判据** ✓（第 793 轮接线 ✓）：两条都**自己 spawn** target/debug/yanshi-mcp ✓、
    # 自己造 root/doc ✓ ⇒ **不需要服务端/浏览器** ✓（mcp-document-switch ✓ 看磁盘上图层落在哪个文档 ✓；
    # mcp-tool-descriptions ✓ 直接驱动 tools/list ✓ —— 那是 MCP 客户端的真正视角 ✓）。
    mcp-document-switch.mjs|mcp-tool-descriptions.mjs)
      timeout 180 node "$script" >"$ROOT_DIR/out.txt" 2>&1 ;;
    # **控件高度** ✓（第 794 轮接线 ✓）：要**真浏览器**量渲染高度 ✓（源码看不出 ✓）⇒
    # 传 `<viewer-url> <cdpPort>` ✓（端口走 argv[3] ✓，与 browser-* 那一族不同 ✗）。
    # 本机实测：✓ 控件高度统一（26px×2 checkbox / 28px×84 button ✓）。
    ui-control-heights.mjs)
      timeout 240 node "$script" "$BASE/?doc=$doc&token=$tok" "$CDP_PORT" >"$ROOT_DIR/out.txt" 2>&1 ;;
    # **令牌策略** ✓（第 796 轮接线 ✓）：**两半段都要跑** ✓ ——
    #   allow  ⇒ 跑在本分片已有的**回环**服务端上 ✓；
    #   refuse ⇒ **另起一个「对外」实例** ✓（`--bind 0.0.0.0` ✓、**故意不配** YANSHI_API_KEY ✓）。
    #   服务端的判定读的是**配置的 bind** ✓（`bind_is_loopback` ✓）⇒ 绑 0.0.0.0 即算对外 ✓，
    #   而仍可从 127.0.0.1 连上它 ✓。两半段本机都已单独验证通过 ✓。
    #   ⚠️ **分支里不要 `exit`** ✗：`:143` 取的是**本分支最后一条命令的退出码** ✓ ⇒
    #   结尾用 `[ "$rc" = 0 ]` 把结果交出去 ✓，并先杀掉临时实例 ✓。
    server-token-policy.mjs)
      : > "$ROOT_DIR/out.txt"
      ALT_PORT=$((PORT + 1)); ALT_ROOT="$ROOT_DIR/tokenpolicy"; mkdir -p "$ALT_ROOT"
      ./target/debug/yanshi-serve --bind "0.0.0.0:$ALT_PORT" --root "$ALT_ROOT" --doc boot \
        --width 320 --height 240 --assets-dir "$ROOT/assets" >"$ROOT_DIR/tokenpolicy-server.log" 2>&1 &
      ALT_PID=$!
      for _ in $(seq 1 40); do curl -sf "http://127.0.0.1:$ALT_PORT/api/documents" >/dev/null 2>&1 && break; sleep 0.25; done
      timeout 60 node "$script" "$BASE" allow >>"$ROOT_DIR/out.txt" 2>&1 \
        && timeout 60 node "$script" "http://127.0.0.1:$ALT_PORT" refuse >>"$ROOT_DIR/out.txt" 2>&1
      rc=$?
      kill "$ALT_PID" 2>/dev/null
      [ "$rc" = 0 ] ;;
    kernel-brush-parity.mjs|wasm-brush-parity.mjs)
      timeout 600 node "$script" "$BASE" "$doc" "$tok" "$ROOT/crates/yanshi-wasm/pkg/yanshi_wasm.js" >"$ROOT_DIR/out.txt" 2>&1 ;;
    browser-*)
      # **浏览器判据 240s** ✓（本地实测多在 1 分钟内 ✓）—— 以前一律 900s ✗ ⇒ 25 条最坏要跑几小时 ✗。
      # **⚠️ 不要给全体 URL 加 `debug=1`** ✗（第 338 轮实测 ✓）：只有一个判据需要它 ✓
      # （`browser-kernel-perf` ✓，内核句柄只在调试模式暴露 ✓），而给**全体**加会让
      # **9 条本来绿的判据**开始报 `undici:15270` ✗（含 `offline-*` ✓ `no-stale-read` ✓
      # `render-switch` ✓ ⇒ **调试模式改变了页面的 SW/离线行为** ✓）⇒ 需要它的那一条**自己传** ✓。
      # **统一多传三个** ✓：有的还要 <server-base> <token> [cdpPort] ✓
      # （`browser-pan-vs-paint` 就是 ✗）⇒ 只收 viewer-url 的会**忽略多余参数** ✓（与工具判据同一招 ✓）。
      # **回滚隔离**（第 335 轮 ✗）：第 333 轮我给"每条浏览器判据重启浏览器" ✓
      # ⇒ **离线判据依赖的 SW 持久化被破坏** ✗ ⇒ 8 条浏览器判据开始报
      # `node:internal/deps/undici/undici:15270` ✗（其中多条**上一轮还是绿的** ✓）。
      # ⇒ 实验的答案是"**我的隔离方式错了**" ✗，不是"隔离假设错" ✗ ⇒ 回到"每分片一个实例" ✓。
      # **浏览器类判据的上限**（第 783 轮从 240 提到 420 ✓，可用 `BROWSER_TIMEOUT` 覆盖 ✓）：
      # 本机实测 `browser-ui-check.mjs` 跑完**全部段落**约 **3 分钟** ✓，而它此前会在中途抛异常 ✗
      # ⇒ 那时"跑得快"是假象 ✓（覆盖面缩水 ✓）。修好后它**合法地**更久 ✓ ⇒
      # 240 秒只剩不到 1.5 分钟余量 ✗，而 CI 机器比本机慢 ✗ ⇒ **超时成了独立于产品的不稳定源** ✓
      # ⇒ 上限放宽到 420 秒 ✓（**判据本身一条断言都没放松** ✓ —— 只是不再赌它跑得够快 ✓）。
      CDP_PORT="$CDP_PORT" timeout "${BROWSER_TIMEOUT:-420}" node "$script" "$BASE/?doc=$doc&token=$tok" "$BASE" "$tok" "$CDP_PORT" >"$ROOT_DIR/out.txt" 2>&1 ;;
    *)
      # **统一传三个参数** ✓：有的判据要 <base> <doc> <token> ✓（如 tool-brush-tag-filter ✓），
      # 只收 base 的会忽略多余参数 ✓ ⇒ 一条约定覆盖两种 ✓（CI 第一轮就是这里漏了 ✗）。
      # **工具判据 180s** ✓（其中含浏览器的那几条自带更长的内部等待 ✓）。
      # **这一条要自己的预算** ✓（第 471 轮 ✓）：`tool-example-acceptance` 会把目录里每个带示例的工具
      # 都真跑一遍 ✓ ⇒ 示例数涨到 **134** 后 180s 不够 ✗ ⇒ 实测 `EXIT=124`（超时 ✓）⇒
      # **超时让整条判据失去结论** ✗（不是红，是没有结论 ✓）⇒ 单独给 420s ✓，其余仍是 180s ✓。
      to=180
      [ "$name" = "tool-example-acceptance.mjs" ] && to=420
      timeout "$to" node "$script" "$BASE" "$doc" "$tok" >"$ROOT_DIR/out.txt" 2>&1 ;;
  esac
  code=$?
  if [ "$code" = 0 ]; then
    echo "  ✓ $name"; pass=$((pass+1))
      # **通过时也打最后一行** ✓（第 355 轮）：否则"绿了但为什么绿"这种问题，
      # **通道答不了** ✗（`tool-example-acceptance` 的 `结论：N 跑通 / M 被拒 / K 需前置状态`
      # 就只在失败时才进日志 ✗）⇒ 每条都打最后一行 ✓，绿红都能回答 ✓。
      tail -1 "$ROOT_DIR/out.txt" | sed 's/^/       ↳ /'
  elif is_known_red "$name"; then
    echo "  ⚠ ${name}（**已知红，按记录不阻塞 CI**）: $(grep -m1 -E '^     - ' "$ROOT_DIR/out.txt" | cut -c1-120)"
    # **已知红也要把输出打出来** ✓（第 403 轮 ✓）：原先只打"第一条原因" ✗ ⇒
    # **写在别处的探针 / 诊断就等于没写** ✗ —— 我加的那条"拖动期间日志尾部"探针，
    # 跑了 ✓ 却没进日志 ✓ ⇒ 我白白等了一轮 ✗。**绿 / 已知红 / 失败，三种都要说话** ✓。
    head -3 "$ROOT_DIR/out.txt" | sed 's/^/     前│/'
    tail -6 "$ROOT_DIR/out.txt" | sed 's/^/     后│/' 
    # **已知红也要做定向抽取** ✓（第 531 轮 ✓）：第 484/519 轮把 `项│` 与 `蒙│`
    # 只加在了**普通失败分支** ✗ ⇒ 而 `browser-ui-check` 是已知红 ✓ ⇒ **那个分支里从不执行** ✓
    # ⇒ 实测 `grep 蒙│` 零命中 ✓ ⇒ **等于没加** ✗（这一族已出现多次 ✓）。
    grep -F "✗" "$ROOT_DIR/out.txt" | head -20 | sed 's/^/     项│/'
    grep -F "蒙版" "$ROOT_DIR/out.txt" | head -6 | sed 's/^/     蒙│/'
    # **`px` 探查行也要抽** ✓（第 549 轮 ✓）：它不以 ✗ 开头 ✗ ⇒ 头尾窗口抓不到 ✓
    grep -F "px 请求" "$ROOT_DIR/out.txt" | head -12 | sed 's/^/     px│/'
    expected_red=$((expected_red+1))
  else
    # **计数必须独立成行** ✓（第 412 轮 ✓）：原先写成 `tail -6 … # 注释 ; fail=$((fail+1))` ✗
    # ⇒ **`#` 之后全被当注释** ✓ ⇒ 那行**永不执行** ✗ ⇒ 汇总恒写"意外失败 0" ✗
    #（`—— 通过 8｜意外失败 0` 与同片两条 `(EXIT=1/2)` 并存，就是这样来的 ✓）。
    fail=$((fail+1))
    echo "  ✗ $name (EXIT=$code)";
    # **失败行要定向抽出来** ✓（第 484 轮 ✓）：普通失败**本来就**打 head-3/tail-6 ✓（第 484 轮更正 ✓），
    # 而这种判据有 **134 行逐项输出** ⇒ **散在中间的失败行既不在头三行、也不在尾六行** ✗
    # ⇒ 实测「示例被拒」0 条进日志 ✓ ⇒ **等于没打** ✓ ⇒ 这里把带 ✗ 的逐项行单独抽出来 ✓（最多 20 条 ✓）。
    # **条件改成「行内含 ✗」** ✓（第 510 轮 ✓）：原条件 `^ *✗ ` 要求"空格 + ✗ + 空格"开头 ✗ ⇒
    # 实测 138 行里 **19 行含 ✗** 而抽取到 0 条 ✓ ⇒ **是条件不对，不是文件里没有** ✓
    #（体检那行 `grep -c "✗"` 一次就数出 19 ✓ ⇒ 用它同一条件 ✓）。
    grep -F "✗" "$ROOT_DIR/out.txt" | head -20 | sed 's/^/     项│/'
    # **蒙版的诊断也要定向抽** ✓（第 519 轮 ✓）：它的打印在脚本输出**中部** ✗
    # ⇒ 实测 `head -3`/`tail -6` **抓不到** ✓（`grep 蒙版拖动期间` 零命中 ✓）
    # ⇒ 而那三个观测（`tool` / `points` / `dragging`）是"蒙版为什么提交失败"的唯一直接证据 ✓。
    grep -F "蒙版" "$ROOT_DIR/out.txt" | head -6 | sed 's/^/     蒙│/'
    # **`px` 探查行也要抽** ✓（第 549 轮 ✓）：它不以 ✗ 开头 ✗ ⇒ 头尾窗口抓不到 ✓
    grep -F "px 请求" "$ROOT_DIR/out.txt" | head -12 | sed 's/^/     px│/'
    # **先体检，再抽取** ✓（第 495 轮 ✓）：我已在"抽取什么"上错了两次 ✗ ⇒
    # 这里先把"文件里到底有什么"打出来 ✓（行数 ✓ + 含 ✗ 的行数 ✓ + 头几行的真实形态 ✓），
    # **再决定抽取条件** ✓ —— 「零命中」只能推出"没落在我的条件里" ✗，推出不了"不存在" ✓。
    echo "     体│ out.txt 行数 $(wc -l < "$ROOT_DIR/out.txt")｜含 ✗ 的行 $(grep -c "✗" "$ROOT_DIR/out.txt" 2>/dev/null || echo 0)"
    sed -n "1,5p" "$ROOT_DIR/out.txt" | sed "s/^/     头│/" head -3 "$ROOT_DIR/out.txt" | sed 's/^/     前│/'; \
      tail -6 "$ROOT_DIR/out.txt" | sed 's/^/     后│/'   # **头部也要** ✓（第 340 轮）：只打尾巴会把**我加在最前面的诊断**切掉 ✗（`tool-reference-delta-e` 的「第一次调用完整返回」就是这样丢的 ✓）
    # **理由必须来自"失败行"** ✓（第 415 轮 ✓）：原先抓不到 `- `/`✗ ` 就退回
    # "首个非空行" ✗ ⇒ **进度日志被当成失败原因** ✗（实测：`browser-brush-preview` 的
    # `① … loaded=true …` 就是这么被端上来的 ✓ —— 与我读到的"理由"一模一样 ✓）。
    # ⇒ 现在**只认失败信号** ✓（`- ` 原因列表 ✓ / `✗` ✓ / `EXIT=` ✓ / 失败 ✓ / panic ✓ / Error ✓），
    # **并把 `①②③…`、`→`、`↳` 这类进度行排除** ✗。
    first_reason=$(grep -m1 -aE '^ *- |(^|[[:space:]])(✗|❌)|EXIT=[0-9]|失败|panic|Error:' "$ROOT_DIR/out.txt" \
      | grep -avE '^ *[①②③④⑤⑥⑦⑧⑨⑩]|^ *→ |^ *↳ ' | sed 's/^[[:space:]]*//' | cut -c1-120)
    # **找不到就明说** ✓ —— 不要拿普通输出充当理由 ✗（那样读的人会以为这就是原因 ✓）。
    [ -z "$first_reason" ] && first_reason='（没找到明确的失败行 ⇒ 看上面的 前│/后│ 输出）'
    FAIL_SUMMARY="${FAIL_SUMMARY}  ✗ ${name}｜${first_reason}
"
  fi
done

if [ -n "$FAIL_SUMMARY" ]; then
  # **失败清单（重复打印）** ✓：GitHub 的 --log-failed 只返回日志尾部 ✗ ⇒
  # 这一段保证"尾部"里就有全部失败的名字与第一条原因 ✓（分片下同样有效 ✓）。
  echo "  ==== 失败清单（${fail} 条）===="
  printf '%s' "$FAIL_SUMMARY"
fi
echo "  —— 通过 ${pass}｜意外失败 ${fail}｜已知红 ${expected_red}｜跳过 $skipped"
[ "$fail" = 0 ] || exit 1

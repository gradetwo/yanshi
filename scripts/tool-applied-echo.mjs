#!/usr/bin/env node
// **`applied` 回显本次调用实际生效的参数**（A5；C2 的 `applied.doc_id` 共用断言）：
//   `brush_stroke` / `medium_stroke` 回执加 `applied: {wait_for_render,
//   wait_budget_ms, preview, silent, doc_id, color_space}` —— 取值一律读
//   `ctx`（`wait_for_render` 缺省 true、`wait_budget_ms` 缺省 500 构造期值、
//   `preview` 缺省 true、`silent` 缺省 false、`doc_id`）与调用方原参数；
//   `color_space` 按输入值的实际解释空间推导（浮点数组 ⇒ `linear`，
//   字节数组 / 对象式 ⇒ `srgb_bytes`，hex ⇒ `hex`）。
//
// **判据**（HTTP 臂 + MCP 粘性臂）：
//   ① 传 `wait_for_render:false` ⇒ `applied.wait_for_render == false`
//     （同时 `wait_budget_ms == 500`、`preview == true`、`silent == false`）；
//   ② 不传 ⇒ 回显缺省 `true`；
//   ③ 传 `doc_id`（换一份文档调）⇒ 回显该 id；
//   ④ 连续两调用、第二调用不传 `doc_id` ⇒ 回显与第一调用相同的 id
//      （粘性切换 —— HTTP 是无状态的，这条只能在 MCP stdio 会话里验：
//       服务端 `options.doc_id` 粘住成为后续调用的缺省）；
//   ⑤ `color_space` 三臂：浮点数组 ⇒ `"linear"`、字节数组 ⇒ `"srgb_bytes"`、
//      hex ⇒ `"hex"`（对象式同属 `srgb_bytes` 臂）；
//   ⑥ `medium_stroke` 也带 `applied`（`doc_id` + `color_space`）。
//
// **变异**：回显恒写缺省（`wait_for_render` 恒 `true`）⇒ ① 红；
//   `color_space` 恒写 `"srgb_bytes"` ⇒ ⑤ 的浮点臂与 hex 臂红；
//   粘性切换失效（第二调用回落到启动文档）⇒ ④ 红。
//   （MCP 臂 spawn `yanshi-mcp`：见 `mcp-document-switch.mjs` 同款做法。）
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-applied-echo.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (name, args, docId = doc, tok = token) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(tok)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const fail = (message) => {
  console.error(`  ✗ ${message}`);
  process.exit(1);
};
const fails = [];
const check = (ok, msg) => {
  if (!ok) fails.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};

const layer = await call("create_layer", { layer_id: "L_applied", name: "applied" });
if (layer.ok !== true && !String(layer.context?.detail ?? "").includes("已存在")) {
  fail(`create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
}
const strokePoints = [
  [40, 60, 0.8],
  [280, 180, 0.8],
];
const stroke = (extra) =>
  call("brush_stroke", {
    layer_id: "L_applied",
    brush: "calligraphy",
    points: strokePoints,
    size: 30,
    color: [0.2, 0.4, 0.9, 1.0],
    ...extra,
  });

// ① 传 wait_for_render:false
const noWait = await stroke({ wait_for_render: false });
if (noWait.ok !== true) fail(`① brush_stroke（wait_for_render:false）⇒ ${JSON.stringify(noWait).slice(0, 160)}`);
check(
  noWait.applied?.wait_for_render === false,
  `① 传 wait_for_render:false ⇒ applied.wait_for_render=${noWait.applied?.wait_for_render}`,
);
check(
  noWait.applied?.wait_budget_ms === 500,
  `① wait_budget_ms 回显构造期值=${noWait.applied?.wait_budget_ms}（缺省 500，非实测等待）`,
);
check(noWait.applied?.preview === true, `① preview 缺省回显=${noWait.applied?.preview}`);
check(noWait.applied?.silent === false, `① silent 缺省回显=${noWait.applied?.silent}`);
check(noWait.applied?.doc_id === doc, `① doc_id 回显=${noWait.applied?.doc_id}（应为 ${doc}）`);

// ② 不传 ⇒ 缺省 true
const def = await stroke({});
if (def.ok !== true) fail(`② brush_stroke（缺省）⇒ ${JSON.stringify(def).slice(0, 160)}`);
check(
  def.applied?.wait_for_render === true,
  `② 不传 wait_for_render ⇒ applied.wait_for_render=${def.applied?.wait_for_render}（缺省 true）`,
);

// ③ 传 doc_id（换一份文档调）⇒ 回显该 id
const otherDoc = `applied_${Date.now().toString(36)}`;
const made = await (
  await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: otherDoc, width: 320, height: 240 }),
  })
).json();
if (!made.token) {
  console.log("  ⊘ 不可判：建不了 ③ 臂的第二份文档");
  process.exit(3);
}
await call("create_layer", { layer_id: "L_applied", name: "applied" }, otherDoc, made.token);
const other = await call(
  "brush_stroke",
  {
    layer_id: "L_applied",
    brush: "calligraphy",
    points: strokePoints,
    size: 30,
    color: [0.2, 0.4, 0.9, 1.0],
  },
  otherDoc,
  made.token,
);
if (other.ok !== true) fail(`③ brush_stroke（换文档）⇒ ${JSON.stringify(other).slice(0, 160)}`);
check(other.applied?.doc_id === otherDoc, `③ 传 doc_id ⇒ 回显 ${other.applied?.doc_id}（应为 ${otherDoc}）`);

// ⑤ color_space 三臂（+ 对象式同属 srgb_bytes 臂）
const colorCases = [
  { color: [0.2, 0.4, 0.9, 1.0], want: "linear", label: "浮点数组 [0.2,0.4,0.9,1.0]" },
  { color: [115, 97, 77], want: "srgb_bytes", label: "字节数组 [115,97,77]" },
  { color: "#73391F", want: "hex", label: "hex #73391F" },
  { color: { r: 115, g: 97, b: 77, a: 255 }, want: "srgb_bytes", label: "对象式 {r:115,g:97,b:77}" },
];
for (const { color, want, label } of colorCases) {
  const r = await stroke({ color });
  if (r.ok !== true) fail(`⑤ brush_stroke（${label}）⇒ ${JSON.stringify(r).slice(0, 160)}`);
  check(r.applied?.color_space === want, `⑤ ${label} ⇒ color_space=${r.applied?.color_space}（应为 ${want}）`);
}

// ⑥ medium_stroke 也带 applied
const medium = await call("medium_stroke", {
  layer_id: "L_applied",
  medium: "oil",
  points: [
    [40, 60, 1],
    [280, 180, 1],
  ],
  size: 40,
  load: 0.8,
  color: { r: 30, g: 60, b: 200, a: 255 },
});
if (medium.ok !== true) fail(`⑥ medium_stroke ⇒ ${JSON.stringify(medium).slice(0, 160)}`);
check(medium.applied?.doc_id === doc, `⑥ medium applied.doc_id=${medium.applied?.doc_id}（应为 ${doc}）`);
check(
  medium.applied?.color_space === "srgb_bytes",
  `⑥ medium 对象式颜色 ⇒ color_space=${medium.applied?.color_space}（应为 srgb_bytes）`,
);
check(
  medium.applied?.wait_for_render === true,
  `⑥ medium wait_for_render 回显=${medium.applied?.wait_for_render}（构造期缺省 true）`,
);

// ④ 粘性切换（MCP stdio 会话：第一调用显式给 doc_id，第二调用不给 ⇒ 粘住）
const BIN = process.env.YANSHI_MCP_BIN || "target/release/yanshi-mcp";
if (!existsSync(BIN)) {
  console.log(`  ⊘ 不可判：MCP 二进制不存在（${BIN}）⇒ ④ 粘性臂跑不了`);
  process.exit(3);
}
const root = trackTemp(mkdtempSync(join(tmpdir(), "mcp-applied-echo-")));
const stickyDoc = `sticky_${Date.now().toString(36)}`;
const child = spawn(BIN, ["--root", root, "--doc", "_bootstrap", "--width", "320", "--height", "240"], {
  stdio: ["pipe", "pipe", "pipe"],
});
process.on("exit", () => {
  try {
    child.kill("SIGKILL");
  } catch {
    /* 已经没了 */
  }
  // 临时目录的清理由 `trackTemp`（`scripts/lib/temp-dirs.mjs`）接管 ✓
});
let buffer = "";
const seen = new Map();
child.stdout.on("data", (chunk) => {
  buffer += chunk;
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index);
    buffer = buffer.slice(index + 1);
    try {
      const message = JSON.parse(line);
      if (message.id) seen.set(message.id, message);
    } catch {
      /* 忽略非 JSON */
    }
  }
});
const send = (object) => child.stdin.write(`${JSON.stringify(object)}\n`);
const waitFor = async (id, ms = 20000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (seen.has(id)) return seen.get(id);
    await new Promise((r) => setTimeout(r, 100));
  }
  return null;
};
send({
  jsonrpc: "2.0",
  id: 1,
  method: "initialize",
  params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "judge", version: "1" } },
});
await waitFor(1);
send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
await new Promise((r) => setTimeout(r, 300));
let nextId = 10;
const mcpCall = async (tool, args) => {
  const current = ++nextId;
  send({ jsonrpc: "2.0", id: current, method: "tools/call", params: { name: tool, arguments: args } });
  const reply = await waitFor(current);
  if (!reply) return null;
  const text = reply.result?.content?.[0]?.text ?? "";
  try {
    return JSON.parse(text);
  } catch {
    return { _unparsed: text.slice(0, 120), _isError: reply.result?.isError };
  }
};
const mcpLayer = await mcpCall("create_layer", { doc_id: stickyDoc, layer_id: "L_sticky", name: "sticky" });
if (!mcpLayer || mcpLayer.ok !== true) {
  console.log(`  ⊘ 不可判：MCP 上建不了粘性测试的图层（${JSON.stringify(mcpLayer ?? null).slice(0, 120)}）`);
  process.exit(3);
}
const first = await mcpCall("brush_stroke", {
  doc_id: stickyDoc,
  layer_id: "L_sticky",
  brush: "calligraphy",
  points: strokePoints,
  size: 30,
  color: [0.2, 0.4, 0.9, 1.0],
});
if (!first || first.ok !== true) fail(`④ MCP 第一调用 ⇒ ${JSON.stringify(first ?? null).slice(0, 160)}`);
check(first.applied?.doc_id === stickyDoc, `④ 第一调用显式 doc_id ⇒ 回显 ${first.applied?.doc_id}`);
const second = await mcpCall("brush_stroke", {
  layer_id: "L_sticky",
  brush: "calligraphy",
  points: strokePoints,
  size: 30,
  color: [0.2, 0.4, 0.9, 1.0],
});
if (!second || second.ok !== true) fail(`④ MCP 第二调用（不传 doc_id）⇒ ${JSON.stringify(second ?? null).slice(0, 160)}`);
check(
  second.applied?.doc_id === stickyDoc,
  `④ 第二调用不传 doc_id ⇒ 回显 ${second.applied?.doc_id}（应与第一调用相同：粘住 ${stickyDoc}）`,
);
child.kill();

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（applied 回显，A5 + C2）`);
  process.exit(1);
}
console.log("\n  ✓ applied 回显：wait_for_render 缺省/显式、doc_id、粘性切换、color_space 三臂全对");

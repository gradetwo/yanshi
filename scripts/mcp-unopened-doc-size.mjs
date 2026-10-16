#!/usr/bin/env node
// **★ 未打开的文档也必须报出**真实尺寸**（真实用户报告 ✓，2026-10-16 ✓）★**
//
// **∴ 用户症状 ✗**：同一个 `--root` ✓、磁盘完好 ✓（`meta.json` 有尺寸、`atoms.jsonl` 有数据 ✓），
//   **`yanshi-serve` 报对** ✓ 而 **`yanshi-mcp` 报 `0×0`** ✗ ⇒ 后续 `brush_stroke` 无法继续 ✗。
//
// **∴ 根因 ✗**（本机实测 ✓）：`Workspace::list_documents` 的**未打开**分支 ✗
//   **只从 `meta.json` 取尺寸** ✓（`unwrap_or(0)` ✗）——
//   而**同一个表达式**已经在为 `layers`/`objects` **折原子**了 ✓
//   ⇒ **∴ 于是**：`meta.json` 一旦缺失 ⇒ **尺寸报 0×0** ✗ 而**图层/对象数却是对的** ✓
//     （本机实测同一响应里 `width:0,height:0` ＋ `layers:2,objects:1` ✓ —— 自相矛盾 ✓）。
//
// **∴ 为什么 serve 正常、MCP 不正常 ✗**（**这就是"两条路不一致"的机制 ✓**）★★：
//   **`serve` 每来一个请求都会**按需打开**那份文档** ✓ ⇒ 走的是"已打开"分支 ✓ ⇒ 尺寸来自**状态** ✓；
//   **`MCP` 只**预打开自己的 `--doc`** ✓ ⇒ **别的**文档在列表里走"**未打开**"分支 ✗
//     ⇒ **∴ 会话挂在 A 文档上时，B 文档就报 0×0** ✗
//     ＋ **∴ 更糟 ✗**：**在同一个进程里**，B 被 `get_document` 打开**之前**报 0×0 ✓、
//       **之后**报真实尺寸 ✗ ⇒ **同一个事实两个数** ✓（本判据正是钉这一条 ✓）。
//
// **∴ `meta.json` 怎么会缺 ✗**（**可达 ✓，不是臆造 ✓**）：
//   `create_document` **先** `append_atom`（`service.rs:1118` ✓）**后** `save_meta`（`:1121` ✓）
//   ⇒ **两步之间进程死掉**就留下「有原子、没 meta」的目录 ✓；
//   ＋ **且 `record_token` 会在 meta 缺失时**凭空写一份 `width:0,height:0`**✗**（`persist.rs:374` ✓）
//     ⇒ **∴ 那个 0×0 会**永久留在盘上**✗**（判据同时钉这一条 ✓）。
//
// **∴ 判据 ✗**（**两条都不许恒真 ✓**）：
//   ① **未打开**时 `list_documents` 报的尺寸 == **真实**尺寸（200×150 ✓），**且不为 0** ✓；
//   ② **同一次运行里**，"未打开时"与"打开之后"报的尺寸**必须相等** ✓（同一事实只能有一个数 ✓）。
//
// **∴ 能红 ✓**：把尺寸改回"只读 meta"（或让 `record_token` 写 0×0 ✓）⇒ ① 立刻红 ✓（实测 ✓）。
//
// 用法：node scripts/mcp-unopened-doc-size.mjs
import { spawn } from "node:child_process";
import { mkdirSync, rmSync, unlinkSync, existsSync, readdirSync } from "node:fs";

const BIN = process.env.YANSHI_MCP_BIN || "target/debug/yanshi-mcp";
const root = "/tmp/mcp-unopened-doc-size";
const DOC = "scene";
const W = 200;
const H = 150;

const fail = (message) => {
  console.log("  ✗ " + message);
  process.exit(1);
};
const cannotJudge = (message) => {
  console.log("  ✗ 判不了：" + message + " ⇒ 退出码 3");
  process.exit(3);
};

rmSync(root, { recursive: true, force: true });
mkdirSync(root, { recursive: true });

const session = (args) => {
  const child = spawn(BIN, args, { stdio: ["pipe", "pipe", "pipe"] });
  let buffer = "";
  const seen = new Map();
  const errors = [];
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
        /* 非 JSON 行忽略 ✓ */
      }
    }
  });
  child.stderr.on("data", (chunk) => errors.push(String(chunk)));
  const send = (object) => child.stdin.write(JSON.stringify(object) + "\n");
  const waitFor = async (id, ms = 30000) => {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      if (seen.has(id)) return seen.get(id);
      await new Promise((resolve) => setTimeout(resolve, 40));
    }
    return null;
  };
  let id = 100;
  return {
    errors,
    init: async () => {
      send({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "judge", version: "1" } },
      });
      await waitFor(1);
      send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
      await new Promise((resolve) => setTimeout(resolve, 250));
    },
    call: async (tool, args) => {
      const current = ++id;
      send({ jsonrpc: "2.0", id: current, method: "tools/call", params: { name: tool, arguments: args } });
      return await waitFor(current);
    },
    kill: () => child.kill(),
  };
};

/// 取 `tools/call` 结果里的 JSON 正文（失败返回 null ✓）。
const payload = (message) => {
  const content = message && message.result && message.result.content;
  const text = Array.isArray(content) ? content.find((item) => item.type === "text") : null;
  if (!text) return null;
  try {
    return JSON.parse(text.text);
  } catch {
    return null;
  }
};

// ---------- ① 用 MCP 建一份**有数据**的文档 ----------
const first = session(["--root", root, "--doc", DOC, "--width", String(W), "--height", String(H)]);
await first.init();
const layer = payload(await first.call("create_layer", { layer_id: "L1", name: "L1" }));
if (!layer || layer.ok !== true) cannotJudge(`建图层失败（${JSON.stringify(layer).slice(0, 120)}）`);
const stroke = payload(
  await first.call("brush_stroke", {
    layer_id: "L1",
    object_id: "s1",
    brush: "100%_Opaque",
    size: 30,
    color: { r: 200, g: 100, b: 50, a: 255 },
    points: [[20, 20], [120, 90]],
  }),
);
if (!stroke || stroke.ok !== true) cannotJudge(`画笔失败（${JSON.stringify(stroke).slice(0, 120)}）`);
first.kill();
await new Promise((resolve) => setTimeout(resolve, 400));

const docDir = `${root}/docs/${DOC}`;
const atomsPath = `${docDir}/atoms.jsonl`;
if (!existsSync(atomsPath)) cannotJudge(`磁盘上没有 ${atomsPath}（MCP 没落盘？）`);
console.log(`  ① 建好：${DOC} = ${W}×${H}，磁盘 ${JSON.stringify(readdirSync(docDir))}`);

// ---------- ② 构造「有原子、没 meta」的状态（= create_document 两步之间死掉 ✓）----------
unlinkSync(`${docDir}/meta.json`);
console.log(`  ② 删掉 meta.json（模拟 append_atom 与 save_meta 之间死掉 ✓）：${JSON.stringify(readdirSync(docDir))}`);

// ---------- ③ 新进程：会话挂在**别的**文档上 ⇒ 本文件走「未打开」那条路 ----------
const second = session(["--root", root, "--doc", "session_anchor"]);
await second.init();
const listedBeforeOpen = payload(await second.call("list_documents", {}));
if (!listedBeforeOpen || !Array.isArray(listedBeforeOpen.documents)) {
  cannotJudge(`list_documents 没返回文档清单（${JSON.stringify(listedBeforeOpen).slice(0, 140)}）`);
}
const before = listedBeforeOpen.documents.find((item) => item.doc_id === DOC);
if (!before) cannotJudge(`清单里没有 ${DOC}（拿到的：${JSON.stringify(listedBeforeOpen.documents.map((d) => d.doc_id))}）`);
const opened = payload(await second.call("get_document", { doc_id: DOC }));
if (!opened) cannotJudge("get_document 没返回 JSON 正文");
const listedAfterOpen = payload(await second.call("list_documents", {}));
const after = listedAfterOpen && listedAfterOpen.documents.find((item) => item.doc_id === DOC);
second.kill();
if (second.errors.length) console.log("  第二个进程 stderr ⇒ " + second.errors.join("").slice(0, 200).replace(/\n/g, " ⏎ "));

console.log(
  `  ③ ${DOC}：未打开时 ${before.width}×${before.height}` +
    `（layers=${before.layers} objects=${before.objects}）｜打开后 ${opened.width}×${opened.height}` +
    `｜再列一次 ${after ? `${after.width}×${after.height}` : "(没有)"}`,
);

const failures = [];
// ① 未打开时就必须报真实尺寸 ✓（0×0 是最刺眼的形态 ✓）
if (before.width !== W || before.height !== H) {
  failures.push(
    `未打开时报 ${before.width}×${before.height}，真实是 ${W}×${H}` +
      `（而**同一响应**已经正确折出 layers=${before.layers}/objects=${before.objects} ✓ ⇒ 尺寸不该另找一个来源 ✗）`,
  );
}
// ② 同一事实只能有一个数 ✓：未打开 / 打开后 / 再列一次，三者必须一致 ✓
if (opened.width !== before.width || opened.height !== before.height) {
  failures.push(`同一个文档在同一个进程里报了两个尺寸：未打开 ${before.width}×${before.height} ✗ vs get_document ${opened.width}×${opened.height} ✓`);
}
if (after && (after.width !== opened.width || after.height !== opened.height)) {
  failures.push(`打开之后再列一次又变了：${after.width}×${after.height}（应为 ${opened.width}×${opened.height}）✗`);
}
if (failures.length) {
  console.log("  ✗ " + failures.join("；"));
  process.exit(1);
}
console.log(`  ✓ 未打开也报真实尺寸 ${W}×${H}，且未打开/打开后/再列三次读数一致 ✓`);
process.exit(0);

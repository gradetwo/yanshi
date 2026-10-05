#!/usr/bin/env node
// **真正删除一份文档**判据（产品负责人报的缺口 ③）。
//
// 被审判的性质**不是**"有个删除按钮" ✗，而是**"删了之后它真的不在了，而且删不掉的时候会明说"** ✓：
//
//   ① **真删**：HTTP `DELETE /api/documents/<id>?confirm=<id>` ⇒ 磁盘上的文档目录消失 ✓、
//      `GET /api/documents` 里不再出现 ✓（关闭 ≠ 删除 ✓：关闭那条路必须**仍然保留** ✓，
//      并且**仍然不进删除语义** ✗）；
//   ② **反例（不存在）**：删一份从来没有过的文档 ⇒ `reference_not_found`，**不是**静默成功 ✓；
//   ③ **反例（正在使用中）**：文档还有**实时 WebSocket 连接**时删它 ⇒ `conflict` ✓，
//      而且它**仍然在列表里、目录也还在** ✓（不能半删 ✗）；
//   ④ **反例（没有确认）**：不带 `confirm`（或 `confirm` 对不上）⇒ 400 ✓，文档毫发无损 ✓；
//   ⑤ **两个面**（产品负责人的硬要求 ✓）：同一件事在 **HTTP 路由**与 **工具层**都能做 ✓ ——
//      工具的调用走 `POST /api/tools/delete_document`（与 MCP **同一个注册表** ✓），
//      并且 MCP 二进制的 core 清单里**真的有这个名字** ✓；
//      两面对**同一种失败**必须给出**同一个错误码** ✓（两条路各写一套就会漂移 ✗）；
//   ⑥ **blob 的诚实交代** ✓：回执里写明 `blobs_kept` ✓ 与"本项目还没有 GC" ✓
//      —— 不假装删了文档磁盘就立刻空了 ✗。
//
// 用法：node scripts/tool-delete-document.mjs <base-url> [docId] [token]
import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { createConnection } from "node:net";
import { randomBytes } from "node:crypto";
import { join } from "node:path";

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-delete-document.mjs <base-url>");
  process.exit(2);
}
let bad = 0;
const check = (label, ok, detail) => {
  console.log((ok ? "  ✓ " : "  ✗ ") + label + (detail ? "｜" + detail : ""));
  if (!ok) bad += 1;
};
const json = async (path, init) => {
  const response = await fetch(base + path, init);
  const text = await response.text();
  let value = null;
  try {
    value = JSON.parse(text);
  } catch (_) {
    value = { _raw: text.slice(0, 200) };
  }
  return { status: response.status, value };
};
const postJson = (path, body) =>
  json(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const listIds = async () => {
  const value = (await json("/api/documents")).value;
  return new Set(((value && value.documents) || []).map((doc) => doc.doc_id));
};
const makeDoc = async (docId) => {
  const created = await postJson("/api/documents", { doc_id: docId, width: 64, height: 48 });
  return created.value && created.value.token;
};

const A = "crit_delete_a";
const B = "crit_delete_b";
const C = "crit_delete_c";
const D = "crit_delete_inuse";
const MISSING = "crit_delete_never_existed";
const tokenA = await makeDoc(A);
const tokenB = await makeDoc(B);
await makeDoc(C);
const tokenD = await makeDoc(D);
if (!tokenA || !tokenB || !tokenD) {
  console.error("判据无法运行：建不出夹具文档（不是通过）");
  process.exit(1);
}

/// **保持连接**的 WebSocket 客户端 ✓（订阅要**一直挂着** ✓ —— 握手完就断的探针测不出"正在使用中" ✗）。
const openSocket = (docId, token) =>
  new Promise((resolve, reject) => {
    const host = new URL(base);
    const socket = createConnection({ host: host.hostname, port: Number(host.port) }, () => {
      socket.write(
        [
          `GET /ws?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)} HTTP/1.1`,
          `Host: ${host.host}`,
          "Upgrade: websocket",
          "Connection: Upgrade",
          `Sec-WebSocket-Key: ${randomBytes(16).toString("base64")}`,
          "Sec-WebSocket-Version: 13",
        ].join("\r\n") + "\r\n\r\n",
      );
    });
    let seen = "";
    socket.on("data", (chunk) => {
      seen += chunk.toString("latin1");
      if (seen.includes("\r\n\r\n")) {
        if (seen.includes("101")) resolve(socket);
        else {
          socket.destroy();
          reject(new Error(seen.split("\r\n")[0]));
        }
      }
    });
    socket.on("error", reject);
    setTimeout(() => reject(new Error("WS 握手超时")), 5000);
  });

const deleteVia = (docId, token) =>
  json(`/api/tools/delete_document?doc=${encodeURIComponent(B)}&token=${encodeURIComponent(token)}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ document_id: docId }),
  });

// ① 反例：删一份**从来没有过**的文档 ⇒ 明确报"不存在"。
const before = await listIds();
const missing = await json(
  `/api/documents/${MISSING}?confirm=${MISSING}`,
  { method: "DELETE" },
);
check(
  "删一份不存在的文档 ⇒ `reference_not_found`（不是静默成功）",
  missing.status === 404 && missing.value && missing.value.error_code === "reference_not_found",
  "status=" + missing.status + " " + JSON.stringify(missing.value).slice(0, 160),
);
const afterMissing = await listIds();
check(
  "这次失败没有改变文档列表",
  before.size === afterMissing.size,
  before.size + " ⇒ " + afterMissing.size,
);

// ② 反例：没有 `confirm` / `confirm` 对不上 ⇒ 400，而且**一个字节都不动**。
const noConfirm = await json(`/api/documents/${A}`, { method: "DELETE" });
const wrongConfirm = await json(`/api/documents/${A}?confirm=${B}`, { method: "DELETE" });
check(
  "不带 confirm ⇒ 400（删除不可逆，要再写一遍 id）",
  noConfirm.status === 400,
  "status=" + noConfirm.status + " " + JSON.stringify(noConfirm.value).slice(0, 140),
);
check(
  "confirm 对不上 ⇒ 400",
  wrongConfirm.status === 400,
  "status=" + wrongConfirm.status + " " + JSON.stringify(wrongConfirm.value).slice(0, 140),
);
check("两次被拒的删除都没有动文档 A", (await listIds()).has(A), "A 仍在列表里");

// ③ 反例：**正在使用中**（有实时连接）⇒ conflict，且不能半删。
const socket = await openSocket(A, tokenA);
await sleep(300);
const inUse = await json(`/api/documents/${A}?confirm=${A}`, { method: "DELETE" });
check(
  "文档有实时连接时删除 ⇒ `conflict`（不是成功、也不是半删）",
  inUse.status === 409 && inUse.value && inUse.value.error_code === "conflict",
  "status=" + inUse.status + " " + JSON.stringify(inUse.value).slice(0, 180),
);
check("被拒绝的删除之后，文档 A **还在列表里**", (await listIds()).has(A), "A 仍在");
const stillReadable = await json(`/api/documents/${A}?token=${encodeURIComponent(tokenA)}`);
check(
  "被拒绝的删除之后，文档 A 的内容**仍然读得到**",
  stillReadable.status === 200 && stillReadable.value && stillReadable.value.ok === true,
  "status=" + stillReadable.status,
);

// ④ 关掉连接之后 ⇒ 真的删掉。
socket.destroy();
let removed = { status: 0, value: null };
for (let attempt = 0; attempt < 20; attempt += 1) {
  removed = await json(`/api/documents/${A}?confirm=${A}`, { method: "DELETE" });
  if (removed.status === 200) break;
  await sleep(250);
}
check(
  "连接关掉之后删除成功（`ok` 与 `deleted` 都为真 —— 回执要能被两处都读懂）",
  removed.status === 200 && removed.value && removed.value.ok === true &&
    removed.value.deleted === true,
  "status=" + removed.status + " " + JSON.stringify(removed.value).slice(0, 180),
);
const afterDelete = await listIds();
check("删掉的文档从 `GET /api/documents` 里消失", !afterDelete.has(A), "A 在列表里=" + afterDelete.has(A));
check("同一次删除**没有误伤**别的文档", afterDelete.has(B) && afterDelete.has(C), "B/C 仍在");
const workspace = process.env.YANSHI_WORKSPACE;
if (workspace) {
  const dir = join(workspace, "docs", A);
  check("磁盘上的文档目录真的没了", !existsSync(dir), dir);
} else {
  console.log("  ⊘ 没有 YANSHI_WORKSPACE ⇒ 跳过「磁盘目录已消失」这一条（不是通过）");
}

// ⑤ **关闭 ≠ 删除**：关闭那条路仍然只关内存。
const closed = await postJson(
  `/api/documents/${B}/close?token=${encodeURIComponent(tokenB)}`,
  {},
);
check(
  "`POST /api/documents/<id>/close` 仍然只关内存（`deleted: false`）",
  closed.status === 200 && closed.value && closed.value.deleted === false,
  "status=" + closed.status + " " + JSON.stringify(closed.value).slice(0, 180),
);
check("关闭之后文档 B **仍在列表里**（关闭不是删除）", (await listIds()).has(B), "B 仍在");

// ⑥ **工具层（MCP 同一个注册表）**：同一个能力、同一种失败。
const catalog = (await json("/api/tools")).value;
const spec = ((catalog && catalog.tools) || []).find((tool) => tool.name === "delete_document");
check(
  "工具清单里有 `delete_document`（Web 与 MCP 共用同一份清单）",
  Boolean(spec) && Boolean(spec.inputSchema),
  JSON.stringify(spec || {}).slice(0, 160),
);
check(
  "它的必填参数是 `document_id`（与会话 `doc_id` 分开 —— 否则「删不存在」会先被创建出来）",
  Boolean(spec) &&
    Array.isArray(spec.inputSchema.required) &&
    spec.inputSchema.required.includes("document_id"),
  JSON.stringify((spec && spec.inputSchema && spec.inputSchema.required) || []),
);
const toolMissing = await deleteVia(MISSING, tokenB);
check(
  "工具面删不存在的文档 ⇒ **与路由同一个** `reference_not_found`",
  toolMissing.value && toolMissing.value.ok === false &&
    toolMissing.value.error_code === missing.value.error_code,
  "工具=" + JSON.stringify(toolMissing.value).slice(0, 120),
);
const socketD = await openSocket(D, tokenD);
await sleep(300);
const toolInUse = await deleteVia(D, tokenB);
check(
  "工具面删正在使用中的文档 ⇒ **与路由同一个** `conflict`",
  toolInUse.value && toolInUse.value.ok === false && toolInUse.value.error_code === inUse.value.error_code,
  "工具=" + JSON.stringify(toolInUse.value).slice(0, 160),
);
socketD.destroy();
await sleep(400);
let toolDeleted = { status: 0, value: null };
for (let attempt = 0; attempt < 20; attempt += 1) {
  toolDeleted = await deleteVia(D, tokenB);
  if (toolDeleted.status === 200) break;
  await sleep(250);
}
check(
  "工具面**真的删掉**了文档 D（不是只关内存）",
  toolDeleted.status === 200 && toolDeleted.value && toolDeleted.value.ok === true &&
    toolDeleted.value.deleted === true &&
    !(await listIds()).has(D),
  "status=" + toolDeleted.status + " " + JSON.stringify(toolDeleted.value).slice(0, 180),
);
check(
  "工具面返回的回执与路由**同一形状**（`deleted` / `freed_bytes` / `blobs_kept`）",
  toolDeleted.value &&
    typeof toolDeleted.value.freed_bytes === "number" &&
    typeof toolDeleted.value.blobs_kept === "boolean",
  JSON.stringify(toolDeleted.value).slice(0, 200),
);
check(
  "回执**明说 blob 没回收**（GC 不在范围内，不许暗示磁盘立刻空了）",
  toolDeleted.value && toolDeleted.value.blobs_kept === true &&
    /GC/.test(String(toolDeleted.value.note || "")),
  String((toolDeleted.value && toolDeleted.value.note) || "").slice(0, 160),
);

// ⑦ MCP 二进制自己的清单里也必须真有这个名字（工具清单一致 ≠ MCP 面真能调 ✓）。
const mcpBin =
  process.env.YANSHI_MCP_BIN ||
  [
    join("target", "debug", "yanshi-mcp"),
    process.env.CARGO_TARGET_DIR ? join(process.env.CARGO_TARGET_DIR, "debug", "yanshi-mcp") : null,
  ]
    .filter(Boolean)
    .find((candidate) => existsSync(candidate)) ||
  join("target", "debug", "yanshi-mcp");
if (existsSync(mcpBin)) {
  try {
    const raw = execFileSync(mcpBin, ["--list-tools", "--profiles", "core"], {
      encoding: "utf8",
      timeout: 60000,
    });
    const names = (JSON.parse(raw).tools || []).map((tool) => tool.name);
    check(
      "MCP 的 core 清单里也有 `delete_document`",
      names.includes("delete_document"),
      "core 工具数 " + names.length,
    );
  } catch (error) {
    check("MCP 的 core 清单可读", false, String(error).slice(0, 160));
  }
} else {
  check("找得到 MCP 可执行文件（否则「两个面」缺一半）", false, mcpBin);
}

console.log(
  bad
    ? `  结论：${bad} 条不成立 ✗（删除还不诚实，或只有一个面）`
    : "  结论：真删、反例清楚、路由与工具两个面一致 ✓",
);
process.exit(bad ? 1 : 0);

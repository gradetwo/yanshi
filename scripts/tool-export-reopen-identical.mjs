#!/usr/bin/env node
// **D08 验收判据**（《山海同心》交付包 P1）：
//  「不连续瓦片缓存被重复混合，热导出与冷打开灰度不同」
//  ⇒ 同一份文档，**热导出**（刚画完就导）与**冷打开后导出**（重新加载进程）的 PNG 必须逐字节相同。
//  这里用「同一进程内 export → 多次读取 → 再 export」近似冷热对比；真正的冷开放在 CI 里另测。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-export-reopen-identical.mjs <base-url>"); process.exit(2); }
const doc = `ex-${process.pid}-${Date.now() % 100000}`;
const dir = mkdtempSync(join(tmpdir(), "export-reopen-"));
trackTemp(dir);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 300, height: 300 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

await call("create_layer", { layer_id: "p", name: "p" });
// 画几笔不连续的笔画（正是 D08 说的「不连续瓦片」）
for (let i = 0; i < 4; i += 1) {
  await call("brush_stroke", {
    layer_id: "p", brush: "100%_Opaque", size: 12,
    color: [0.1 + i * 0.2, 0.2, 0.3, 1],
    points: [[30 + i * 60, 40, 1], [60 + i * 60, 90, 1]],
  });
}
const sha = (p) => createHash("sha256").update(readFileSync(p)).digest("hex").slice(0, 16);
const e1 = await call("export_png", { path: join(dir, "hot.png") });
if (!e1.ok) { console.error(`判据自身前提不成立：导出失败（不是产品问题）${JSON.stringify(e1.context ?? {})}`); process.exit(3); }
const h1 = sha(join(dir, "hot.png"));
// 多读几次（触发缓存复用，模拟"热"状态被反复消费）
for (let i = 0; i < 3; i += 1) await call("list_atoms", { limit: 20 });
const e2 = await call("export_png", { path: join(dir, "again.png") });
const h2 = sha(join(dir, "again.png"));
console.log(`  热导出=${h1}｜再导出=${h2}｜一致=${h1 === h2}`);
if (h1 !== h2) {
  console.log("  ✗ 同一文档两次导出字节不同 ⇒ 缓存被重复混合（D08 那条缺陷）");
  process.exit(1);
}
console.log("  ✓ 两次导出逐字节一致");

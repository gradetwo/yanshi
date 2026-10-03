#!/usr/bin/env node
// **声明的参数面 vs 实现接受的参数面** —— 一条能红的对账判据。
//
// **为什么需要它**：实测到 `brush_preview` 的实现会读 hardness/opacity，而它的公开参数面里没有，
// 调用方只好"猜"；这类不一致正是"每个工具带可复制调用示例"要治的病。
//
// **做法**：① 从 `/api/tools` 取每个工具声明的参数名；② 用**一个不存在的参数**去调它，
// 工具层会把"可用参数"列出来（实测过：`不接受参数 hardness（拼写错误？）；可用参数：brush, si…`）；
// ③ 两边的**集合**必须相等。
// 若某个工具先报别的错（缺必填项等）⇒ 记为"未能对账"（**如实计数**，不算通过）。
const base = process.argv[2];
const token = process.argv[3];
if (!base || !token) { console.error("用法: node scripts/tool-param-parity.mjs <server-base> <token>"); process.exit(2); }
const catalogue = await fetch(`${base}/api/tools`).then((r) => r.json());
const tools = catalogue.tools || catalogue.data?.tools || [];
if (!tools.length) { console.error("没取到工具清单 ✗"); process.exit(2); }
const doc = "tp1";
// **必须用它自己创建文档时返回的 token** ✗ —— 旧版建了 `tp1` ✓ 却仍用**调用方传进来的 token**（属于别的文档 ✗）
// ⇒ 服务端先报"token/文档不匹配" ✗ ⇒ "可用参数：…"提示永不出现 ⇒ **127 个全对不了账** ✓
//（第 245/246 轮观察到的现象 ✓，真因就在这里 ✓）。
const created = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 900, height: 640 }),
}).then((r) => r.json()).catch(() => ({}));
const probeToken = created.token || token;                 // 建不出来就退回调用方给的 ✓
if (created.token) console.log("  已为对账文档 " + doc + " 取到专用 token ✓");
else console.log("  ⚠ 未能为 " + doc + " 取到 token ⇒ 退回调用方那个（可能不匹配 ⇒ 会全判无效 ✗）");
let checked = 0, mismatched = 0, unchecked = 0;
const rows = [];
for (const tool of tools) {
  const name = tool.name;
  // **目录的形状是 `inputSchema`（JSON Schema）** ✗ —— 本判据原来只认 `tool.parameters` ✓，
  // 而服务端早已改成 `inputSchema.properties` / `inputSchema.required` ✓
  // ⇒ "声明的参数"恒为空 ⇒ 探针只发 `{__probe_unknown__:1}` ⇒ 每个工具都先报"缺必填" ✗
  // ⇒ 结果就是 **127 个全部对不了账** ✓（本判据自己报的"先报了别的错" ✓，见第 245 轮 ✓）。
  // 这里**两种形状都认** ✓（向后兼容 ✓，也让这条判据在两种目录下都有效 ✓）。
  const declaredParams = Array.isArray(tool.parameters)
    ? tool.parameters
    : Object.entries(tool.inputSchema?.properties || {}).map(([name, schema]) => ({
        name,
        type: schema?.type,
        required: (tool.inputSchema?.required || []).includes(name),
        description: schema?.description,
      }));
  const declared = new Set(declaredParams.map((p) => p.name));
  // **按声明的类型造一份像样的参数** —— 否则工具先报"缺必填项"✗，那个"可用参数"提示根本不出现 ✗
  //（上一版就是这么得到"可对账 0 个"却报"全部一致"的**假绿** ✗ —— 判据必须先证明自己会红 ✓）。
  const argumentsForProbe = { __probe_unknown__: 1 };
  // **只发那一个"未知参数"** ✓ —— 本判据要的是服务端回"**可用参数：…**"那句提示 ✓，
  // 而它的**取值**就是我需要对账的"实现真正接受的参数名" ✓。
  // 旧版会**同时**塞一份"像样的参数" ✗ ⇒ 那些参数先撞上别的校验（例如 `layer_id: "x"` 不存在 ✗）
  // ⇒ 未知参数提示不出现 ⇒ **127 个全被判"未能对账"** ✓（第 245 轮观察到的现象 ✓）。
  // ⇒ 这里保持"**只发未知参数**" ✓（这也是它注释里写的初衷 ✓）。
  const response = await fetch(`${base}/api/tools?doc=${doc}&token=${probeToken}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: name, arguments: argumentsForProbe }),
  }).then((r) => r.json());
  const detail = (response.context || {}).detail || "";
  const match = detail.match(/可用参数：(.+)$/);
  if (!match) { unchecked += 1; continue; }
  // **先把尾注切掉** ✗ —— 服务端那句是
  // `…；可用参数：a, b, c；框架级参数：doc_id, actor, session` ✓，
  // 旧版只按 `[,，、]` 分隔 ✗ ⇒ **最后一个参数名会粘上 `；框架级参数：…`** ✗
  // ⇒ 于是 127 个工具全被误报成"实现收了但没声明" ✓（见第 247 轮 ✓）。
  const acceptedText = match[1].split(/[；;]\s*框架级参数/)[0];
  // **"无"不是一个参数名** ✗ —— 工具不收任何参数时服务端会说 `可用参数：无` ✓，
  // 旧版把它当名字 ⇒ 13 个工具被误报成不一致 ✓（见第 247 轮 ✓）。
  const accepted = new Set(
    acceptedText.split(/[,，、]/).map((s) => s.trim())
      .filter((name) => name && name !== "无" && name !== "none"),
  );
  declared.delete("__probe_unknown__");
  const missing = [...accepted].filter((p) => !declared.has(p));       // 实现收、声明没有
  const extra = [...declared].filter((p) => !accepted.has(p));         // 声明有、实现不收
  checked += 1;
  if (missing.length || extra.length) {
    mismatched += 1;
    rows.push({ name, missing, extra });
  }
}
console.log(`  对账完成：可对账 ${checked} 个 ✓、未能对账 ${unchecked} 个（先报了别的错）、不一致 ${mismatched} 个`);
for (const row of rows.slice(0, 12)) {
  console.log(`  ${row.name.padEnd(24)} 实现收了但没声明: [${row.missing.join(", ")}]｜声明了但不收: [${row.extra.join(", ")}]`);
}
// **零对账 = 失败** ✗（不然"什么都没查到"会被读成"全都对"✗ —— 这正是本项目最反对的假绿 ✓）。
if (checked === 0) {
  console.log("结论：**一个都没能对账** ✗ ⇒ 判据无效（不是「全部一致」✗）");
  process.exit(1);
}
console.log(mismatched ? `结论：${mismatched} 个工具的声明与实现不一致 ✗` : "结论：全部一致 ✓");
process.exit(mismatched ? 1 : 0);

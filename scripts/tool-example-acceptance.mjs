#!/usr/bin/env node
// 示例必须**真的能被工具接受**（不只是名字对得上）。
//
// 判据：把目录里每个工具自己的 `example` 原样发给它 ——
//   若回的是「不接受参数 …」或「类型/形状不对」一类 ⇒ **红** ✗（说明抄走会被拒）；
//   其它错误（缺前置对象、文档状态、值域…）⇒ 记为「参数被接受，另有原因」✓ 并打印出来。
// 这样"可复制示例"就有了实质判据，而不只是"名字出现在参数面里"。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-example-acceptance.mjs <server-base>"); process.exit(2); }
const doc = "ea1";
// **用自己这份文档的 token** —— 上一版拿外面传进来的 token，结果每次都是
// capa token 无效（工具根本没执行），而脚本却报「参数被接受」：**典型的假绿**。
const created = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 900, height: 640 }),
}).then((r) => r.json());
const token = created.token;
if (!token) { console.error("建文档没拿到 token ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const catalogue = await fetch(`${base}/api/tools?doc=${doc}&token=${token}`).then((r) => r.json());
const withExample = (catalogue.tools || []).filter((tool) => tool.example);
console.log(`  目录里带示例的工具：${withExample.length} 个`);
let rejected = 0, accepted = 0;
for (const tool of withExample) {
  const response = await fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: tool.name, arguments: tool.example }),
  }).then((r) => r.json());
  const detail = (response.context || {}).detail || "";
  // **鉴权失败 ⇒ 判据无效** ✗（不是"通过"）：工具根本没执行，谈不上"参数被接受"。
  if (/token|鉴权|未授权|unauthorized/i.test(detail)) {
    console.error(`  ✗ ${tool.name}：**token 无效 ⇒ 判据无效**（不是通过）⇒ ${detail.slice(0, 60)}`);
    process.exit(1);
  }
  const badParameter = /不接受参数|不是合法 JSON|invalid type|缺少必填|应当/.test(detail);
  if (badParameter) { rejected += 1; console.log(`  ✗ ${tool.name}：示例被拒 ⇒ ${detail.slice(0, 90)}`); }
  else { accepted += 1; console.log(`  ✓ ${tool.name}：参数被接受${response.ok ? "（并且成功）" : "（另有原因：" + detail.slice(0, 46) + "）"}`); }
}
console.log(`  结论：${accepted} 个示例的参数被接受 ✓、${rejected} 个被参数/类型拒绝 ✗`);
process.exit(rejected ? 1 : 0);

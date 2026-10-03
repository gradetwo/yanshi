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
const call = async (name, args) =>
  fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: name, arguments: args }),
  }).then((r) => r.json());

// **先把前置建好** —— 否则 brush_stroke / get_object / delete_object 必然因为
// 没有图层或对象而失败，判据就只能停在"参数被接受"（那比"真的能用"弱一层）。
// 图层与对象的名字**故意与示例里写的一致**（L1 / o1），这样示例可以原样抄走。
// **前置就用 create_layer 自己的那个示例** —— 上一版我另外手建了一次 L1，
// 于是轮到 create_layer 的示例时它报「图层 L1 已存在」⇒ **判据被自己的前置绊倒** ✗。
// 现在只建这一次：它既是前置，也正好是 create_layer 示例的那一次运行。
const layerExample = (catalogue.tools || []).find((t) => t.name === "create_layer" && t.example);
const layer = layerExample ? await call("create_layer", layerExample.example) : { ok: false, context: { detail: "没有 create_layer 示例" } };
console.log(`  前置（create_layer 示例本身）：${layer.ok ? "成功" : "失败 ⇒ " + ((layer.context || {}).detail || "").slice(0, 60)}`);
const painted = await call("brush_stroke", {
  layer_id: "L1", object_id: "o1", brush: "100%_Opaque", size: 40,
  points: [[60, 60, 1], [120, 60, 1]],
});
console.log(`  建对象 o1：${painted.ok ? "成功" : "失败 ⇒ " + ((painted.context || {}).detail || "").slice(0, 60)}`);

// **破坏性的排到最后** —— 上一轮实测：`delete_layer` 先把图层删了，
// 后面 `brush_stroke` / `get_object` 就全都没得用了（判据被自己的顺序搞红 ✗）。
// **破坏性的排到最后，而且删对象的要排在删图层之前** —— 实测：
// `delete_layer` 会把它上面的对象一起清掉（对象被 tombstone ⇒ 再删就被拒 ✗）。
const rank = (name) => (/delete_layer/.test(name) ? 2 : /delete/.test(name) ? 1 : 0);
const withExample = (catalogue.tools || [])
  .filter((tool) => tool.example)
  .sort((a, b) => rank(a.name) - rank(b.name));
console.log(`  目录里带示例的工具：${withExample.length} 个`);
let rejected = 0, accepted = 0;
for (const tool of withExample) {
  if (tool.name === "create_layer") {
    accepted += 1;
    console.log(`  ✓ create_layer：**成功** ✓（就是上面那次前置运行）`);
    continue;
  }
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
  // **值/格式/白名单类错误也算红** ✓ —— 上一轮它们只被记成"另有原因"✗，
  // 而"另有原因"里混着"颜色写成字符串""资产种类不存在"这类**抄走就会失败**的问题 ✓。
  const badParameter =
    /不接受参数|不是合法 JSON|invalid type|缺少必填|应当|格式非法|未知资产种类|要一起给|必须|不在允许/.test(detail);
  // **门槛收到"必须成功"** ✓：前置已经建好 ⇒ 任何失败都算这个示例还没到位 ✗。
  if (response.ok) { accepted += 1; console.log(`  ✓ ${tool.name}：**成功** ✓`); }
  else if (badParameter) { rejected += 1; console.log(`  ✗ ${tool.name}：示例被拒 ⇒ ${detail.slice(0, 90)}`); }
  else { rejected += 1; console.log(`  ✗ ${tool.name}：参数没问题但**没成功** ⇒ ${detail.slice(0, 90)}`); }
}
console.log(`  结论：${accepted} 个示例**跑通** ✓、${rejected} 个没跑通 ✗`);
process.exit(rejected ? 1 : 0);

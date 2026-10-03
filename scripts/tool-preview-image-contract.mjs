#!/usr/bin/env node
// **P0-1 契约判据**：调了 `include_image: true` 就**必须**能拿到图 ✓，或者**必须**明确说明为什么没给 ✓。
// 来源：第三方 MCP 实测报告 P0-1 ✓（"画 50 笔之前看不到任何效果，只能盲画" ✗）——
// 当时的真实原因是 `render_region` 在区域 >512px 时**静默降级**成 `yanshi://blob/…` ✗
// （调用方拿到地址却**不知道为什么不给图** ✗）。这条判据把这个契约**钉死** ✓：
//   ① `render_region` 大区域 ⇒ 必须 `image_omitted` + 非空 `image_omitted_reason` ✓（且**不许**有 image ✓）；
//   ② `render_region` 小区域 ⇒ 必须有 `image.data` ✓；
//   ③ `render_region` 大区域 + `max_px` 放大 ⇒ 必须有 `image.data` ✓（**给得出图**是硬要求 ✓）；
//   ④ `brush_preview` + `include_image` ⇒ 必须有 `image.data` ✓（它本来就该内嵌 ✓）。
// 用法：node scripts/tool-preview-image-contract.mjs <server-base> <doc-id> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-preview-image-contract.mjs <server-base> <doc-id> <token>");
  process.exit(2);
}
const call = async (name, args) => {
  const response = await fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: name, arguments: args }),
  });
  return response.json();
};
const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? ` ⇒ ${detail}` : ""}`);
  if (!ok) failures.push(label);
};
const big = await call("render_region", { region: [0, 0, 900, 640], include_image: true });
check(big.image_omitted === true, "大区域明确标记 image_omitted", `image_omitted=${big.image_omitted}`);
check(typeof big.image_omitted_reason === "string" && big.image_omitted_reason.length > 10,
  "大区域给出**可读的原因**", String(big.image_omitted_reason || "").slice(0, 60));
check(big.image === undefined, "大区域**不**塞超大数据", `image=${big.image ? "有" : "无"}`);
const small = await call("render_region", { region: [0, 0, 64, 64], include_image: true });
check(typeof small.image?.data === "string" && small.image.data.length > 0,
  "小区域正常内嵌 base64", `base64 ${small.image?.data?.length || 0} 字符`);
const raised = await call("render_region", { region: [0, 0, 900, 640], include_image: true, max_px: 900 });
check(typeof raised.image?.data === "string" && raised.image.data.length > 0,
  "提高 max_px 后大区域**也给得出图**", `base64 ${raised.image?.data?.length || 0} 字符`);
const preview = await call("brush_preview", { brush: "100%_Opaque", size: 24, include_image: true });
check(typeof preview.image?.data === "string" && preview.image.data.length > 0,
  "brush_preview 内嵌 base64", `base64 ${preview.image?.data?.length || 0} 字符`);
if (failures.length) {
  console.log(`  ✗ P0-1 契约未满足：${failures.length} 条（${failures.join("；")}）`);
  process.exit(1);
}
console.log("  ✓ P0-1 契约满足：要图有图、不给图必说明原因");

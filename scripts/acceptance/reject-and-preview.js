// **「拒绝标注」与「预览建议」的真机验收** ✓。
// 判据四条事实 ✓：① 拒绝后状态 = rejected ✓；② 预览**给出了逐步校验** ✓；
// ③ 预览**没有应用**（效果数不变 ✓）；④ 接受之后效果数**才** +1 ✓。
(async () => {
  const out = {};
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const effects = async () => ((await call("list_effects", {})).effects || []).length;
  // ① 造一条标注 ✓ 再点「拒绝」✓
  await call("create_layer", { layer_id: "L" });
  const made = await call("create_annotation", { type: "region", intent: "modify",
    target: { target: "region", bbox: { x: 10, y: 10, w: 16, h: 16 } }, content: "拒绝验收" });
  out.annotationOk = made.ok;
  const annReload = document.getElementById("annotationRefresh");
  if (annReload) annReload.click();
  await new Promise((r) => setTimeout(r, 1500));
  const reject = Array.from(document.querySelectorAll("#annotationList button")).find((b) => b.textContent === "拒绝");
  out.rejectButtonExists = !!reject;
  if (reject) reject.click();
  await new Promise((r) => setTimeout(r, 1800));
  const listed = await call("list_annotations", { status: "rejected" });
  out.rejectedOnServer = (listed.annotations || []).length;
  // ② 造一条带补丁的建议 ✓ 点「预览」✓
  out.effectsBeforePreview = await effects();
  const suggested = await call("suggest", { summary: "预览验收", priority: 5,
    patch: [{ tool: "add_adjustment", arguments: { layer_id: "L", adjustment_type: "brightness_contrast", params: {} } }] });
  out.suggestOk = suggested.ok;
  const sgReload = document.getElementById("suggestionReload");
  if (sgReload) sgReload.click();
  await new Promise((r) => setTimeout(r, 1500));
  const preview = Array.from(document.querySelectorAll("#suggestionList button")).find((b) => b.textContent === "预览");
  out.previewButtonExists = !!preview;
  if (preview) preview.click();
  await new Promise((r) => setTimeout(r, 2200));
  out.previewText = (document.getElementById("suggestionPreview") || {}).textContent;
  out.effectsAfterPreview = await effects();
  // ③ 再点「接受」⇒ 这时效果数才 +1 ✓
  const accept = Array.from(document.querySelectorAll("#suggestionList button")).find((b) => b.textContent === "接受");
  if (accept) accept.click();
  await new Promise((r) => setTimeout(r, 2500));
  out.effectsAfterAccept = await effects();
  return out;
})()

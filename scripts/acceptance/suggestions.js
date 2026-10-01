// **建议面板的真机验收** ✓：造一条带可执行补丁的建议 → 面板列出 → 接受 ⇒ 补丁被重放 ✓。
// 判据两条事实 ✓：① 效果列表多出一条 ✓（= 补丁真的执行了 ✓）；② 建议状态变 accepted ✓。
(async () => {
  const out = {};
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const effects = async () => ((await call("list_effects", {})).effects || []).length;
  const suggestions = async (status) => {
    const listed = await call("list_suggestions", status ? { status } : {});
    return (listed.suggestions || listed.items || []);
  };
  out.panelExists = !!document.getElementById("suggestionList");
  // ① 先备一个图层 ✓，再**造一条带补丁的建议** ✓（补丁 = 加一个调整 ✓）
  const layer = await call("create_layer", { layer_id: "L_sg", name: "建议验收" });
  out.layerOk = layer.ok;
  out.effectsBefore = await effects();
  const made = await call("suggest", {
    summary: "把这一笔的亮度提一点（示例补丁）",
    priority: 7,
    patch: [{ tool: "add_adjustment", arguments: { layer_id: "L_sg", adjustment_type: "brightness_contrast", params: {} } }],
  });
  out.suggestOk = made.ok;
  await new Promise((r) => setTimeout(r, 800));
  // ② 面板列出 ✓
  const reload = document.getElementById("suggestionReload");
  if (reload) reload.click();
  await new Promise((r) => setTimeout(r, 1500));
  out.rows = document.querySelectorAll("#suggestionList .annotation-row").length;
  out.rowsBefore = out.rows;
  // ③ 点「接受」 ✓
  const accept = Array.from(document.querySelectorAll("#suggestionList button")).find((b) => b.textContent === "接受");
  if (accept) accept.click();
  await new Promise((r) => setTimeout(r, 3000));
  out.effectsAfter = await effects();
  out.accepted = (await suggestions("accepted")).length;
  out.pending = (await suggestions("pending")).length;
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-200);
  return out;
})()

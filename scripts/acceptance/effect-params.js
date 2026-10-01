// **调整/滤镜"改参数"的真机验收** ✓：加一个 → 点它 → 改参数 → 更新 ✓。
// 判据两条**事实** ✓：效果**条数不变** ✓（是更新不是新增 ✓）、**参数真的变了** ✓。
(async () => {
  const out = {};
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const effects = async () => {
    const value = await call("list_effects", {});
    return (value.effects || []).map((e) => ({ id: e.object_id, type: e.adjustment_type || e.filter_name, params: e.params }));
  };
  // ① 选一个调整并应用 ✓（参数留空 ⇒ 用它的默认值 ✓）
  const kind = document.getElementById("effectKind");
  if (kind) { kind.value = "adjustment"; kind.dispatchEvent(new Event("change", { bubbles: true })); }
  await new Promise((r) => setTimeout(r, 600));
  const box = document.getElementById("effectParams");
  if (box) box.value = "{}";
  const apply = document.getElementById("effectApply");
  if (apply) apply.click();
  await new Promise((r) => setTimeout(r, 2000));
  out.afterAdd = await effects();
  // ② 点列表里那一行 ⇒ 进入编辑态 ✓
  const row = document.querySelector("#effectsList div");
  if (row) row.click();
  await new Promise((r) => setTimeout(r, 400));
  out.applyLabel = apply ? apply.textContent : null;
  out.paramsBoxFilled = box ? box.value : null;
  // ③ 改参数 ⇒ 更新 ✓（用一个**独特的键** ✓ ⇒ 便于在服务端认出来 ✓）
  if (box) box.value = '{"harmonyProbe": 7}';
  if (apply) apply.click();
  await new Promise((r) => setTimeout(r, 2000));
  out.afterUpdate = await effects();
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-160);
  return out;
})()

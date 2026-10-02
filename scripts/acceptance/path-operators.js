// **路径算子真机验收（第三版）** ✓：
//  ① 图层与笔迹都走**界面/服务端** ✓，且对象落在**查看器当前图层** ✓（= 最后新建的那个 ✓）；
//  ② 界面里做 boolean union ⇒ 服务端对象数**增长** ✓；③ 元数不足时界面**自己拒绝** ✓。
(async () => {
  const out = {};
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const objects = async () => (await call("list_objects", {})).objects || [];
  // ① 界面建图层 ✓ ⇒ 取**最后**一个作为当前图层 ✓（查看器会把新建的选中 ✓）
  document.getElementById("addLayer").click();
  await new Promise((r) => setTimeout(r, 1200));
  const layers = (await call("list_layers", {})).layers || [];
  const current = layers[layers.length - 1].layer_id;
  out.currentLayer = current;
  // ② 用**笔迹** ✓（`convert_to_path` 只收笔迹 ✓）+ **重叠**两条 ✓
  await call("draw_stroke", { layer_id: current, object_id: "st1",
    data: { points: [[40, 40], [120, 40], [120, 120], [40, 120]], size: 10, color: [0.8, 0.2, 0.2, 1] } });
  await call("draw_stroke", { layer_id: current, object_id: "st2",
    data: { points: [[80, 80], [160, 80], [160, 160], [80, 160]], size: 10, color: [0.2, 0.3, 0.8, 1] } });
  for (const [from, to] of [["st1", "pa"], ["st2", "pb"]]) {
    await call("path_edit", { op: "convert_to_path", object_id: from, path_id: to });
  }
  await new Promise((r) => setTimeout(r, 900));
  out.before = (await objects()).map((o) => o.type);
  // ③ 刷新面板 ⇒ 勾选**两个** ⇒ boolean union ⇒ 执行 ✓
  document.getElementById("objectRefresh").click();
  await new Promise((r) => setTimeout(r, 1500));
  const boxes = document.querySelectorAll("#objectList input[type=checkbox]");
  out.boxes = boxes.length;
  for (const c of boxes) { c.checked = true; c.dispatchEvent(new Event("change", { bubbles: true })); }
  const countBefore = (await objects()).length;
  const op = document.getElementById("pathOp");
  op.value = "boolean";
  op.dispatchEvent(new Event("change", { bubbles: true }));
  document.getElementById("pathMode").value = "union";
  const log = document.getElementById("log");
  const before = (log ? log.textContent : "").length;
  document.getElementById("objectPath").click();
  await new Promise((r) => setTimeout(r, 3000));
  out.after = (await objects()).map((o) => o.type);
  out.countGrew = (await objects()).length > countBefore;
  out.newLogText = (log ? log.textContent : "").slice(before, before + 180);
  // ④ 元数守卫 ✓：只勾一个 + boolean ⇒ 界面应当拒绝 ✓
  for (const c of document.querySelectorAll("#objectList input[type=checkbox]")) {
    c.checked = false; c.dispatchEvent(new Event("change", { bubbles: true }));
  }
  const one = document.querySelector("#objectList input[type=checkbox]");
  if (one) { one.checked = true; one.dispatchEvent(new Event("change", { bubbles: true })); }
  const countMid = (await objects()).length;
  const before2 = (log ? log.textContent : "").length;
  document.getElementById("objectPath").click();
  await new Promise((r) => setTimeout(r, 1500));
  out.arityGuardHeld = (await objects()).length === countMid;
  out.guardText = (log ? log.textContent : "").slice(before2, before2 + 140);
  return out;
})()

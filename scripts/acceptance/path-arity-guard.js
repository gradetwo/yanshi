(async () => {
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  document.getElementById("addLayer").click();
  await new Promise((r) => setTimeout(r, 1200));
  const layers = (await call("list_layers", {})).layers || [];
  const current = layers[layers.length - 1].layer_id;
  await call("draw_stroke", { layer_id: current, object_id: "g1",
    data: { points: [[30, 30], [90, 30]], size: 8, color: [0.9, 0.9, 0.2, 1] } });
  await call("path_edit", { op: "convert_to_path", object_id: "g1", path_id: "gp" });
  document.getElementById("objectRefresh").click();
  await new Promise((r) => setTimeout(r, 1500));
  const one = document.querySelector("#objectList input[type=checkbox]");
  if (one) { one.checked = true; one.dispatchEvent(new Event("change", { bubbles: true })); }
  const op = document.getElementById("pathOp");
  op.value = "boolean";
  op.dispatchEvent(new Event("change", { bubbles: true }));
  document.getElementById("objectPath").click();
  await new Promise((r) => setTimeout(r, 1500));
  const log = document.getElementById("log");
  const text = log ? log.textContent : "";
  return {
    guardMessagePresent: text.indexOf("需要两个对象") >= 0,
    tailAll: text.slice(-200),
  };
})()

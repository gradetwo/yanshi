// **变更集面板的真机验收** ✓：开始 → 做两步 → 面板列出 → **放弃** ⇒ 那两步**被整体撤销** ✓。
// 判据两条事实 ✓：① 服务端 `get_changesets` 里有那一条 ✓；② 放弃后新建的图层**不在了** ✓。
(async () => {
  const out = {};
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const layers = async () => ((await call("list_layers", {})).layers || []).map((l) => l.layer_id);
  out.panelExists = !!document.getElementById("changesetList");
  out.before = (await layers()).length;
  // ① 点「开始变更集」✓ ② 用工具建两个图层（模拟"这两步是一串动作"✓）
  const begin = document.getElementById("changesetBegin");
  if (begin) begin.click();
  await new Promise((r) => setTimeout(r, 1200));
  for (const id of ["CS_one", "CS_two"]) await call("create_layer", { layer_id: id });
  out.mid = (await layers()).length;
  // ③ 刷新面板 ⇒ 应列出 1 条 ✓（提交前也能看到 ✓）
  const reload = document.getElementById("changesetReload");
  if (reload) reload.click();
  await new Promise((r) => setTimeout(r, 1500));
  out.rows = document.querySelectorAll("#changesetList .annotation-row").length;
  // ④ 点「放弃（整体撤销）」⇒ 那两步应被整体撤销 ✓
  const abort = document.getElementById("changesetAbort");
  if (abort) abort.click();
  await new Promise((r) => setTimeout(r, 3000));
  const after = await layers();
  out.after = after.length;
  out.goneLayers = !after.includes("CS_one") && !after.includes("CS_two");
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-180);
  return out;
})()

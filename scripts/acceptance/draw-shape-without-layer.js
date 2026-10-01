// 打开文档之后**把图层删掉** ✓ 再画矩形 ✓ ⇒ 修好之后必须**留下明确的日志** ✓（不再静默 ✗）。
(async () => {
  const params = new URLSearchParams(location.search);
  const doc = params.get("doc"), token = params.get("token");
  const call = function (name, payload) {
    return fetch("/api/tools/" + name + "?doc=" + doc + "&token=" + token, { method: "POST",
      headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) })
      .then(function (r) { return r.json(); }).catch(function () { return {}; });
  };
  const listed = await call("list_layers", {});
  const ids = (listed.layers || []).map(function (l) { return l.layer_id; });
  for (let i = 0; i < ids.length; i += 1) { await call("delete_layer", { layer_id: ids[i] }); }
  const after = await call("list_layers", {});
  const rect = document.querySelector('button[data-tool="rect"]');
  if (rect) rect.click();
  await new Promise(function (r) { setTimeout(r, 600); });
  const board = document.getElementById("board");
  const toClient = function (x, y) {
    const box = board.getBoundingClientRect();
    return { clientX: box.left + x / (board.width / box.width), clientY: box.top + y / (board.height / box.height) };
  };
  const opts = function (p) {
    return { bubbles: true, cancelable: true, pointerId: 61, pointerType: "mouse", isPrimary: true,
             buttons: 1, clientX: p.clientX, clientY: p.clientY };
  };
  board.dispatchEvent(new PointerEvent("pointerdown", opts(toClient(120, 120))));
  await new Promise(function (r) { setTimeout(r, 120); });
  board.dispatchEvent(new PointerEvent("pointermove", opts(toClient(360, 260))));
  await new Promise(function (r) { setTimeout(r, 120); });
  board.dispatchEvent(new PointerEvent("pointerup", opts(toClient(360, 260))));
  await new Promise(function (r) { setTimeout(r, 2500); });
  const logEl = document.getElementById("log");
  const text = logEl ? logEl.textContent : "";
  return { layersAfterDelete: (after.layers || []).length, logTail: text.slice(-260) };
})()

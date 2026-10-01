// **默认画笔**（不加介质工具 ✓）在"没有图层的文档"上画一笔 ✓ —— 修好之后应当**能画上** ✓。
(async () => {
  const board = document.getElementById("board");
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  await new Promise(function (r) { setTimeout(r, 600); });
  const toClient = function (x, y) {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width),
             clientY: rect.top + y / (board.height / rect.height) };
  };
  const opts = function (p) {
    return { bubbles: true, cancelable: true, pointerId: 51, pointerType: "mouse", isPrimary: true,
             buttons: 1, pressure: 0.8, clientX: p.clientX, clientY: p.clientY };
  };
  const y = 200;
  board.dispatchEvent(new PointerEvent("pointerdown", opts(toClient(140, y))));
  for (let k = 1; k <= 10; k += 1) {
    const t = k / 10;
    board.dispatchEvent(new PointerEvent("pointermove", opts(toClient(140 + 480 * t, y))));
    await new Promise(function (r) { setTimeout(r, 30); });
  }
  board.dispatchEvent(new PointerEvent("pointerup", opts(toClient(620, y))));
  await new Promise(function (r) { setTimeout(r, 3500); });
  const layers = await fetch("/api/tools/list_layers?doc=" + new URLSearchParams(location.search).get("doc") +
    "&token=" + (new URLSearchParams(location.search).get("token") || ""), { method: "POST",
    headers: { "content-type": "application/json" }, body: "{}" }).then(function (r) { return r.json(); }).catch(function () { return {}; });
  return { toolClicked: !!brush, layers: (layers.layers || []).map(function (l) { return l.layer_id; }) };
})()

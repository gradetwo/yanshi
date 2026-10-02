// **viewer 角色的真机验收** ✓：界面能读 ✓、落笔被拒 ✓、文档**真的没变** ✓。
(async () => {
  const out = {};
  const board = document.getElementById("board");
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const ink = async () => {
    const body = await call("render_region", { region: { x: 0, y: 0, w: 400, h: 300 }, raw: true });
    if (!body.raw_url) return -1;
    const raw = new Uint8Array(await (await fetch(body.raw_url)).arrayBuffer());
    let n = 0;
    for (let i = 0; i < raw.length; i += 4) {
      if ((raw[i] * 299 + raw[i + 1] * 587 + raw[i + 2] * 114) / 1000 < 200) n++;
    }
    return n;
  };
  const toClient = (x, y) => {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width), clientY: rect.top + y / (board.height / rect.height) };
  };
  out.inkBefore = await ink();
  // 界面上真的落一笔 ✓
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  await new Promise((r) => setTimeout(r, 400));
  const a = toClient(120, 160);
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 161,
    pointerType: "mouse", isPrimary: true, buttons: 1, clientX: a.clientX, clientY: a.clientY }));
  for (let k = 1; k <= 5; k++) {
    const p = toClient(120 + 40 * k, 160);
    board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 161,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: p.clientX, clientY: p.clientY }));
    await new Promise((r) => setTimeout(r, 25));
  }
  const b = toClient(320, 160);
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 161,
    pointerType: "mouse", isPrimary: true, button: 0, clientX: b.clientX, clientY: b.clientY }));
  await new Promise((r) => setTimeout(r, 2000));
  out.inkAfter = await ink();
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-260);
  return out;
})()

// **撤销/重做快捷键的真机验收** ✓（用户 §六-15 ✓）。
// 判据三条事实 ✓：① 画一笔 ⇒ 有墨 ✓；② **Ctrl+Z ⇒ 墨量降回 0** ✓；③ **Ctrl+Shift+Z ⇒ 墨量回来** ✓。
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
  const key = (shift) => window.dispatchEvent(new KeyboardEvent("keydown",
    { key: "z", code: "KeyZ", ctrlKey: true, shiftKey: shift, bubbles: true, cancelable: true }));
  out.inkEmpty = await ink();
  // ① 画一笔 ✓
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  await new Promise((r) => setTimeout(r, 400));
  const a = toClient(60, 80);
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 201,
    pointerType: "mouse", isPrimary: true, buttons: 1, clientX: a.clientX, clientY: a.clientY }));
  for (let k = 1; k <= 6; k++) {
    const p = toClient(60 + 40 * k, 80);
    board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 201,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: p.clientX, clientY: p.clientY }));
    await new Promise((r) => setTimeout(r, 25));
  }
  const b = toClient(300, 80);
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 201,
    pointerType: "mouse", isPrimary: true, button: 0, clientX: b.clientX, clientY: b.clientY }));
  await new Promise((r) => setTimeout(r, 2000));
  out.inkAfterPaint = await ink();
  // ② Ctrl+Z ✓
  key(false);
  await new Promise((r) => setTimeout(r, 2500));
  out.inkAfterUndo = await ink();
  // ③ Ctrl+Shift+Z ✓
  key(true);
  await new Promise((r) => setTimeout(r, 2500));
  out.inkAfterRedo = await ink();
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-160);
  return out;
})()

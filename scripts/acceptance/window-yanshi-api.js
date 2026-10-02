// **window.yanshi 的真机验收** ✓（用户 §六-5 ✓）。
// 判据三条事实 ✓：① 设置能被读到 ✓；② **像素真的变成设的颜色** ✓（不是只改了输入框 ✓）；③ 找不到的工具返回 false ✓。
(async () => {
  const out = {};
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const board = document.getElementById("board");
  const toClient = (x, y) => {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width), clientY: rect.top + y / (board.height / rect.height) };
  };
  out.apiPresent = typeof window.yanshi === "object";
  // ① 用 API 设颜色与粗细 ✓
  out.setColor = window.yanshi.setColor("#0080ff");
  out.railColor = (document.getElementById("color") || {}).value;
  out.setSize = window.yanshi.setSize(22);
  await new Promise((r) => setTimeout(r, 300));
  out.state = window.yanshi.state();
  // ② 用 API 选笔 ✓，然后画一笔 ✓
  out.setTool = window.yanshi.setTool("brush");
  await new Promise((r) => setTimeout(r, 400));
  const a = toClient(60, 120);
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 301,
    pointerType: "mouse", isPrimary: true, buttons: 1, clientX: a.clientX, clientY: a.clientY }));
  for (let k = 1; k <= 5; k++) {
    const p = toClient(60 + 40 * k, 120);
    board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 301,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: p.clientX, clientY: p.clientY }));
    await new Promise((r) => setTimeout(r, 25));
  }
  const b = toClient(260, 120);
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 301,
    pointerType: "mouse", isPrimary: true, button: 0, clientX: b.clientX, clientY: b.clientY }));
  await new Promise((r) => setTimeout(r, 2500));
  // ③ **读像素**：数蓝色像素 ✓（证明设置真的影响了笔触 ✓）
  const body = await call("render_region", { region: { x: 0, y: 0, w: 400, h: 300 }, raw: true });
  if (body.raw_url) {
    const raw = new Uint8Array(await (await fetch(body.raw_url)).arrayBuffer());
    let blue = 0;
    let total = 0;
    for (let i = 0; i < raw.length; i += 4) {
      const r = raw[i], g = raw[i + 1], bl = raw[i + 2];
      if ((r * 299 + g * 587 + bl * 114) / 1000 < 220) {
        total++;
        if (bl > 120 && bl > r + 40) blue++;
      }
    }
    out.inkedPixels = total;
    out.bluePixels = blue;
    const samples = [];
    for (let i = 0; i < raw.length && samples.length < 6; i += 4) {
      const r = raw[i], g = raw[i + 1], bl = raw[i + 2];
      if ((r * 299 + g * 587 + bl * 114) / 1000 < 220) samples.push([r, g, bl]);
    }
    out.samples = samples;
    out.mediumValue = (document.getElementById("medium") || {}).value;
  }
  out.unknownTool = window.yanshi.setTool("definitely-not-a-tool");
  return out;
})()

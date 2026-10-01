// **对象变换的真机验收** ✓：画一条**横**笔触 → 量墨迹包围盒 → 旋转 90° → 再量 ✓。
// 判据 ✓：包围盒由"宽而扁"变成"高而窄" ✓（且墨量近似不变 ✓）—— 可量化 ✓。
(async () => {
  const out = {};
  const board = document.getElementById("board");
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const inkBox = async () => {
    const body = await call("render_region", { region: { x: 0, y: 0, w: 820, h: 600 }, raw: true });
    const raw = new Uint8Array(await (await fetch(body.raw_url)).arrayBuffer());
    let minX = 1e9, maxX = -1, minY = 1e9, maxY = -1, n = 0;
    for (let y = 0; y < 600; y++) {
      for (let x = 0; x < 820; x++) {
        const i = (y * 820 + x) * 4;
        if ((raw[i] * 299 + raw[i + 1] * 587 + raw[i + 2] * 114) / 1000 < 220) {
          n++; minX = Math.min(minX, x); maxX = Math.max(maxX, x);
          minY = Math.min(minY, y); maxY = Math.max(maxY, y);
        }
      }
    }
    return { ink: n, w: n ? maxX - minX + 1 : 0, h: n ? maxY - minY + 1 : 0 };
  };
  const toClient = (x, y) => {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width), clientY: rect.top + y / (board.height / rect.height) };
  };
  out.controls = {
    button: !!document.getElementById("objectTransform"),
    rotate: !!document.getElementById("transformRotate"),
    scale: !!document.getElementById("transformScale"),
  };
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  await new Promise((r) => setTimeout(r, 400));
  // ① 一条**横**线 ✓
  const a = toClient(120, 200);
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 121,
    pointerType: "mouse", isPrimary: true, buttons: 1, clientX: a.clientX, clientY: a.clientY }));
  for (let k = 1; k <= 8; k++) {
    const p = toClient(120 + 45 * k, 200);
    board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 121,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: p.clientX, clientY: p.clientY }));
    await new Promise((r) => setTimeout(r, 20));
  }
  const b = toClient(480, 200);
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 121,
    pointerType: "mouse", isPrimary: true, button: 0, clientX: b.clientX, clientY: b.clientY }));
  await new Promise((r) => setTimeout(r, 1500));
  out.boxBefore = await inkBox();
  // ② 勾选那一笔 ✓（按行内类型文字选 ✓ —— 不再按位置猜 ✗）
  const refresh = document.getElementById("objectRefresh");
  if (refresh) refresh.click();
  await new Promise((r) => setTimeout(r, 1200));
  const rows = Array.from(document.querySelectorAll("#objectList .annotation-row"));
  const row = rows.find((r) => (r.textContent || "").indexOf("stroke") >= 0);
  const check = row ? row.querySelector("input[type=checkbox]") : null;
  if (check) { check.checked = true; check.dispatchEvent(new Event("change", { bubbles: true })); }
  // ③ 旋转 90° ⇒ 变换 ✓
  const rotate = document.getElementById("transformRotate");
  if (rotate) { rotate.value = "90"; rotate.dispatchEvent(new Event("change", { bubbles: true })); }
  const button = document.getElementById("objectTransform");
  if (button) button.click();
  await new Promise((r) => setTimeout(r, 2500));
  out.boxAfter = await inkBox();
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-180);
  return out;
})()

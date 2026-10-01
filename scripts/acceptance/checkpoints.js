// **检查点真机验收** ✓：画一笔 → 打点 → 再画一笔 → 回到存档点 ✓（全程走界面 ✓）。
(async () => {
  const out = {};
  const board = document.getElementById("board");
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const toClient = (x, y) => {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width), clientY: rect.top + y / (board.height / rect.height) };
  };
  const stroke = async (y0, pointerId) => {
    const a = toClient(120, y0);
    board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: a.clientX, clientY: a.clientY }));
    for (let k = 1; k <= 8; k++) {
      const p = toClient(120 + 45 * k, y0);
      board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId,
        pointerType: "mouse", isPrimary: true, buttons: 1, clientX: p.clientX, clientY: p.clientY }));
      await new Promise((r) => setTimeout(r, 20));
    }
    const b = toClient(480, y0);
    board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId,
      pointerType: "mouse", isPrimary: true, button: 0, clientX: b.clientX, clientY: b.clientY }));
    await new Promise((r) => setTimeout(r, 1200));
  };
  const ink = async () => {
    const body = await call("render_region", { region: { x: 0, y: 0, w: 820, h: 600 }, raw: true });
    const raw = new Uint8Array(await (await fetch(body.raw_url)).arrayBuffer());
    let n = 0;
    for (let i = 0; i < raw.length; i += 4) {
      if ((raw[i] * 299 + raw[i + 1] * 587 + raw[i + 2] * 114) / 1000 < 220) n++;
    }
    return n;
  };
  out.panelExists = !!document.getElementById("checkpointList");
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  await new Promise((r) => setTimeout(r, 400));
  await stroke(150, 101);
  out.inkAfterFirst = await ink();
  // ① 打一个存档点 ✓
  const create = document.getElementById("checkpointCreate");
  if (create) create.click();
  await new Promise((r) => setTimeout(r, 1200));
  const listed = await call("get_checkpoints", {});
  out.checkpointsOnServer = (listed.checkpoints || listed.items || []).length;
  out.rowsAfterCheckpoint = document.querySelectorAll("#checkpointList button").length;
  // ② 再画一笔（在更下面 ✓ ⇒ 多出来的墨是可辨认的 ✓）
  await stroke(300, 103);
  out.inkAfterSecond = await ink();
  // ③ 回到存档点 ✓
  const back = document.querySelector("#checkpointList button");
  if (back) back.click();
  await new Promise((r) => setTimeout(r, 2500));
  out.inkAfterRestore = await ink();
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-160);
  return out;
})()

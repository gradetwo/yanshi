// **油画插件介质的端到端验收** ✓（目标①："让用户能测油画的那一步" ✓）。
// 判据两条事实 ✓：① **像素真的被改了** ✓；② 对象里**记着插件 id + version** ✓（目标明写的契约 ✓）。
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
    const body = await call("render_region", { region: { x: 0, y: 0, w: 820, h: 600 }, raw: true });
    const raw = new Uint8Array(await (await fetch(body.raw_url)).arrayBuffer());
    let n = 0;
    for (let i = 0; i < raw.length; i += 4) {
      if ((raw[i] * 299 + raw[i + 1] * 587 + raw[i + 2] * 114) / 1000 < 200) n++;
    }
    return n;
  };
  // ① 界面上能不能选到油画 ✓
  const oilButton = document.querySelector('button[data-qp-medium="oil"]');
  out.oilButtonExists = !!oilButton;
  const select = document.getElementById("medium");
  out.mediumOptions = select ? Array.from(select.options).map((o) => o.value) : [];
  // ② 真的把它选上 ✓（走真实控件 ✓）
  if (select) {
    select.value = "oil";
    select.dispatchEvent(new Event("change", { bubbles: true }));
  } else if (oilButton) {
    oilButton.click();
  }
  // **必须切到「介质」工具** ✓ —— 交互落笔走的是 `state.tool === "medium_dab"` 那条路 ✓；
  // 用画笔工具画出来的是一根普通 `draw_stroke` ✗、对象里**不带 medium** ✗（第一次验收就是这么失败的 ✓）。
  const mediumTool = document.querySelector('button[data-tool="medium_dab"]');
  out.mediumToolExists = !!mediumTool;
  if (mediumTool) mediumTool.click();
  await new Promise((r) => setTimeout(r, 2500));
  out.mediumValue = select ? select.value : null;
  out.oilPressed = oilButton ? oilButton.getAttribute("aria-pressed") : null;
  // ③ 用油画介质画一笔 ✓
  out.inkBefore = await ink();
  const toClient = (x, y) => {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width), clientY: rect.top + y / (board.height / rect.height) };
  };
  const a = toClient(160, 240);
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 151,
    pointerType: "mouse", isPrimary: true, buttons: 1, clientX: a.clientX, clientY: a.clientY }));
  for (let k = 1; k <= 8; k++) {
    const p = toClient(160 + 40 * k, 240 + Math.sin(k / 2) * 14);
    board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 151,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: p.clientX, clientY: p.clientY }));
    await new Promise((r) => setTimeout(r, 25));
  }
  const b = toClient(480, 240);
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 151,
    pointerType: "mouse", isPrimary: true, button: 0, clientX: b.clientX, clientY: b.clientY }));
  await new Promise((r) => setTimeout(r, 2500));
  out.inkAfter = await ink();
  // ④ 事实二：对象里记着插件 id + version ✓
  const listed = await call("list_objects", {});
  const items = listed.objects || [];
  out.mediumsOnObjects = items.map((it) => it.medium).filter(Boolean);
  out.distinctMediums = Array.from(new Set(out.mediumsOnObjects.map((m) => m.id + "@" + m.version)));
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-200);
  return out;
})()

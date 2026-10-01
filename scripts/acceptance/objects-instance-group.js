// **实例/组 的真机验收** ✓：全程走界面 ✓（画笔落一笔 → 对象面板 → 实例化 → 编组 ✓）。
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
  out.panelExists = !!document.getElementById("objectList");
  // ① 用画笔落一笔 ✓（这样当前图层里就有对象了 ✓）
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  await new Promise((r) => setTimeout(r, 500));
  const at = toClient(150, 200);
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 99,
    pointerType: "mouse", isPrimary: true, buttons: 1, clientX: at.clientX, clientY: at.clientY }));
  for (let k = 1; k <= 8; k++) {
    const p = toClient(150 + 40 * k, 200);
    board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 99,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: p.clientX, clientY: p.clientY }));
    await new Promise((r) => setTimeout(r, 20));
  }
  const up = toClient(470, 200);
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 99,
    pointerType: "mouse", isPrimary: true, button: 0, clientX: up.clientX, clientY: up.clientY }));
  await new Promise((r) => setTimeout(r, 1500));
  // ② 刷新对象面板 ✓
  const refresh = document.getElementById("objectRefresh");
  if (refresh) refresh.click();
  await new Promise((r) => setTimeout(r, 1200));
  out.rowsAfterStroke = document.querySelectorAll("#objectList input[type=checkbox]").length;
  // ③ 勾上第一个 ✓ 再实例化 ✓
  const checks = document.querySelectorAll("#objectList input[type=checkbox]");
  if (checks[0]) { checks[0].checked = true; checks[0].dispatchEvent(new Event("change", { bubbles: true })); }
  const inst = document.getElementById("objectInstance");
  if (inst) inst.click();
  await new Promise((r) => setTimeout(r, 1500));
  out.rowsAfterInstance = document.querySelectorAll("#objectList input[type=checkbox]").length;
  // ④ 再勾两个 ✓ 编组 ✓
  const checks2 = document.querySelectorAll("#objectList input[type=checkbox]");
  for (const c of checks2) { c.checked = true; c.dispatchEvent(new Event("change", { bubbles: true })); }
  const group = document.getElementById("objectGroup");
  if (group) group.click();
  await new Promise((r) => setTimeout(r, 1500));
  // ⑤ 服务端交叉核对 ✓
  const listed = await call("list_objects", {});
  const items = listed.objects || listed.items || [];
  out.serverObjects = items.map((it) => it.type || "?");
  out.serverGroups = items.filter((it) => (it.type || "").indexOf("group") >= 0).length;
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-220);
  return out;
})()

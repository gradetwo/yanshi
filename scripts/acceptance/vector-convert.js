// **矢量互转的真机验收** ✓：画一笔 → 勾选 → 转为形状 → 转为路径 ✓（看服务端的对象**类型** ✓）。
(async () => {
  const out = {};
  const board = document.getElementById("board");
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const types = async () => {
    const listed = await call("list_objects", {});
    return (listed.objects || listed.items || []).map((it) => it.type || "?");
  };
  const toClient = (x, y) => {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width), clientY: rect.top + y / (board.height / rect.height) };
  };
  out.buttons = {
    shape: !!document.getElementById("objectToShape"),
    path: !!document.getElementById("objectToPath"),
  };
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  await new Promise((r) => setTimeout(r, 400));
  const a = toClient(140, 180);
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 111,
    pointerType: "mouse", isPrimary: true, buttons: 1, clientX: a.clientX, clientY: a.clientY }));
  for (let k = 1; k <= 8; k++) {
    const p = toClient(140 + 45 * k, 180);
    board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 111,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: p.clientX, clientY: p.clientY }));
    await new Promise((r) => setTimeout(r, 20));
  }
  const b = toClient(500, 180);
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 111,
    pointerType: "mouse", isPrimary: true, button: 0, clientX: b.clientX, clientY: b.clientY }));
  await new Promise((r) => setTimeout(r, 1500));
  out.typesAfterStroke = await types();
  const refresh = document.getElementById("objectRefresh");
  if (refresh) refresh.click();
  await new Promise((r) => setTimeout(r, 1200));
  const checks = document.querySelectorAll("#objectList input[type=checkbox]");
  if (checks[0]) { checks[0].checked = true; checks[0].dispatchEvent(new Event("change", { bubbles: true })); }
  const toShape = document.getElementById("objectToShape");
  if (toShape) toShape.click();
  await new Promise((r) => setTimeout(r, 2000));
  out.typesAfterShape = await types();
  // **再画一笔** ✓ —— `convert_to_path` **只从笔迹转换** ✓ ⇒ 拿刚才那个形状去转是无效的 ✓
  //（服务端原话："…不是笔迹（本片的 convert_to_path 只从笔迹转换；路径转路径无需转换）" ✓）。
  const c = toClient(140, 300);
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 113,
    pointerType: "mouse", isPrimary: true, buttons: 1, clientX: c.clientX, clientY: c.clientY }));
  for (let k = 1; k <= 8; k++) {
    const p = toClient(140 + 45 * k, 300);
    board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 113,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: p.clientX, clientY: p.clientY }));
    await new Promise((r) => setTimeout(r, 20));
  }
  const d = toClient(500, 300);
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 113,
    pointerType: "mouse", isPrimary: true, button: 0, clientX: d.clientX, clientY: d.clientY }));
  await new Promise((r) => setTimeout(r, 1500));
  if (refresh) refresh.click();
  await new Promise((r) => setTimeout(r, 1200));
  out.typesBeforePath = await types();
  const checks2 = document.querySelectorAll("#objectList input[type=checkbox]");
  // **按行内文字选"笔迹"那一行** ✓ —— 别再按位置猜 ✗：
  // 我上一版挑"最后一个复选框" ✗ ⇒ 挑到了 shape ⇒ 服务端只好又说一遍"不是笔迹" ✓。
  const rows = Array.from(document.querySelectorAll("#objectList .annotation-row"));
  const strokeRow = rows.find((row) => (row.textContent || "").indexOf("stroke") >= 0);
  const pick = strokeRow ? strokeRow.querySelector("input[type=checkbox]") : null;
  if (pick) { pick.checked = true; pick.dispatchEvent(new Event("change", { bubbles: true })); }
  const toPath = document.getElementById("objectToPath");
  if (toPath) toPath.click();
  await new Promise((r) => setTimeout(r, 2000));
  out.typesAfterPath = await types();
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-200);
  return out;
})()

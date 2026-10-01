// **标注的真机验收** ✓：全程走**界面**（选工具 ✓、点画布 ✓、改文字 ✓、解决 ✓）✓，
// 并用 `list_annotations`（服务端权威 ✓）交叉核对 ✓。
(async () => {
  const out = { steps: [] };
  const board = document.getElementById("board");
  const call = async (name, payload) => {
    const params = new URLSearchParams(location.search);
    const response = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return response.json();
  };
  const serverCount = async () => {
    const listed = await call("list_annotations", {});
    const items = listed.annotations || listed.items || [];
    return items.length;
  };
  out.panelExists = !!document.getElementById("annotationList");
  out.pinsLayerExists = !!document.getElementById("annotationPins");
  out.before = await serverCount();
  // ① 选「标注」工具 ✓
  const tool = document.querySelector('button[data-tool="annotate"]');
  out.toolExists = !!tool;
  if (tool) tool.click();
  await new Promise((r) => setTimeout(r, 400));
  // ② 在画布上点两个位置 ✓
  const toClient = (x, y) => {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width), clientY: rect.top + y / (board.height / rect.height) };
  };
  for (const [x, y] of [[180, 160], [420, 300]]) {
    const at = toClient(x, y);
    board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 91,
      pointerType: "mouse", isPrimary: true, buttons: 1, clientX: at.clientX, clientY: at.clientY }));
    board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 91,
      pointerType: "mouse", isPrimary: true, button: 0, clientX: at.clientX, clientY: at.clientY }));
    await new Promise((r) => setTimeout(r, 700));
  }
  await new Promise((r) => setTimeout(r, 800));
  out.afterTwoClicks = await serverCount();
  out.pinsAfterTwo = document.querySelectorAll("#annotationPins button").length;
  out.rowsAfterTwo = document.querySelectorAll("#annotationList input").length;
  // ③ 改第一条的文字 ✓（走面板里的 input ✓ ⇒ 触发 change ✓）
  const first = document.querySelector("#annotationList input");
  if (first) {
    first.value = "这里的袖子要压深一点";
    first.dispatchEvent(new Event("change", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 900));
  }
  const listed = await call("list_annotations", {});
  const items = listed.annotations || listed.items || [];
  out.textsOnServer = items.map((item) => (item.payload && item.payload.text) || item.text || "");
  // ④ 解决第一条 ✓
  const resolve = Array.from(document.querySelectorAll("#annotationList button")).find((b) => b.textContent === "解决");
  if (resolve) resolve.click();
  await new Promise((r) => setTimeout(r, 900));
  const listed2 = await call("list_annotations", {});
  const items2 = listed2.annotations || listed2.items || [];
  out.resolvedOnServer = items2.filter((item) => item.resolved || (item.payload && item.payload.resolved)).length;
  out.pinsAfterResolve = document.querySelectorAll("#annotationPins button").length;
  return out;
})()

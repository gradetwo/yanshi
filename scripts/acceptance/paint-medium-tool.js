// **用真正的介质工具**（`data-tool="medium_dab"` ✓）画一笔 —— 我此前一直用默认画笔 ✗。
(async () => {
  const board = document.getElementById("board");
  const medium = document.getElementById("medium");
  const button = document.querySelector('button[data-tool="medium_dab"]');
  if (button) button.click();
  await new Promise(function (r) { setTimeout(r, 400); });
  if (medium) { medium.value = "oil"; medium.dispatchEvent(new Event("change", { bubbles: true })); }
  await new Promise(function (r) { setTimeout(r, 1500); });
  const toClient = function (x, y) {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width),
             clientY: rect.top + y / (board.height / rect.height) };
  };
  const opts = function (p) {
    return { bubbles: true, cancelable: true, pointerId: 31, pointerType: "mouse", isPrimary: true,
             buttons: 1, pressure: 0.8, clientX: p.clientX, clientY: p.clientY };
  };
  const y = 180;
  board.dispatchEvent(new PointerEvent("pointerdown", opts(toClient(120, y))));
  for (let k = 1; k <= 10; k += 1) {
    const t = k / 10;
    board.dispatchEvent(new PointerEvent("pointermove", opts(toClient(120 + 520 * t, y))));
    await new Promise(function (r) { setTimeout(r, 40); });
  }
  board.dispatchEvent(new PointerEvent("pointerup", opts(toClient(640, y))));
  await new Promise(function (r) { setTimeout(r, 4000); });
  const stats = window.yanshiStats || {};
  return { toolSelected: !!button, medium: medium ? medium.value : null,
           mediumStats: stats.medium || null, log: (document.getElementById("log") || {}).textContent ? "有" : "无" };
})()

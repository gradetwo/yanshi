// **压感验收** ✓：用 `pointerType: "pen"` 画一条**压感递增**的横线 ✓
// ⇒ 若压感真的生效 ✓，画面上这条线应当**越往右越粗** ✓（可量化 ✓）。
(async () => {
  const board = document.getElementById("board");
  const tool = document.querySelector('button[data-tool="medium_dab"]');
  const medium = document.getElementById("medium");
  if (tool) tool.click();
  await new Promise(function (r) { setTimeout(r, 500); });
  if (medium) { medium.value = "oil"; medium.dispatchEvent(new Event("change", { bubbles: true })); }
  await new Promise(function (r) { setTimeout(r, 1800); });
  const toClient = function (x, y) {
    const rect = board.getBoundingClientRect();
    return { clientX: rect.left + x / (board.width / rect.width),
             clientY: rect.top + y / (board.height / rect.height) };
  };
  const opts = function (p, pressure) {
    return { bubbles: true, cancelable: true, pointerId: 71, pointerType: "pen", isPrimary: true,
             buttons: 1, pressure: pressure, clientX: p.clientX, clientY: p.clientY };
  };
  const y = 200;
  board.dispatchEvent(new PointerEvent("pointerdown", opts(toClient(100, y), 0.15)));
  for (let k = 1; k <= 24; k += 1) {
    const t = k / 24;
    board.dispatchEvent(new PointerEvent("pointermove", opts(toClient(100 + 620 * t, y), 0.15 + 0.85 * t)));
    await new Promise(function (r) { setTimeout(r, 30); });
  }
  board.dispatchEvent(new PointerEvent("pointerup", opts(toClient(720, y), 1.0)));
  await new Promise(function (r) { setTimeout(r, 4000); });
  const stats = window.yanshiStats || {};
  return { pressureDabs: stats.pressureUsed || 0, medium: stats.medium || null };
})()

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
    return { bubbles: true, cancelable: true, pointerId: 81, pointerType: "pen", isPrimary: true,
             buttons: 1, pressure: pressure, clientX: p.clientX, clientY: p.clientY };
  };
  const y = 300;
  board.dispatchEvent(new PointerEvent("pointerdown", opts(toClient(100, y), 0.6)));
  for (let k = 1; k <= 12; k += 1) {
    const t = k / 12;
    board.dispatchEvent(new PointerEvent("pointermove", opts(toClient(100 + 500 * t, y), 0.6)));
    await new Promise(function (r) { setTimeout(r, 120); });
  }
  board.dispatchEvent(new PointerEvent("pointerup", opts(toClient(600, y), 0.6)));
  await new Promise(function (r) { setTimeout(r, 9000); });
  const stats = window.yanshiStats || {};
  const logEl = document.getElementById("log");
  return { pressureDabs: stats.pressureUsed || 0, dabs: stats.mediumDabs || null,
           medium: stats.medium || null, logTail: (logEl ? logEl.textContent : "").slice(-420) };
})()

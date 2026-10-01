// 只**读**画布 ✓：整幅哈希 + "有墨"像素数 ✓。
// **必须先等文档画上去** ✗ —— 我第一版只靠驱动的固定 800ms 等待 ✓ ⇒ 有时读到**空白画布** ✓
// ⇒ 输出"墨量 0" ✗，我差点把它当成"笔画没落上去" ✗。**先修仪器再读数** ✓。
(async () => {
  const board = document.getElementById("board");
  const ctx = board.getContext("2d", { willReadFrequently: true });
  const measure = function () {
    const data = ctx.getImageData(0, 0, board.width, board.height).data;
    let hash = 2166136261;
    let ink = 0;
    for (let i = 0; i < data.length; i += 4) {
      const lum = (data[i] * 299 + data[i + 1] * 587 + data[i + 2] * 114) / 1000;
      if (lum < 200) ink += 1;
      hash ^= data[i] + data[i + 1] * 3 + data[i + 2] * 7;
      hash = (hash * 16777619) >>> 0;
    }
    return { hash: hash, inkPixels: ink };
  };
  const started = performance.now();
  let result = measure();
  // 轮到"有墨"为止 ✓（最多 20 秒 ✓）；本来就是空文档的话也无所谓 ✓（下面会把等待时间一起报出来 ✓）。
  while (result.inkPixels === 0 && performance.now() - started < 20000) {
    await new Promise(function (r) { setTimeout(r, 250); });
    result = measure();
  }
  result.waitedMs = Math.round(performance.now() - started);
  result.width = board.width;
  result.height = board.height;
  result.medium = (document.getElementById("medium") || {}).value;
  return result;
})()

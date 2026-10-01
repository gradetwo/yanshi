// 真机验收「导入 PSD」✓：文件由驱动塞进 `#importFile` ✓，这里只**观察结果** ✓。
(() => {
  const input = document.getElementById("importFile");
  const logEl = document.getElementById("log");
  return {
    accept: input ? input.getAttribute("accept") : null,
    logTail: (logEl ? logEl.textContent : "").slice(-240),
  };
})()

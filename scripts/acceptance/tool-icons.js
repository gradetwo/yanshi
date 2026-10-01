// **工具图标的真机验收** ✓（判据三条事实 ✓）：
// ① 每个工具按钮里都有**真实的图形** ✓（不是空 svg ✗）；② 标题**带快捷键** ✓；③ **按快捷键真的切得过去** ✓。
(async () => {
  const out = {};
  const buttons = Array.from(document.querySelectorAll("#tools button[data-tool]"));
  out.toolButtons = buttons.length;
  // ① 空按钮检测 ✓（没有子元素的 svg = 空 svg ✗）
  const empty = buttons.filter((b) => {
    const svg = b.querySelector("svg");
    return !svg || svg.children.length === 0;
  }).map((b) => b.dataset.tool);
  out.emptyButtons = empty;
  // ② 标题里带快捷键 ✓
  const withKey = buttons.filter((b) => /\([A-Z]\)/.test(b.getAttribute("title") || ""));
  out.withShortcutInTitle = withKey.length;
  // ③ 按标注的快捷键（N）⇒ 真的切过去 ✓
  const annotate = document.querySelector('#tools button[data-tool="annotate"]');
  out.annotateTitle = annotate ? annotate.getAttribute("title") : null;
  out.annotateHasShape = annotate ? annotate.querySelectorAll("svg *").length : 0;
  const before = document.querySelector('#tools button[aria-pressed="true"]');
  out.activeBefore = before ? before.dataset.tool : null;
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "n", code: "KeyN", bubbles: true }));
  await new Promise((r) => setTimeout(r, 600));
  const after = document.querySelector('#tools button[aria-pressed="true"]');
  out.activeAfter = after ? after.dataset.tool : null;
  return out;
})()

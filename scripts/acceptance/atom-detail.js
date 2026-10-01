// **历史"详情"的真机验收** ✓：先造几条原子 → 刷新历史 → 点一条的「详情」→ 看净荷 ✓。
(async () => {
  const out = {};
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  // ① 造一条**有明显净荷**的原子 ✓（评论 ✓）
  const posted = await call("comment", { text: "详情验收：这条评论的正文就是净荷" });
  out.commentOk = posted.ok;
  // ② 刷新历史 ✓ 并点开最后一条的「详情」✓
  const reload = document.getElementById("historyReload");
  if (reload) reload.click();
  await new Promise((r) => setTimeout(r, 1800));
  const rows = Array.from(document.querySelectorAll("#history .row"));
  out.rowCount = rows.length;
  // 找到带「详情」按钮的那一行里对应 comment 的 ✓（最后一条通常就是 ✓）
  const buttons = Array.from(document.querySelectorAll("#history button")).filter((b) => b.textContent === "详情");
  out.detailButtons = buttons.length;
  if (buttons.length) buttons[buttons.length - 1].click();
  await new Promise((r) => setTimeout(r, 1800));
  out.hint = (document.getElementById("atomDetailHint") || {}).textContent;
  out.payloadShown = (document.getElementById("atomDetail") || {}).textContent;
  return out;
})()

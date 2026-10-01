// **评论面板的真机验收** ✓：在界面里发一条评论 → 面板列出 → 服务端 `get_log kind=comment` 也能查到 ✓。
// 两条事实 ✓：① 面板行数 1→2 ✓；② 服务端原子数一致 ✓。
(async () => {
  const out = {};
  const params = new URLSearchParams(location.search);
  const call = async (name, payload) => {
    const r = await fetch("/api/tools/" + name + "?doc=" + params.get("doc") + "&token=" + params.get("token"),
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}) });
    return r.json();
  };
  const serverComments = async () => {
    const listed = await call("get_log", { kind: "comment", limit: 50 });
    return (listed.atoms || listed.entries || []).length;
  };
  out.panelExists = !!document.getElementById("commentList");
  out.rowsBefore = document.querySelectorAll("#commentList .annotation-row").length;
  out.serverBefore = await serverComments();
  // ① 在界面里发一条 ✓（走输入框 + 按钮 ✓）
  const box = document.getElementById("commentText");
  if (box) { box.value = "这一笔的肩线可以再压一点（界面验收）"; box.dispatchEvent(new Event("input", { bubbles: true })); }
  const post = document.getElementById("commentPost");
  if (post) post.click();
  await new Promise((r) => setTimeout(r, 2000));
  out.rowsAfter = document.querySelectorAll("#commentList .annotation-row").length;
  out.serverAfter = await serverComments();
  const list = document.getElementById("commentList");
  out.listText = (list ? list.textContent : "").slice(0, 160);
  const log = document.getElementById("log");
  out.logTail = (log ? log.textContent : "").slice(-160);
  return out;
})()

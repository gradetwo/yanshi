// **存储面板的真机验收** ✓：统计 ✓、不勾确认时的回收被挡住 ✓、勾上后回收 ✓。
(async () => {
  const out = {};
  const panel = document.getElementById("storageReport0");
  const log = document.getElementById("log");
  out.panelExists = !!panel;
  out.buttons = {
    report: !!document.getElementById("storageReport"),
    collect: !!document.getElementById("storageCollect"),
    demote: !!document.getElementById("storageDemote"),
    confirmBox: !!document.getElementById("storageConfirm"),
  };
  // ① 统计 ✓
  const report = document.getElementById("storageReport");
  if (report) report.click();
  await new Promise((r) => setTimeout(r, 2500));
  out.reportText = (panel ? panel.textContent : "").slice(0, 300);
  // ② **不勾确认**就点回收 ✓ ⇒ 必须被挡住 ✓
  const logBefore = log ? log.textContent.length : 0;
  const collect = document.getElementById("storageCollect");
  if (collect) collect.click();
  await new Promise((r) => setTimeout(r, 2000));
  const tail = (log ? log.textContent : "").slice(logBefore);
  out.refusedWithoutConfirm = tail.indexOf("我确认") >= 0;
  out.logAfterRefusal = tail.slice(0, 120);
  // ③ 勾上确认 ⇒ 回收 ✓（新工作区里没有过 TTL 的孤儿 ⇒ 应为 0 个 ✓）
  const box = document.getElementById("storageConfirm");
  if (box) { box.checked = true; box.dispatchEvent(new Event("change", { bubbles: true })); }
  if (collect) collect.click();
  await new Promise((r) => setTimeout(r, 3000));
  out.reportAfterCollect = (panel ? panel.textContent : "").slice(-200);
  out.logFinal = (log ? log.textContent : "").slice(-200);
  return out;
})()

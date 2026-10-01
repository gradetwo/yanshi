// 默认视图 + **功能性**验收 ✓：点下去看它真的切换 ✓（不是只看元素存在 ✓）。
(() => {
  const out = { steps: [] };
  const body = document.body;
  const click = function (id) { const el = document.getElementById(id); if (el) el.click(); return !!el; };
  out.has = {
    toggleRail: !!document.getElementById("toggleRail"),
    toggleDockers: !!document.getElementById("toggleDockers"),
    toggleZen: !!document.getElementById("toggleZen"),
    workspace: !!document.getElementById("workspace"),
  };
  // ① 隐藏左栏 ✓
  const before = body.className;
  click("toggleRail");
  out.steps.push({ step: "隐藏左栏", before: before, after: body.className,
                   ok: body.classList.contains("hide-rail") });
  click("toggleRail");
  out.steps.push({ step: "恢复左栏", after: body.className, ok: !body.classList.contains("hide-rail") });
  // ② 隐藏右侧各种窗口 ✓
  click("toggleDockers");
  out.steps.push({ step: "隐藏右栏窗口", after: body.className, ok: body.classList.contains("hide-dockers") });
  click("toggleDockers");
  out.steps.push({ step: "恢复右栏窗口", after: body.className, ok: !body.classList.contains("hide-dockers") });
  // ③ 工作区预设 ✓：切到 review，看折叠是否随预设变 ✓
  const select = document.getElementById("workspace");
  if (select) {
    const cards = Array.from(document.querySelectorAll("[data-docker], .docker, details"));
    out.presetOptions = Array.from(select.options).map(function (o) { return o.value; });
    const collapsedBefore = document.querySelectorAll(".collapsed").length;
    select.value = "review";
    select.dispatchEvent(new Event("change", { bubbles: true }));
    const collapsedAfter = document.querySelectorAll(".collapsed").length;
    out.steps.push({ step: "切到 review 预设", collapsedBefore: collapsedBefore,
                     collapsedAfter: collapsedAfter, ok: collapsedAfter !== collapsedBefore || collapsedAfter > 0 });
    select.value = "paint";
    select.dispatchEvent(new Event("change", { bubbles: true }));
    out.steps.push({ step: "切回 paint 预设", collapsed: document.querySelectorAll(".collapsed").length });
  }
  // ④ 全屏画布（zen）✓
  click("toggleZen");
  out.steps.push({ step: "进入全屏画布", after: body.className, ok: body.classList.contains("zen") });
  click("toggleZen");
  out.steps.push({ step: "退出全屏画布", after: body.className, ok: !body.classList.contains("zen") });
  // ⑤ 光标处右键快捷面板 ✓（对画布派发 contextmenu ✓）
  const board = document.getElementById("board") || document.querySelector("canvas");
  if (board) {
    const rect = board.getBoundingClientRect();
    const quick = document.getElementById("quickPanel");
    const hiddenBefore = quick ? getComputedStyle(quick).display : null;
    board.dispatchEvent(new MouseEvent("contextmenu", {
      bubbles: true, cancelable: true, clientX: rect.left + 60, clientY: rect.top + 60, button: 2,
    }));
    out.steps.push({ step: "光标处右键", hiddenBefore: hiddenBefore,
                     displayAfter: quick ? getComputedStyle(quick).display : null,
                     ok: quick ? getComputedStyle(quick).display !== "none" : false });
    out.quickChildren = quick ? Array.from(quick.querySelectorAll("[id]")).map(function (n) { return n.id; }).slice(0, 8) : [];
  }
  // ⑥ 中栏与右栏此刻的可见性 ✓（给截图做注脚 ✓）
  out.visible = {
    rail: document.getElementById("toggleRail") ? !body.classList.contains("hide-rail") : null,
    dockers: !body.classList.contains("hide-dockers"),
    zen: body.classList.contains("zen"),
    quickPanel: (function () { const q = document.getElementById("quickPanel"); return q ? getComputedStyle(q).display !== "none" : null; })(),
  };
  out.finalClass = body.className;
  return out;
})()

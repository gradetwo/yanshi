#!/usr/bin/env node
// **打开面板的文档列表**判据（产品负责人："文件里头打开那也很乱" ✗）。
//
// 这一条**必须**是真浏览器 ✓：被审判的是"渲染出来长什么样" ✓（行有没有名字 / 元信息 /
// 明确的「打开」动作 ✓，空列表与读失败长得一不一样 ✓）—— 静态读源码答不了这个问题 ✗。
//
// 判据（每条都能红 ✓）：
//   ① `#docList` 进入 `ready` 后：每一行都有**非空名字 + 非空元信息** ✓，
//      元信息里有**尺寸**（`W×H` ✓）与**创建时间**（`YYYY-MM-DD HH:MM` ✓）；
//   ② 每一行都有**明确的开打动作** ✓（`[data-action="open"]`，文字是「打开」✓），
//      并且**只有一行**标着「当前」且就是当前文档 ✓；
//   ③ 列表里**没有永远空着的白框** ✗：`GET /api/documents` 不回 `thumb_url` ✓
//      ⇒ 原来的 `<img>` 只能是白的 ✓（这条判据钉住"要么真有图、要么别放图" ✓）；
//   ④ 三种非就绪状态**互不相同且都能到** ✓：`loading` / `empty` / `error` ✓
//      （用页内替换 `fetch` 的方式驱动 ✓ —— 不是"读源码里有没有这几个字" ✗）；
//   ⑤ 点某一行的「打开」⇒ **真的切过去** ✓（`state.docId` 变成那一行 ✓、对话框收起 ✓）；
//   ⑥ 零控制台错误 ✓。
//
// 用法：node scripts/browser-open-list.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>" <server-base>
const url = process.argv[2];
const serverBase = process.argv[3];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-open-list";
if (!url) {
  console.error("用法: node scripts/browser-open-list.mjs <viewer-url> [server-base]");
  process.exit(2);
}
const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const viewerBase = url.split("?")[0];
const target =
  list.find((t) => t.type === "page" && t.url.startsWith(viewerBase)) ||
  list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
const consoleLines = [];
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.method === "Runtime.consoleAPICalled") {
    consoleLines.push((message.params.args || []).map((a) => a.value ?? "").join(" "));
  }
  if (message.method === "Runtime.exceptionThrown") {
    const details = message.params.exceptionDetails || {};
    consoleLines.push("exception: " + (details.exception?.description || details.text || ""));
  }
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
await new Promise((resolve) => ws.addEventListener("open", resolve));
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const current = id++;
    pending.set(current, resolve);
    ws.send(JSON.stringify({ id: current, method, params }));
  });
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result
    ?.result?.value;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const waitFor = async (expression, label, timeoutMs = 25000) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (await evaluate(expression)) return true;
    await sleep(200);
  }
  console.error(`  ⏱ 等待超时：${label}`);
  return false;
};
const capture = async (name) => {
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const data = shot.result?.data;
  if (!data) return null;
  const fs = await import("node:fs/promises");
  await fs.mkdir(shotsDir, { recursive: true });
  const path = `${shotsDir}/${name}.png`;
  await fs.writeFile(path, Buffer.from(data, "base64"));
  return path;
};
let bad = 0;
const check = async (label, ok, detail) => {
  console.log((ok ? "  ✓ " : "  ✗ ") + label + (detail ? "｜" + detail : ""));
  if (!ok) {
    bad += 1;
    await capture("open-list-failed-" + bad);
  }
};

// **先造第二份文档** ✓：只有一份的时候"点某一行的打开真的切过去"无从验证 ✓。
if (serverBase) {
  await fetch(`${serverBase}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "crit_open_list_other", width: 48, height: 32 }),
  }).catch(() => undefined);
}

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
await send("Page.reload", { ignoreCache: true });
consoleLines.length = 0;
if (!(await waitFor("typeof window.yanshiFileMenu === 'object'", "查看器就绪"))) {
  process.exit(3);
}

// ① 打开面板（走用户真的会走的那个按钮 ✓，不是直接调函数 ✗）。
await evaluate(`document.getElementById("openDoc").click()`);
const ready = await waitFor(
  `document.getElementById("docList").dataset.state === "ready"`,
  "文档列表进入 ready",
  20000,
);
check("打开面板后列表进入 ready", ready, "state=" + (await evaluate(`document.getElementById("docList").dataset.state`)));
if (!ready) {
  await capture("open-list-not-ready");
  process.exit(1);
}
await capture("open-list-ready");

// ①②③ 逐行看。
const rows = await evaluate(`(() => {
  const list = document.getElementById("docList");
  const rows = Array.from(list.querySelectorAll(".doc-row"));
  return {
    count: rows.length,
    images: list.querySelectorAll("img").length,
    rows: rows.map((row) => {
      const open = row.querySelector('[data-action="open"]');
      const name = row.querySelector(".doc-name");
      const meta = row.querySelector(".doc-meta");
      return {
        docId: row.dataset.docId || "",
        current: row.classList.contains("current"),
        name: name ? name.textContent.trim() : "",
        meta: meta ? meta.textContent.trim() : "",
        openText: open ? open.textContent.trim() : "",
        hasOpen: Boolean(open),
      };
    }),
    currentDoc: state.docId,
  };
})()`);
const everyRowNamed = rows.rows.every((row) => row.name.length > 0 && row.meta.length > 0);
const everyRowHasOpen = rows.rows.every((row) => row.hasOpen && row.openText.includes("打开"));
const everyRowDated = rows.rows.every(
  (row) => /\d+×\d+/.test(row.meta) && /\d{4}-\d{2}-\d{2} \d{2}:\d{2}/.test(row.meta),
);
check("列表里有 ≥2 行（判据需要一份别的作品）", rows.count >= 2, "行数 " + rows.count);
check("每一行都有**名字 + 元信息**", everyRowNamed, JSON.stringify(rows.rows[0] || {}));
check(
  "元信息里有**尺寸**与**创建时间**",
  everyRowDated,
  rows.rows.map((row) => row.meta).join(" ｜ ").slice(0, 160),
);
check("每一行都有明确的「打开」动作", everyRowHasOpen, rows.rows.map((row) => row.openText).join("，"));
check(
  "列表里没有**永远空着的白框**（`/api/documents` 不回 thumb_url ⇒ 不放 img）",
  rows.images === 0,
  "img 数 " + rows.images,
);
const currentRows = rows.rows.filter((row) => row.current);
check(
  "**只有一行**标着「当前」且就是当前文档",
  currentRows.length === 1 && currentRows[0].docId === rows.currentDoc,
  JSON.stringify(currentRows),
);

// ④ 三种非就绪状态：用**页内替换 fetch** 驱动 ✓（不是"源码里有没有这几个字" ✗）。
const stateOf = async (mode) => {
  const script = `(async () => {
    if (!window.__openListOriginalFetch) window.__openListOriginalFetch = window.fetch;
    const original = window.__openListOriginalFetch;
    if (${JSON.stringify(mode)} === "loading") {
      window.fetch = () => new Promise(() => {});
    } else if (${JSON.stringify(mode)} === "empty") {
      window.fetch = (input, init) => {
        const target = typeof input === "string" ? input : (input && input.url) || "";
        const method = ((init && init.method) || "GET").toUpperCase();
        if (target.indexOf("/api/documents") === 0 && method === "GET") {
          return Promise.resolve(new Response(JSON.stringify({ ok: true, documents: [] }), {
            status: 200, headers: { "content-type": "application/json" } }));
        }
        return original(input, init);
      };
    } else {
      window.fetch = (input, init) => {
        const target = typeof input === "string" ? input : (input && input.url) || "";
        const method = ((init && init.method) || "GET").toUpperCase();
        if (target.indexOf("/api/documents") === 0 && method === "GET") {
          return Promise.reject(new Error("judged failure"));
        }
        return original(input, init);
      };
    }
    if (${JSON.stringify(mode)} === "loading") {
      void refreshDocumentList();
    } else {
      await refreshDocumentList();
    }
    window.fetch = original;
    const list = document.getElementById("docList");
    return { state: list.dataset.state, message: list.dataset.message || "" };
  })()`;
  return await evaluate(script);
};
const loading = await stateOf("loading");
check("载入态可达（data-state=loading）", loading && loading.state === "loading", JSON.stringify(loading));
const empty = await stateOf("empty");
check(
  "空列表 ⇒ `empty` 且**给出下一步**（不是一片空白）",
  empty && empty.state === "empty" && empty.message.length > 0,
  JSON.stringify(empty),
);
const failure = await stateOf("error");
check(
  "读失败 ⇒ `error` 且**与空不同**（不再是同一句话）",
  failure && failure.state === "error" && failure.message.length > 0 && failure.message !== empty.message,
  JSON.stringify(failure),
);
await capture("open-list-states");

// **状态测试会把列表清空**（最后一个状态是 `error` ✓）⇒ 点「打开」之前先**恢复真列表** ✓
// —— 这一条我自己踩过 ✗：第一版直接去点那一行 ⇒ 行早就不在了 ⇒ 判据报"没切过去" ✗
//（那看起来像产品坏了 ✓，其实是我把 DOM 换掉了 ✓）。
await evaluate("refreshDocumentList()");
await waitFor(
  `document.getElementById("docList").dataset.state === "ready"`,
  "列表恢复 ready",
  20000,
);

// ⑤ 点别的作品的「打开」⇒ 真的切过去。
const targetRow = rows.rows.find((row) => row.docId !== rows.currentDoc);
if (!targetRow) {
  check("列表里有一份**别的**作品可供切换", false, JSON.stringify(rows.rows.map((row) => row.docId)));
} else {
  await evaluate(`(() => {
    const row = document.querySelector('.doc-row[data-doc-id=' + JSON.stringify(${JSON.stringify(targetRow.docId)}) + ']');
    const button = row && row.querySelector('[data-action="open"]');
    if (button) button.click();
  })()`);
  const switched = await waitFor(
    `state.docId === ${JSON.stringify(targetRow.docId)}`,
    "切到 " + targetRow.docId,
    30000,
  );
  const dialogOpen = await evaluate(`document.getElementById("openDialog").open === true`);
  check("点某一行的「打开」⇒ 真的切到那份作品", switched, "现在 docId=" + (await evaluate("state.docId")));
  check("切换后对话框已收起", dialogOpen === false, "open=" + dialogOpen);
  await capture("open-list-switched");
}

// ⑥ 零控制台错误。
const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);
check("零控制台错误", errors.length === 0, errors.slice(0, 3).join(" ｜ ").slice(0, 200));

ws.close();
console.log(
  bad
    ? `  结论：${bad} 条不成立 ✗（打开面板仍不清楚）`
    : "  结论：打开面板是「一行一份 + 明确开打 + 三种状态互不相同」 ✓",
);
process.exit(bad ? 1 : 0);

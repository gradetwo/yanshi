#!/usr/bin/env node
// **只读工具对"不存在的 doc_id"必须报错，而且不许把文档建出来** ✓（第 635 轮；外部 `report.md` D10）
//
// **⇒ 症状（他们的实测）**：`get_document({"doc_id": "test2"})` 里 `test2` 根本不在 `list_documents`，
//   却回了一份"1024×1024、atoms=1、head_seq=1"的**空白文档** ✗ ⇒ 审计时读成"这幅画是空的" ✗，
//   而且**工作区被改了**（`list_documents` 从此多了这个 id）✗。
//
// **⇒ 判据两条**：
//   ① `get_document` 对不存在的 id ⇒ `ok=false` 且**对无权限的调用方报 `permission_denied`** ✓
//     （05 复审 NEW-C1 ✓：`d6525652` 起授权先于开文档 ⇒ **存在性不外泄** ✗
//      判据随之升级为**恒等式**：「不存在的文档＋外来令牌」与「存在的文档＋坏令牌」**必须同码** ✓
//      —— 不同码就是存在性预言机 ✓）；
//   ② 调用**前后**的文档列表里都**没有**这个 id（**无副作用** ✓）。
//
// **变异**：把 `server.rs` 里那段"只读工具不建文档"的判断去掉 ⇒ ① 变成 `ok=true` ⇒ 红 ✓；
//   恒等式的"能红"有**现成实证** ✓：`d6525652` 之前 ①a 报 `reference_not_found` 而 ①b 报
//   `permission_denied` ⇒ **两个码在真实历史里就是分叉过的**（CI run 40640839341 正是这么红的 ✓），
//   所以这条断言不是恒真 ✓。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-unknown-doc-is-not-created.mjs <base> <doc> <token>");
  process.exit(2);
}
const fake = `no_such_doc_${Date.now().toString(36)}`;
const call = async (name, docId, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const ids = async () => {
  const listing = await (await fetch(`${base}/api/documents?token=${encodeURIComponent(token)}`)).json();
  return (listing.documents ?? listing ?? []).map((entry) =>
    typeof entry === "string" ? entry : entry.doc_id,
  );
};

const before = await ids();
if (before.includes(fake)) {
  console.error(`  ⊘ 不可判：伪造 id ${fake} 居然已经存在`);
  process.exit(3);
}

const answer = await call("get_document", fake, {});
console.log(`  get_document(${fake}) ⇒ ok=${answer.ok}｜error_code=${answer.error_code ?? "-"}`);
if (answer.ok !== false) {
  console.error("  ✗ 对不存在的文档居然返回了成功（外部 D10：会编出一份空白文档，误导审计）");
  process.exit(1);
}
if (answer.error_code !== "permission_denied") {
  console.error(`  ✗ 对无权限调用方的错误码不是 permission_denied（实测 ${answer.error_code}）⇒ NEW-C1 的"存在性不外泄"被破坏`);
  process.exit(1);
}
// **恒等式（05 复审 NEW-C1 ✓）**：「**存在的文档 ＋ 坏令牌**」必须与上面**同码** ✓
//   ⇒ 若这里报出**不同**的码（如 reference_not_found ⇄ permission_denied 不对称 ✓），
//     调用方就能拿一个请求**枚举哪些 id 存在** ✗（存在性预言机 ✗）。
const bogus = "f".repeat(64);
const probe = await (await fetch(
  `${base}/api/tools/get_document?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(bogus)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: "{}" },
)).json();
console.log(`  get_document(${doc}, 坏令牌) ⇒ ok=${probe.ok}｜error_code=${probe.error_code ?? "-"}`);
if (probe.ok !== false) {
  console.error("  ⊘ 不可判：坏令牌居然读到了已存在的文档（这是更严重的问题，但本条先不冒充产品结论）");
  process.exit(3);
}
if (probe.error_code !== answer.error_code) {
  console.error(`  ✗ 存在性预言机复活：不存在=${answer.error_code}｜存在但无权限=${probe.error_code} ⇒ 两码不对称 ✓✗`);
  process.exit(1);
}

const after = await ids();
if (after.includes(fake)) {
  console.error(`  ✗ 调用之后 ${fake} 出现在文档列表里 ⇒ 只读工具**建了文档**（工作区被改）`);
  process.exit(1);
}
console.log(`✓ 不存在的 doc_id：报 ${answer.error_code}，且文档列表 ${before.length} → ${after.length}（没建）`);

#!/usr/bin/env node
// **`smart_select` 的 `mode="add"` 必须**真的合并**（第 1292 轮；外部 `E1041` 的代价换来的判据）
//
// **⇒ 为什么有这条判据** ✗：
//   外部（apple2011）曾用「七次 `smart_select` 叠选区」（`mode="add"`）⇒
//   **每次都以为在累加** ⇒ **而实际只保留了最后一次** ⇒ 他因此放弃了多边形选区路线。
//   根因（第三轮复审 N3D-04）：`fold.rs:683` 只做「允许覆盖」，而真正的布尔合并
//   （`merge_selection_shapes`）**从未被调用** ⇒ 于是 `add` 静默覆盖。
//
// **⇒ 判据（外部建议的形状）**：两次 `add`，两个**不相交**的矩形
//   ⇒ 最终选区的 `canvas_coverage_ratio` 应 ≈ **两矩形面积之和**
//   ⇒ 若 ≈ 其中一个 ⇒ **覆盖 bug 复发** ⇒ 红。
//
// **⇒ 为什么用 `canvas_coverage_ratio` 而不是像素数**：它是**比值** ⇒ 跨机器稳定
//   （判据优先断结构量/相对量，不断绝对像素数）。
import { existsSync } from "node:fs";

const [base, passedDoc, passedToken] = process.argv.slice(2);
let doc = passedDoc;
let token = passedToken;
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-smart-select-add-merges.mjs <base> <doc> <token>");
  process.exit(3);
}

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
    signal: AbortSignal.timeout(60_000),
  });
  return await reply.json();
};

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

// **★ 必须**自建文档**（第 1384 轮修）★**
//   **⇒ 原错** ✗：它读「传进来的那个 doc」的尺寸 ⇒ 而运行器传的是 **64×64 的 `_bootstrap`**
//     ⇒ 于是 `W=H=64` ⇒ 两块（`x 10..50` 与 `x W-50..W-10` = `14..54`）**重叠** ✗
//     ⇒ 第二次 add 选到的是**同一团** ⇒ `area` 恒 1600 ⇒ **比值 1.00 ⇒ 恒红** ✗
//   **⇒ ⇒ 而 CI 上碰巧过**（CI 的运行器给的是更大的文档）⇒ **典型「本机红 / CI 绿」** ✓
//   **⇒ 所以**判据自己建一个 **200×150** 的文档 ⇒ 与运行器无关 ✓
{
  const stamp = Date.now();
  const mine = `add_merges_${stamp}`;
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: mine, width: 200, height: 150 }),
  })).json();
  if (!made.token) {
    console.error("  ⊘ 不可判：建不出自己的文档（不是产品问题）");
    process.exit(3);
  }
  doc = mine;
  token = made.token;
}
const W = 200, H = 150;
const layer = await call("create_layer", { layer_id: "L_add", name: "ADD", if_exists: "reuse" });
if (layer.ok !== true) {
  console.error("  ⊘ 不可判：建层失败");
  process.exit(3);
}
// 左上红块 40×40，右下蓝块 40×40（互不相交、颜色不同 ⇒ 魔棒各选一块）
const red = await call("fill_region", {
  layer_id: "L_add", shape: { type: "rect", x: 10, y: 10, w: 40, h: 40 },
  color: "#c81818", opacity: 1.0,
});
check(red.ok === true, `① 铺红块 ⇒ ok=${red.ok}`);
const blue = await call("fill_region", {
  layer_id: "L_add", shape: { type: "rect", x: W - 50, y: H - 50, w: 40, h: 40 },
  color: "#1818c8", opacity: 1.0,
});
check(blue.ok === true, `② 铺蓝块 ⇒ ok=${blue.ok}`);

// 第一次 add：选红块
const first = await call("smart_select", {
  selection_id: "S_add", mode: "add", seed: [30, 30],
  tolerance: 0.15, feather: 0,
});
check(first.ok === true, `③ 第一次 smart_select(mode=add, 红) ⇒ ok=${first.ok}｜${JSON.stringify(first.context?.detail ?? "").slice(0, 80)}`);
// **★ 必须读 `list_selections` 的 `area`，不能读 `canvas_coverage_ratio`** ✓（第 1301 轮实测）
//   **为什么**：`canvas_coverage_ratio` 反映的是**本次** mask 的覆盖比
//     ⇒ 而**合并后的选区**在**已存储的状态**里 ⇒ 所以那个比值第二次仍等于第一次
//     ⇒ 于是判据**永远红**（而产品其实是对的：`area 1600 → 3200`）✗
//   ⇒ **测量比被测对象更容易错**（AGENTS.md 那条）
const areaOf = async () => {
  const r = await call("list_selections", {});
  const hit = (r.selections ?? []).find((s) => s.selection_id === "S_add");
  return Number(hit?.area ?? 0);
};
const areaOfName = async (name) => {
  const r = await call("list_selections", {});
  const hit = (r.selections ?? []).find((s) => s.selection_id === name);
  return Number(hit?.area ?? 0);
};
const r1 = await areaOf();

// 第二次 add：选蓝块（**不相交** ⇒ 正确时应累加）
const second = await call("smart_select", {
  selection_id: "S_add", mode: "add", seed: [W - 30, H - 30],
  tolerance: 0.15, feather: 0,
});
check(second.ok === true, `④ 第二次 smart_select(mode=add, 蓝) ⇒ ok=${second.ok}`);
const r2 = await areaOf();

// **核心断言**：第二次之后应 ≈ 两次之和（≈ 2×r1，因为两块面积相同）
//   若 bug 复发（覆盖）⇒ r2 ≈ r1 ⇒ 比值 ≈ 1.0 ⇒ 红
const ratio = r1 > 0 ? r2 / r1 : 0;
check(
  ratio > 1.5,
  `⑤ mode=add 必须**累加**（第二次/第一次 = ${ratio.toFixed(2)}，应 > 1.5；` +
  `若 ≈ 1.0 ⇒ 覆盖 bug 复发）｜area1=${r1} area2=${r2}`,
);

// **★ 失败路径：合并为空 / 合并报错 ⇒ 必须说出来，不许静默覆盖** ✓（第 1414 轮）
//   **∴ 原缺陷**：`write_smart_select` 写的是 `if let Ok(Some(…))` ⇒ 合并报错（例如
//     N3D-01 的「区域过大」）或被判为空结果时**分支被静默跳过** ⇒ 落到下面
//     「带 mode 直接覆盖」⇒ 调用方看到 `ok:true` 而 `mode` 语义没生效 ✗
//   **∴ 判据**：① 交集为空 ⇒ `invalid_argument` ＋ 旧选区面积不变
//             ② 并集超 64 MP ⇒ 明确报「区域过大」＋ 旧选区面积不变

// —— ⑥ 交集为空：S_int 只选红块，再与**不相交**的绿块求交 ⇒ 结果为空
const green = await call("fill_region", {
  layer_id: "L_add", shape: { type: "rect", x: 100, y: 10, w: 20, h: 20 },
  color: "#18c818", opacity: 1.0,
});
check(green.ok === true, `⑥a 铺绿块（与红蓝都不相交）⇒ ok=${green.ok}`);
const intFirst = await call("smart_select", {
  selection_id: "S_int", mode: "new", seed: [30, 30], tolerance: 0.15, feather: 0,
});
check(intFirst.ok === true, `⑥b 建 S_int（红块）⇒ ok=${intFirst.ok}`);
const intArea1 = await areaOfName("S_int");
const emptyMerge = await call("smart_select", {
  selection_id: "S_int", mode: "intersect", seed: [110, 20], tolerance: 0.15, feather: 0,
});
check(
  emptyMerge.ok === false && emptyMerge.error_code === "invalid_argument"
    && String(emptyMerge.context?.detail ?? "").includes("空选区"),
  `⑥c mode=intersect 交集为空必须明确报 invalid_argument（实测 ok=${emptyMerge.ok}｜` +
  `error_code=${emptyMerge.error_code}｜detail=${String(emptyMerge.context?.detail ?? "").slice(0, 80)}）`,
);
const intArea2 = await areaOfName("S_int");
check(
  intArea1 > 0 && intArea2 === intArea1,
  `⑥d 空交集不许覆盖旧选区（面积 ${intArea1} → ${intArea2}；变了＝静默覆盖复发）`,
);

// —— ⑦ 并集超上限：S_big 是画布外的巨大矩形，再 add 一小块 ⇒ 并集 ≈ 200 MP > 64 MP
const big = await call("create_selection", {
  selection_id: "S_big", mode: "new",
  shape: { kind: "rect", x: 100000, y: 0, w: 100000, h: 1000 },
});
check(big.ok === true, `⑦a 建超大选区 S_big ⇒ ok=${big.ok}｜${JSON.stringify(big.context?.detail ?? "").slice(0, 80)}`);
const bigArea1 = await areaOfName("S_big");
const bigAdd = await call("smart_select", {
  selection_id: "S_big", mode: "add", seed: [30, 30], tolerance: 0.15, feather: 0,
});
check(
  bigAdd.ok === false && bigAdd.error_code === "invalid_argument"
    && String(bigAdd.context?.detail ?? "").includes("区域过大"),
  `⑦b 并集超 64 MP 必须明确报「区域过大」（实测 ok=${bigAdd.ok}｜` +
  `error_code=${bigAdd.error_code}｜detail=${String(bigAdd.context?.detail ?? "").slice(0, 80)}）`,
);
const bigArea2 = await areaOfName("S_big");
check(
  bigArea1 > 0 && bigArea2 === bigArea1,
  `⑦c 合并报错不许覆盖旧选区（面积 ${bigArea1} → ${bigArea2}；变了＝静默覆盖复发）`,
);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（smart_select 的 mode=add 必须合并）`);
  process.exit(1);
}
console.log("\n  ✓ smart_select 的 mode=add 真的累加（不是覆盖）");

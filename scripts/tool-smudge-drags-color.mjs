#!/usr/bin/env node
// **`brush_params.smudge` 必须真的**拖带下面的颜色****（第 1585 轮）
//
// **⇒ 为什么写这条**：外部要"融合"（笔带起下面的颜色），而我上一条建议（`dabs_per_basic_radius`）
//   **测不出效果**（第 1584 轮自我纠正）⇒ 而 **`smudge` 这条我能测** ✓
// **⇒ ⇒ 实测**（红底 40px + 蓝笔 14px 横划）：
//   `smudge` 缺省 ⇒ 蓝笔内 **R=40.0**；`smudge: 0.0` ⇒ **R=40.0**（**与缺省相同**）；
//   `smudge: 1.0` ⇒ **R=89.3**（**+49 ⇒ 拖带了红**）✓
//
// **⇒ 断三条**：
//   ① `smudge: 0.0` 与缺省**同色**（**缺省不偷偷开 smudge**）✓
//   ② `smudge: 1.0` 让蓝笔内的 R **显著高于** `0.0` 的（**拖带发生**）✓
//   ③（第三轮，甲语义）`smudge: 1.0` 的携带**沿路径持续**：中间点 R 远高于
//     `0.0` 的蓝基线、且相对拖曳区衰减有限 ⇒ 红主导沿路径搬运，不是局部 dab 效应 ✓
//
// **⇒ 变异**：把 `smudge` 写死成 0 ⇒ ② 红 ✓
//
// **Y26-006 修订第二轮（own-layer 语义，2026-10-30；第一轮几何错了，见下）**：
//   第一轮把蓝笔与红笔同中心线、中间点取旧三点（cx、cx±0.4*armX），以为“中间点在拖曳区外，
//   底图是蓝笔自身 ⇒ 仍是蓝”——这是错的：底图是**落笔前**的本层内容（红线），不是蓝笔自身；
//   smudge 1.0 的每一 dab 都采红线 ⇒ 整条蓝笔都画成红（CI 实证：中间 B=37.3、R=186.3，
//   就是红线色 [200,40,40]，不是蓝）。所以红线上的任何点都不可能通过 ③。
//   ⇒ 第二轮：红线只盖拖曳区（[x0, redEnd]，redEnd = x0 + 0.3*armX），红笔改细（2*thin，
//     带宽盖住蓝带即可）；③ 点移到蓝笔压空白底的一段（redEnd + 2/2.5/3*thin），
//     那里 smudge 无红可拖 ⇒ 仍是蓝；合成里另两轮（缺省/0.0）的蓝也在同处 ⇒ ③ 更稳。
//     关键是半径：dab 盘会把红漆带到路径端点之外（红漆止于 redEnd + thin），
//     ③ 点还须躲开 dab 半径 + 回望 back（共 0.75*thin），故首点取 redEnd + 2*thin，
//     余量恒为 0.25*thin，与长宽比无关；末点须在蓝笔终点内，见下余量守卫。
//   ⇒ ①② 的拖曳区与断言形式全不动。
//
// **Y26-006 修订第三轮（甲，2026-10-30；第二轮几何路已死）**：
//   第二轮把中间点移出红漆 60–90px（320×240 口径：红漆尾 156.4，中间点
//   186.4/201.4/216.4），B−R 只从 −149 改善到 −134.4（CI 实测中间
//   B=43.3、R=177.7，上一轮 37.3/186.3）；剩余蓝笔仅约 33px，而到达标线
//   （B−R > 20）还需 +154 ⇒ 笔内几何移点不可能补上，禁止再移点。
//   根因是前提错，不是几何错：判据的“蓝笔”本身就是 smudge=1.0 的涂抹笔，
//   起笔点 x0 落在红线上；smudge_dab 每 dab 从可变表面采样，smudge=1.0 时
//   每 dab 精确等于采样值（纯搬运），红主导不动点沿路径形成。中间点的底图
//   不是空白底，而是本笔已搬运过的红主导表面。
//   ⇒ ③ 改为甲语义断言：全强度涂抹本来就是沿路径搬运颜色，从红线上起笔、
//     拖 150px+ 后路径变红是正确语义，不是 bug。③ 不再要求“中间点保持蓝色”，
//     改断两条关系（均用三点平均，相对量，跨机器）：
//     (a) 中间 R 比 `smudge: 0.0` 的同点蓝基线高 50 以上 ⇒ 携带确实到达中间点，
//         而非只在起笔附近；(b) 中间 R 不低于拖曳区 R 的 70% ⇒ 沿路径衰减有限
//         （CI 实测 177.7/200.0，衰减仅 11%，留 30% 容差）；另断中间 R > B，
//         即中间点确为红主导——若改成落笔前快照语义（乙），中间点会变回蓝，
//         本条即红，③ 由此区分甲/乙语义。
//   ⇒ 变异：把 `smudge: 1.0` 写死成 0.0 ⇒ 中间 R 回落到蓝基线附近，(a) 与
//     R > B 两条红（见盒上变异证据）；把中间点改回“要求 B > R + 20” ⇒ 在真
//     1.0 行为下恒红（第二轮 CI 已证），故新断言不是安慰剂。
//
// 用法：node scripts/tool-smudge-drags-color.mjs <base> <doc> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-smudge-drags-color.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args),
  });
  return reply.json();
};
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// **坐标按画布缩放**（第 1578 轮的同一个坑）
const canvas = await call("describe_canvas", {});
const W = canvas?.width ?? 0;
const H = canvas?.height ?? 0;
if (!(W > 0) || !(H > 0)) {
  console.error(`  ⊘ 不可判：拿不到画布尺寸`);
  process.exit(3);
}
const cx = W / 2;
const cy = H / 2;
const thin = Math.max(6, Math.round(H / 8));
const armX = Math.round(W * 0.35);

// **拖曳区采样点**（Y26-006 修订 ✓）：蓝笔起笔点 `x0 = cx - armX * 0.6`；
//   涂抹向后拖 `back = smudge_length(0.5) × 半径(thin/2) = thin/4`（`100%_Opaque` 的
//   `smudge_length` 无输入曲线 ⇒ 求值恒 0.5；`size` 是直径 ⇒ 半径 = thin/2；
//   两式相乘得 back，与画布尺寸无关 ✓）。拖曳区 = [x0, x0 + back) ⇒
//   三个拖曳点取 x0 + thin/16、thin/8、thin*3/16（均 < back，有 4/3 倍以上余量 ✓）。
// **红线只盖拖曳区**（第二轮 ✓）：红线 [x0, redEnd]，redEnd = x0 + 0.3*armX；
//   拖曳区全在红线路径内（0.3*armX ≥ back，见下余量守卫），② 的红底成立；红线外是空白底。
//   红笔改细为 2*thin（带宽 cy±thin，盖住蓝带 cy±thin/2 即可；旧 wide 太肥，红漆尾
//   redEnd + wide/2 会把 ③ 窗口挤没）。
// **中间点**（第三轮，甲）：redEnd + 2/2.5/3*thin——坐标不动，但语义改了：
//   这三点不再是“压空白底、应保持蓝”的点，而是“红线下游的携带点”：
//   smudge 1.0 下它们应为红主导（CI 实测 R=177.7、B=43.3），③ 断的就是这条
//   携带关系（见 ③ 断言），不是蓝色存活。`0.0` 轮的同点是蓝基线（R≈40），
//   两轮同点对比 ⇒ 携带到达中间点与否可直接读出。
const x0 = cx - armX * 0.6;
const redEnd = x0 + armX * 0.3;
const blueEnd = cx + armX * 0.8;
const redSize = thin * 2;
const back = thin / 4;
if (!(armX * 0.3 >= back) || !(redEnd + thin * 3 <= blueEnd)) {
  console.error(`  ⊘ 不可判：画布长宽比下红区/蓝区余量不足（0.3*armX=${(armX * 0.3).toFixed(1)}，redEnd+3*thin=${(redEnd + thin * 3).toFixed(1)}，blueEnd=${blueEnd.toFixed(1)}，back=${back.toFixed(1)}）⇒ 几何前提不成立，不是产品失败`);
  process.exit(3);
}
const dragXs = [x0 + thin / 16, x0 + thin / 8, x0 + thin * 3 / 16];
const midXs = [redEnd + thin * 2, redEnd + thin * 2.5, redEnd + thin * 3];

const run = async (tag, smudge) => {
  const layer = `L_${tag}`;
  await call("create_layer", { layer_id: layer, name: layer });
  await call("brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", preview: false,
    points: [[x0, cy, 1], [redEnd, cy, 1]], size: redSize, color: [200, 40, 40],
  });
  const args = {
    layer_id: layer, brush: "100%_Opaque", preview: false,
    points: [[x0, cy, 1], [blueEnd, cy, 1]],
    size: thin, color: [40, 40, 200],
  };
  if (smudge !== null) args.brush_params = { smudge };
  const reply = await call("brush_stroke", args);
  if (reply?.ok !== true) return null;
  // **一次采样、两组点** ✓（`sample_colors` 一次只要一个区域 ⇒ 6 点同批，不加渲染趟数 ✓）：
  //   前 3 个是拖曳点（①② 用），后 3 个是中间点（③ 用）✓。
  const sampled = await call("sample_colors", {
    points: [...dragXs.map((x) => [x, cy]), ...midXs.map((x) => [x, cy])],
  });
  const px = (sampled?.samples || []).map((z) => (z?.rgba || [0, 0, 0, 0]).slice(0, 3));
  if (px.length !== 6) return null;
  const avg = (arr) => ({
    r: arr.reduce((a, p) => a + p[0], 0) / arr.length,
    b: arr.reduce((a, p) => a + p[2], 0) / arr.length,
  });
  return { drag: avg(px.slice(0, 3)), mid: avg(px.slice(3, 6)), dragPx: px.slice(0, 3) };
};

const dflt = await run("dflt", null);
const zero = await run("zero", 0.0);
const one = await run("one", 1.0);
if (!dflt || !zero || !one) {
  console.error("  ⊘ 不可判：笔画或采样失败");
  process.exit(3);
}
console.log(`  ⊙ 缺省拖曳区 R=${dflt.drag.r.toFixed(1)}｜smudge 0.0 拖曳区 R=${zero.drag.r.toFixed(1)}｜smudge 1.0 拖曳区 R=${one.drag.r.toFixed(1)}｜smudge 1.0 中间 B=${one.mid.b.toFixed(1)} R=${one.mid.r.toFixed(1)}`);
// **校准用逐点打印** ✓（不参与判定 ✓）：CI 日志里留拖曳区三点的实测，供下一轮标定复核 ✓。
console.log(`  ⊙ 拖曳区三点（smudge 1.0）：${one.dragPx.map((p) => `(${p[0]},${p[1]},${p[2]})`).join(" ")}`);

// ① **缺省**不应该像 `smudge: 1.0` 那样拖带** ✓（拖曳区点上算 ✓：
//   缺省若偷偷开了涂抹，拖曳区会先露出它；中间点在拖曳区外，对缺省不敏感 ⇒ 不用它算 ① ✓）
//   **⇒ 沿用关系式而非硬等价** ✓（旧教训：第一版要求"缺省 == `smudge: 0.0`"（差 ≤ 2）✗
//     ⇒ **实测**本机 40.0 vs 40.0（相等），而 **CI 上 45.3 vs 40.0**（差 5.3）✗
//     ⇒ **所以"缺省值"跨机器不同** ⇒ **硬等价是**跨不了机器**的断言** ⇒ 改成关系：缺省拖带量远小于 `1.0`（取 1/3 作界）✓）
const span = Math.max(1, one.drag.r - zero.drag.r);
const defaultRise = dflt.drag.r - zero.drag.r;
if (defaultRise <= span / 3) {
  ok(`① 缺省只比 \`smudge: 0.0\` 高 ${defaultRise.toFixed(1)}，而 \`1.0\` 高 ${span.toFixed(1)} ⇒ **缺省远没到 1.0 的程度** ✓`);
} else {
  bad(`① 缺省比 \`smudge: 0.0\` 高 ${defaultRise.toFixed(1)}，而 \`1.0\` 只高 ${span.toFixed(1)} ⇒ 缺省本身就明显拖带`);
}

// ② smudge 1.0 在拖曳区拖带更多红（拖曳区点上算 ✓：只有这里的底图是起笔前的红漆 ✓）
if (one.drag.r > zero.drag.r + 10) ok(`② \`smudge: 1.0\` 让拖曳区内 R **升高 ${(one.drag.r - zero.drag.r).toFixed(1)}** ⇒ **拖带了下面的红** ✓`);
else bad(`② \`smudge: 1.0\` 只让拖曳区 R 升高 ${(one.drag.r - zero.drag.r).toFixed(1)} ⇒ **拖带不明显**`);

// ③（甲）携带沿路径持续：中间点相对拖曳区衰减有限，且远高于 0.0 的蓝基线，
//   同时中间点确为红主导——全强度涂抹从红线上起笔后，路径变红就是正确语义。
//   三条子关系缺一不可：(a) 防“只在起笔附近拖带、到不了中间点”；
//   (b) 防“携带半路衰减殆尽”；(c) 防“乙语义（快照）下中间点变回蓝却仍过”。
//   任一子关系失败即 ③ 红（不是打印，是 bad 入 fails ⇒ exit 1）。
const midCarry = one.mid.r - zero.mid.r;
const midDecay = one.drag.r > 0 ? one.mid.r / one.drag.r : 0;
if (midCarry > 50 && midDecay >= 0.7 && one.mid.r > one.mid.b) {
  ok(`③ 携带沿路径持续：中间 R=${one.mid.r.toFixed(1)} 比 0.0 基线 ${zero.mid.r.toFixed(1)} 高 ${midCarry.toFixed(1)}，为拖曳区 R=${one.drag.r.toFixed(1)} 的 ${(midDecay * 100).toFixed(0)}%（衰减 ${((1 - midDecay) * 100).toFixed(0)}% ≤ 30%），且 R > B=${one.mid.b.toFixed(1)} ⇒ 红主导沿路径搬运 ✓`);
} else {
  bad(`③ 携带未沿路径持续：中间 R=${one.mid.r.toFixed(1)}（0.0 基线 ${zero.mid.r.toFixed(1)}，差 ${midCarry.toFixed(1)}，须 > 50；拖曳区 R=${one.drag.r.toFixed(1)}，比值 ${midDecay.toFixed(2)}，须 ≥ 0.70；中间 B=${one.mid.b.toFixed(1)}，须 R > B）`);
}

if (fails.length > 0) { console.log(`\n  ✗ ${fails.length} 处不符（smudge 拖带）`); process.exit(1); }
console.log("\n  ✓ `smudge` 真的把下面的颜色带起来了");

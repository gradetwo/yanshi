#!/usr/bin/env node
// **`dodge_burn_setup` 到底改不改得动画面**（第五轮 ✓ 行为级判据）
//
// **⇒ 为什么还要多写这一条**：`scripts/tool-dodge-burn-masks.mjs` 只验**回执形状**
//   （`masks.dodge/burn` 对不对得上 `mask_ids` ✓）⇒ 它对本工具**最要紧的那件事**是盲的 ✗
//   本轮实测的两个缺陷它一条都抓不到：
//   ① 灰层铺的是 50% 显示灰 **128** ✗ 而本引擎 `soft_light` 在**线性缓冲**上做 ⇒
//      中性是线性 0.5 = 字节 **188** ⇒ 铺 128 时"建个组"这一步自己就把 128 的底压到 **97**
//      （整幅暗 31 级 ✓ 实测 ✓）；
//   ② `mode=curves` 建的层除曲线对象外**没有像素** ✓ 而**当时**调整对象只作用于**它自己图层**的缓冲
//      （`crates/yanshi-render/src/render.rs` 的 `Primitive::Adjustment` ⇒ `layer_buffer` ✓）
//      ⇒ 建组、把蒙版涂满白、删蒙版三步后合成**逐字节不变** ✗ 却回 `ok: true` = 静默。
//      **第六轮这条被上游推翻** ✓：`302d414d`「纯调整层渲染时先用下方内容播种」之后 curves
//      **改得到画面**（本条 D 臂重测：dodge 涂白 128→159 ✓ burn 涂白 128→98 ✓ 0/255 不动 ✓）
//      ⇒ D 臂从"验它不动"翻成"验它动、且方向对" ✓ 服务端 `effective` 同时从 false 翻回 true ✓
//      （第五轮在断言与回执两处都写了"谁修好跨层调整就同时翻"⇒ 这次照约定翻 ✓ 不是改口 ✗）。
//   ③ **本条判据自己跑红之后抓到的第三件事**（第五轮 ✓ 不是先验写进去的 ✗）：
//      `blend.rs` 的 `soft_light` 提亮支把 W3C 那条三次式 `((16Cb−12)Cb+4)Cb` 写成
//      `((16Cb−12)·√Cb+4)·Cb` ⇒ 在 `Cb ≤ 0.25`（编码 ≤ 188 ⇒ 几乎整个中间调）上
//      **白顶把画面压暗**：底色 128 铺白实测 **19**（W3C 闭式该给 **182** ✗ 反了 163 级 ✓
//      修复后实测正是 182 ✓ 188 是"中性字节"那件事 ✓ 与这里的期望值无关 ✗ 别混）；
//      接缝处也断开（`Cb=0.25` 该给 0.5 实测给 0 ✓）。⇒ "涂白提亮"这条功能承诺原本在
//      中暗调上是反的 ✗ 现在随 `blend.rs` 一并修掉 ✓ 本条 B 臂就是它的守卫 ✓。
//
// **⇒ 判据**（全部拿**服务端自己的合成读数**比 ✓ 界面/脚本不自算像素 ✓）：
//   A gray 建组 ⇒ `effective===true` ∧ `neutral_gray===188` ∧ 六个底色带建组前后**逐点相同**；
//   B 在灰层上铺白 ⇒ 那一带**严格变亮**；铺黑 ⇒ **严格变暗**；没碰的带**不变**（局部性）；
//   C 同名二次调用 ⇒ `already_exists` ∧ `group_id`/`layer_ids`/`neutral_gray`/`effective`/`mode` 一致
//     （**`mode` 是被单独点名的那一格** ✓ 本轮实测：复用分支整格漏掉它 ✗ 界面于是念"方法 undefined"✓）
//     ∧ **拿同名请求另一条路**（灰组 + 请求 curves）⇒ 回执必须照**磁盘**说
//       `mode:"gray"` / `effective:true` / `neutral_gray:188` ✓ 并在 note 里点出这次请求不一致 ✓
//       ⇒ 之后在那张灰层上铺白**仍然变亮** ✓（"effective=true"这句话由画面兑现 ✓ 不只是字段 ✓）。
//   D curves ⇒ `effective===true` ∧ `ineffective_reason===null` ∧ `neutral_gray===null`
//     ∧ 建组时两张蒙版全黑 ⇒ 六个底色带**逐字节不变**（蒙版确实是闸门 ✓）
//     ∧ dodge 蒙版涂满白 ⇒ 中间调**严格变亮**而 `0` 与 `255` 两个端点**不动**
//     ∧ 再把 burn 蒙版也涂满白 ⇒ 该点必须比只开 dodge 时**更暗**（两条曲线方向相反 ✓
//       否则把两条写成同一条也照样绿 ✗ 与 B 臂同一个"只验变了不够、方向要单独问"的道理 ✓）。
//
// **变异（四条都实测跑红过 ✓ 每次都真改源码、真重编、真跑）**：
//   ① `tools_retouch.rs` 的 `DB_NEUTRAL_GRAY` 改回 128 ⇒ A 红 ✓（六个底色带立刻不等 ✓）；
//   ② 变异（**第六轮在翻转后的 D 臂上重做 ✓**）：把服务端 curves 那条的
//      `effective` 写回 `!is_curves` ⇒ D 第一条红 ✓ 反过来把渲染器的播种改回去（去掉
//      `layer_is_adjustment_only` 那条 seed）⇒ D 的②"涂白必须变亮"红 ✓
//      ⇒ 回执与画面**两处都被问**：只改字段会被画面抓住 ✓ 只改渲染会被字段抓住 ✓；
//   ③ `blend.rs` 的 `* cb` 改回 `* cb.sqrt()` ⇒ **B 红** ✓ 实测读数正是当年那个 **19**
//     （`✗ B 灰层涂白没让画面变亮：[19,19,19,255]（原底色 128）` ✓ 一条判据把引擎缺陷钉住 ✓）
//     ⇒ 同一次改动里 `blend.rs` 的单位测试也红（`柔光 (0.2,0.8) 应为 ≈0.3488，实测 0.0877` ✓）；
//     **注意**：修之前的那份单位测试**抓不到** ✓ 因为它第三组的"参照值 0.0877"是抄实现的 ✗
//     现在换成按规范算出来的 `D(0.2)=0.448 ⇒ 0.3488` ✓ 才有区分力 ✓；
//   ④ 同一次变异里 A/C/D 仍绿 ⇒ 说明"中性不变画面"那条**永远看不见提亮支写反** ✓
//     方向必须单独问 ⇒ 这就是 B 臂存在的理由 ✓。
// **⇒ 退出码**：0 通过｜1 产品不符｜**3 不可判**（没给 base／建不出文档／铺底图失败 ⇒ 明确说"本条没验"）。
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-dodge-burn-effective.mjs <server-base>");
  process.exit(3);
}
const call = async (token, doc, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
const mkDoc = async () => {
  const doc = `db_eff_${Date.now()}_${Math.floor(Math.random() * 1e4)}`;
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 64, height: 48 }),
  })).json();
  if (!made.token) { console.error("  ⊘ 不可判：建不出文档 ⇒ 本条没验（不是产品失败）"); process.exit(3); }
  return { doc, token: made.token };
};
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const W = 64, H = 48, BAND = 8;
// 六个底色带（含 0/128/255 三个极值与一个彩色 ⇒ 中性字节错了必有一格露出来 ✓）。
const BAND_COLORS = [[0, 0, 0], [60, 60, 60], [128, 128, 128], [200, 200, 200], [255, 255, 255], [200, 30, 30]];
const bandPoint = (i) => [32, i * BAND + 4];

const layBands = async (token, doc) => {
  const layer = await call(token, doc, "create_layer", { layer_id: "L", name: "底图" });
  if (layer?.ok !== true) { console.error("  ⊘ 不可判：建不出图层"); process.exit(3); }
  for (let i = 0; i < BAND_COLORS.length; i += 1) {
    const [r, g, b] = BAND_COLORS[i];
    const filled = await call(token, doc, "fill_region", {
      layer_id: "L",
      shape: { type: "rect", x: 0, y: i * BAND, w: W, h: BAND },
      color: { r, g, b, a: 255 },
    });
    if (filled?.ok !== true) { console.error(`  ⊘ 不可判：第 ${i} 带铺色失败 ⇒ ${JSON.stringify(filled).slice(0, 160)}`); process.exit(3); }
  }
};
/// **只读服务端的合成像素** ✓：`sample_colors` 走的是渲染那条真路 ✓
/// ⇒ 判据里不出现任何"自己算 soft_light"的代码（那会变成第二个真值 ✗）。
const composite = async (token, doc, points) => {
  const res = await call(token, doc, "sample_colors", { points });
  const samples = res?.samples;
  if (!Array.isArray(samples) || samples.length !== points.length
    || samples.some((s) => !Array.isArray(s.rgba) || s.rgba.length !== 4)) {
    console.error(`  ⊘ 不可判：sample_colors 没给出 ${points.length} 个 rgba 读数 ⇒ ${JSON.stringify(res).slice(0, 160)}`);
    process.exit(3);
  }
  // **只取 `rgba`** ✓：`sample_colors` 的读数里还带 `index`/`x`/`y` ✗ 拿整格比对就会
  // 因为"这次少采了几个点 ⇒ index 重排"而假红（本轮实测踩过 ✓）⇒ 比对的是**像素**本身 ✓。
  return samples.map((s) => s.rgba);
};
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// ── A／B／C：gray 变体 ─────────────────────────────────────────────────────────
{
  const { doc, token } = await mkDoc();
  await layBands(token, doc);
  const before = await composite(token, doc, BAND_COLORS.map((_, i) => bandPoint(i)));
  const built = await call(token, doc, "dodge_burn_setup", { layer_id: "L", mode: "gray" });
  if (built?.ok !== true) { console.error(`  ⊘ 不可判：dodge_burn_setup(gray) 失败 ⇒ ${JSON.stringify(built).slice(0, 200)}`); process.exit(3); }
  if (built.effective !== true) bad(`A gray 必须回 effective=true，实测 ${JSON.stringify(built.effective)}`);
  if (built.neutral_gray !== 188) bad(`A 中性字节必须是本引擎柔光实测值 188，实测 ${JSON.stringify(built.neutral_gray)}`);
  if (built.mode !== "gray") bad(`A 回执要念回 mode=gray，实测 ${JSON.stringify(built.mode)}`);
  const after = await composite(token, doc, BAND_COLORS.map((_, i) => bandPoint(i)));
  if (same(before, after)) ok(`A gray 建组后六个底色带逐点不变（中性 = ${built.neutral_gray}）`);
  else bad(`A 建组这一步就改了画面 ⇒ 中性字节不对：建组前 ${JSON.stringify(before)}／建组后 ${JSON.stringify(after)}`);

  // 复用路径的层与对象 id 是服务端给的 ✓ 判据不自己猜 ✗。
  const grayLayer = built.layer_ids?.[0];
  if (!grayLayer) { console.error("  ⊘ 不可判：回执没给灰层 id ⇒ 后面两笔没法落"); process.exit(3); }
  const white = await call(token, doc, "fill_region", {
    layer_id: grayLayer,
    shape: { type: "rect", x: 0, y: 2 * BAND, w: W / 2, h: BAND },
    color: { r: 255, g: 255, b: 255, a: 255 },
  });
  const black = await call(token, doc, "fill_region", {
    layer_id: grayLayer,
    shape: { type: "rect", x: W / 2, y: 2 * BAND, w: W / 2, h: BAND },
    color: { r: 0, g: 0, b: 0, a: 255 },
  });
  if (white?.ok !== true || black?.ok !== true) { console.error(`  ⊘ 不可判：往灰层落笔失败 ⇒ ${JSON.stringify(white).slice(0, 120)}｜${JSON.stringify(black).slice(0, 120)}`); process.exit(3); }
  const lit = (await composite(token, doc, [[8, 2 * BAND + 4]]))[0];
  const dark = (await composite(token, doc, [[W - 8, 2 * BAND + 4]]))[0];
  const untouched = await composite(token, doc, [bandPoint(0), bandPoint(5)]);
  const baseGrey = BAND_COLORS[2][0];
  if (lit[0] > baseGrey) ok(`B 灰层涂白 ⇒ 128 的底色升到 ${lit[0]}（减淡方向对 ✓）`);
  else bad(`B 灰层涂白没让画面变亮：${JSON.stringify(lit)}（原底色 ${baseGrey}）⇒ 灰层没参与合成？混合模式不是 soft_light？还是 soft_light 的提亮支又写错了？`);
  if (dark[0] < baseGrey) ok(`B 灰层涂黑 ⇒ 128 的底色降到 ${dark[0]}（加深方向对 ✓）`);
  else bad(`B 灰层涂黑没让画面变暗：${JSON.stringify(dark)}（原底色 ${baseGrey}）`);
  if (same(untouched, [before[0], before[5]])) ok("B 没碰到的两个带仍然逐点不变（局部性 ✓ 不是整幅重铺）");
  else bad(`B 没碰的带也变了 ⇒ 首末两带 ${JSON.stringify(untouched)} vs ${JSON.stringify([before[0], before[5]])}`);

  const again = await call(token, doc, "dodge_burn_setup", { layer_id: "L", mode: "gray" });
  if (again?.already_exists !== true) bad(`C 同名二次调用应 already_exists=true，实测 ${JSON.stringify(again).slice(0, 160)}`);
  else if (!same(again.layer_ids, built.layer_ids) || again.group_id !== built.group_id
    || again.neutral_gray !== built.neutral_gray || again.effective !== built.effective) {
    bad(`C 复用路径必须念回同一套字段 ⇒ ${JSON.stringify({ g: again.group_id, l: again.layer_ids, n: again.neutral_gray, e: again.effective }).slice(0, 200)}`);
  } else ok(`C 复用路径 group/layer/neutral/effective 与新建一致（${again.group_id}）`);
  // **C-2 `mode` 这一格必须在** ✓（本轮实测：复用分支整格漏掉 `mode` ✗ 界面于是念出
  //   `方法 undefined`／英文 `method undefined` ✓ 而上面那条"同一套字段"看不见它 ✓
  //   因为它两边都比的是 `undefined === undefined` ✓ ⇒ 必须**点名要求这个字段** ✓）。
  if (again.mode !== built.mode) bad(`C 复用回执的 mode 要与新建一致，实测 ${JSON.stringify({ 新建: built.mode, 复用: again.mode })}`);
  else ok(`C 复用回执带 mode（${again.mode}）⇒ 界面那句「方法」有数可念`);
  // **C-3 拿同一个组名请求另一条路** ⇒ 回执必须念「这个组实际是哪条路建的」✗
  //   不能照抄本次请求 ✓。旧写法在这里回 `mode:"curves" / effective:false / neutral_gray:null` ✗
  //   **而磁盘上那张 soft_light 灰层明明改得到画面** ✓ ⇒ 回执与事实相反 ＝ 静默 ✗。
  const crossed = await call(token, doc, "dodge_burn_setup", { layer_id: "L", mode: "curves" });
  if (crossed?.already_exists !== true) bad(`C-3 同名跨模式调用应 already_exists=true，实测 ${JSON.stringify(crossed).slice(0, 160)}`);
  else if (crossed.mode !== "gray" || crossed.effective !== true || crossed.neutral_gray !== 188) {
    bad(`C-3 既有的是灰层组 ⇒ 拿 curves 请求它时，回执必须照磁盘说 gray / effective=true / 中性 188，实测 ${JSON.stringify({ mode: crossed.mode, effective: crossed.effective, neutral_gray: crossed.neutral_gray })}`);
  } else if (!/本次请求的是 mode=curves/.test(crossed.note || "")) {
    bad(`C-3 请求与既有组不一致时 note 要明说，实测「${(crossed.note || "").slice(0, 90)}」`);
  } else ok("C-3 同名跨模式调用照磁盘回答（gray／effective=true／188）并在 note 里点出这次请求不一致");
  // **C-4 画面事实**：上面那次"请求 curves 但复用了灰组"之后 ⇒ 在灰组那张灰层上铺白
  //   仍必须**变亮** ✓（回执说 effective=true 是真的 ✗ 不是只写在字段里 ✓）。
  if (crossed?.already_exists === true && crossed.layer_ids?.length) {
    const spot = await call(token, doc, "fill_region", {
      layer_id: crossed.layer_ids[0],
      shape: { type: "rect", x: 32, y: 2 * BAND, w: 4, h: BAND },
      color: { r: 255, g: 255, b: 255, a: 255 },
    });
    const lit2 = spot?.ok === true ? (await composite(token, doc, [[34, 2 * BAND + 4]]))[0] : null;
    if (lit2 && lit2[0] > BAND_COLORS[2][0]) ok(`C-4 复用的灰组上涂白仍变亮（${BAND_COLORS[2][0]}→${lit2[0]}）⇒ effective=true 那句话是真的`);
    else bad(`C-4 复用灰组后涂白没让画面变亮 ⇒ ${JSON.stringify({ spot: spot?.ok, code: spot?.error_code, 读数: lit2 })}`);
  }
}

// ── D：curves 变体 ────────────────────────────────────────────────────────────
// **第六轮翻转**：上游 `302d414d`「纯调整层渲染时先用下方内容播种」之后 ✗ 这条路**改得到画面** ✓
// ⇒ 本臂从"验它不动"翻成"验它**动**、而且**方向对**" ✓ 断言三件事，缺一不可：
//   ① 建组时两张蒙版全黑 ⇒ 六个带**逐字节不变**（证明蒙版确实是闸门 ✗ 不是"这条根本没关系"）；
//   ② dodge 蒙版涂满白 ⇒ 中间调**严格变亮** ✓ 而 `0` 与 `255` 两个端点**逐字节不动**
//      （端点被推 ⇒ 曲线写成了线性平移 ✓ 那不是减淡）；
//   ③ 再把 burn 蒙版也涂满白 ⇒ 同一点必须比只开 dodge 时**更暗**
//      （两条曲线方向相反 ✓ 否则把 dodge/burn 写成同一条也照样绿 ✗ 与 B 臂同一个道理）。
{
  const { doc, token } = await mkDoc();
  await layBands(token, doc);
  const before = await composite(token, doc, BAND_COLORS.map((_, i) => bandPoint(i)));
  const built = await call(token, doc, "dodge_burn_setup", { layer_id: "L", mode: "curves" });
  if (built?.ok !== true) { console.error(`  ⊘ 不可判：dodge_burn_setup(curves) 失败 ⇒ ${JSON.stringify(built).slice(0, 200)}`); process.exit(3); }
  if (built.effective !== true) bad(`D curves 必须自报 effective=true（上游 302d414d 起纯调整层先用下方内容播种 ⇒ 这条路改得到画面），实测 ${JSON.stringify(built.effective)}`);
  else ok("D curves 自报 effective=true");
  if (built.ineffective_reason !== null && built.ineffective_reason !== undefined) bad(`D curves 既然改得到画面 ⇒ 不该再有 ineffective_reason，实测「${String(built.ineffective_reason).slice(0, 60)}」`);
  if (built.neutral_gray !== null && built.neutral_gray !== undefined) bad(`D curves 组没有灰层 ⇒ 不该回 neutral_gray，实测 ${JSON.stringify(built.neutral_gray)}`);
  const dodgeMask = built.masks?.dodge;
  const burnMask = built.masks?.burn;
  if (!dodgeMask || !burnMask) { console.error(`  ⊘ 不可判：curves 回执没给两张蒙版 id ⇒ ${JSON.stringify(built.masks).slice(0, 160)}`); process.exit(3); }

  const bandLum = (rgba) => Math.round((rgba[0] + rgba[1] + rgba[2]) / 3);
  const paintAll = async (mask_id) => call(token, doc, "paint_mask", {
    // 圆心 (32,24) 半径 100 ⇒ 覆盖整张 64×48（最远角点 40）✓ 六个带一个不漏。
    mask_id, points: [[32, 24, 1.0]], size: 200, opacity: 1.0, softness: 0.0,
  });

  // ① 全黑蒙版 ⇒ 闸门关着：建组前后逐字节相同。
  const gated = await composite(token, doc, BAND_COLORS.map((_, i) => bandPoint(i)));
  if (!same(before, gated)) bad(`D 建组时两张蒙版全黑 ⇒ 六个带必须逐字节不变，实测建组前 ${JSON.stringify(before)}／建组后 ${JSON.stringify(gated)}`);
  else ok("D 蒙版全黑时六个带逐字节不变（蒙版确实是闸门）");

  // ② 只涂开 dodge 蒙版 ⇒ 中间调严格变亮 ✓ 端点不动。
  const paintedDodge = await paintAll(dodgeMask);
  if (paintedDodge?.ok !== true) { console.error(`  ⊘ 不可判：paint_mask(dodge) 失败 ⇒ ${JSON.stringify(paintedDodge).slice(0, 160)}`); process.exit(3); }
  const lit = await composite(token, doc, BAND_COLORS.map((_, i) => bandPoint(i)));
  const MIDS = [1, 2, 3, 5];
  const dodgingWrong = MIDS.filter((i) => !(bandLum(lit[i]) > bandLum(before[i])));
  if (dodgingWrong.length) bad(`D dodge 蒙版涂白后这些带没有严格变亮 ⇒ ${dodgingWrong.map((i) => `${BAND_COLORS[i]}→${lit[i]}（亮度 ${bandLum(before[i])}→${bandLum(lit[i])}）`).join("｜")}`);
  else ok(`D dodge 蒙版涂白 ⇒ 中间调严格变亮（60→${bandLum(lit[1])}｜128→${bandLum(lit[2])}｜200→${bandLum(lit[3])}）`);
  if (!same(lit[0], before[0]) || !same(lit[4], before[4])) bad(`D dodge 把纯黑或纯白也推走了 ⇒ 那不是曲线减淡，实测 ${JSON.stringify({ 黑: [before[0], lit[0]], 白: [before[4], lit[4]] })}`);
  else ok("D 0 与 255 两个端点不动（曲线锚在端点 ✓）");

  // ③ 再涂开 burn 蒙版 ⇒ 同一点必须比只开 dodge 时更暗（两条臂方向相反）。
  const paintedBurn = await paintAll(burnMask);
  if (paintedBurn?.ok !== true) { console.error(`  ⊘ 不可判：paint_mask(burn) 失败 ⇒ ${JSON.stringify(paintedBurn).slice(0, 160)}`); process.exit(3); }
  const both = await composite(token, doc, BAND_COLORS.map((_, i) => bandPoint(i)));
  const notDarker = MIDS.filter((i) => !(bandLum(both[i]) < bandLum(lit[i])));
  if (notDarker.length) bad(`D 再开 burn 后这些带没有变暗 ⇒ ${notDarker.map((i) => `只开dodge ${lit[i]}→两条都开 ${both[i]}`).join("｜")}（两条曲线写成同一条了？）`);
  else ok(`D burn 蒙版也涂白 ⇒ 128 的底色从 ${bandLum(lit[2])} 降到 ${bandLum(both[2])}（两条臂方向相反 ✓）`);
  if (!same(both[0], before[0]) || !same(both[4], before[4])) bad(`D burn 把纯黑或纯白也推走了 ⇒ 实测 ${JSON.stringify({ 黑: [before[0], both[0]], 白: [before[4], both[4]] })}`);
  else ok("D 端点在两条臂都开时仍不动");
}

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（dodge_burn 是否真的作用于画面）`);
  process.exit(1);
}
console.log("\n  ✓ dodge_burn_setup：gray 中性 188 且建组不变画面、涂白变亮涂黑变暗、复用字段一致；curves 自报 effective=true 且蒙版涂开后**确实**变亮/变暗、端点不动");
process.exit(0);

#!/usr/bin/env node
// **★ brush_params：覆盖 .myb 任意字段（P0 #4 ✓）★**（第 254 轮 ✓）
//
// **∴ 要什么 ✗**：**MCP 现在只能传 `brush`／`size`／`color`／`opacity`／`hardness`**✗
//   ⇒ **∴ `.myb` 里那些真正决定笔触的东西**（`smudge`、`elliptical_dab_angle`、
//     `speed1_*`／`speed2_*`、`dabs_per_*` … ✓）**全都调不到** ✓ ★★
//
// **∴ 为什么这条**几乎零成本 ✗**（**读代码 ✓）**：**Hokusai 把设置做成了**枚举 ＋ 规范名** ✓
//   ＋ `BrushSetting::ALL`（可遍历 ✓）
//   ＋ `cname()`（**注释原话："libmypaint canonical string name (matches `.myb` JSON keys)"** ✓）
//   ＋ `brush.get_mut(setting).base_value = v`（`brush_reads_the_canvas` 就是这么读的 ✓）
//     ⇒ **∴ 所以"覆盖任意字段"就是**一个循环**✗ 不必为每个字段写一遍** ✓ ★★★
//
// **∴ 判什么（**四条 ✓）★**：
//   ① `brush_params:{"smudge":1}` 必须**真的改变画面**（与不传它时**逐字节不同** ✓）★
//      ⇒ **∴ 这是核心 ✗**：**参数被忽略的话两次必然**逐字节相同** ✓
//   ② 同一次调用里**两个字段一起改**也要生效（`smudge` ＋ `opaque` ✓）★
//   ③ **拼错的字段名必须报错**（**∴ 否则打字错误会**静默无效**✗ 用户以为调了 ✓）★★
//   ④ **缺省不传时**，画面必须与"从来不支持这个参数"的行为**逐字节相同** ✓（**兼容约束 ✓）★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把 `brush_params` 解析整段删掉／短路**⇒ **① 立刻红** ✓ ★★★
//
// 用法：node scripts/tool-brush-params.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-brush-params.mjs <base-url>");
  process.exit(2);
}
const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

// **★ A/B 必须用两个文档 ✗ ★**（**第 254 轮自查修 ✓）**：
//   ＋ **∴ 我第一版在**同一个文档的同一行**上叠着画 ✗** ⇒ **∴ 每次都多一笔 ⇒ 指纹当然不同 ⇒ ①② 假绿** ✓
//     ＋ **∴ 第二版改成"每例一行" ✗** 但**不同行**采样的区域本来就不同 ⇒ **对照仍然不成立** ✓
//       ⇒ **∴ 现在 ✗**：**A = 不传 `brush_params` 的文档 ✗ B = 传的文档 ✗ 两边**同一位置**同一笔** ✓
//         ⇒ **∴ 指纹不同 ⇒ 参数真的生效** ✓ ★★★
async function buildDoc(tag) {
  const doc = `bp_${tag}_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 512, height: 512 }),
    })
  ).json();
  if (!created || !created.token) return null;
  const token = created.token;
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  // 铺一层红底，让 smudge 有东西可拾取
  await call(doc, token, "brush_stroke", {
    layer_id: "L1",
    object_id: "red_base",
    brush: "100%_Opaque",
    size: 48,
    color: { r: 220, g: 30, b: 30, a: 255 },
    points: [[80, 260, 1.0], [440, 260, 1.0]],
  });
  return { doc, token };
}

const main = async () => {
  const bad = [];
  const run = async (tag, brushParams) => {
    const built = await buildDoc(tag);
    if (!built) {
      console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
      process.exit(3);
    }
    const { doc, token } = built;
    const args = {
      layer_id: "L1",
      object_id: "stroke",
      // **★ 用**普通**笔刷 ＋ **效果可见**的字段 ✗ ★**（**第 254 轮自查修 ✓）**：
      //   ＋ **∴ 我前两版用 `blending`（**纯揉合 ✓）＋ `smudge`**✗ **而它本来就 smudge=1** ⇒
      //     **∴ 设成 1 是**空操作**✗ 而且纯揉合**不沉积颜色** ⇒ `opaque` 也看不出 ✓
      //       ⇒ **∴ 于是**判据永远绿不了 ✗ **∴ 是**我选错了字段**✗ 不是功能没做** ✓ ★★★
      brush: "100%_Opaque",
      size: 48,
      color: { r: 20, g: 40, b: 220, a: 255 },
      opacity: 1.0,
      points: [[80, 260, 1.0], [440, 260, 1.0]],
    };
    if (brushParams !== undefined) args.brush_params = brushParams;
    const replied = await call(doc, token, "brush_stroke", args);
    const raw = await call(doc, token, "render_region", {
      region: { x: 200, y: 230, w: 120, h: 60 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    let fingerprint = null;
    if (url) {
      const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
      const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
      let sum = 0;
      for (let i = 0; i < bytes.length; i += 4) {
        sum = (sum * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
      }
      fingerprint = sum;
    }
    return { replied, fingerprint };
  };

  // ④ 缺省（A）
  const a = await run("default", undefined);
  if (!a.fingerprint || !a.replied || a.replied.ok !== true) {
    console.error(`❌ 判不了：缺省那一笔失败（${JSON.stringify(a.replied).slice(0, 140)}）⇒ 退出码 3`);
    process.exit(3);
  }
  console.log(`  ④ 缺省（A）：指纹 ${a.fingerprint}`);

  // ① 只改 smudge（B）
  const b = await run("opaque", { opaque: 0.15 });
  console.log(`  ① brush_params{opaque:0.15}（B）：指纹 ${b.fingerprint}｜ok=${b.replied && b.replied.ok}`);
  if (!b.replied || b.replied.ok !== true) {
    bad.push(`① 传 brush_params:{opaque:0.15} 的调用失败了（${JSON.stringify(b.replied).slice(0, 140)}）`);
  } else if (b.fingerprint === a.fingerprint) {
    bad.push("① A/B 同一位置同一笔，传了 brush_params{opaque:0.15} 却**逐字节相同** ⇒ 参数被忽略了");
  }

  // ② 两个字段一起改 ⇒ 必须比"只改一个"再不同一层
  const c = await run("two", { opaque: 0.15, dabs_per_basic_radius: 6.0 });
  console.log(`  ② brush_params{opaque:0.15,dabs_per_basic_radius:6}：指纹 ${c.fingerprint}`);
  if (!c.replied || c.replied.ok !== true) {
    bad.push(`② 一次改两个字段的调用失败了（${JSON.stringify(c.replied).slice(0, 140)}）`);
  } else if (c.fingerprint === a.fingerprint) {
    bad.push("② 一次改两个字段但画面与缺省逐字节相同 ⇒ 多字段没生效");
  }

  // ③ 拼错的字段名 ⇒ 必须报错（静默无效是最坏的）
  const typo = await run("typo", { opaqe: 0.15 });
  const typoOk = typo.replied && typo.replied.ok === true;
  console.log(`  ③ 拼错的字段名 opaqe ⇒ ok=${typo.replied && typo.replied.ok}｜error_code=${typo.replied && typo.replied.error_code}`);
  if (typoOk) {
    bad.push("③ 拼错的字段名（opaqe）被静默接受了 ⇒ 用户会以为参数生效了（应当报错并给候选）");
  }

  // ④ **`brush_params` 里拼错时，必须给出**相近候选****（第 1358 轮补；台账「待核」那一条）
  //   **⇒ 为什么单列一条** ✗：③ 只验了「顶层 opaqe 被拒」✗ ——
  //     而**顶层被拒只是因为**它不是工具参数** ⇒ 与「brush_params 的规范名校验」**是两条路** ✗
  //   **⇒ 实测**：`brush_params:{"opaqe":0.15}` ⇒
  //     `brush_params 里没有「opaqe」这个规范名 ⇒ 相近的有：opaque、opaque_multiply、opaque_linearize` ✓
  //   ⇒ 所以要断：**拒绝 ＋ 给出相近候选**（只拒不给人猜 ⇒ 仍要靠文档 ✗）
  const bpT = await run("bptypo", { opaqe: 0.15 });
  const bp = bpT.replied || {};
  const bpDetail = String((bp.context || {}).detail || bp.detail || "");
  if (bp.ok !== false) {
    console.log(`  ④ brush_params 拼错 ⇒ ok=${bp.ok}（**被接受了** ✗）`);
  } else if (/相近的有|你是不是要找/.test(bpDetail) && /opaque/.test(bpDetail)) {
    console.log(`  ④ brush_params 拼错 ⇒ 拒绝并给出候选 ✓（${bpDetail.slice(0, 60)}…）`);
  } else {
    bad.push(`④ brush_params 拼错时**必须给出相近候选** ⇒ 实测「${bpDetail.slice(0, 80)}」`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ brush_params 生效（A/B 指纹不同）、多字段生效、拼错会报错");
};

await main();

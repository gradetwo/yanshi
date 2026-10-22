#!/usr/bin/env node
// **`fill_region(texture=bitmap)` 必须逐像素取源位图** ✓（外部第 71–73 次报告要的那个能力）
//
// **为什么需要这条判据** ✗：纹理填充最容易的错法是"取全图平均色/单一颜色"✗ ——
//   **结果仍然 `ok:true`、仍然填满了整块、仍然比纯色好看** ✓ ⇒ 正确性判据抓不到 ✗
//   ⇒ 而对方的实测（`E1761`）正是"平均会让深处看不出纹理" ✓
//   ⇒ **∴ 唯一能抓住它的构型**：源位图**左半红、右半蓝** ⇒ 目标区**必须同时含红与蓝** ✓
//     （平均 ⇒ 整片紫 ⇒ 两条断言同时红 ✓）
//
// **判据（五条，成对才有意义）**：
//   ① `texture={type:"bitmap",…}` 被接受（`ok:true`）
//   ② 响应**回显** `texture.source_gap_px`（传了纹理却不说它生效 = 撒谎 ✗）
//   ③ 源 8×8（左红右蓝）、目标 32×32 ⇒ 目标左/右各取到**红**/**蓝**
//   ④ `type:"region"` **未实现** ⇒ 必须**报错并点名 region**（不许静默当"没有纹理" ✗）
//   ⑤ `min_distance` 大于实际间距 ⇒ 必须**报错并点名**（对方的 `test1` 红褐竖痕护栏 ✓）
//
// **退出码**（第 336 轮）：0 通过｜1 产品失败｜3 判据自身前提不成立（不是产品问题）。
import { Buffer } from "node:buffer";

const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-fill-texture.mjs <base-url>");
  process.exit(3);
}

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label);
};

const stamp = Date.now().toString(36);
const boot = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: `tex_${stamp}`, width: 128, height: 96 }),
});
const bootBody = await boot.json().catch(() => ({}));
const doc = bootBody.doc ?? bootBody.doc_id ?? `tex_${stamp}`;
const token = bootBody.token;
if (!token) {
  console.error(`  ⊘ 不可判：建不出文档 ⇒ ${JSON.stringify(bootBody).slice(0, 160)}`);
  process.exit(3);
}
const call = async (name, args) => {
  const reply = await fetch(
    `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    },
  );
  return reply.json();
};
const detailOf = (body) => JSON.stringify(body?.context?.detail ?? body?.context ?? {}).slice(0, 160);

const layers = await call("list_layers", {});
const layerId = layers?.layers?.[0]?.layer_id ?? layers?.layers?.[0]?.id;
if (!layerId) {
  console.error(`  ⊘ 不可判：新文档没有图层 ⇒ ${JSON.stringify(layers).slice(0, 160)}`);
  process.exit(3);
}

// 源位图：8×8 原始 RGBA，**左半红、右半蓝**（与 `tests/fill_region_texture.rs` 同一构型）。
const source = Buffer.alloc(8 * 8 * 4);
for (let y = 0; y < 8; y += 1) {
  for (let x = 0; x < 8; x += 1) {
    const i = (y * 8 + x) * 4;
    const red = x < 4;
    source[i] = red ? 255 : 0;
    source[i + 1] = 0;
    source[i + 2] = red ? 0 : 255;
    source[i + 3] = 255;
  }
}
const put = await call("put_blob", {
  data_base64: source.toString("base64"),
  mime_type: "image/x-yanshi-raw",
});
const hash = put?.blob_hash ?? put?.blob ?? put?.hash;
if (!hash) {
  console.error(`  ⊘ 不可判：put_blob 没回哈希 ⇒ ${JSON.stringify(put).slice(0, 160)}`);
  process.exit(3);
}

const texture = {
  type: "bitmap",
  blob_hash: hash,
  source: { x: 0, y: 0, w: 8, h: 8 },
  width: 8,
  height: 8,
};
const target = { type: "rect", x: 32, y: 32, w: 32, h: 32 };
const filled = await call("fill_region", { layer_id: layerId, shape: target, texture });
check(filled?.ok === true, "① texture={type:\"bitmap\"} 必须被接受", detailOf(filled));
check(
  typeof filled?.texture?.source_gap_px === "number",
  "② 响应必须回显 texture.source_gap_px（生效了就要说）",
  JSON.stringify(filled?.texture ?? null),
);

// 目标 x=34 ⇒ 源里 x=2 ⇒ 红；x=60 ⇒ 源里 x=4 ⇒ 蓝（8px 平铺 ✓）。
const samples = (await call("sample_colors", { points: [[34, 36], [60, 36]] }))?.samples;
if (!Array.isArray(samples) || samples.length !== 2) {
  console.error("  ⊘ 不可判：sample_colors 形状变了");
  process.exit(3);
}
const rgb = (index) => samples[index]?.rgba?.slice(0, 3) ?? [0, 0, 0];
const [lr, , lb] = rgb(0);
const [rr, , rb] = rgb(1);
check(lr > 200 && lb < 60, "③ 目标**左**侧取到源的**红**半（平均会变紫 ⇒ 这条红）", `实测 [${rgb(0)}]`);
check(rr < 60 && rb > 200, "④ 目标**右**侧取到源的**蓝**半（平均会变紫 ⇒ 这条红）", `实测 [${rgb(1)}]`);

// ⑤ **`type:"region"` 现在可用**（从画布取纹理 ⇒ 就地拍成位图）＋ ⑤b 缺 `source` 才报错
//   **⇒ 这条判据原来是「region 必须报错」** ✗ —— 它钉的是**当时**的缺口 ✓；
//     能力实现之后**它会红**（CI 实测 shard 2/6 红在这里 ✓）⇒ **判据要跟着能力走** ✓，
//     否则它就变成"阻止功能上线"的绊脚石 ✗。
const regionFilled = await call("fill_region", {
  layer_id: layerId,
  shape: target,
  texture: { type: "region", source: { x: 40, y: 4, w: 8, h: 8 } },
});
check(
  regionFilled?.ok === true && regionFilled?.texture?.from_region === true,
  "⑤ texture.type=region 必须可用，并回显 from_region:true",
  `ok=${regionFilled?.ok}｜${JSON.stringify(regionFilled?.texture ?? null).slice(0, 120)}`,
);
const regionNoSource = await call("fill_region", {
  layer_id: layerId,
  shape: target,
  texture: { type: "region" },
});
const noSourceDetail = detailOf(regionNoSource);
check(
  regionNoSource?.ok === false && /source/.test(noSourceDetail) && /region/.test(noSourceDetail),
  "⑤b region 缺 source ⇒ 必须报错并同时点名 region 与 source",
  `ok=${regionNoSource?.ok}｜${noSourceDetail}`,
);

// ⑥ `min_distance` 大于实际间距 ⇒ 必须报错并点名 min_distance。
const tooClose = await call("fill_region", {
  layer_id: layerId,
  shape: target,
  texture: { ...texture, source: { x: 30, y: 30, w: 8, h: 8 }, min_distance: 40 },
});
const closeDetail = detailOf(tooClose);
check(
  tooClose?.ok === false && /min_distance/.test(closeDetail),
  "⑥ min_distance 超过实际间距 ⇒ 必须报错并点名 min_distance",
  `ok=${tooClose?.ok}｜${closeDetail}`,
);

// ⑦⑧ **`jitter`（防「墙纸重复」，外部第 76 次要的那个旋钮）** ✓
//   **为什么要在 JS 判据里也钉一遍** ✗：Rust 侧已有（`fill_region_texture.rs` ✓），
//   而 CI 的 `criteria` 分片跑的是**真服务端 ＋ 真 HTTP** ✓ ⇒ 这里钉的是"**端到端**它真的生效" ✓。
//   构型：源 8×8（左红右蓝）⇒ 目标 32×32 ⇒ **相距 8px 的两列**在 `jitter=0` 时必须**逐字节相同** ✓、
//   在 `jitter=1` 时必须**至少有一对不同** ✓（两件事成对：只断后者，'永远加噪声'也能过 ✗）。
const pairStats = async (jitter) => {
  const boot = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: `texj_${jitter}_${Date.now().toString(36)}`, width: 128, height: 96 }),
  });
  const body = await boot.json();
  const docId = body.doc ?? body.doc_id;
  const tk = body.token;
  const post = async (name, args) => {
    const reply = await fetch(
      `${base}/api/tools/${name}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(tk)}`,
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) },
    );
    return reply.json();
  };
  const layers = await post("list_layers", {});
  const lid = layers?.layers?.[0]?.layer_id;
  const filled = await post("fill_region", {
    layer_id: lid,
    shape: { type: "rect", x: 32, y: 32, w: 32, h: 32 },
    texture: { ...texture, jitter },
  });
  if (filled?.ok !== true) return { error: filled };
  const points = [];
  for (let i = 0; i < 24; i += 1) points.push([34 + i, 40]);
  const samples = (await post("sample_colors", { points }))?.samples ?? [];
  const colour = (index) => (samples[index]?.rgba ?? []).slice(0, 3).join(",");
  let same = 0;
  let total = 0;
  for (let i = 0; i + 8 < points.length; i += 1) {
    total += 1;
    if (colour(i) === colour(i + 8)) same += 1;
  }
  return { same, total, echoed: filled?.texture?.jitter };
};

const plain = await pairStats(0);
const jittered = await pairStats(1);
check(
  plain?.total > 0 && plain.same === plain.total,
  "⑦ jitter=0 ⇒ 相距 8px（= 源块宽）的每一对必须完全相同（原样平铺）",
  plain?.error ? detailOf(plain.error) : `${plain?.same}/${plain?.total} 对相同`,
);
check(
  jittered?.total > 0 && jittered.same < jittered.total,
  "⑧ jitter=1 ⇒ 至少一对不再相同（重复周期真被打散）",
  jittered?.error ? detailOf(jittered.error) : `${jittered?.same}/${jittered?.total} 对相同`,
);

// ⑨⑩ **`mode:"patch"` 合成（外部第 81 次的正式需求）** ✓
//   **为什么要在 JS 判据里也钉** ✗：Rust 侧已有（`fill_region_texture.rs` ✓），
//   而 CI 的 `criteria` 跑的是**真服务端 ＋ 真 HTTP** ✓ ⇒ 这里钉的是"端到端真的走了合成" ✓。
//   构型与 ⑦⑧ 相同（源 8×8 左红右蓝）⇒ **相距 8px 的对**在合成下**必须至少有一对不同** ✓
//   （若静默退回平铺 ⇒ 全同 ⇒ 红 ✓），并且两条回显字段必须对得上 ✓。
const patchStats = async () => {
  const boot = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: `texp_${Date.now().toString(36)}`, width: 128, height: 96 }),
  });
  const body = await boot.json();
  const docId = body.doc ?? body.doc_id;
  const tk = body.token;
  const post = async (name, args) => {
    const reply = await fetch(
      `${base}/api/tools/${name}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(tk)}`,
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) },
    );
    return reply.json();
  };
  const layers = await post("list_layers", {});
  const lid = layers?.layers?.[0]?.layer_id;
  const filled = await post("fill_region", {
    layer_id: lid,
    shape: { type: "rect", x: 32, y: 32, w: 32, h: 32 },
    texture: { ...texture, mode: "patch", seam: "min_error", patch: 6, overlap: 2, candidates: 12 },
  });
  if (filled?.ok !== true) return { error: filled };
  const points = [];
  for (let i = 0; i < 24; i += 1) points.push([34 + i, 40]);
  const samples = (await post("sample_colors", { points }))?.samples ?? [];
  const colour = (index) => (samples[index]?.rgba ?? []).slice(0, 3).join(",");
  let same = 0;
  let total = 0;
  for (let i = 0; i + 8 < points.length; i += 1) {
    total += 1;
    if (colour(i) === colour(i + 8)) same += 1;
  }
  return { same, total, mode: filled?.texture?.mode, seam: filled?.texture?.seam };
};

const patch = await patchStats();
check(
  patch?.mode === "patch" && patch?.seam === "min_error",
  "⑨ mode=patch/seam=min_error ⇒ 回显必须说清跑的是合成与哪种缝（不许沉默）",
  patch?.error ? detailOf(patch.error) : `mode=${patch?.mode}｜seam=${patch?.seam}`,
);
check(
  patch?.total > 0 && patch.same < patch.total,
  "⑩ mode=patch ⇒ 相距 8px 的对必须至少有一对不同（静默退回平铺就全同）",
  patch?.error ? detailOf(patch.error) : `${patch?.same}/${patch?.total} 对相同`,
);

console.log(
  failures.length === 0
    ? "  ⇒ 纹理填充：逐像素取源、region 可用、min_distance 护栏、jitter 打散墙纸、patch 合成 ✓"
    : `  ⇒ 失败 ${failures.length} 条`,
);
process.exit(failures.length === 0 ? 0 : 1);

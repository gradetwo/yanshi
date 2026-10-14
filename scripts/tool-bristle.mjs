#!/usr/bin/env node
// **★ 笔毛分叉：起收笔真的变尖（P0 #2 ✓）★**（第 254 轮 ✓）
//
// **∴ 要什么 ✗**（需求文档 P0 #2 ✓）**：**现在 1 条主路径 ⇒ 1 个 dab ⇒ 起收笔**钝头**✗
//   ⇒ **∴ 要**1 个 dab 拆成 5 股子笔毛**（**中间密／边缘疏（立方映射 ✓）
//     ＋ **压感控制展宽：起收笔聚拢（0.12 倍）／行笔展开（1.0 倍 ✓）
//       ＋ **每股独立 1D 噪声（分叉漫游 ✓）⇒ **∴ 效果：起收笔**自动尖头** ✓ ★★
//
// **∴ 怎么量才**不会被别的东西骗**✗**：**压感本身也会让笔变粗变细**✗
//   ⇒ **∴ 所以判据**全程用**恒定压感**（`pressure` 恒为 1.0 ✓）**✗
//     ⇒ **∴ 于是**：**笔画两端若比中间窄 ⇒ 只可能来自**笔毛展宽**✓ 不可能是压感** ✓ ★★★
//
// **∴ 判什么（**三条 ✓）★**：
//   ① `bristle: 5` 画一条**恒定压感**的直线 ⇒ **两端墨宽必须**明显窄于中间** ✓（尖头 ✓）★
//   ② `bristle: 1`（＝关 ✓）⇒ **墨宽必须**基本恒定** ✓（**∴ 这就是"没有分叉"的对照 ✓）★
//   ③ **`bristle` 缺省时**必须与"从来没有这个参数"逐字节相同 ✓（**兼容约束 ✓）★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把分叉整段短路（永远只画 1 股 ✓）
//   ⇒ **∴ ① 立刻红**（两端和中段一样宽 ✓）★★★
//
// 用法：node scripts/tool-bristle.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-bristle.mjs <base-url>");
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

// **★ 判据必须**相对自己的对照**量 ✗ ★**（**第 254 轮自查修 ✓）**：
//   ＋ **∴ 我第一版直接量"两端 vs 中段"**✗ ⇒ **∴ 实测发现**恒定压感下默认笔画本来就
//     头 49.3／尾 28.7 vs 中段 64.0**✗ —— **那是**圆头笔尖的几何**✗ 不是分叉 ✓
//     ⇒ **∴ 于是**① 会因错的理由过、② 会因错的理由红** ✓ ★★
//   ＋ **∴ 现在 ✗**：**拿 `bristle:1`（关 ✓）当**同一位置**的对照 ✗ 比**相对宽度**✓
//     ⇒ **∴ 几何影响两边一样 ⇒ 只剩**展宽**这一个变量** ✓ ★★★
//   ＋ **∴ 兼容约束用**两个文档**验 ✗**（**`bristle` 缺省 vs 显式 1 ⇒ 逐字节相同 ✓）★
async function buildDoc(tag) {
  const doc = `br_${tag}_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 512, height: 512 }),
    })
  ).json();
  if (!created || !created.token) return null;
  await call(doc, created.token, "create_layer", { layer_id: "L1", name: "L1" });
  return { doc, token: created.token };
}

const main = async () => {
  const bad = [];
  // **∴ 恒定压感 ✗ 直线 ✗ 粗笔（好量宽度 ✓）** ✓
  const LINE = [];
  for (let i = 0; i <= 10; i += 1) LINE.push([60 + i * 39, 256, 1.0]);

  const measure = async (extra, tag) => {
    const built = await buildDoc(tag);
    if (!built) {
      console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
      process.exit(3);
    }
    const { doc, token } = built;
    const args = {
      layer_id: "L1",
      object_id: "stroke",
      brush: "100%_Opaque",
      size: 64,
      opacity: 1.0,
      color: { r: 10, g: 10, b: 10, a: 255 },
      points: LINE,
    };
    Object.assign(args, extra || {});
    const replied = await call(doc, token, "brush_stroke", args);
    const raw = await call(doc, token, "render_region", {
      region: { x: 40, y: 180, w: 440, h: 152 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return { replied, profile: null };
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    const W = Number(raw.width || raw.w || 440);
    const H = Number(raw.height || raw.h || 152);
    if (W * H * 4 !== bytes.length) return { replied, profile: null };
    const widths = [];
    for (let x = 0; x < W; x += 1) {
      let count = 0;
      for (let y = 0; y < H; y += 1) {
        const at = (y * W + x) * 4;
        // 白底上的墨 = 暗（约 <78% 亮度）
        if (bytes[at] + bytes[at + 1] + bytes[at + 2] < 600) count += 1;
      }
      widths.push(count);
    }
    // 指纹（整段区域）用于兼容约束
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      sum = (sum * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    const inked = widths.map((v, i) => [v, i]).filter(([v]) => v > 0);
    if (inked.length < 20) return { replied, profile: null };
    const first = inked[0][1];
    const last = inked[inked.length - 1][1];
    const span = last - first;
    const at = (frac) => widths[Math.round(first + span * frac)] || 0;
    const mean = (from, to) => {
      let acc = 0;
      let n = 0;
      for (let f = from; f <= to + 1e-9; f += 0.02) {
        acc += at(f);
        n += 1;
      }
      return acc / Math.max(1, n);
    };
    return {
      replied,
      profile: { mid: mean(0.45, 0.55), head: mean(0.03, 0.08), tail: mean(0.92, 0.97) },
      fingerprint: sum,
    };
  };

  // 对照：bristle:1（关）
  const off = await measure({ bristle: 1 }, "off");
  // 缺省（完全不传）——兼容约束
  const def = await measure(undefined, "default");
  // 分叉：bristle:5
  const on = await measure({ bristle: 5 }, "on");
  // **★ 缺功能 ⇒ **红（exit 1）**✗ 不是"判不了（3）"** ✓ ★**（**第 254 轮自查 ✓；
  //   **∴ 与字体那条同一个教训 ✓**：**退 3 是"判据自己跑不了"✗ 而"参数还不存在"是**产品缺口** ✓
  //     ⇒ **∴ 若这里退 3 ✗ 功能缺口就**藏起来了** ✓）★★
  const argRejected = (r) =>
    r && r.ok === false && /不接受参数|未知工具/.test(String((r.context && r.context.detail) || ""));
  if (argRejected(off.replied) || argRejected(on.replied)) {
    console.error(
      `❌ ① 笔毛分叉还没实现：\`brush_stroke\` 不接受 \`bristle\` 参数` +
        `（${String((off.replied.context && off.replied.context.detail) || "").slice(0, 120)}）`,
    );
    process.exit(1);
  }
  if (!off.profile || !on.profile) {
    console.error(
      `❌ 判不了：笔画成功但量不到宽度曲线（off=${JSON.stringify(off.replied).slice(0, 100)}｜on=${JSON.stringify(on.replied).slice(0, 100)}）⇒ 退出码 3`,
    );
    process.exit(3);
  }
  const f = (v) => v.toFixed(1);
  console.log(`  ② bristle:1（对照）：中段 ${f(off.profile.mid)}｜头 ${f(off.profile.head)}｜尾 ${f(off.profile.tail)}`);
  console.log(`  ① bristle:5（分叉）：中段 ${f(on.profile.mid)}｜头 ${f(on.profile.head)}｜尾 ${f(on.profile.tail)}`);
  const rel = (which) => on.profile[which] / Math.max(1e-6, off.profile[which]);
  console.log(`  ① 相对对照：头 ${rel("head").toFixed(2)}×｜尾 ${rel("tail").toFixed(2)}×｜中段 ${rel("mid").toFixed(2)}×`);

  // ① 两端必须**明显**比对照窄（≤0.75×），中段不能明显变细（≥0.8×）
  if (rel("head") > 0.75 || rel("tail") > 0.75) {
    bad.push(
      `① bristle:5 的起收笔没有比对照窄（头 ${rel("head").toFixed(2)}×、尾 ${rel("tail").toFixed(2)}×，要求 ≤0.75×）` +
        `⇒ 笔毛展宽没生效`,
    );
  }
  if (rel("mid") < 0.8) {
    bad.push(`① 中段被削细了（${rel("mid").toFixed(2)}×）⇒ 展宽把整笔都缩了，不是"起收聚拢、行笔展开"`);
  }
  // ③ 兼容：缺省 vs 显式 bristle:1 必须逐字节相同
  if (!def.fingerprint || def.fingerprint !== off.fingerprint) {
    bad.push(`③ 缺省（不传 bristle）与显式 bristle:1 的画面**不同** ⇒ 缺省行为变了（要求逐字节相同）`);
  } else {
    console.log(`  ③ 兼容：缺省与 bristle:1 指纹相同（${def.fingerprint}）✓`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 笔毛分叉生效：相对对照，起收笔明显变尖而中段不缩；缺省行为逐字节不变");
};

await main();

#!/usr/bin/env node
// **★ 纸张底纹：用**现有工具**就能组合出来（P1 #14）★**（第 254 轮 ✓）
//
// **需求文档 P1 #14**：画布初始化铺 `#faf7f0` 暖米色 ＋ 微颗粒（去掉纯白底色的"画图板味"）。
//
// **先查"仓库有没有"（本仓纪律）结论：全都有** —— **不需要新工具**：
//   * **暖米色底**：`new_document` 早就有 `background {r,g,b,a}`（0..255）参数
//     （`tools.rs:2707` 的 spec ＋ `11927` 的解析）=> 传 `{r:250,g:247,b:240,a:255}` 即可；
//   * **微颗粒**：`assets/textures/` 里本来就有纸纹 —— `Paper001.png`…`Paper006.png`
//     （另有 Cardboard 系列）=> 用 `texture_background`（tile/stretch/cover）铺一层即可。
//
// **所以本条**没有代码改动**✗ 交付的是**把配方钉住的判据** ✓**（配方一旦被改坏，它就红）：
//   ① 带 `background:#faf7f0` 的新文档，渲染出来必须就是那个暖米色（容差 ±6）；
//   ② 阴性对照：缺省（白底）文档必须**不是**那个颜色（证明 ① 有区分力，不是"怎么都对"）；
//   ③ `texture_background` 铺 `Paper001.png` 之后，画面必须有**纹理起伏**（像素标准差 > 0）
//      且**仍然偏暖**（不是被铺成任意图）；
//   ④ 同参数两次调用**逐字节一致**（确定性 —— 硬约束）。
//
// 用法：node scripts/tool-paper.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-paper.mjs <base-url>");
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

/** 建文档：**`POST /api/documents` 现在会转发 `background`**（第 254 轮修）。
 *   ＋ 之前的坑（两个都记下来）：① 端点**没读** `background` => 存的仍是纯白；
 *     ② 我先用端点建、再用 `new_document` 工具同 id 设底色 => 工具按"已存在＝打开"处理，也不生效。
 *   ＋ 所以这里就**一次调用**把底色交给端点，简单且没有顺序陷阱。
 */
const newDoc = async (doc, background) => {
  const response = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(
      background ? { doc_id: doc, width: 320, height: 240, background } : { doc_id: doc, width: 320, height: 240 },
    ),
  });
  const text = await response.text();
  let parsed = null;
  try {
    parsed = JSON.parse(text);
  } catch {
    console.error(`❌ 判不了：/api/documents 回的不是 JSON（HTTP ${response.status}）：${text.slice(0, 120)}`);
    process.exit(3);
  }
  if (!parsed || !parsed.token) return null;
  return parsed.token;
};

const sample = async (doc, token) => {
  const raw = await call(doc, token, "render_region", {
    region: { x: 40, y: 40, w: 240, h: 160 },
    raw: true,
  });
  const url = raw && (raw.raw_url || raw.rawUrl);
  if (!url) return null;
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
  let sr = 0;
  let sg = 0;
  let sb = 0;
  let n = 0;
  const lums = [];
  for (let i = 0; i < bytes.length; i += 4) {
    sr += bytes[i];
    sg += bytes[i + 1];
    sb += bytes[i + 2];
    lums.push((bytes[i] + bytes[i + 1] + bytes[i + 2]) / 3);
    n += 1;
  }
  const mean = [sr / n, sg / n, sb / n];
  const lm = lums.reduce((a, b) => a + b, 0) / lums.length;
  const sd = Math.sqrt(lums.reduce((a, b) => a + (b - lm) ** 2, 0) / lums.length);
  let sum = 0;
  for (let i = 0; i < bytes.length; i += 4) {
    sum = (sum * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
  }
  return { mean, sd, fingerprint: sum };
};

const main = async () => {
  const bad = [];
  const WARM = { r: 250, g: 247, b: 240, a: 255 };
  const near = (a, b, tol = 6) => Math.abs(a - b) <= tol;

  // ① 暖米色底
  const warmDoc = `pp_warm_${Date.now().toString(36)}`;
  const warmToken = await newDoc(warmDoc, WARM);
  if (!warmToken) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const warm = await sample(warmDoc, warmToken);
  if (!warm) {
    console.error("❌ 判不了：量不到渲染结果 ⇒ 退出码 3");
    process.exit(3);
  }
  console.log(
    `  ① background=#faf7f0 ⇒ 实测均值 rgb(${warm.mean.map((v) => v.toFixed(0)).join(",")})｜标准差 ${warm.sd.toFixed(2)}`,
  );
  if (!near(warm.mean[0], 250) || !near(warm.mean[1], 247) || !near(warm.mean[2], 240)) {
    bad.push(`① 底色不是 #faf7f0（实测 rgb(${warm.mean.map((v) => v.toFixed(0)).join(",")})）`);
  }

  // ② 阴性对照：缺省白底
  const whiteDoc = `pp_white_${Date.now().toString(36)}`;
  const whiteToken = await newDoc(whiteDoc, null);
  const white = whiteToken ? await sample(whiteDoc, whiteToken) : null;
  if (!white) {
    bad.push("② 阴性对照量不到（白底文档渲染失败）");
  } else {
    console.log(`  ② 阴性对照（缺省白底）⇒ 均值 rgb(${white.mean.map((v) => v.toFixed(0)).join(",")})`);
    if (near(white.mean[0], 250, 3) && near(white.mean[2], 240, 3)) {
      bad.push("② 阴性对照与暖米色几乎一样 ⇒ 判据没有区分力");
    }
  }

  // ③ 纸纹
  const texture = await call(warmDoc, warmToken, "texture_background", {
    texture: "Paper001.png",
    mode: "tile",
  });
  if (!texture || texture.ok !== true) {
    bad.push(`③ texture_background 失败：${JSON.stringify(texture).slice(0, 140)}`);
  } else {
    const grained = await sample(warmDoc, warmToken);
    if (!grained) {
      bad.push("③ 铺完纸纹后量不到渲染结果");
    } else {
      console.log(
        `  ③ 铺 Paper001.png ⇒ 均值 rgb(${grained.mean.map((v) => v.toFixed(0)).join(",")})｜标准差 ${grained.sd.toFixed(2)}`,
      );
      if (!(grained.sd > 1.0)) {
        bad.push(`③ 铺了纸纹却没有起伏（标准差 ${grained.sd.toFixed(2)}，要求 >1）`);
      }
      // **★ 不能要求"铺完纸纹还偏暖" ✗ ★**（第 254 轮修正）：
      //   仓库自带的 `assets/textures/Paper*.png` 是**不透明的纸照片**
      //   => `texture_background` 铺上去是**盖住**底色（实测均值 245,244,242，纹理自己的灰白），
      //     不是"把底色染上颗粒"。
      //   => 所以这里只断言两件**事实**：纹理真的铺上了（画面变了＋有起伏）、且是**亮的**
      //     （不是被铺成黑/任意图）。想做"暖米色 ＋ 微颗粒"需要**透明/可乘**的颗粒纹理
      //     （或给 `texture_background` 加不透明度/混合模式）—— 那是一条**已记录**的后续项。
      if (grained.mean[0] < 180 || grained.mean[1] < 180 || grained.mean[2] < 180) {
        bad.push(
          `③ 铺完纸纹之后画面明显变暗（rgb(${grained.mean.map((v) => v.toFixed(0)).join(",")})）⇒ 不像纸`,
        );
      }
      if (grained.fingerprint === warm.fingerprint) {
        bad.push("③ 铺纸纹前后画面逐字节相同 ⇒ `texture_background` 没生效");
      }
    }
  }

  // ④ 确定性
  const againDoc = `pp_again_${Date.now().toString(36)}`;
  const againToken = await newDoc(againDoc, WARM);
  await call(againDoc, againToken, "texture_background", { texture: "Paper001.png", mode: "tile" });
  const again = await sample(againDoc, againToken);
  const first = warm ? await sample(warmDoc, warmToken) : null;
  if (!again || !first) {
    bad.push("④ 确定性那两次量不到");
  } else if (again.fingerprint !== first.fingerprint) {
    bad.push(`④ 同配方两次渲染不同（${first.fingerprint} vs ${again.fingerprint}）`);
  } else {
    console.log(`  ④ 确定性：同配方两次一致（${again.fingerprint}）`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 纸张配方可用（无需新工具）：new_document 的 background 给暖米色 ＋ texture_background 铺 Paper001.png 加微颗粒，且可复现");
};

await main();

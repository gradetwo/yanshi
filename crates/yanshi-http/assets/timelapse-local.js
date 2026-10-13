/**
 * **★ 页面侧：把当前工程导出成**绘画过程视频**✗ ★**（**第 249 轮 ✓）
 *
 * **∴ 形态 ✗**：**传统脚本 ✗ 只往 `globalThis` 挂 `YanshiTimelapse`** ✓
 *   ＋ **∴ 为什么不是 ESM ✗**：**本仓页面**没有模块加载器**✗（`viewer.rs` 把
 *     `viewer-app.js` 整段 `include_str!` 内联进一段 `<script>` ✓）⇒ **∴ `export` 会**语法错** ✓
 *       ＋ **∴ 交付包那份 `pwa/yanshi-timelapse.js` 就是这么写的 ✗ 在本仓页面里装不进去 ✓ ★★
 *
 * **∴ 与交付版的三处不同（**都是本仓的实际情况 ✓）✗**：
 *   ＋ **∴ ① 尺寸 ✗**：**取**文档真实尺寸**✗ 不再写死 1920×1080** ✓
 *     （**∴ 交付版拿输出尺寸当画布尺寸 ⇒ **4K 工程只会导出左上角一块** ✓）★
 *   ＋ **∴ ② atom 种类 ✗**：**一个都**不过滤**✗ 全喂给内核** ✓
 *     （**∴ 交付版只认 `import_image` ＋ `brush_stroke` ✗ 而**本仓的手绘笔触叫
 *       `draw_stroke`** ⇒ **∴ 那样导出**一笔都没有** ✓）★★
 *   ＋ **∴ ③ 封装 ✗**：**用同仓的 `mp4-lite.js`**✗ 不用 `mp4-muxer`** ✓
 *     （**∴ 页面没有 ESM ✗ 裸模块名无从解析 ✓）★
 *
 * **∴ 编码细节（**一个容易踩的坑 ✓）✗**：**H.264 必须显式要 **AVCC** 格式**
 *   （`avc: { format: "avc" }` ✓）—— **∴ 默认可能是**Annex-B**（**带起始码 ✓）
 *     ⇒ **∴ 而**MP4 的样本要的是**长度前缀**✗ 且 `avcC` 描述的是 AVCC** ✓
 *       ⇒ **∴ 要错了 ⇒ 文件能生成 ✗ 但**解不出来** ✓ ★★
 */

(function () {
  "use strict";

  /** **默认每多少个 atom 出一帧 ✗**（**∴ 与原生 CLI 同一默认 ✓）★ */
  const DEFAULT_FRAME_EVERY = 10;
  /** **默认帧率 ✓。 */
  const DEFAULT_FPS = 30;
  /** **内核内存预算 ✗**（**∴ 与页面主内核同一档再放大些 ✓）★ */
  const KERNEL_MEMORY = 2 * 1024 * 1024 * 1024;
  /** **每帧渲染出来的 RGBA 是不透明的 ✗ 编码前合成到白底** ✓（**∴ 否则透明变黑 ✓）★ */
  const WHITE = 255;

  /** **中止错误 ✗**（**∴ 与"失败"分开 ✗ 便于 UI 区分 ✓）★ */
  class Cancelled extends Error {
    constructor() {
      super("已取消");
      this.name = "Cancelled";
    }
  }

  const throwIfAborted = (signal) => {
    if (signal && signal.aborted) throw new Cancelled();
  };

  /**
   * **导出绘画过程视频 ✗**
   *
   * @param {object} opts
   * @param {Function} opts.WasmKernel 页面已加载的 `WasmKernel` 类 ✓（**必填**）
   * @param {string} opts.docId 文档 id ✓
   * @param {number} opts.width 文档真实宽度 ✓（**必填 ⇒ 不再猜**）
   * @param {number} opts.height 文档真实高度 ✓
   * @param {Array} opts.atoms 全部 atom ✓（**按 seq 升序 ✗ 不筛种类 ✓）
   * @param {Function} opts.fetchBlob `async (hash) => Uint8Array|null` ✓
   * @param {number} [opts.frameEvery=10] 每 N 个 atom 一帧 ✓
   * @param {number} [opts.fps=30] ✓
   * @param {Function} [opts.onProgress] `(done, total, phase)` ✓
   * @param {AbortSignal} [opts.signal] 取消 ✓
   * @returns {Promise<{blob: Blob, frames: number, ms: number}>}
   */
  async function exportTimelapse(opts) {
    const {
      WasmKernel,
      docId,
      width,
      height,
      atoms,
      fetchBlob,
      frameEvery = DEFAULT_FRAME_EVERY,
      fps = DEFAULT_FPS,
      onProgress = () => {},
      signal = null,
    } = opts || {};

    if (typeof WasmKernel !== "function") {
      // **∴ 实测踩到 ✗**：**页面内核**还没就绪**时 `state.wasm` 是 `null`** ✓
      //   ⇒ **∴ 与其抛一句**读属性失败**✗ 不如**说人话 ✗ 并给出下一步** ✓ ★
      throw new Error("页面的 WASM 内核还没就绪（等它加载完再点，或刷新一次）");
    }
    if (!width || !height) throw new Error("缺少文档真实尺寸（width/height）");
    if (!Array.isArray(atoms) || atoms.length === 0) throw new Error("这个工程里没有 atom");
    const VideoEncoderCtor = globalThis.VideoEncoder;
    if (typeof VideoEncoderCtor !== "function") {
      const error = new Error("这个浏览器不支持 WebCodecs（VideoEncoder）⇒ 换 Chrome/Edge/Safari 16.4+");
      error.code = "webcodecs_unsupported";
      throw error;
    }

    // **★★★ 选一个**这台机器真能编**的编解码器 ✗ ★★★**（**第 250 轮 ✓）
    //
    // **∴ 为什么要"选"而不是直接要 H.264 ✗**（**实测 ✓）★**：
    //   ＋ **∴ CI 的浏览器是 `setup-chrome` 装的 **Chrome for Testing**✗
    //     ⇒ **∴ 它**不带专有编解码器** ⇒ `avc1.640028` 的 `isConfigSupported`
    //       **返回 unsupported** ✓（**∴ 而** headless Linux **没有 GPU** ⇒ **也没有
    //         硬件编码器可退 ✓）⇒ **∴ 于是**：**那条路**在 CI 里根本走不通** ✓
    //   ＋ **∴ 而** VP8／VP9 是 Chromium **一定自带**的（非专有 ✓）
    //     ⇒ **∴ 任何 Chromium／Firefox／Safari 都能编** ✓ ★★
    //   ＋ **∴ 且**交付说明自己就写了"Firefox 较晚 ⇒ 需 fallback" ✓
    //     ⇒ **∴ 所以**这条路**不是权宜 ✗ 是**必需** ✓ ★★★
    //
    // **∴ 取舍（**两面 ✓）**：**MP4／H.264** ⇒ 兼容性最好 ✓ 体积小 ✓（**能用就用 ✓）；
    //   **WebM／VP9** ⇒ 哪都能编 ✓ 但部分老播放器／剪辑软件要转一道 ✗ ⇒ **∴ 兜底 ✓** ★
    const wantCodec = String(
      (opts && opts.codec) || globalThis.__YANSHI_TIMELAPSE_CODEC || "auto",
    ).toLowerCase();
    const CODECS = [
      {
        key: "avc",
        codec: "avc1.640028",
        container: "mp4",
        mime: "video/mp4",
        ext: "mp4",
        extra: { avc: { format: "avc" } },
        muxer: "YanshiMp4",
      },
      {
        key: "vp9",
        codec: "vp09.00.10.08",
        container: "webm",
        mime: "video/webm",
        ext: "webm",
        extra: {},
        muxer: "YanshiWebm",
        webmCodec: "V_VP9",
      },
      {
        key: "vp8",
        codec: "vp8",
        container: "webm",
        mime: "video/webm",
        ext: "webm",
        extra: {},
        muxer: "YanshiWebm",
        webmCodec: "V_VP8",
      },
    ];
    const candidates =
      wantCodec === "auto" ? CODECS : CODECS.filter((c) => c.key === wantCodec);
    if (!candidates.length) throw new Error(`不认识的 codec：${wantCodec}`);
    const probeSupported = async (entry) => {
      if (typeof VideoEncoderCtor.isConfigSupported !== "function") return true;
      try {
        const probe = await VideoEncoderCtor.isConfigSupported({
          codec: entry.codec,
          width,
          height,
          bitrate: 8_000_000,
          framerate: fps,
          ...entry.extra,
        });
        return Boolean(probe && probe.supported);
      } catch (error) {
        console.warn(`[timelapse] 探测 ${entry.codec} 时出错`, error);
        return false;
      }
    };
    let chosen = null;
    const tried = [];
    for (const entry of candidates) {
      if (typeof globalThis[entry.muxer] !== "function") {
        tried.push(`${entry.key}（缺封装器 ${entry.muxer}）`);
        continue;
      }
      if (await probeSupported(entry)) {
        chosen = entry;
        break;
      }
      tried.push(entry.key);
    }
    if (!chosen) {
      const error = new Error(
        `这台机器没有一个可用的视频编码器（试过：${tried.join("／")}）⇒ 换一台或换浏览器再试`,
      );
      error.code = "no_encoder";
      throw error;
    }
    console.log(
      `[timelapse] 编码器：${chosen.codec}（${chosen.container}）` +
        (tried.length ? `｜跳过了 ${tried.join("／")}` : ""),
    );

    const startedAt = Date.now();
    const every = Math.max(1, Math.floor(frameEvery));
    const total = atoms.length;

    // **∴ ① 独立内核 ✗**：**不碰用户正在画的那个** ✓（**∴ 交付版这一点做对了 ✗ 保留 ✓）★
    onProgress(0, total, "准备");
    const kernel = new WasmKernel(
      `timelapse-${docId || "doc"}-${Date.now()}`,
      256,
      width,
      height,
      KERNEL_MEMORY,
    );

    // **∴ ② 先把所有 blob 放进去 ✗**：**它们**按内容哈希寻址**✗ 先放先能用 ✓**
    const hashes = new Set();
    for (const atom of atoms) {
      const hash = atom && atom.payload && atom.payload.bitmap && atom.payload.bitmap.blob_hash;
      if (typeof hash === "string" && hash) hashes.add(hash.replace(/^sha256:/, ""));
    }
    let loaded = 0;
    for (const hash of hashes) {
      throwIfAborted(signal);
      try {
        const bytes = await fetchBlob(hash);
        if (bytes && bytes.length) kernel.blob_put(bytes);
      } catch (error) {
        console.warn(`[timelapse] blob ${hash.slice(0, 8)} 取不到 ⇒ 这一笔会缺内容`, error);
      }
      loaded += 1;
      onProgress(loaded, hashes.size, "取位图");
    }

    // **∴ ③ 编码器 ＋ 封装器 ✗**：**`avcC` 要等第一块编码结果才知道**✗
    //    ⇒ **∴ 于是**：**封装器**延迟创建** ✓（**在第一个 `output` 里 ✓）★★
    const chunks = [];
    let muxer = null;
    let dropped = 0;
    let lastDuration = Math.round(1_000_000 / fps);
    let description = null;

    const frames = [];
    const encoder = new VideoEncoderCtor({
      output: (chunk, metadata) => {
        const bytes = new Uint8Array(chunk.byteLength);
        chunk.copyTo(bytes);
        frames.push({ bytes, timestamp: chunk.timestamp, type: chunk.type });
        if (!description && metadata && metadata.decoderConfig && metadata.decoderConfig.description) {
          const d = metadata.decoderConfig.description;
          description = d instanceof Uint8Array ? d : new Uint8Array(d);
        }
      },
      error: (error) => {
        dropped += 1;
        console.error("[timelapse] 编码器报错:", error);
      },
    });
    encoder.configure({
      codec: chosen.codec,
      width,
      height,
      bitrate: 8_000_000,
      framerate: fps,
      // **∴ 见文件头：要 H.264 时必须显式要 AVCC ✗ 否则 MP4 里的样本是
      //   Annex-B ⇒ 解不出来 ✓（**∴ WebM 那两档没有这个字段 ✓）★★
      ...chosen.extra,
    });

    // **∴ ④ 逐 atom 重放 ✗ 每 N 个渲一帧 ✓** ★
    let encoded = 0;
    const canvas = new Uint8Array(width * height * 4);
    for (let i = 0; i < total; i++) {
      throwIfAborted(signal);
      const atom = atoms[i];
      try {
        kernel.apply_atom_json(typeof atom === "string" ? atom : JSON.stringify(atom));
      } catch (error) {
        console.warn(`[timelapse] 第 ${i} 个 atom 没被内核接受`, error);
      }
      const isLast = i === total - 1;
      if ((i + 1) % every === 0 || isLast) {
        const rgba = kernel.render_region_rgba(0, 0, width, height);
        // **∴ 透明像素要合到白底 ✗**（**∴ 否则编码出来是黑的 ✓）★
        for (let p = 0; p < canvas.length; p += 4) {
          const a = rgba[p + 3];
          if (a === WHITE) {
            canvas[p] = rgba[p];
            canvas[p + 1] = rgba[p + 1];
            canvas[p + 2] = rgba[p + 2];
            canvas[p + 3] = 255;
          } else {
            const f = a / 255;
            canvas[p] = Math.round(rgba[p] * f + WHITE * (1 - f));
            canvas[p + 1] = Math.round(rgba[p + 1] * f + WHITE * (1 - f));
            canvas[p + 2] = Math.round(rgba[p + 2] * f + WHITE * (1 - f));
            canvas[p + 3] = 255;
          }
        }
        const timestamp = Math.round((encoded * 1_000_000) / fps);
        const frame = new VideoFrame(canvas, {
          format: "RGBA",
          codedWidth: width,
          codedHeight: height,
          timestamp,
        });
        // **∴ 每 2 秒一个关键帧 ✗**（**∴ 与交付版同一策略 ✓ 便于拖动 ✓）★
        encoder.encode(frame, { keyFrame: encoded % (fps * 2) === 0 });
        frame.close();
        encoded += 1;
        onProgress(i + 1, total, "渲染");
      }
      // **∴ 每 8 帧让一次主线程 ✗**（**∴ 否则进度条不动 ✗ 取消也点不动 ✓）★
      if (i % 8 === 7) await new Promise((resolve) => setTimeout(resolve, 0));
    }

    onProgress(total, total, "收尾");
    await encoder.flush();
    encoder.close();

    if (dropped > 0) {
      console.warn(`[timelapse] 有 ${dropped} 次编码报错 ⇒ 视频可能不完整`);
    }
    if (!frames.length) throw new Error("一帧都没编出来 ⇒ 拿不到视频");

    // **∴ ⑤ 组装 ✗**：**时长由相邻时间戳相减 ✗ 最后一帧沿用上一段 ✓** ★
    //   ＋ **∴ 两个容器的**时间单位不同 ✗**（**MP4 用 timescale 刻度 ✓
    //     WebM 用**毫秒**（TimestampScale=1e6 ✓））⇒ **∴ 所以**分两条各自算 ✓ ★★
    let finished;
    if (chosen.container === "mp4") {
      muxer = new globalThis.YanshiMp4({
        width,
        height,
        timescale: 1_000_000,
        description,
      });
      for (let i = 0; i < frames.length; i++) {
        const current = frames[i];
        let duration = lastDuration;
        if (i + 1 < frames.length) {
          duration = Math.max(1, frames[i + 1].timestamp - current.timestamp);
          lastDuration = duration;
        }
        muxer.addSample(current.bytes, duration, current.type === "key");
      }
      finished = muxer.finish();
    } else {
      muxer = new globalThis.YanshiWebm({
        width,
        height,
        codec: chosen.webmCodec,
        fps,
      });
      for (const current of frames) {
        muxer.addSample(current.bytes, Math.round(current.timestamp / 1000), current.type === "key");
      }
      finished = muxer.finish();
    }
    void chunks;
    return {
      blob: new Blob([finished], { type: chosen.mime }),
      frames: frames.length,
      ms: Date.now() - startedAt,
      container: chosen.container,
      codec: chosen.codec,
    };
  }

  /** **导出并下载 ✗**（**∴ UI 用的就是它 ✓）★ */
  async function exportAndDownload(opts) {
    const result = await exportTimelapse(opts);
    // **∴ 扩展名必须跟**实际容器**一致 ✗**（**∴ WebM 存成 .mp4**会让播放器按错
    //   容器去解 ✓）⇒ **∴ 用 `result.container` 决定 ✓ ★★
    const ext = result.container === "webm" ? "webm" : "mp4";
    const name =
      (opts && opts.filename) ||
      `timelapse-${(opts && opts.docId) || "doc"}-${Date.now()}.${ext}`;
    const url = URL.createObjectURL(result.blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = name;
    document.body.appendChild(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
    return { ...result, filename: name };
  }

  globalThis.YanshiTimelapse = { exportTimelapse, exportAndDownload, Cancelled };
})();

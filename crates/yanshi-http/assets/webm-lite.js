/**
 * **★ yanshi 的**极简 WebM 封装器** ✗ ★**（**第 250 轮 ✓；**视频、单轨、VP8／VP9 ✓）
 *
 * **∴ 为什么需要它 ✗**（**这不是"退而求其次" ✗ 是**必须有的兜底** ✓）★★★**：
 *   ＋ **∴ 实测 ✗**：**CI 的浏览器是 `browser-actions/setup-chrome` 装出来的
 *     **Chrome for Testing**✗ ⇒ **∴ 它**不带专有编解码器** ✓ ⇒ **∴ `avc1.640028`
 *     **`isConfigSupported ⇒ unsupported`** ✓（**∴ 而** headless Linux **又没有 GPU**
 *       ⇒ **∴ 也没有**硬件编码器可退** ✓）
 *   ＋ **∴ 而**交付说明自己就写了：**Firefox 130+ **较晚 ✗ 需 fallback** ✓** ⇒
 *     **∴ 所以**：**一条**不依赖专有编解码器**的路**本来就是需求** ✓ ★★
 *   ＋ **∴ VP8／VP9 是 Chromium **一定自带**的（非专有 ✓）⇒ **∴ 任何 Chromium／
 *     Firefox／Safari 都能编** ⇒ **∴ 于是**：**这条路**在 CI 里**真能跑** ✓ ★★★
 *
 * **∴ 形态与 `mp4-lite.js` 一致 ✗**：**传统脚本 ✗ 只往 `globalThis` 挂 `YanshiWebm`** ✓
 *   ⇒ **∴ 浏览器内联可用 ✗ node `await import()` 也能测** ✓
 *
 * **∴ 代价（**如实 ✓）✗**：**WebM 这一路**没有 MP4 那么好的兼容性**（**部分老播放器／
 *   剪辑软件要转一道 ✓）✗ 也没有音频／字幕／多轨** ✓ ⇒ **∴ 所以**：**有 H.264 时
 *     仍然**优先 MP4**✗ 这一路只做兜底** ✓ ★★
 *
 * **∴ 为什么自己写 ✗**：**与 `mp4-lite` 同一个理由 ✗**（**页面没有模块加载器 ⇒
 *   ESM 库装不进去 ✓）** ＋ **∴ 本仓已有手写 tar／PNG／MP4 的先例** ✓ ★★
 */

/** **EBML 的整数写法 ✗**（**`value` 用最小的长度 ✓；**全 1 是保留值 ⇒ 避开 ✓）★ */
function vint(value) {
  for (let len = 1; len <= 8; len++) {
    // **∴ 保留值 ✗**：**长度 `len` 的 VINT 里，所有值位全 1 **是"未知长度"** ✓
    //   ⇒ **∴ 所以**：**能表示的最大**普通值**是 `2^(7*len) - 2`** ✓
    const max = Math.pow(2, 7 * len) - 2;
    if (value <= max) {
      const bytes = new Uint8Array(len);
      let rest = value;
      for (let i = len - 1; i >= 0; i--) {
        bytes[i] = rest & 0xff;
        rest = Math.floor(rest / 256);
      }
      bytes[0] |= 1 << (8 - len); // **∴ 长度标记位 ✓**
      return bytes;
    }
  }
  throw new Error("VINT 太长：" + value);
}

const concat = (parts) => {
  let total = 0;
  for (const p of parts) total += p.length;
  const out = new Uint8Array(total);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
};

const u8 = (v) => Uint8Array.from([v & 0xff]);

/** **EBML 元素 ✗**：`id` 是**带长度位的原样字节**✓（**比如 `0x1A45DFA3` ⇒ 4 字节 ✓）★ */
function element(idBytes, payload) {
  return concat([idBytes, vint(payload.length), payload]);
}

const ID = {
  EBML: Uint8Array.from([0x1a, 0x45, 0xdf, 0xa3]),
  EBMLVersion: Uint8Array.from([0x42, 0x86]),
  EBMLReadVersion: Uint8Array.from([0x42, 0xf7]),
  EBMLMaxIDLength: Uint8Array.from([0x42, 0xf2]),
  EBMLMaxSizeLength: Uint8Array.from([0x42, 0xf3]),
  DocType: Uint8Array.from([0x42, 0x82]),
  DocTypeVersion: Uint8Array.from([0x42, 0x87]),
  DocTypeReadVersion: Uint8Array.from([0x42, 0x85]),
  Segment: Uint8Array.from([0x18, 0x53, 0x80, 0x67]),
  Info: Uint8Array.from([0x15, 0x49, 0xa9, 0x66]),
  TimestampScale: Uint8Array.from([0x2a, 0xd7, 0xb1]),
  MuxingApp: Uint8Array.from([0x4d, 0x80]),
  WritingApp: Uint8Array.from([0x57, 0x41]),
  Duration: Uint8Array.from([0x44, 0x89]),
  Tracks: Uint8Array.from([0x16, 0x54, 0xae, 0x6b]),
  TrackEntry: Uint8Array.from([0xae]),
  TrackNumber: Uint8Array.from([0xd7]),
  TrackUID: Uint8Array.from([0x73, 0xc5]),
  TrackType: Uint8Array.from([0x83]),
  FlagLacing: Uint8Array.from([0x9c]),
  CodecID: Uint8Array.from([0x86]),
  DefaultDuration: Uint8Array.from([0x23, 0xe3, 0x83]),
  Video: Uint8Array.from([0xe0]),
  PixelWidth: Uint8Array.from([0xb0]),
  PixelHeight: Uint8Array.from([0xba]),
  Cluster: Uint8Array.from([0x1f, 0x43, 0xb6, 0x75]),
  Timestamp: Uint8Array.from([0xe7]),
  SimpleBlock: Uint8Array.from([0xa3]),
};

/** **把非负整数写成 EBML 整数 ✗**（**去掉多余的前导 0 ✓）★ */
function uintBytes(value) {
  if (value === 0) return u8(0);
  const bytes = [];
  let rest = value;
  while (rest > 0) {
    bytes.unshift(rest & 0xff);
    rest = Math.floor(rest / 256);
  }
  return Uint8Array.from(bytes);
}

const text = (s) => {
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i) & 0xff;
  return out;
};

/** **★ 极简 WebM 封装器 ✗ ★**（**视频、单轨、VP8／VP9 ✓）
 *
 * **用法 ✗**：
 * ```js
 * const muxer = new YanshiWebm({ width, height, codec: "V_VP9", fps: 30 });
 * muxer.addSample(bytes, timestampMs, isKey);
 * const webm = muxer.finish();          // Uint8Array
 * ```
 * ＋ **∴ `timestampMs` 是**毫秒整数**✗**（**WebM 的 TimestampScale 就取 1 ms ✓）
 *   ⇒ **∴ 调用方**把微秒域的值除一千交进来** ✓ ★
 */
class YanshiWebm {
  constructor(options) {
    const opts = options || {};
    this.width = opts.width || 0;
    this.height = opts.height || 0;
    /** **`V_VP8` 或 `V_VP9`** ✓。
     */
    this.codec = opts.codec === "V_VP8" ? "V_VP8" : "V_VP9";
    this.fps = opts.fps || 30;
    this.samples = [];
  }

  /** **加一帧 ✗**（**`timestampMs` 必须**单调不减**✓；**重复时间戳**允许**✓）★ */
  addSample(bytes, timestampMs, isKey) {
    const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    this.samples.push({ data, ts: Math.max(0, Math.round(timestampMs)), key: Boolean(isKey) });
    return this;
  }

  #header() {
    const ebml = element(
      ID.EBML,
      concat([
        element(ID.EBMLVersion, u8(1)),
        element(ID.EBMLReadVersion, u8(1)),
        element(ID.EBMLMaxIDLength, u8(4)),
        element(ID.EBMLMaxSizeLength, u8(8)),
        element(ID.DocType, text("webm")),
        element(ID.DocTypeVersion, u8(2)),
        element(ID.DocTypeReadVersion, u8(2)),
      ]),
    );
    const info = element(
      ID.Info,
      concat([
        // **∴ TimestampScale = 1 000 000 ns ✗**（**＝ 1 ms ✓ ⇒ **∴ 时间戳单位是毫秒** ✓）★
        element(ID.TimestampScale, uintBytes(1_000_000)),
        element(ID.MuxingApp, text("yanshi-webm-lite")),
        element(ID.WritingApp, text("yanshi-webm-lite")),
        // **∴ Duration ✗**：**浮点 8 字节 ✓（**最后一个时间戳 ＋ 一帧时长 ✓）★
        element(
          ID.Duration,
          (() => {
            const last = this.samples.length ? this.samples[this.samples.length - 1].ts : 0;
            const buffer = new ArrayBuffer(8);
            new DataView(buffer).setFloat64(0, last + 1000 / this.fps);
            return new Uint8Array(buffer);
          })(),
        ),
      ]),
    );
    const tracks = element(
      ID.Tracks,
      element(
        ID.TrackEntry,
        concat([
          element(ID.TrackNumber, u8(1)),
          element(ID.TrackUID, uintBytes(1)),
          element(ID.TrackType, u8(1)), // 1 ＝ 视频 ✓
          element(ID.FlagLacing, u8(0)),
          element(ID.CodecID, text(this.codec)),
          element(ID.DefaultDuration, uintBytes(Math.round(1e9 / this.fps))),
          element(
            ID.Video,
            concat([
              element(ID.PixelWidth, uintBytes(this.width)),
              element(ID.PixelHeight, uintBytes(this.height)),
            ]),
          ),
        ]),
      ),
    );
    return { ebml, info, tracks };
  }

  /** **出文件 ✗**（**返回 `Uint8Array` ✓）★
   *
   * **∴ 为什么可以先算长度 ✗**：**样本已经在内存里**✗（**与 `mp4-lite` 同样的取舍 ✓）
   *   ⇒ **∴ 于是**：**每个元素的**长度都已知**✗ ⇒ **∴ 不需要"未知长度"那种直播写法** ✓
   *     （**∴ 它**虽然合法 ✗ 但**有些老播放器**处理得不好 ✓）★
   */
  finish() {
    const { ebml, info, tracks } = this.#header();
    // **∴ 按关键帧切 Cluster ✗**（**∴ 也是 seek 的最小单位 ✓）★
    const clusters = [];
    let current = null;
    for (const sample of this.samples) {
      if (!current || (sample.key && current.blocks.length > 0)) {
        current = { ts: sample.ts, blocks: [] };
        clusters.push(current);
      }
      // **∴ SimpleBlock ✗**：`[trackNumber=1][int16 相对时间戳][flags][帧字节]` ✓
      const relative = Math.max(-32768, Math.min(32767, sample.ts - current.ts));
      const head = new Uint8Array(4);
      head[0] = 0x81; // track 1 的 VINT ✓
      head[1] = (relative >> 8) & 0xff;
      head[2] = relative & 0xff;
      head[3] = sample.key ? 0x80 : 0x00;
      current.blocks.push(
        element(ID.SimpleBlock, concat([head, sample.data])),
      );
    }
    const clusterBytes = clusters.map((cluster) =>
      element(
        ID.Cluster,
        concat([element(ID.Timestamp, uintBytes(cluster.ts)), ...cluster.blocks]),
      ),
    );
    const segment = element(ID.Segment, concat([info, tracks, ...clusterBytes]));
    return concat([ebml, segment]);
  }
}

globalThis.YanshiWebm = YanshiWebm;

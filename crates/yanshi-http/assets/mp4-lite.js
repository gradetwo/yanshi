/**
 * **★ yanshi 的**极简 MP4 封装器** ✗ ★**（**第 249 轮 ✓；**视频、H.264、无音频 ✓）
 *
 * **∴ 为什么要自己写 ✗**（**而**不是 `npm i mp4-muxer` ✓）★**：
 *   ＋ **∴ 本仓的前端**没有打包器**✗：`viewer-app.js` 是 `include_str!` **整段内联**
 *     进页面的**（`viewer.rs` 的 `page_template` ✓）⇒ **∴ 于是**：
 *        **∴ ①** 页内**没有模块加载器**✗ ⇒ **`import … from 'mp4-muxer'` 这种**裸模块名**
 *           在页面里**根本不成立** ✓
 *        **∴ ②** 而**把一个 ESM 库**内联**进传统 `<script>`**✗ ⇒ **它的 `export` 会**直接报错** ✓
 *          ⇒ **∴ 所以**：**要么**引入打包器（**∴ 本仓不要 ✓）✗ 要么**自己写一份小的 ✓ ★★
 *   ＋ **∴ 而**本仓**已经在**手写格式**✗：`archive.rs` 手写 tar ✓ `png.rs` 手写 PNG ✓
 *     ⇒ **∴ 于是**：**MP4 也只是**一种盒子格式**✗ 视频这一路**够小** ✓ ★★
 *   ＋ **∴ 代价（**如实 ✓）✗**：**只支持**视频、单轨、H.264（`avc1` ✓）✗ 不写音频／字幕／
 *     多轨** ✓ ⇒ **∴ 够**本功能用**✗ 但**不是**通用 MP4 库** ✓ ★★
 *
 * **∴ 怎么被两边用 ✗**：
 *   ＋ **∴ 浏览器 ✗**：**传统 `<script>` 内联**✗ ⇒ **∴ 它**只往 `globalThis` 挂一个名字** ✓
 *   ＋ **∴ node ✗**：**`await import()` 这个文件**✗ ⇒ **∴ 顶层赋值会执行**✗
 *     ⇒ **∴ 测试**从 `globalThis` 取** ✓（**∴ 于是**一份代码**两边都能测 ✓）★★
 */

/** **把 big-endian 写进去的游标 ✗**（**∴ MP4 全是 BE ✓）★ */
class Writer {
  constructor() {
    this.bytes = [];
  }

  u8(value) {
    this.bytes.push(value & 0xff);
  }

  u16(value) {
    this.u8(value >> 8);
    this.u8(value);
  }

  u24(value) {
    this.u8(value >> 16);
    this.u8(value >> 8);
    this.u8(value);
  }

  u32(value) {
    this.u8(Math.floor(value / 0x1000000));
    this.u8(value >> 16);
    this.u8(value >> 8);
    this.u8(value);
  }

  /** **无符号 64 位 ✗**（**JS 的安全整数够用到 8 PB ✓）★ */
  u64(value) {
    this.u32(Math.floor(value / 0x100000000));
    this.u32(value >>> 0);
  }

  raw(bytes) {
    for (const b of bytes) this.bytes.push(b & 0xff);
  }

  fourcc(text) {
    for (let i = 0; i < 4; i++) this.u8(text.charCodeAt(i));
  }

  zeros(count) {
    for (let i = 0; i < count; i++) this.u8(0);
  }

  get length() {
    return this.bytes.length;
  }

  /** **取一段的字节 ✗**（**供盒子计算长度前先写好内容 ✓）★ */
  slice(from, to) {
    return this.bytes.slice(from, to === undefined ? this.bytes.length : to);
  }

}

/** **写一个盒子 ✗**：**先把内容写到临时游标 ✓ 再补 8 字节头 ✓）★
 *
 *  **∴ 为什么先算内容 ✗**：**盒子的长度**不含头里的那 8 字节**✗（**`size` 是含头的总长 ✓）
 *    ⇒ **∴ 于是**：**先写内容 ⇒ 长度已知 ⇒ 再把头插到前面 ✓** ★
 */
function box(writer, type, fill) {
  const inner = new Writer();
  fill(inner);
  writer.u32(inner.length + 8);
  writer.fourcc(type);
  writer.raw(inner.slice(0));
}

/** **完整盒 ✗**（**内容已经写好时用 ✓）★ */
function fullBox(writer, type, version, flags, fill) {
  box(writer, type, (w) => {
    w.u8(version);
    w.u24(flags);
    fill(w);
  });
}

/** **★ 极简 MP4 封装器 ✗ ★**（**视频、单轨、H.264 ✓）
 *
 *  **用法 ✗**：
 *  ```js
 *  const muxer = new YanshiMp4({ width, height, timescale: 90000, description: avcC });
 *  muxer.addSample(bytes, durationTicks, isKey);
 *  const mp4 = muxer.finish();          // Uint8Array
 *  ```
 *  ＋ **∴ `description` ✗**：**来自 `VideoEncoder` 的
 *    `metadata.decoderConfig.description`**（**就是 `avcC` 的内容 ✗ 不含盒子头 ✓）★
 */
class YanshiMp4 {
  constructor(options) {
    const opts = options || {};
    this.width = opts.width || 0;
    this.height = opts.height || 0;
    this.timescale = opts.timescale || 90000;
    /** **`avcC` 的**盒内内容**✗**（**∴ 不含 `avcC` 那 8 字节头 ✓）★ */
    this.description = opts.description ? new Uint8Array(opts.description) : null;
    this.samples = [];
    this.totalTicks = 0;
  }

  /**
   * **加一帧 ✗**（**`durationTicks` 用**同一个 timescale** ✓）
   *
   * **∴ 为什么必须显式给时长 ✗**：**WebCodecs 只给**时间戳**✗
   *   ⇒ **∴ 时长**要由相邻时间戳**相减得到**✗ 而**最后一帧没有下一帧** ✓
   *     ⇒ **∴ 所以**：**让调用方**把时长算好交进来**✗ 而不是在这里猜** ✓ ★
   */
  addSample(bytes, durationTicks, isKey) {
    const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const duration = Math.max(1, Math.round(durationTicks));
    this.samples.push({ data, duration, key: Boolean(isKey) });
    this.totalTicks += duration;
    return this;
  }

  /** **出文件 ✗**（**返回 `Uint8Array` ✓）★
   *
   *  **★★★ 为什么**要两遍**✗ ★★★**（**∴ 这是**判据抓出来的真错** ✓）**：
   *    ＋ **∴ `stco` 里要写的是**绝对文件偏移**✗ 而**它**依赖 `moov` 有多长** ✓
   *      ＋ **∴ 而**`moov` 的长度**又只取决于**样本表的结构**✗ **与偏移的**数值**无关** ✓
   *        （**每个偏移恒占 4 字节 ✓）⇒ **∴ 于是**：
   *          **∴ 第一遍**用 `mdatBase = 0` 建一次 ⇒ **量出 `moov` 的长度** ✓
   *          **∴ 第二遍**用真正的 `mdatBase` 再建一次 ⇒ **∴ 这回 `stco` 直接就是对的** ✓ ★★
   *    ＋ **∴ 第一版怎么错的 ✗**：**我**一边建盒子一边记"待回填位置"**✗
   *      ＋ **∴ 而**那些位置是**内层 Writer 的坐标**✗ 我却拿去**外层**回填** ✓
   *        ⇒ **∴ 结果**`stco` 全是 0**✗ ⇒ **∴ 而**判据里"按偏移取字节"那一条
   *          **当场红** ✓（**∴ 这就是那条判据的价值 ✗ 它抓的不是崩溃 ✗ 是**文件打不开** ✓）★★★
   */
  finish() {
    // **∴ 第一遍 ✗**：**偏移全用 0**✗ 只为量长度 ✓
    const probe = this.#build(0);
    const ftypEnd = probe.ftypEnd;
    const moovEnd = probe.moovEnd;
    // **∴ `mdat` 的数据从**它的 8 字节头之后**开始 ✓** ✓
    const mdatBase = moovEnd + 8;
    void ftypEnd;
    // **∴ 第二遍 ✗**：**真偏移 ✓**（**∴ 长度与第一遍**逐字节相同**✗ 因为只有数值变了 ✓）★
    return this.#build(mdatBase).bytes;
  }

  /** **建一整份文件 ✗**（**`mdatBase` 用于 `stco` ✓ 返回各段末尾位置 ✓）★ */
  #build(mdatBase) {
    const w = new Writer();
    // **∴ `ftyp` ✗**：**isom ＋ avc1 兼容位** ✓
    box(w, "ftyp", (b) => {
      b.fourcc("isom");
      b.u32(512);
      b.fourcc("isom");
      b.fourcc("iso2");
      b.fourcc("avc1");
      b.fourcc("mp41");
    });
    const ftypEnd = w.length;
    const offsets = [];
    let at = mdatBase;
    for (const sample of this.samples) {
      offsets.push(at);
      at += sample.data.length;
    }
    box(w, "moov", (moov) => {
      this.#mvhd(moov);
      box(moov, "trak", (trak) => {
        this.#tkhd(trak);
        box(trak, "mdia", (mdia) => {
          this.#mdhd(mdia);
          fullBox(mdia, "hdlr", 0, 0, (h) => {
            h.u32(0);
            h.fourcc("vide");
            h.zeros(12);
            h.raw([0]); // 名字以 NUL 结尾 ⇒ 空名字
          });
          box(mdia, "minf", (minf) => {
            fullBox(minf, "vmhd", 0, 1, (v) => {
              v.u16(0);
              v.u16(0);
              v.u16(0);
              v.u16(0);
            });
            box(minf, "dinf", (dinf) => {
              fullBox(dinf, "dref", 0, 0, (d) => {
                d.u32(1);
                fullBox(d, "url ", 0, 1, () => {});
              });
            });
            box(minf, "stbl", (stbl) => {
              this.#stsd(stbl);
              this.#stts(stbl);
              this.#stsc(stbl);
              this.#stsz(stbl);
              this.#stco(stbl, offsets);
              this.#stss(stbl);
            });
          });
        });
      });
    });
    const moovEnd = w.length;
    // **∴ `mdat` ✗**：**样本按顺序拼 ✓**
    const sampleBytes = this.samples.reduce((sum, s) => sum + s.data.length, 0);
    w.u32(sampleBytes + 8);
    w.fourcc("mdat");
    for (const sample of this.samples) w.raw(sample.data);
    return { bytes: new Uint8Array(w.bytes), ftypEnd, moovEnd };
  }

  #mvhd(w) {
    fullBox(w, "mvhd", 0, 0, (b) => {
      b.u32(0); // creation
      b.u32(0); // modification
      b.u32(this.timescale);
      b.u32(this.totalTicks);
      b.u32(0x00010000); // rate 1.0
      b.u16(0x0100); // volume 1.0
      b.u16(0);
      b.zeros(8);
      b.u32(0x00010000);
      b.u32(0);
      b.u32(0);
      b.u32(0);
      b.u32(0x00010000);
      b.u32(0);
      b.u32(0);
      b.u32(0);
      b.u32(0x40000000);
      b.zeros(24);
      b.u32(2); // next track id
    });
  }

  #tkhd(w) {
    fullBox(w, "tkhd", 0, 3, (b) => {
      b.u32(0);
      b.u32(0);
      b.u32(1);
      b.u32(0);
      b.u32(this.totalTicks);
      b.zeros(8);
      b.u16(0); // layer
      b.u16(0); // alternate group
      b.u16(0); // volume（视频为 0）
      b.u16(0);
      b.u32(0x00010000);
      b.u32(0);
      b.u32(0);
      b.u32(0);
      b.u32(0x00010000);
      b.u32(0);
      b.u32(0);
      b.u32(0);
      b.u32(0x40000000);
      b.u32(this.width * 65536);
      b.u32(this.height * 65536);
    });
  }

  #mdhd(w) {
    fullBox(w, "mdhd", 0, 0, (b) => {
      b.u32(0);
      b.u32(0);
      b.u32(this.timescale);
      b.u32(this.totalTicks);
      b.u16(0x55c4); // 语言 und
      b.u16(0);
    });
  }

  #stsd(w) {
    fullBox(w, "stsd", 0, 0, (b) => {
      b.u32(1);
      box(b, "avc1", (v) => {
        v.zeros(6);
        v.u16(1); // data reference index
        v.zeros(16);
        v.u16(this.width);
        v.u16(this.height);
        v.u32(0x00480000); // 72 dpi
        v.u32(0x00480000);
        v.u32(0);
        v.u16(1); // frame count
        v.zeros(32);
        v.u16(0x0018); // depth
        v.u16(0xffff); // 预定义 = -1
        if (this.description && this.description.length) {
          box(v, "avcC", (c) => c.raw(this.description));
        }
      });
    });
  }

  #stts(w) {
    // **∴ 把**相同时长**的样本并成一条 ✓**（**∴ 常见的定帧率 ⇒ 只有 1 条 ✓）★
    const runs = [];
    for (const s of this.samples) {
      const last = runs[runs.length - 1];
      if (last && last.duration === s.duration) last.count += 1;
      else runs.push({ duration: s.duration, count: 1 });
    }
    fullBox(w, "stts", 0, 0, (b) => {
      b.u32(runs.length);
      for (const run of runs) {
        b.u32(run.count);
        b.u32(run.duration);
      }
    });
  }

  #stsc(w) {
    // **∴ 每个样本**一个 chunk**✗ ⇒ **∴ 只有一条记录 ✓**（**∴ 简单 ✗ 但**足够**✓）★
    fullBox(w, "stsc", 0, 0, (b) => {
      b.u32(1);
      b.u32(1);
      b.u32(1);
      b.u32(1);
    });
  }

  #stsz(w) {
    fullBox(w, "stsz", 0, 0, (b) => {
      b.u32(0); // 每样本大小不同 ⇒ 逐条列
      b.u32(this.samples.length);
      for (const s of this.samples) b.u32(s.data.length);
    });
  }

  /** **`stco` ✗**：**直接写**真偏移**✗ 不再回填** ✓（**∴ 两遍构建 ✓）★ */
  #stco(w, offsets) {
    fullBox(w, "stco", 0, 0, (b) => {
      b.u32(offsets.length);
      for (const off of offsets) b.u32(off);
    });
  }

  #stss(w) {
    const keys = [];
    this.samples.forEach((s, i) => {
      if (s.key) keys.push(i + 1);
    });
    if (!keys.length) return;
    fullBox(w, "stss", 0, 0, (b) => {
      b.u32(keys.length);
      for (const k of keys) b.u32(k);
    });
  }
}

globalThis.YanshiMp4 = YanshiMp4;

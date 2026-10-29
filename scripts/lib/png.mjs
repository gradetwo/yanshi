// **PNG 解码的唯一实现** ✓（第 662 轮）
//
// **⇒ 为什么需要它**：判据里比对"导出的 PNG 与渲染的 raw"必须能读 PNG 像素 ✗，
//   而我原来在判据里 `spawnSync("ffmpeg", …)` ⇒ **CI 上没有 ffmpeg** ✗ ⇒
//   `tool-export-under-linear-mixing.mjs` 在 CI 上 `ENOENT` 假红 ✓。
//   **⇒ 而**仓里**早就有**一份手写解码器（`tool-import-image-visible.mjs` 里内联了 40 行 ✓）
//   ⇒ **∴ 复制一份**会让两处漂移 ✗ ⇒ **∴ 所以**抽到这里，两处都引用 ✓（同一事实一处 ✓）。
//
// **支持**：8 位、颜色类型 0（灰度＝遮罩 patch）、2（RGB）与 6（RGBA）、五种行过滤器 ✓。
import { inflateSync } from "node:zlib";

/**
 * 解码 PNG ⇒ `{ width, height, channels, data }`（`data` 是 `width*height*channels` 字节）。
 * @param {Buffer} buffer PNG 文件内容
 */
export function decodePng(buffer) {
  if (buffer.length < 8 || buffer.readUInt32BE(0) !== 0x89504e47) {
    throw new Error("不是 PNG（签名不对）");
  }
  let pos = 8;
  let width = 0;
  let height = 0;
  let colourType = 0;
  let bitDepth = 0;
  const idat = [];
  while (pos < buffer.length) {
    const length = buffer.readUInt32BE(pos);
    const type = buffer.toString("ascii", pos + 4, pos + 8);
    const data = buffer.subarray(pos + 8, pos + 8 + length);
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colourType = data[9];
    }
    if (type === "IDAT") idat.push(data);
    pos += 12 + length;
  }
  if (bitDepth !== 8 || (colourType !== 0 && colourType !== 2 && colourType !== 6)) {
    throw new Error(`不支持的颜色类型／位深：colourType=${colourType} bitDepth=${bitDepth}`);
  }
  // **★ 颜色类型 0（灰度 8 位）是遮罩片段的格式 ✓**（`crates/yanshi-render/src/png.rs encode_png_l8`
  //   ＋ `decode_png_l8` ⇒ 烘焙遮罩／`create_mask` 的 patch 就是它 ✗ 判据要逐字节比网格必须有它 ✓）
  const channels = colourType === 6 ? 4 : colourType === 2 ? 3 : 1;
  const stride = width * channels;
  const raw = inflateSync(Buffer.concat(idat));
  const out = Buffer.alloc(height * stride);
  for (let y = 0; y < height; y += 1) {
    const filter = raw[y * (stride + 1)];
    const src = raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
    for (let x = 0; x < stride; x += 1) {
      const a = x >= channels ? out[y * stride + x - channels] : 0;
      const b = y > 0 ? out[(y - 1) * stride + x] : 0;
      const c = x >= channels && y > 0 ? out[(y - 1) * stride + x - channels] : 0;
      let value = src[x];
      if (filter === 1) value += a;
      else if (filter === 2) value += b;
      else if (filter === 3) value += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a);
        const pb = Math.abs(p - b);
        const pc = Math.abs(p - c);
        value += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      out[y * stride + x] = value & 255;
    }
  }
  return { width, height, channels, data: out };
}

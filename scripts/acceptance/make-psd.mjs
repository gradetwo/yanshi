// 生成一份**最小合法 PSD**（8 位 / RGB / RLE）—— 用于真机验收「导入 PSD」✓。
// 与 Rust 侧 `encode_psd` 同一格式 ✓：头 + 三段空段 + 压缩方式 + 逐通道 RLE ✓。
import { writeFileSync } from "node:fs";

const width = 240, height = 160;
const rgba = Buffer.alloc(width * height * 4);
for (let y = 0; y < height; y++) {
  for (let x = 0; x < width; x++) {
    const i = (y * width + x) * 4;
    // 一眼能认出的图案：红色渐变 + 右侧绿色方块 + 中间蓝色横带 ✓
    rgba[i] = Math.round((x / width) * 255);
    rgba[i + 1] = x > width * 0.6 ? 210 : 30;
    rgba[i + 2] = Math.abs(y - height / 2) < 16 ? 220 : 40;
    rgba[i + 3] = 255;
  }
}
const packbits = (row) => {
  const out = [];
  let at = 0;
  while (at < row.length) {
    let run = 1;
    while (at + run < row.length && row[at + run] === row[at] && run < 128) run++;
    if (run >= 3) { out.push((1 - run) & 0xff, row[at]); at += run; continue; }
    const start = at;
    while (at < row.length && at - start < 128) {
      let ahead = 1;
      while (at + ahead < row.length && row[at + ahead] === row[at] && ahead < 3) ahead++;
      if (ahead >= 3 && at > start) break;
      at++;
    }
    out.push(at - start - 1, ...row.subarray(start, at));
  }
  return Buffer.from(out);
};
const chunks = [];
for (let channel = 0; channel < 3; channel++) {
  const plane = Buffer.alloc(width * height);
  for (let i = 0; i < width * height; i++) plane[i] = rgba[i * 4 + channel];
  for (let y = 0; y < height; y++) chunks.push(packbits(plane.subarray(y * width, (y + 1) * width)));
}
const header = Buffer.alloc(26);
header.write("8BPS", 0, "ascii");
header.writeUInt16BE(1, 4);
header.writeUInt16BE(3, 12);
header.writeUInt32BE(height, 14);
header.writeUInt32BE(width, 18);
header.writeUInt16BE(8, 22);
header.writeUInt16BE(3, 24);
const sections = Buffer.alloc(12);
const lengths = Buffer.alloc(chunks.length * 2);
chunks.forEach((c, i) => lengths.writeUInt16BE(c.length, i * 2));
const body = Buffer.concat([header, sections, Buffer.from([0, 1]), lengths, ...chunks]);
writeFileSync(process.argv[2] || "/home/crow/yanshi-tmp/probe.psd", body);
console.log("  已写出 " + (process.argv[2] || "/home/crow/yanshi-tmp/probe.psd") + "：" + body.length + " 字节（" + width + "×" + height + " ✓ RLE ✓）");

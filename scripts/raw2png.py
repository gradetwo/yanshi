#!/usr/bin/env python3
"""把 Yanshi 的 `render_region` 原始像素（RGBA）存成 PNG ✓ —— 只依赖标准库 ✓。

**为什么自己写** ✓：导出画作本来用 `PIL` ✓，而那个 venv 在清理缓存时被我删掉 ✗
⇒ 一个"看画"的小工具不该依赖一个随时可能被清理的环境 ✓（这也是"创作压测"逼出来的一条 ✓）。
"""
import struct, sys, urllib.request, zlib, json

def fetch_raw(port, doc, token, width, height):
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}/api/tools/render_region?doc={doc}&token={token}",
        data=json.dumps({"region": {"x": 0, "y": 0, "w": width, "h": height}, "raw": True}).encode(),
        headers={"content-type": "application/json"})
    value = json.loads(urllib.request.urlopen(request, timeout=180).read())
    url = value.get("raw_url")
    if not url:
        raise SystemExit(f"没有 raw_url：{value}")
    absolute = f"http://127.0.0.1:{port}{url}" + ("&" if "?" in url else "?") + "token=" + token
    raw = urllib.request.urlopen(absolute, timeout=180).read()
    if len(raw) != width * height * 4:
        raise SystemExit(f"原始像素长度不对：{len(raw)} ≠ {width * height * 4}")
    return raw

def write_png(path, raw, width, height, scale=1):
    """8 位 RGB PNG ✓（每行前置 filter 0 ✓）。`scale` 用整数倍缩放做**预览** ✓（1/2/3…）。"""
    out_w, out_h = width // scale, height // scale
    rows = bytearray()
    for y in range(out_h):
        rows.append(0)
        source_y = y * scale
        base = source_y * width * 4
        for x in range(out_w):
            off = base + x * scale * 4
            rows += bytes((raw[off], raw[off + 1], raw[off + 2]))
    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", out_w, out_h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(rows), 6))
    png += chunk(b"IEND", b"")
    open(path, "wb").write(png)
    return out_w, out_h

if __name__ == "__main__":
    port, doc, token, width, height, path = sys.argv[1:7]
    scale = int(sys.argv[7]) if len(sys.argv) > 7 else 1
    raw = fetch_raw(int(port), doc, token, int(width), int(height))
    size = write_png(path, raw, int(width), int(height), scale)
    ink = sum(1 for i in range(0, len(raw), 4)
              if raw[i] < 240 or raw[i + 1] < 240 or raw[i + 2] < 240)
    print(f"  已写出 {path} {size}｜非白 {ink}（{ink / (int(width) * int(height)) * 100:.1f}%）")

#!/usr/bin/env python3
"""分析一个 .yanshi 工程包的**成分字节占比**，并判断哪些是必须的、哪些不是。

用法：python3 scripts/analyze-yanshi-package.py <包路径> [--json]

为什么要有它：用户实测"导出很大"，需要**按成分**给出证据，而不是猜。
本脚本只读、不依赖第三方库（只用标准库），对每个条目还做一次内存 gzip 试探，
好回答"压缩能省多少"。
"""
import sys, os, json, gzip, struct, tarfile, collections

PNG_MAGIC = b"\x89PNG\r\n\x1a\n"

def kind_of(name: str, data: bytes) -> str:
    if name.endswith(".png") or data[:8] == PNG_MAGIC:
        if len(data) > 24:
            try:
                w, h = struct.unpack(">II", data[16:24])
                return f"PNG {w}x{h}"
            except Exception:
                return "PNG"
        return "PNG"
    if name.endswith(".json") or name.endswith(".jsonl"):
        return "JSON/JSONL（文本 ✓）"
    if name.endswith(".txt") or name.endswith(".md") or name.endswith("INFO"):
        return "文本"
    head = data[:64]
    if all(32 <= b < 127 or b in (9, 10, 13) for b in head):
        return "看起来是文本"
    return "二进制"

def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    path = sys.argv[1]
    as_json = "--json" in sys.argv
    if not os.path.exists(path):
        print(f"  找不到文件：{path}")
        return 1
    total_file = os.path.getsize(path)
    rows = []
    with tarfile.open(path) as tar:
        members = [m for m in tar.getmembers() if m.isfile()]
        for member in members:
            data = tar.extractfile(member).read()
            rows.append({
                "name": member.name,
                "bytes": len(data),
                "gz": len(gzip.compress(data, 6)) if data else 0,
                "kind": kind_of(member.name, data),
            })
    inner = sum(r["bytes"] for r in rows)
    overhead = total_file - inner
    rows.sort(key=lambda r: -r["bytes"])
    if as_json:
        print(json.dumps({"file": path, "file_bytes": total_file, "inner_bytes": inner,
                          "entries": rows}, ensure_ascii=False, indent=2))
        return 0
    print(f"文件 {total_file} 字节｜条目内文合计 {inner}｜条目 {len(rows)} 个"
          f"｜tar 开销 {overhead} 字节 ({overhead * 100 // max(total_file, 1)}%)")
    print(f"{'字节':>10} {'占比':>5} {'gzip后':>10}  {'类型':<18} 条目")
    for r in rows:
        print(f"{r['bytes']:>10} {r['bytes'] * 100 // max(inner, 1):>4}% {r['gz']:>10}  {r['kind']:<18} {r['name']}")
    print(f"{'合计 gzip 后':>10}: {sum(r['gz'] for r in rows)} 字节"
          f"（相对原内文省 {(1 - sum(r['gz'] for r in rows) / max(inner, 1)) * 100:.0f}%）")
    biggest = rows[0] if rows else None
    if biggest and biggest["bytes"] * 100 // max(inner, 1) >= 50:
        print(f"提示：最大条目 {biggest['name']} 占 {biggest['bytes'] * 100 // max(inner, 1)}%"
              f"（{biggest['bytes']} 字节，类型 {biggest['kind']}）⇒ 先看它是否可由日志重建。")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())

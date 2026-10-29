#!/usr/bin/env python3
"""**变异验证**（PWA 本地层 ✓）：把 `web/api-local.js` 的原子日志三条分支改坏，
看 `scripts/tool-pwa-atom-log.mjs` 里**该红的那条**是不是真红、**别的**是不是没被带红。

规矩（`AGENTS.md`）：变异粒度是**断言**（不是判据 ✓）；跑完**必须还原**（`finally` ✓）；
还原要**逐字节**（`cmp` ✓，不是「凭记忆写回」）；还原后复测必须全绿 ✓。

`web/api-local.js` 是 `crates/yanshi-http/assets/api-local.js` 的**生成拷贝** ✓
⇒ 变异只动 `web/`，还原 = 用**源头那份**覆盖回来 ＋ `cmp` ⇒ 不会污染单一真源 ✓。
（判据读的是 `web/` ✓；服务端二进制读的是 `assets/` ✓ ⇒ 变异不动服务端 ✓。）

用法：`python3 scripts/mutations/mutate-pwa-atom-log.py`
"""

import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[2])
WEB = REPO / "web/api-local.js"
SRC = REPO / "crates/yanshi-http/assets/api-local.js"
CRIT = "scripts/tool-pwa-atom-log.mjs"

ORIGINAL = WEB.read_text(encoding="utf-8")
SOURCE = SRC.read_text(encoding="utf-8")
if ORIGINAL != SOURCE:
    print("⊘ 不可判：web/ 与源头不同 ⇒ 先跑 node scripts/pwa-sync-viewer.mjs（判据前提不成立）")
    sys.exit(3)

MUTATIONS = [
    # ① `get_stroke_log` 不排除已撤销
    ("stroke_log_reverted_kept",
     "if (reverted.has(a.id)) { revertedExcluded += 1; continue; }",
     "if (false) { revertedExcluded += 1; continue; }",
     ["§A get_stroke_log 全表"]),  # 本地少了 B4 ⇒ 全表比对先红（服务端那几条数字断言读的是 s.body ✓）
    # ② `order:"newest"` 不反转
    ("stroke_log_newest_not_reversed",
     "if (newestFirst) all.reverse();",
     "if (false) all.reverse();",
     ["§A4 order=newest"]),
    # ③ 压力 `last` 抄错了位置
    ("pressure_last_wrong",
     "first: pressures[0], last: pressures[pressures.length - 1],",
     "first: pressures[0], last: pressures[0],",
     ["§A get_stroke_log 全表"]),
    # ④ `offset` 被忽略
    ("stroke_log_offset_ignored",
     "const page = all.slice(offset, offset + limit);",
     "const page = all.slice(0, limit);",
     ["§A3 offset=2"]),
    # ⑤ 矢量笔（`payload.data` 回退）被漏掉
    ("vector_fallback_dropped",
     'if (a.kind !== "draw_stroke" && a.kind !== "erase") continue;',
     'if (a.kind !== "erase") continue;',
     ["§A get_stroke_log 全表"]),
    # ⑥ `group_id` 筛选不生效
    ("stroke_log_group_ignored",
     "if (groupFilter !== null && (a.group_id === undefined ? null : a.group_id) !== groupFilter) continue;",
     "if (false) continue;",
     ["§A7 group_id=G1"]),
    # ⑦ `since_seq` 筛选不生效
    ("stroke_log_since_ignored",
     "        const seq = Number(r.seq) || 0;\n        if (seq <= since) continue;",
     "        const seq = Number(r.seq) || 0;\n        if (false) continue;",
     ["§A8 since_seq"]),
    # ⑧ `list_atoms` 缺省不藏非笔迹
    ("list_atoms_hides_nothing",
     "if (!includeNonStroke && pl.source === undefined) {",
     "if (false) {",
     ["§B list_atoms 全表"]),
    # ⑨ `list_atoms` 的 `offset` 被忽略
    ("list_atoms_offset_ignored",
     "if (matched <= offset) continue;",
     "if (false) continue;",
     ["§B12 offset=2"]),
    # ⑩ `brush` 筛选变成大小写敏感
    ("brush_filter_case_sensitive",
     'const wantBrush = strOf("brush") === null ? null : strOf("brush").toLowerCase();',
     'const wantBrush = strOf("brush");',
     ["§B7 brush=OPAQUE"]),
    # ⑪ `bbox` 相交筛选被关掉
    ("bbox_filter_disabled",
     "if (apart) continue;",
     "if (false) continue;",
     ["§B9 bbox 画布外"]),
    # ⑫ `thumb_size` 不缩放
    ("thumb_scale_ignored",
     "if (thumbSize > 0 && (ew > thumbSize || eh > thumbSize)) {",
     "if (false) {",
     ["§E2 thumb_size=4"]),
    # ⑬ `thumb_size >= 2^32` 不报错（静默回绕）
    ("thumb_overflow_silent",
     "if (askedThumb > 4294967295) {",
     "if (false) {",
     ["§D4"]),
    # ⑭ `limit: 0` 也报截断（自相矛盾）
    ("has_more_without_limit",
     "const hasMore = limit > 0 && hits.length > limit;",
     "const hasMore = hits.length > limit;",
     ["§B13 limit=0"]),
    # ⑮ `get_atom` 丢 `label`
    ("get_atom_label_dropped",
     "label: a.label === undefined ? null : a.label,",
     "label: null,",
     ["§C get_atom"]),  # 本地丢 label ⇒ 全字段比对先红（服务端那条 label 断言读 s.body ✓）
    # ⑯ `list_atoms` 入口校验被关掉（未知键静默通过）
    ("list_atoms_validation_off",
     'const specBad = p2ValidateArgsFor("list_atoms", body);',
     "const specBad = null;",
     ["§D5"]),
    # ⑰ 本地写入不盖 `origin` 戳
    ("origin_stamp_wrong",
     "atom.payload.origin = p2AtomOriginOf(atom.kind, atom.payload);",
     'atom.payload.origin = "generated";',
     ["§E 本地写入的 import_image 应盖 origin 戳"]),
]


def run_criterion():
    proc = subprocess.run(
        ["bash", "scripts/run-criteria-local.sh", CRIT],
        cwd=REPO, capture_output=True, text=True,
    )
    return proc.returncode, (proc.stdout + proc.stderr)


def main():
    bad = []
    try:
        for name, old, _new, expected in MUTATIONS:
            assert ORIGINAL.count(old) == 1, (
                f"{name}: 锚点出现 {ORIGINAL.count(old)} 次（必须恰好 1 次）✗")
        for name, old, new, expected in MUTATIONS:
            WEB.write_text(ORIGINAL.replace(old, new, 1), encoding="utf-8")
            code, out = run_criterion()
            hits = [e for e in expected if e in out]
            missing = [e for e in expected if e not in hits]
            if code == 0:
                print(f"  {name}: 判据**没红**（退出码 0）✗")
                bad.append(name)
            elif code == 3:
                print(f"  {name}: 判据 exit(3)（不可判，不是产品失败）✗")
                bad.append(name)
            elif missing:
                print(f"  {name}: 红了但**该红的断言没出现** ✗ 缺 {missing}")
                print("    " + "\n    ".join(out.strip().splitlines()[-8:]))
                bad.append(name)
            else:
                print(f"  {name}: ✓ 红在该红的那条（{hits[0]}）")
    finally:
        shutil.copyfile(SRC, WEB)
    if WEB.read_text(encoding="utf-8") != SOURCE:
        print("✗ 还原不逐字节（cmp 不等）⇒ 不交付")
        sys.exit(1)
    if subprocess.run(["cmp", "-s", str(WEB), str(SRC)]).returncode != 0:
        print("✗ cmp 报两边不同 ⇒ 不交付")
        sys.exit(1)
    code, out = run_criterion()
    if code != 0:
        print("✗ 还原后复测没绿 ⇒ 变异记录无效")
        print(out.strip()[-1200:])
        sys.exit(1)
    print("✓ 还原逐字节一致（cmp ✓）＋ 复测全绿")
    if bad:
        print(f"✗ {len(bad)} 条变异没打中：{bad}")
        sys.exit(1)
    print(f"✓ {len(MUTATIONS)} 条变异全部红在该红的断言上")


if __name__ == "__main__":
    main()

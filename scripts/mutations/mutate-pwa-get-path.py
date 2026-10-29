#!/usr/bin/env python3
"""**变异验证**（PWA 本地层 ✓）：把 `web/api-local.js` 的 `get_path` 分支／路径助手改坏，
看 `scripts/tool-pwa-get-path.mjs` 里**该红的那条**是不是真红。

规矩（`AGENTS.md`）：变异粒度是**断言**（不是判据 ✓）；跑完**必须还原**（`finally` ✓）；
还原要**逐字节**（`cmp` ✓）；还原后复测必须全绿 ✓。

`web/api-local.js` 是 `crates/yanshi-http/assets/api-local.js` 的**生成拷贝** ✓
⇒ 变异只动 `web/`，还原 = 用**源头那份**覆盖回来 ＋ `cmp` ⇒ 不污染单一真源 ✓。

用法：`python3 scripts/mutations/mutate-pwa-get-path.py`
"""

import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[2])
WEB = REPO / "web/api-local.js"
SRC = REPO / "crates/yanshi-http/assets/api-local.js"
CRIT = "scripts/tool-pwa-get-path.mjs"

ORIGINAL = WEB.read_text(encoding="utf-8")
SOURCE = SRC.read_text(encoding="utf-8")
if ORIGINAL != SOURCE:
    print("⊘ 不可判：web/ 与源头不同 ⇒ 先跑 node scripts/pwa-sync-viewer.mjs（判据前提不成立）")
    sys.exit(3)

MUTATIONS = [
    # ① 入口声明校验被关掉（缺 object_id／非字符串／未知键静默通过，「入口先于对象查找」也失效）
    ("validation_off",
     'const specBad = p2ValidateArgsFor("get_path", body);',
     "const specBad = null;",
     ["§B1 缺 object_id"]),
    # ② 不存在回到 400（服务端是 404 reference_not_found）
    ("missing_wrong_status",
     "        return p2Missing(`对象 ${objectId} 不存在`);",
     "        return p2Bad(`对象 ${objectId} 不存在`);",
     ["§B4 对象不存在"]),
    # ③ 已删回到 400（服务端是 404）
    ("deleted_wrong_status",
     "      if (death.deleted) return p2Missing(`对象 ${objectId} 已被删除`);",
     "      if (death.deleted) return p2Bad(`对象 ${objectId} 已被删除`);",
     ["§B5 对象已删"]),
    # ④ 对象类型守卫被关掉（笔里塞 path 几何会被误读成 200）
    ("type_guard_off",
     '      if (p2ObjectTypeOf(rows, objectId) !== "shape") return notPath();',
     "      if (false) return notPath();",
     ["§B8 笔里塞 path 几何"]),
    # ⑤ `geometry.kind` 判断被关掉（矩形带 subpaths 会被误读成 200）
    ("kind_guard_off",
     '      if (geometry.kind !== "path") return notPath();',
     "      if (false) return notPath();",
     ["§B14 矩形带 subpaths"]),
    # ⑥ `fill_rule` 非法值被接受（服务端 400）
    ("fill_rule_accepted",
     '      if (value !== "evenodd" && value !== "nonzero") return null;',
     "      if (false) return null;",
     ["§B10 fill_rule 为 null"]),
    # ⑦ 空 subpaths 被接受（服务端 400）
    ("empty_subpaths_accepted",
     "    if (!Array.isArray(list) || list.length === 0) return null;",
     "    if (!Array.isArray(list)) return null;",
     ["§B9 空 subpaths"]),
    # ⑧ 未知段算子被当成直段接受（服务端 400）
    ("unknown_op_accepted",
     """          segments.push({ op: "C", c1, c2, to });
        } else {
          return null;
        }""",
     """          segments.push({ op: "C", c1, c2, to });
        } else {
          segments.push({ op: "L", to: [0, 0] });
        }""",
     ["§B11 未知段算子"]),
    # ⑨ `closed` 非布尔被接受（服务端 400）
    ("closed_type_accepted",
     '        if (typeof sub.closed !== "boolean") return null;',
     "        if (false) return null;",
     ["§B15 closed 非布尔"]),
    # ⑩ bbox 漏掉控制点（只用端点 ⇒ 三次段 h 变 0）
    ("bbox_skips_controls",
     """          visit(seg.c1);
          visit(seg.c2);
          visit(seg.to);""",
     """          visit(seg.to);""",
     ["§A2 三次段：全字段不一致"]),
    # ⑪ `node_counts` 用了展平采样数（服务端是段数 + 1）
    ("node_counts_flattened",
     "        node_counts: parsed.subpaths.map((sub) => sub.segments.length + 1),",
     "        node_counts: parsed.subpaths.map((sub) => p2FlattenSubpath(sub, 0.1).length),",
     ["§A2 三次段：全字段不一致"]),
    # ⑫ 闭合不回首页（总长少 10）
    ("closed_not_returned",
     """    if (sub.closed) {
      const last = out[out.length - 1] || sub.start;""",
     """    if (false) {
      const last = out[out.length - 1] || sub.start;""",
     ["§A3 闭合方形：全字段不一致"]),
    # ⑬ `d` 保留 JS 的指数形（Rust Display 无科学计数法）
    ("rust_f64_keeps_exponent",
     """    if (!/e/i.test(text)) return text;
    const match""",
     """    if (true) return text;
    const match""",
     ["§A8 小数：全字段不一致"]),
    # ⑭ 小数展开少补一个零
    ("rust_f64_small_zeros_off",
     '    if (point <= 0) return sign + "0." + "0".repeat(-point) + digits;',
     '    if (point <= 0) return sign + "0." + "0".repeat(-point - 1) + digits;',
     ["§A8 小数：全字段不一致"]),
    # ⑮ 闭合子路径丢了 `Z`
    ("svg_d_no_z",
     '      if (sub.closed) text += " Z";',
     '      if (false) text += " Z";',
     ["§A3 闭合方形：全字段不一致"]),
    # ⑯ 子路径之间没空格
    ("svg_d_join_off",
     """    return parts.join(" ");
  }""",
     """    return parts.join("");
  }""",
     ["§A4 多子路径：全字段不一致"]),
    # ⑰ 建对象被中和后仍当存在（服务端 404「不存在」）
    ("neutral_create_ignored",
     """      const view = p2ObjectView(rows, objectId);
      if (!view.found || neutral.has(view.__create_id)) {
        return p2Missing(`对象 ${objectId} 不存在`);
      }""",
     """      const view = p2ObjectView(rows, objectId);
      if (!view.found) {
        return p2Missing(`对象 ${objectId} 不存在`);
      }""",
     ["§C 建对象被撤"]),
    # ⑱ 墓碑检查被关掉（已删对象照读）
    ("death_ignored",
     "      if (death.deleted) return p2Missing(`对象 ${objectId} 已被删除`);",
     "      if (false) return p2Missing(`对象 ${objectId} 已被删除`);",
     ["§B5 对象已删"]),
    # ⑲ `layer_id` 回执写死 null
    ("layer_id_null",
     'layer_id: view.layer_id ?? null, kind: "path",',
     'layer_id: null, kind: "path",',
     ["§A1 直段：全字段不一致"]),
    # ⑳ 顶层 `fill_rule` 写死 evenodd（nonzero 不回显）
    ("fill_rule_echo_hardcoded",
     """        ok: true, object_id: objectId, layer_id: view.layer_id ?? null, kind: "path",
        fill_rule: parsed.fillRule,""",
     """        ok: true, object_id: objectId, layer_id: view.layer_id ?? null, kind: "path",
        fill_rule: "evenodd",""",
     ["§A5 nonzero：全字段不一致"]),
    # ㉑ 顶层 `kind` 写错
    ("kind_echo_wrong",
     '        ok: true, object_id: objectId, layer_id: view.layer_id ?? null, kind: "path",',
     '        ok: true, object_id: objectId, layer_id: view.layer_id ?? null, kind: "shape",',
     ["§A1 直段：全字段不一致"]),
    # ㉒ 霍纳求值丢了常数项（曲线整体偏移 ⇒ 长度变）
    ("cubic_at_drops_constant",
     "      return ((a3 * t + a2) * t + a1) * t + a;",
     "      return ((a3 * t + a2) * t + a1) * t;",
     ["§A2 三次段：全字段不一致"]),
    # ㉓ Wang 的 L 恒 0（采样数恒 1 ⇒ 折线退化成弦）
    ("wang_l_zero",
     "    return Math.max(d1, d2);",
     "    return 0;",
     ["§A2 三次段：全字段不一致"]),
    # ㉔ 折线总长忘了开方
    ("polyline_squared",
     "      sum += Math.sqrt(dx * dx + dy * dy);",
     "      sum += dx * dx + dy * dy;",
     ["§A1 直段：全字段不一致"]),
    # ㉕ 展平容差写错（采样数变 ⇒ 总长与字段比不一致）
    ("length_tolerance_wrong",
     "      for (const sub of parsed.subpaths) length += p2PolylineLength(p2FlattenSubpath(sub, 0.1));",
     "      for (const sub of parsed.subpaths) length += p2PolylineLength(p2FlattenSubpath(sub, 1.0));",
     ["§A2 三次段：全字段不一致"]),
    # ㉖ bbox 恒零（控制点盒没被读）
    ("bbox_zero",
     "      const bbox = p2PathControlBbox(parsed.subpaths) || [0, 0, 0, 0];",
     "      const bbox = [0, 0, 0, 0];",
     ["§A1 直段：全字段不一致"]),
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
        print("✗ cmp 两边不同 ⇒ 不交付")
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
    print(f"✓ {len(MUTATIONS)} 条变异全红在该红的断言")


if __name__ == "__main__":
    main()

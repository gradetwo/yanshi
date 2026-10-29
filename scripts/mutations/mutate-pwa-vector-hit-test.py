#!/usr/bin/env python3
"""**变异验证**（PWA 本地层 ✓）：把 `web/api-local.js` 的 `vector_hit_test` 分支／几何助手改坏，
看 `scripts/tool-pwa-vector-hit-test.mjs` 里**该红的那条**是不是真红。

规矩（`AGENTS.md`）：变异粒度是**断言**（不是判据 ✓）；跑完**必须还原**（`finally` ✓）；
还原要**逐字节**（`cmp` ✓）；还原后复测必须全绿 ✓。

`web/api-local.js` 是 `crates/yanshi-http/assets/api-local.js` 的**生成拷贝** ✓
⇒ 变异只动 `web/`，还原 = 用**源头那份**覆盖回来 ＋ `cmp` ⇒ 不污染单一真源 ✓。

用法：`python3 scripts/mutations/mutate-pwa-vector-hit-test.py`
"""

import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[2])
WEB = REPO / "web/api-local.js"
SRC = REPO / "crates/yanshi-http/assets/api-local.js"
CRIT = "scripts/tool-pwa-vector-hit-test.mjs"

ORIGINAL = WEB.read_text(encoding="utf-8")
SOURCE = SRC.read_text(encoding="utf-8")
if ORIGINAL != SOURCE:
    print("⊘ 不可判：web/ 与源头不同 ⇒ 先跑 node scripts/pwa-sync-viewer.mjs（判据前提不成立）")
    sys.exit(3)

MUTATIONS = [
    # ① 入口声明校验被关掉（缺 layer_id／缺 x／x 是字符串／未知键静默通过，且「入口先于图层查找」也失效）
    ("validation_off",
     'const specBad = p2ValidateArgsFor("vector_hit_test", body);',
     "const specBad = null;",
     ["§B1 缺 layer_id"]),
    # ② 不存在的图层回到 400（服务端是 404 reference_not_found）
    ("missing_layer_wrong_status",
     "      if (layerType === null) return p2Missing(`图层 ${layerId} 不存在`);",
     "      if (layerType === null) return p2Bad(`图层 ${layerId} 不存在`);",
     ["§B5 图层不存在"]),
    # ③ 矢量层守卫被关掉（光栅／组／蒙版层不再 400）
    ("vector_guard_off",
     '      if (layerType !== "vector") {',
     "      if (false) {",
     ["§B6 光栅层"]),
    # ④ Rust `Debug` 类型名写错（Raster ⇒ raster）
    ("debug_name_wrong",
     'const debugName = { raster: "Raster", layer_group: "LayerGroup", mask: "Mask" }[layerType]',
     'const debugName = { raster: "raster", layer_group: "LayerGroup", mask: "Mask" }[layerType]',
     ["§B6 光栅层"]),
    # ⑤ 负容差不再报错
    ("negative_tolerance_off",
     '      if (tolerance < 0) return p2Bad("tolerance 不能为负");',
     "      if (false) return p2Bad(\"tolerance 不能为负\");",
     ["§B9 容差为负"]),
    # ⑥ 未给容差时的缺省值写错（0 ⇒ 1）
    ("tolerance_default_wrong",
     'const tolerance = typeof body.tolerance === "number" ? body.tolerance : 0;',
     'const tolerance = typeof body.tolerance === "number" ? body.tolerance : 1;',
     ["§A5 tolerance=null"]),
    # ⑦ `strokes_examined` 挪到几何解析之后（坏几何不再算「查过」）
    ("increment_after_geometry",
     """        if (p2ObjectTypeOf(rows, summary.object_id) !== "stroke") continue;
        strokesExamined += 1;
        const view = p2ObjectView(rows, summary.object_id);
        const geometry = p2VectorGeometry(view.data);
        if (geometry === null) continue;""",
     """        if (p2ObjectTypeOf(rows, summary.object_id) !== "stroke") continue;
        const view = p2ObjectView(rows, summary.object_id);
        const geometry = p2VectorGeometry(view.data);
        if (geometry === null) continue;
        strokesExamined += 1;""",
     ["§A3 只查笔迹"]),
    # ⑧ 类型筛子被关掉（图形／组也被数进 `strokes_examined`）
    ("type_filter_off",
     'if (p2ObjectTypeOf(rows, summary.object_id) !== "stroke") continue;',
     "if (false) continue;",
     ["§A3 只查笔迹"]),
    # ⑨ 隐藏对象被筛掉（服务端 `objects_in_layer` 不过滤可见性）
    ("include_hidden_off",
     "layerFilter: layerId, includeHidden: true }",
     "layerFilter: layerId, includeHidden: false }",
     ["§A3 只查笔迹"]),
    # ⑩ 容差筛子被关掉（余量再大也入命中）
    ("tolerance_filter_off",
     "if (hit === null || !(hit.clearance <= tolerance)) continue;",
     "if (hit === null) continue;",
     ["§A1 笔心"]),
    # ⑪ 排序反了（余量降序）
    ("sort_reversed",
     "hits.sort((a, b) => (a.clearance - b.clearance) || (a.object_id < b.object_id ? -1",
     "hits.sort((a, b) => (b.clearance - a.clearance) || (a.object_id < b.object_id ? -1",
     ["§A4 两条命中排序"]),
    # ⑫ 压力剖面被忽略（半径不随压力变）
    ("pressure_ignored",
     "const p = Math.min(1, Math.max(0, pressure));",
     "const p = 1;",
     ["§A2 压力中点 P1"]),
    # ⑬ `min_size_ratio` 被忽略（恒用缺省 0.25）
    ("min_ratio_ignored",
     """    const ratio = typeof d.min_size_ratio === "number"
      ? Math.min(1, Math.max(0, d.min_size_ratio)) : 0.25;""",
     "    const ratio = 0.25;",
     ["§A2 压力中点 P2"]),
    # ⑭ `smooth` 被忽略（直接用存储点列当脊线）
    ("smooth_ignored",
     "const spine = geometry.smooth ? p2VectorSmooth(geometry.points, 8) : geometry.points;",
     "const spine = geometry.points;",
     ["§A2 smooth 有效脊线"]),
    # ⑮ 单点笔支被关掉（单点笔永远不命中）
    ("single_point_off",
     "if (spine.length === 1) {",
     "if (false) {",
     ["§A2 单点笔 tol=10"]),
    # ⑯ 图层类型折叠改成「只看活层」（删了的层变 404，而服务端仍 200／0 笔）
    ("deleted_layer_alive_only",
     "        if (id === layerId && !found) {",
     """        if (id === layerId && !found
            && !rows.some((r) => r && r.atom && r.atom.kind === "tombstone"
              && (r.atom.payload || {}).layer_id === layerId
              && !(r.atom.payload || {}).object_id && !neutral.has(r.atom.id))) {""",
     ["§C1 已删图层"]),
    # ⑰ 建层被 revert 后仍算「层存在」（已中和的 create 不再跳过）
    ("revert_create_ignored",
     'if (a.kind === "create_layer" && !neutral.has(a.id)) {',
     'if (a.kind === "create_layer") {',
     ["§C2 建层被撤"]),
    # ⑱ 回执把 x／y 写反
    ("echo_swapped",
     "layer_id: layerId, x, y, tolerance,",
     "layer_id: layerId, x: y, y: x, tolerance,",
     ["§A1 笔心"]),
    # ⑲ `count` 用了 `strokes_examined`（与 `hits.length` 脱钩）——
    #    §D 那个 case 恰好 1 笔 1 命中（count 与 examined 同值）⇒ 先红在 §A2 P1（4 笔 1 命中）。
    ("count_wrong",
     "strokes_examined: strokesExamined, hits, count: hits.length, server: false });",
     "strokes_examined: strokesExamined, hits, count: strokesExamined, server: false });",
     ["§A2 压力中点 P1"]),
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

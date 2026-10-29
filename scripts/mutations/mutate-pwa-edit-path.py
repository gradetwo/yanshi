#!/usr/bin/env python3
"""**变异验证**（PWA 本地层 ✓）：把 `web/api-local.js` 的 `edit_path` 分支／节点助手改坏，
看 `scripts/tool-pwa-edit-path.mjs` 里**该红的那条**是不是真红。

规矩（`AGENTS.md`）：变异粒度是**断言**（不是判据 ✓）；跑完**必须还原**（`finally` ✓）；
还原要**逐字节**（`cmp` ✓）；还原后复测必须全绿 ✓。

`web/api-local.js` 是 `crates/yanshi-http/assets/api-local.js` 的**生成拷贝** ✓
⇒ 变异只动 `web/`，还原 = 用**源头那份**覆盖回来 ＋ `cmp` ⇒ 不污染单一真源 ✓。

用法：`python3 scripts/mutations/mutate-pwa-edit-path.py`
"""

import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[2])
WEB = REPO / "web/api-local.js"
SRC = REPO / "crates/yanshi-http/assets/api-local.js"
CRIT = "scripts/tool-pwa-edit-path.mjs"

ORIGINAL = WEB.read_text(encoding="utf-8")
SOURCE = SRC.read_text(encoding="utf-8")
if ORIGINAL != SOURCE:
    print("⊘ 不可判：web/ 与源头不同 ⇒ 先跑 node scripts/pwa-sync-viewer.mjs（判据前提不成立）")
    sys.exit(3)

MUTATIONS = [
    # ① 入口声明校验被关掉（缺 object_id／未知键静默通过）
    ("validation_off",
     '      const specBad = p2ValidateArgsFor("edit_path", body);',
     "      const specBad = null;",
     ["§E1 缺 object_id"]),
    # ② 算子白名单被关掉（未知算子落进 set_handle 分支，报缺 node 而不是未知算子）
    ("op_whitelist_off",
     '      if (!["move_node", "insert_node", "delete_node", "set_handle"].includes(op)) {',
     "      if (false) {",
     ["§E3 未知算子"]),
    # ③ 不存在回到 400（服务端是 404 reference_not_found）
    # 锚点要带 `const rows = await atomsOf(handle, doc);` —— `get_path` 分支那几行
    # **逐字相同**（只有这里的 `rows` 不带 `p2StateWindow`）⇒ 不带就会命中两处。
    ("missing_wrong_status",
     """      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const view = p2ObjectView(rows, objectId);
      if (!view.found || neutral.has(view.__create_id)) {
        return p2Missing(`对象 ${objectId} 不存在`);
""",
     """      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const view = p2ObjectView(rows, objectId);
      if (!view.found || neutral.has(view.__create_id)) {
        return p2Bad(`对象 ${objectId} 不存在`);
""",
     ["§E7 对象不存在"]),
    # ④ 已删回到 400（服务端是 404）
    ("deleted_wrong_status",
     """      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const view = p2ObjectView(rows, objectId);
      if (!view.found || neutral.has(view.__create_id)) {
        return p2Missing(`对象 ${objectId} 不存在`);
      }
      const death = p2ObjectDeath(p2PresetLayers(rows, neutral), view);
      if (death.deleted) return p2Missing(`对象 ${objectId} 已被删除`);
""",
     """      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const view = p2ObjectView(rows, objectId);
      if (!view.found || neutral.has(view.__create_id)) {
        return p2Missing(`对象 ${objectId} 不存在`);
      }
      const death = p2ObjectDeath(p2PresetLayers(rows, neutral), view);
      if (death.deleted) return p2Bad(`对象 ${objectId} 已被删除`);
""",
     ["§E12 对象已删"]),
    # ⑤ 对象类型守卫被关掉（**注入 path 几何的笔**会被当成 path 往下走；P6 那种没有几何的笔
    #    下游解析也会拒 ⇒ 测不出来 ✗ ⇒ 判据另一条 §E13 用 P11 ✓）。
    #    锚点要带 edit_path 独有的尾注 —— get_path 分支同样的几行**逐字相同** ✗。
    ("type_guard_off",
     """      if (p2ObjectTypeOf(rows, objectId) !== "shape") return notPath();
      const data = view.data;
      const geometry = data && typeof data === "object" && !Array.isArray(data)
        ? data.geometry : null;
      if (!geometry || typeof geometry !== "object" || Array.isArray(geometry)) return notPath();
      if (geometry.kind !== "path") return notPath();
      const parsed = p2PathSubpaths(geometry);
      if (parsed === null) return notPath();
      // **非负整数下标 ✓**""",
     """      if (false) return notPath();
      const data = view.data;
      const geometry = data && typeof data === "object" && !Array.isArray(data)
        ? data.geometry : null;
      if (!geometry || typeof geometry !== "object" || Array.isArray(geometry)) return notPath();
      if (geometry.kind !== "path") return notPath();
      const parsed = p2PathSubpaths(geometry);
      if (parsed === null) return notPath();
      // **非负整数下标 ✓**""",
     ["§E13 笔里塞 path 几何"]),
    # ⑥ `geometry.kind` 判断被关掉（**矩形带 subpaths** 会被误读成 path；裸矩形没有 subpaths ⇒
    #    下游解析也会拒 ⇒ 测不出来 ✗ ⇒ 判据另一条 §E14 用 P12 ✓）。
    ("kind_guard_off",
     """      if (geometry.kind !== "path") return notPath();
      const parsed = p2PathSubpaths(geometry);
      if (parsed === null) return notPath();
      // **非负整数下标 ✓**""",
     """      if (false) return notPath();
      const parsed = p2PathSubpaths(geometry);
      if (parsed === null) return notPath();
      // **非负整数下标 ✓**""",
     ["§E14 矩形带 subpaths"]),
    # ⑦ `subpath` 越界被放行（undefined 子路径 ⇒ 抛异常 ⇒ 500）
    ("subpath_bounds_off",
     "      if (subpathIndex >= parsed.subpaths.length) {",
     "      if (false) {",
     ["§E5 subpath 越界"]),
    # ⑧ 负下标被当成合法（服务端 400）
    ("subpath_negative_accepted",
     "        return typeof value === \"number\" && Number.isInteger(value) && value >= 0 ? value : null;",
     "        return typeof value === \"number\" && Number.isInteger(value) ? value : null;",
     ["§E6 subpath 负下标"]),
    # ⑨ 节点下标越界被放行（anchors[node] === undefined ⇒ 抛异常）
    ("move_range_off",
     "    if (node >= anchors.length) return { error: p2PathNodeRangeError(node, anchors.length) };\n"
     "    const old = anchors[node];",
     "    if (false) return { error: p2PathNodeRangeError(node, anchors.length) };\n"
     "    const old = anchors[node];",
     ["§A5 节点越界"]),
    # ⑩ `to` 只查是数组，不查两元（`[1,2,3]` 被接受 ⇒ 文案不同；`[1]` 仍被「第二个不是有限数」拒
    #    ⇒ 测不出来 ✗ ⇒ 判据 §A9 用三元 ✓）
    ("to_arity_off",
     "        if (!Array.isArray(value) || value.length !== 2) return null;",
     "        if (!Array.isArray(value)) return null;",
     ["§A9 to 三元"]),
    # ⑪ 移动节点时不刚性平移入段的 `c2`
    ("move_no_c2_translate",
     "        seg.c2 = [seg.c2[0] + delta[0], seg.c2[1] + delta[1]];",
     "        seg.c2 = [seg.c2[0], seg.c2[1]];",
     ["§A2 三次段末端"]),
    # ⑫ 移动节点时不刚性平移出段的 `c1`
    ("move_no_c1_translate",
     "      if (seg.op === \"C\") seg.c1 = [seg.c1[0] + delta[0], seg.c1[1] + delta[1]];",
     "      if (seg.op === \"C\") seg.c1 = [seg.c1[0], seg.c1[1]];",
     ["§A3 三次段首节点"]),
    # ⑬ 节点 0 移动时不改 `start`
    ("move_start_not_set",
     "    if (node === 0) {\n      sub.start = to;\n    } else {",
     "    if (node === 0) {\n    } else {",
     ["§A3 三次段首节点"]),
    # ⑭ 直段插入不看 `t`（恒取中点）
    ("insert_t_default_half",
     "      const mid = p2PathLerp(p0, seg.to, t);",
     "      const mid = p2PathLerp(p0, seg.to, 0.5);",
     ["§B1 直段插点"]),
    # ⑮ `t` 的开区间校验被关掉（t=1 被接受）
    ("insert_t_open_off",
     "    if (!Number.isFinite(t) || t <= 0 || t >= 1) {",
     "    if (!Number.isFinite(t)) {",
     ["§B6 t=1"]),
    # ⑯ de Casteljau 的 r0 用了 1−t
    ("insert_r0_mirrored",
     "      const r0 = p2PathLerp(q0, q1, t);",
     "      const r0 = p2PathLerp(q0, q1, 1 - t);",
     ["§B2 三次段插点"]),
    # ⑰ 闭合回首虚拟段被关掉（segment=段数 ⇒ 报越界）
    ("insert_closed_virtual_off",
     "    if (segment === sub.segments.length && sub.closed && sub.segments.length > 0) {",
     "    if (false && segment === sub.segments.length && sub.closed && sub.segments.length > 0) {",
     ["§B4 闭合回首段"]),
    # ⑱ `new_node` 不回填（恒 null）
    ("insert_new_node_null",
     "        if (outcome.ok) newNode = outcome.index;",
     "        if (outcome.ok) newNode = null;",
     ["§B1 直段插点：new_node 应为 1"]),
    # ⑲ 只剩一段的删除被放行（零段 ⇒ 复验报「编辑后几何非法」⇒ 文案不同）
    ("delete_min_segments_off",
     "    if (sub.segments.length < 2) {",
     "    if (false) {",
     ["§C5 只剩一段拒绝"]),
    # ⑳ 删首节点不改 `start`
    ("delete_zero_start_stale",
     "      const newStart = anchors[1];\n      sub.segments.shift();\n      sub.start = newStart;",
     "      const newStart = anchors[1];\n      sub.segments.shift();",
     ["§C2 删首节点"]),
    # ㉑ 删末节点不丢段
    ("delete_last_pop_off",
     "      sub.segments.pop();",
     "      sub.segments.length = sub.segments.length;",
     ["§C3 删末节点"]),
    # ㉒ 合并段丢进入段的 `c1`（恒取被删锚点）
    ("delete_merge_c1_wrong",
     "      const c1 = incoming.op === \"C\" ? incoming.c1 : deleted;",
     "      const c1 = deleted;",
     ["§C4 三三合并"]),
    # ㉓ 直段 Out 升格的 2/3 点写成 1/3
    ("set_out_lerp_third",
     "                                 c2: p2PathLerp(anchors[node], seg.to, 2 / 3), to: seg.to };",
     "                                 c2: p2PathLerp(anchors[node], seg.to, 1 / 3), to: seg.to };",
     ["§D1 直段 Out 升格"]),
    # ㉔ 直段 In 升格的 1/3 点写成 2/3
    ("set_in_lerp_two_thirds",
     '        sub.segments[node - 1] = { op: "C", c1: p2PathLerp(anchors[node - 1], seg.to, 1 / 3),\n                                   c2: to, to: seg.to };\n',
     '        sub.segments[node - 1] = { op: "C", c1: p2PathLerp(anchors[node - 1], seg.to, 2 / 3),\n                                   c2: to, to: seg.to };\n',
     ["§D2 直段 In 升格"]),
    # ㉕ 既有三次段的 Out 直改被跳过（反而走升格分支）
    ("set_cubic_out_skipped",
     '        if (seg.op === "C") {\n          seg.c1 = to;\n        } else {\n          sub.segments[node] = { op: "C", c1: to,',
     '        if (false) {\n          seg.c1 = to;\n        } else {\n          sub.segments[node] = { op: "C", c1: to,',
     ["§D3 三次段 Out 直改"]),
    # ㉖ 既有三次段的 In 直改被跳过
    ("set_cubic_in_skipped",
     "      if (seg.op === \"C\") {\n        seg.c2 = to;\n      } else {",
     "      if (false) {\n        seg.c2 = to;\n      } else {",
     ["§D4 三次段 In 直改"]),
    # ㉗ 开放子路径首节点的 In 不报错（静默成功）
    ("set_open_first_in_allowed",
     "    return { error: `节点 ${node} 是开放子路径的首节点，没有 in 控制柄（其前没有段）` };",
     "    return { ok: true };",
     ["§D7 开放首节点无 in"]),
    # ㉘ 开放子路径末节点的 Out 不报错（静默成功）
    ("set_open_last_out_allowed",
     "      return { error: `节点 ${node} 是开放子路径的末节点，没有 out 控制柄（其后没有段）` };",
     "      return { ok: true };",
     ["§D8 开放末节点无 out"]),
    # ㉙ side 白名单被关掉（left 被当成 in 处理 ⇒ 文案不同）
    ("side_whitelist_off",
     "        if (sideName !== \"in\" && sideName !== \"out\") {",
     "        if (false) {",
     ["§D9 side 非法"]),
    # ㉚ 缺 side 不报缺参（null 被当成 in）
    ("side_missing_off",
     "        if (sideName === null) return p2Bad(\"缺少必填参数 side\");",
     "        if (false) return p2Bad(\"缺少必填参数 side\");",
     ["§D10 缺 side"]),
    # ㉛ 闭合首节点 In 的 c1 用 2/3 点
    ("closed_in_lerp_two_thirds",
     "      sub.segments.push({ op: \"C\", c1: p2PathLerp(last, sub.start, 1 / 3),",
     "      sub.segments.push({ op: \"C\", c1: p2PathLerp(last, sub.start, 2 / 3),",
     ["§D5 闭合首节点 In"]),
    # ㉜ 闭合末节点 Out 的 c2 用 1/3 点
    ("closed_out_lerp_third",
     "                            c2: p2PathLerp(last, sub.start, 2 / 3), to: sub.start });",
     "                            c2: p2PathLerp(last, sub.start, 1 / 3), to: sub.start });",
     ["§D6 闭合末节点 Out"]),
    # ㉝ 编辑回执的 bbox 用编辑前的几何（**必须重新解析** —— 助手是原地改 ✓，
    #    `parsed.subpaths` 与 `subpaths` 是**同一个数组** ⇒ 直接换引用测不出来 ✗）
    ("bbox_pre_edit",
     "      const bbox = p2PathControlBbox(subpaths) || [0, 0, 0, 0];",
     "      const bbox = p2PathControlBbox(p2PathSubpaths(geometry).subpaths) || [0, 0, 0, 0];",
     ["§A1 bbox 应为控制点盒"]),
    # ㉞ 编辑回执的 `d` 用编辑前的几何（同上：重新解析才是真的编辑前 ✓）
    ("d_pre_edit",
     "                    subpaths, d: p2PathSvgD(subpaths), bbox: box, length,",
     "                    subpaths, d: p2PathSvgD(p2PathSubpaths(geometry).subpaths), bbox: box, length,",
     ["§A1 中间节点：全字段不一致"]),
    # ㉟ `node_counts` 恒 +2（恒等式与期望值都红）
    ("node_counts_off_by_one",
     "                    node_counts: subpaths.map((each) => each.segments.length + 1),",
     "                    node_counts: subpaths.map((each) => each.segments.length + 2),",
     ["§A1 中间节点：node_counts 应与每条子路径段数 + 1 一致"]),
    # ㊱ 不落 `supersede` 原子（回执对、读回旧 ⇒ 写读分裂）
    ("supersede_not_written",
     "      const seq = await putAtom(handle, doc,\n"
     "        p2Envelope(\"supersede\", { object_id: objectId, data: newData }));",
     "      const seq = 0;",
     ["§A1 中间节点 回读：全字段不一致"]),
    # ㊲ 折叠时**忽略被撤销的 supersede**（撤销之后仍读编辑后的数据）
    ("supersede_revert_ignored",
     "      else if (a.kind === \"supersede\" && pl.object_id === objectId\n"
     "               && !neutralEarly.has(a.id)) supersededData = pl.data;",
     "      else if (a.kind === \"supersede\" && pl.object_id === objectId) supersededData = pl.data;",
     ["§F 撤销后读回：全字段不一致"]),
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

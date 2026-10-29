#!/usr/bin/env python3
"""**变异验证**（PWA 本地层 `create_range_mask` ✓）：把 `web/api-local.js` 的
**烘焙管线／回执／落盘原子／错误族／挂接**逐条改坏，看
`scripts/tool-pwa-range-mask-parity.mjs` 里**该红的那条**是不是真红。

规矩（`AGENTS.md`）：变异粒度是**断言**（不是判据 ✓）—— 本判据有 §A 错误族／§B 回执键集与数值／
§B 原子形状／§B blob 自洽／§B L8 闸／§B 恒等式重算／§C 空覆盖／§D invert 互补／§E 图层作用域／
§F 挂接，**每一族一条变异 ✓**；跑完**必须还原**（`finally` ✓）；还原要**逐字节**（`cmp` ✓）；
还原后复测必须全绿 ✓。

`web/api-local.js` 是 `crates/yanshi-http/assets/api-local.js` 的**生成拷贝** ✓
⇒ 变异只动 `web/`，还原 = 用**源头那份**覆盖回来 ＋ `cmp` ⇒ 不污染单一真源 ✓。

用法：`python3 scripts/mutations/mutate-pwa-range-mask.py`
"""

import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[2])
WEB = REPO / "web/api-local.js"
SRC = REPO / "crates/yanshi-http/assets/api-local.js"
CRIT = "scripts/tool-pwa-range-mask-parity.mjs"

ORIGINAL = WEB.read_text(encoding="utf-8")
SOURCE = SRC.read_text(encoding="utf-8")
if ORIGINAL != SOURCE:
    print("⊘ 不可判：web/ 与源头不同 ⇒ 先跑 node scripts/pwa-sync-viewer.mjs（判据前提不成立）")
    sys.exit(3)

MUTATIONS = [
    # ── §A 错误族（状态／错误码／文案／context 键集／reason／零新原子）───────────
    ("rm_no_validate",
     '      const specBad = p2ValidateArgsFor("create_range_mask", body);',
     "      const specBad = null;",
     ["§A 未知键"]),
    ("rm_never_layer_as_400",
     '        return p2ToolFailure(404, "reference_not_found", `图层 ${layerId} 不存在`, null);',
     '        return p2Bad(`缺少必填参数 layer_id`);',
     ["§A 层从未存在"]),
    ("rm_deleted_layer_no_name",
     '          `图层 ${layerId}（「${layer.name}」）已删除`, null);',
     '          `图层 ${layerId} 已删除`, null);',
     ["§A 层已删除"]),
    ("rm_empty_layer_gate_off",
     "      if (layerObjects.length === 0) {",
     "      if (false) {",
     ["§A 空层（无对象）"]),
    ("rm_adjust_only_gate_off",
     '      if (layerObjects.every((o) => o.type === "adjustment" || o.type === "filter")) {',
     "      if (false) {",
     ["§A 纯调整层"]),
    ("rm_dup_mask_gate_off",
     "      if (liveMask && !liveMask.deleted) {",
     "      if (false) {",
     ["§A 重复蒙版"]),
    ("rm_dup_mask_no_atom_id",
     '        return p2ToolFailure(409, "precondition_failed", `蒙版 ${maskId} 已存在`,\n'
     '                             { atom_id: maskAtom.id });',
     '        return p2ToolFailure(409, "precondition_failed", `蒙版 ${maskId} 已存在`, null);',
     ["§A 重复蒙版（提交期 409）：context 键集不同"]),
    # `reason` 是**本地独有的键**（界面读它 ✓）⇒ 与 `context.detail` 漂开时**必须有断言能红 ✓**
    ("rm_reason_drift",
     '    return json({ ok: false, error: code, error_code: code, reason: detail,',
     '    return json({ ok: false, error: code, error_code: code, reason: code,',
     ["reason 与 context.detail 不是同一句"]),
    # ── §B 回执（键集／未登记键／seq／逐键值）────────────────────────────────
    ("rm_matched_off_by_one",
     "                         coverage_matched_pixels: matched,",
     "                         coverage_matched_pixels: matched + 1,",
     ["本地回执的 coverage_matched_pixels"]),
    ("rm_ratio_wrong_denom",
     "                         coverage_matched_ratio: matched / grid.length,",
     "                         coverage_matched_ratio: matched / (matched || 1),",
     ["coverage_matched_ratio ≠"]),
    ("rm_mean_no_255",
     "      const mean = sum / (grid.length * 255.0);",
     "      const mean = sum / grid.length;",
     ["coverage_mean ≠ 网格均值"]),
    ("rm_range_echo_hardcoded",
     "      response.range = rangeName;",
     '      response.range = "luminance";',
     ["字段「range」两边不一致"]),
    ("rm_note_drift",
     '      response.note = "烘焙快照：遮罩取自源图层当前渲染的像素，源图层此后的修改不传播；"',
     '      response.note = "烘焙快照：遮罩取自源图层当前渲染的像素；"',
     ["字段「note」两边不一致"]),
    ("rm_stray_key",
     "      response.range = rangeName;",
     '      response.probe_extra = 1;\n      response.range = rangeName;',
     ["本地多给了未登记的键"]),
    ("rm_seq_off_by_one",
     "      const response = { ok: true, seq, head: seq, server: false,",
     "      const response = { ok: true, seq: seq + 1, head: seq, server: false,",
     ["两边写入日志的序号不同"]),
    # ── §B 落盘原子（形状／引用集／内容寻址自洽／L8 闸）───────────────────────
    ("rm_refs_missing_blob",
     '        patches: [{ bbox: [0, 0, renderW, renderH], target_u8: 255, op: "replace_all", blob: blobHash }],\n'
     "        refs: { blobs: [blobHash] },",
     '        patches: [{ bbox: [0, 0, renderW, renderH], target_u8: 255, op: "replace_all", blob: blobHash }],\n'
     "        refs: { blobs: [] },",
     ["refs.blobs 与 patches[].blob 不是同一批"]),
    ("rm_blob_bytes_mismatch",
     "      await putBlob(handle, blobHash, Array.from(png));\n      // **⑦ 提交期唯一会拒的一条",
     "      await putBlob(handle, blobHash, Array.from(await encodePngL8(renderW, renderH, grid.map(() => 0))));\n      // **⑦ 提交期唯一会拒的一条",
     ["patch.blob 与实际取回的字节对不上"]),
    ("rm_l8_becomes_rgb8",
     "  ihdr[8] = 8; ihdr[9] = 0;",
     "  ihdr[8] = 8; ihdr[9] = 2;",
     ["遮罩片段不是**灰度 L8**"]),
    ("rm_payload_shape_wrong",
     '        shape: { kind: "rect", bbox: { x: 0, y: 0, w: canvasW, h: canvasH } },',
     '        shape: { kind: "ellipse", bbox: { x: 0, y: 0, w: canvasW, h: canvasH } },',
     ["create_mask 原子载荷不同"]),
    # ── §B 像素＋§B 恒等式（f32 口径／烘焙窗口）─────────────────────────────
    ("rm_f32_dropped",
     "const p2Fr = Math.fround;",
     "const p2Fr = (v) => v;",
     ["字段「baked」两边不一致"]),
    ("rm_luma_weights_swapped",
     "  const m1 = p2Fr(p2Fr(0.2126) * r);\n  const m2 = p2Fr(p2Fr(0.7152) * g);\n  const m3 = p2Fr(p2Fr(0.0722) * b);",
     "  const m1 = p2Fr(p2Fr(0.0722) * r);\n  const m2 = p2Fr(p2Fr(0.7152) * g);\n  const m3 = p2Fr(p2Fr(0.2126) * b);",
     ["烘焙出的遮罩像素不同"]),
    ("rm_invert_ignored",
     "      const grid = p2BakeRangeGrid(rgba, renderW, renderH, range, invert);",
     "      const grid = p2BakeRangeGrid(rgba, renderW, renderH, range, false);",
     ["不互补"]),
    ("rm_hue_window_always_full",
     "function p2HueWindow(hue, sat, center, width, smoothness) {",
     "function p2HueWindow(hue, sat, center, width, smoothness) {\n  if (sat >= 0) return 255;",
     ["§C"]),
    ("rm_lowhigh_f64",
     "        if (p2Fr(low.value) > p2Fr(high.value)) {",
     "        if (low.value > high.value) {",
     ["§B f32 等值（f64 不等）"]),
    # ── §E 图层作用域（把整幅文档当成这一层 ✓ 形态③）────────────────────────
    ("rm_layer_scope_off",
     "        rendered = await p2cRenderRgba(0, 0, canvasW, canvasH,\n"
     "                                       p2LayerScopeAtoms(rows, props, layerId));",
     "        rendered = await p2cRenderRgba(0, 0, canvasW, canvasH,\n"
     "                                       p2LayerScopeAtoms(rows, props, null));",
     ["§E"]),
    # ── §F 挂接（`set_property{mask_id}` 的 context.atom_id ✗ 本轮补的那个缺口）──
    ("rm_sp_guard_no_atom_id",
     '      if (!rec || rec.deleted) {\n'
     "        return { status: 404, code: \"reference_not_found\",\n"
     "                 detail: `set_property ${payload.key} 引用的实体 ${ref} 不存在或已删除`,\n"
     "                 extra: atomId ? { atom_id: atomId } : null };",
     '      if (!rec || rec.deleted) {\n'
     "        return { status: 404, code: \"reference_not_found\",\n"
     "                 detail: `set_property ${payload.key} 引用的实体 ${ref} 不存在或已删除`,\n"
     "                 extra: null };",
     ["§F 挂接不存在的蒙版：context 键集不同"]),
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
            n = ORIGINAL.count(old)
            assert n == 1, f"{name}: 锚点出现 {n} 次（必须恰好 1 次）✗"
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
                print("    " + "\n    ".join(out.strip().splitlines()[-4:]))
                bad.append(name)
            elif missing:
                print(f"  {name}: 红了但**该红的断言没出现** ✗ 缺 {missing}")
                print("    " + "\n    ".join(out.strip().splitlines()[-8:]))
                bad.append(name)
            else:
                print(f"  {name}: ✓ 红在该红的那条（{hits[0]}）")
    finally:
        shutil.copyfile(SRC, WEB)
    if subprocess.run(["cmp", "-s", str(WEB), str(SRC)]).returncode != 0:
        print("✗ 还原不逐字节（cmp 不等）⇒ 不交付")
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

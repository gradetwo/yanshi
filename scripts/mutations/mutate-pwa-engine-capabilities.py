#!/usr/bin/env python3
"""**变异验证**（PWA 本地层 ✓）：把 `web/api-local.js` 的
`list_engine_capabilities`／`set_layer_blend`／`lock_layer`／`set_property`／`update_layer`
改坏，看 `scripts/tool-pwa-engine-capabilities.mjs` 里**该红的那条**是不是真红。

规矩（`AGENTS.md`）：变异粒度是**断言**（不是判据 ✓）；跑完**必须还原**（`finally` ✓）；
还原要**逐字节**（`cmp` ✓）；还原后复测必须全绿 ✓。

`web/api-local.js` 是 `crates/yanshi-http/assets/api-local.js` 的**生成拷贝** ✓
⇒ 变异只动 `web/`，还原 = 用**源头那份**覆盖回来 ＋ `cmp` ⇒ 不污染单一真源 ✓。

用法：`python3 scripts/mutations/mutate-pwa-engine-capabilities.py`
"""

import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[2])
WEB = REPO / "web/api-local.js"
SRC = REPO / "crates/yanshi-http/assets/api-local.js"
CRIT = "scripts/tool-pwa-engine-capabilities.mjs"

ORIGINAL = WEB.read_text(encoding="utf-8")
SOURCE = SRC.read_text(encoding="utf-8")
if ORIGINAL != SOURCE:
    print("⊘ 不可判：web/ 与源头不同 ⇒ 先跑 node scripts/pwa-sync-viewer.mjs（判据前提不成立）")
    sys.exit(3)

MUTATIONS = [
    # ── §A list_engine_capabilities ──────────────────────────────────────────
    # ① note 写错（整份响应对不上）
    ("cap_note",
     "        note: P2_ENGINE_NOTE,",
     '        note: P2_ENGINE_NOTE + "（本地）",',
     ["§A 能力表"]),
    # ② counts 与数组脱钩（恒等式 `counts.X === X.length` 必须能红）
    ("cap_counts_off",
     "          brush_settings: P2_BRUSH_SETTING_NAMES.length,",
     "          brush_settings: P2_BRUSH_SETTING_NAMES.length - 1,",
     ["§A counts 恒等式（本地）：brush_settings"]),
    # ③ brush_settings 里混进重复项
    ("cap_dup_brush",
     "        brush_settings: P2_BRUSH_SETTING_NAMES.slice(),",
     "        brush_settings: [P2_BRUSH_SETTING_NAMES[0], ...P2_BRUSH_SETTING_NAMES.slice()],",
     ["§A brush_settings（本地）不应有重复项"]),
    # ④ retouch_kinds 被截断 ⇒ retouch_kind_tools 的键集合对不上
    ("cap_retouch_tools_off",
     "        retouch_kinds: P2_ENGINE_STATIC.retouch_kinds.slice(),",
     "        retouch_kinds: P2_ENGINE_STATIC.retouch_kinds.slice(0, 2),",
     ["§A retouch_kind_tools（本地）"]),
    # ⑤ 入口校验被关掉（零参数工具的未知键静默通过）
    ("cap_no_validate",
     '      const specBad = p2ValidateArgsFor("list_engine_capabilities", body);',
     "      const specBad = null;",
     ["§A2 未知参数"]),
    # ── §B set_layer_blend ──────────────────────────────────────────────────
    # ⑥ 混合模式清单退回 11 项（正是这一轮修的那个缺口）
    ("blend_list_11",
     '                        "linear_light", "soft_light"];',
     "                        ];",
     ["§B1 模式 linear_light"]),
    # ⑦ 不做小写归一（服务端 `to_ascii_lowercase`）
    ("blend_no_lowercase",
     "      const mode = body.mode.toLowerCase();",
     "      const mode = body.mode;",
     ["§B2 归一化 SCREEN"]),
    # ⑧ 未知模式的文案丢掉可用清单
    ("blend_reject_detail",
     '        return p2Bad("未知混合模式 " + body.mode + " ⇒ 可用：" + P2_BLEND_NAMES.join(" / "));',
     '        return p2Bad("未知混合模式 " + body.mode);',
     ["§B3 未知模式"]),
    # ⑨ 白名单整个失效（未知模式照收 ⇒ 渲染层会静默退化 normal）
    ("blend_no_whitelist",
     "      if (P2_BLEND_NAMES.indexOf(mode) < 0) {",
     "      if (false) {",
     ["§B3 未知模式"]),
    # ⑩ 入口校验被关掉（非字符串 layer_id 走到 handler 里）
    ("blend_no_validate",
     '      const specBad = p2ValidateArgsFor("set_layer_blend", body);',
     "      const specBad = null;",
     ["§B6 layer_id 非字符串"]),
    # ⑪ 提交期引用校验被关掉（不存在的图层照写原子）
    ("blend_no_ref",
     '''      const guard = p2RefGuard(rows, layers, neutral, "set_property", { layer_id: layerId });
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      const seq = await putAtom(handle, doc, p2Envelope("set_property",
        { layer_id: layerId, key: "blend_mode", value: mode }));''',
     '''      const guard = null;
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      const seq = await putAtom(handle, doc, p2Envelope("set_property",
        { layer_id: layerId, key: "blend_mode", value: mode }));''',
     ["§B10 图层不存在"]),
    # ⑫ 回执回显调用方原文（大写不进归一）
    ("blend_mode_echo_raw",
     "      return json({ ok: true, seq, server: false, layer_id: layerId, mode,",
     "      return json({ ok: true, seq, server: false, layer_id: layerId, mode: body.mode,",
     ["§B2 归一化 SCREEN"]),
    # ── §C lock_layer／unlock_layer ──────────────────────────────────────────
    # ⑬ 共用分支把工具名写死（unlock 的错误文案带成 lock）
    ("lock_toolname_hardcoded",
     '      const toolName = path.endsWith("/lock_layer") ? "lock_layer" : "unlock_layer";',
     '      const toolName = "lock_layer";',
     ["§C9 unlock 未知参数"]),
    # ⑭ 锁与解锁的布尔反了
    ("lock_boolean_swapped",
     '      const locked = path === "/api/tools/lock_layer";',
     '      const locked = path !== "/api/tools/lock_layer";',
     ["§C1 本地回执应带 locked:true"]),
    # ⑮ 提交期引用校验被关掉
    ("lock_no_ref",
     '''      const guard = p2RefGuard(rows, layers, neutral, "set_property", { layer_id: layerId });
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      const seq = await putAtom(handle, doc, p2Envelope("set_property",
        { layer_id: layerId, key: "locked", value: locked }));''',
     '''      const guard = null;
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      const seq = await putAtom(handle, doc, p2Envelope("set_property",
        { layer_id: layerId, key: "locked", value: locked }));''',
     ["§C7 图层不存在"]),
    # ── §D set_property ─────────────────────────────────────────────────────
    # ⑯ 入口校验被关掉（key 为 null 静默通过）
    ("sp_no_validate",
     '      const specBad = p2ValidateArgsFor("set_property", body);',
     "      const specBad = null;",
     ["§D16 key 为 null"]),
    # ⑰ 提交期引用校验被关掉
    ("sp_no_ref",
     '''      const guard = p2RefGuard(rows, layers, neutral, "set_property", payload);
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);''',
     "      const guard = null;",
     ["§D19 图层不存在"]),
    # ⑱ 值类型校验被关掉（`visible:"yes"` 在折叠层会静默当 true）
    ("sp_no_value_check",
     "      const valueBad = p2PropertyValueError(payload);",
     "      const valueBad = null;",
     ["§D6 visible 非布尔"]),
    # ⑲ z_index 丢了 i64 上界（`as_i64()` 的口径）
    ("sp_zindex_no_bound",
     '''      if (!(typeof value === "number" && Number.isInteger(value)
            && Math.abs(value) <= 9223372036854775807)) return reject("整数");''',
     '''      if (!(typeof value === "number" && Number.isInteger(value))) return reject("整数");''',
     ["§D9 z_index 超出 i64"]),
    # ⑳ `type` 的两张表装反了（对象侧拿到图层类型表）
    ("sp_type_object_side_wrong",
     '''      const known = objectSide
        ? ["stroke", "shape", "text", "adjustment", "filter", "raster_patch", "retouch",
           "liquify", "instance", "group", "path"]
        : ["raster", "vector", "layer_group", "mask"];''',
     '''      const known = objectSide
        ? ["raster", "vector", "layer_group", "mask"]
        : ["stroke", "shape", "text", "adjustment", "filter", "raster_patch", "retouch",
           "liquify", "instance", "group", "path"];''',
     ["§D5 合法对象类型"]),
    # ㉑ 缺省 value 不再照 `unwrap_or(Null)`
    ("sp_default_value_wrong",
     "      const value = body.value === undefined ? null : body.value;",
     '      const value = body.value === undefined ? "unset" : body.value;',
     ["§D2 本地回执的 value 应为 null"]),
    # ㉒ 空串 object_id 又被当成「没给」（服务端 `optional_str` 是 Some("")）
    ("sp_empty_object_absent",
     '      const hasObject = typeof objectId === "string";',
     '      const hasObject = typeof objectId === "string" && objectId !== "";',
     ["§D28 空 object_id"]),
    # ㉓ 空串 layer_id 又被当成「没给」
    ("sp_empty_layer_absent",
     '      const hasLayer = typeof layerId === "string";',
     '      const hasLayer = typeof layerId === "string" && layerId !== "";',
     ["§D27 空 layer_id"]),
    # ㉔ 提交层 `p2RefGuard` 又把空串层 id 当缺席（影响所有端点）
    ("ref_empty_layer_absent",
     "    const layerId = typeof payload.layer_id === \"string\" ? payload.layer_id : null;",
     "    const layerId = typeof payload.layer_id === \"string\" && payload.layer_id"
     " ? payload.layer_id : null;",
     ["§B12 空 layer_id"]),
    # ㉕ 忘了「图层自身的管理属性不受锁影响」⇒ 锁层改不了自己的 opacity
    ("d26_lock_blocks_layer_props",
     '''    const objectId = typeof pl.object_id === "string" && pl.object_id ? pl.object_id : null;
    if (objectId === null) return null;''',
     '''    const objectId = typeof pl.object_id === "string" && pl.object_id ? pl.object_id : null;
    if (objectId === null) {
      const neutral = p2Neutralized(rows);
      const { layers } = p1LayerOrder(rows);
      const layer = typeof pl.layer_id === "string" ? layers.get(pl.layer_id) : null;
      if (layer && layer.locked === true) {
        return { status: 403, code: "permission_denied",
                 detail: `图层 ${pl.layer_id} 已锁定`, extra: { layer_id: pl.layer_id } };
      }
      return null;
    }''',
     ["§D26 锁层的图层属性"]),
    # ㉖ 忘了「改 locked/visible 永远放行」
    ("d25_exception_off",
     '    if (atom.kind === "set_property" && (pl.key === "locked" || pl.key === "visible")) return null;',
     '    if (atom.kind === "set_property" && pl.key === "locked") return null;',
     ["§D25 锁层里改 visible"]),
    # ㉗ 不查对象所在图层的锁
    ("d23_layer_lock_ignored",
     '        : (layerLocked(view.layer_id) ? "所在图层已锁定" : null);',
     "        : null;",
     ["§D23 锁层里的对象被拒"]),
    # ── §E update_layer ────────────────────────────────────────────────────
    # ㉘ 入口校验被关掉（patch 非对象报成「没有可修改的图层属性」）
    ("up_no_validate",
     '      const specBad = p2ValidateArgsFor("update_layer", body);',
     "      const specBad = null;",
     ["§E12 patch 非对象"]),
    # ㉙ 照抄服务端的半写（先落盘的键不再回滚）
    ("up_half_write",
     '''      for (const key of keys) {
        const bad = p2PropertyValueError({ layer_id: layerId, key, value: patch[key] });
        if (bad) return p2ToolFailure(400, "invalid_argument", bad);
      }''',
     '''      for (const key of keys) {
        const bad = p2PropertyValueError({ layer_id: layerId, key, value: patch[key] });
        if (bad) break;
      }''',
     ["§E19 半写分歧"]),
    # ㉚ 提交期引用校验被关掉
    ("up_no_ref",
     '''      const guard = p2RefGuard(rows, layers, neutral, "set_property", { layer_id: layerId });
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      // **∴ 值类型校验 ✗**''',
     '''      const guard = null;
      // **∴ 值类型校验 ✗**''',
     ["§E15 图层不存在"]),
    # ㉛ 只写第一个键（count 与 atoms 的恒等式对上，但数量错了）
    ("up_count_hardcoded",
     "      const keys = UPDATE_LAYER_KEYS.filter((k) => patch[k] !== undefined);",
     "      const keys = UPDATE_LAYER_KEYS.filter((k) => patch[k] !== undefined).slice(0, 1);",
     ["§E2 两个键"]),
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

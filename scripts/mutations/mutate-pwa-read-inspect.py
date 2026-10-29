#!/usr/bin/env python3
"""**变异验证**（batch9 读测三端点，本地层 ✓）：作者没随包带电池（batch10–13 有 ✓），
本电池是**我们自己的**变异记录（规矩：四层齐的第 4 层 ✓，粒度＝断言 ✓）。

把 `web/api-local.js` 的「图层作用域三处补偿／合成原子单次折回／list_effects 排序／
luma 桶权重」逐处改坏，看 `scripts/tool-pwa-read-inspect.mjs` 里**该红的那条**是不是真红。

规矩（`AGENTS.md`）：变异只动 `web/`（生成拷贝 ✓），还原 = 源头覆盖回来 ＋ `cmp` 逐字节 ✓；
跑完必须还原（`finally` ✓）；还原后复测必须全绿 ✓。

用法：`python3 scripts/mutations/mutate-pwa-read-inspect.py`
"""

import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[2])
WEB = REPO / "web/api-local.js"
SRC = REPO / "crates/yanshi-http/assets/api-local.js"
CRIT = "scripts/tool-pwa-read-inspect.mjs"

ORIGINAL = WEB.read_text(encoding="utf-8")
SOURCE = SRC.read_text(encoding="utf-8")
if ORIGINAL != SOURCE:
    print("⊘ 不可判：web/ 与源头不同 ⇒ 先跑 node scripts/pwa-sync-viewer.mjs（判据前提不成立）")
    sys.exit(3)

MUTATIONS = [
    # ① 补偿①关掉：隐藏层 H1 不再被合成成可见 ⇒ §C7 只剩背景
    ("visible_comp_off",
     "push({ layer_id: targetId, key: \"visible\", value: true });",
     ";",
     ["§C7"]),
    # ② 补偿②关掉：组内层 L2 不断开父链 ⇒ 继承组不透明度 0.5 ⇒ alpha255 掉到 ≈128
    ("parent_break_off",
     "if (target.parent_id) push({ layer_id: targetId, key: \"parent_id\", value: null });",
     "if (false && target.parent_id) push({ layer_id: targetId, key: \"parent_id\", value: null });",
     ["§C6"]),
    # ③ 补偿③关掉：纯调整层 L3 不再补 clipping_mask ⇒ invert 播种下方 ⇒ 背景被反相
    ("clipping_comp_off",
     "if (any && only && !target.clipping_mask) {",
     "if (false && any && only && !target.clipping_mask) {",
     ["§C5"]),
    # ④ list_effects 排序关掉（保持插入序）⇒ §A「顺序应为 (layer_id, z, id)」红
    #    （锚必须**整块**：只换第一行会截断跨行表达式 ⇒ 语法错 ⇒ 内核起不来 ⇒ exit(3) ✓
    #     ——本电池第一次跑就栽在这里，红的是电池自己 ✗⇒已修 ✓）
    #    （**首个红在 compareBody** ✗：插入库序 (L6,F2)(L1,A0)(L3,A1)(L3,A2)(L4,F1) 与
    #      权威序 (A0,A1,A2,F1,F2) 不同 ⇒ 全字段比对先炸 ⇒ 期望两条都认 ✓）
    ("effects_sort_off",
     """effects.sort((a, b) => cmpId(String(a.layer_id), String(b.layer_id))
        || (a.__z - b.__z) || cmpId(String(a.object_id), String(b.object_id)));""",
     "effects.sort(() => 0);",
     ["§A 顺序应为", "§A list_effects 全表"]),
    # ⑤ 合成原子渲后折回关掉 ⇒ 内核增量折叠把上一次作用域的状态当成「日志没看」
    #    （六百六十五轮实测形态：L5 墓碑层混进 H1 的黄色 ⇒ 后一次作用域拿到脏状态）
    ("foldback_off",
     "          if (Array.isArray(extraAtoms)) {\n            const back = String(k.load_atoms_json(JSON.stringify(atoms.map((r) => r.atom))) || \"\");",
     "          if (false && Array.isArray(extraAtoms)) {\n            const back = String(k.load_atoms_json(JSON.stringify(atoms.map((r) => r.atom))) || \"\");",
     ["§C"]),
    # ⑥ luma 权重常数项写错（Rec.601 的 +500 舍入项被去掉）⇒ 与服务端桶逐桶不符
    ("luma_rounding_off",
     "luma[Math.floor((299 * pr + 587 * pg + 114 * pb + 500) / 1000)] += 1;",
     "luma[Math.floor((299 * pr + 587 * pg + 114 * pb) / 1000)] += 1;",
     ["§C1"]),
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
            if code == 0:
                print(f"  {name}: 判据**没红**（退出码 0）✗")
                bad.append(name)
            elif code == 3:
                print(f"  {name}: 判据 exit(3)（不可判，不是产品失败）✗")
                bad.append(name)
            elif not hits:
                print(f"  {name}: 红了但**该红的断言没出现** ✗ 缺 {expected}")
                print("    " + "\n    ".join(out.strip().splitlines()[-8:]))
                bad.append(name)
            else:
                print(f"  {name}: ✓ 红在该红的那条（{hits[0]}）")
    finally:
        shutil.copyfile(SRC, WEB)
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

#!/usr/bin/env python3
"""**变异验证**（PWA 本地层 ✓）：把 `web/api-local.js` 的 `revert_to_seq` 分支改坏，
看 `scripts/tool-pwa-revert-to-seq.mjs` 里**该红的那条**是不是真红、**别的**是不是没被带红。

规矩（`AGENTS.md`）：变异粒度是**断言**（不是判据 ✓）；跑完**必须还原**（`finally` ✓）；
还原要**逐字节**（`cmp` ✓）；还原后复测必须全绿 ✓。

`web/api-local.js` 是 `crates/yanshi-http/assets/api-local.js` 的**生成拷贝** ✓
⇒ 变异只动 `web/`，还原 = 用**源头那份**覆盖回来 ＋ `cmp` ⇒ 不污染单一真源 ✓。

用法：`python3 scripts/mutations/mutate-pwa-revert-to-seq.py`
"""

import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[2])
WEB = REPO / "web/api-local.js"
SRC = REPO / "crates/yanshi-http/assets/api-local.js"
CRIT = "scripts/tool-pwa-revert-to-seq.mjs"

ORIGINAL = WEB.read_text(encoding="utf-8")
SOURCE = SRC.read_text(encoding="utf-8")
if ORIGINAL != SOURCE:
    print("⊘ 不可判：web/ 与源头不同 ⇒ 先跑 node scripts/pwa-sync-viewer.mjs（判据前提不成立）")
    sys.exit(3)

MUTATIONS = [
    # ① 取消了 `seq > target` 的筛子（目标及之前的笔也被撤）
    ("seq_filter_off",
     "if (!(seq > target)) continue;",
     "if (false) continue;",
     ["§A revert_to_seq"]),  # 本地多撤了第 1 笔 ⇒ 全字段比对先红
    # ② 取消了「净荷必须有 `source` 键」的筛子（`draw_shape` 也被撤）
    ("source_filter_off",
     'if (!(pl && typeof pl === "object"\n              && Object.prototype.hasOwnProperty.call(pl, "source"))) continue;',
     "if (false) continue;",
     ["§F 只撤笔迹"]),
    # ③ 取消了「朴素曾撤过」的跳过（已撤的笔被撤第二遍）
    ("already_reverted_off",
     "if (alreadyReverted.has(id)) continue;",
     "if (false) continue;",
     ["§C 已撤跳过"]),
    # ④ 空集支的原因串被改写
    ("empty_note_wrong",
     'note: "没有需要撤销的笔迹（seq 大于它的一笔都没有）", server: false });',
     'note: "无可撤销笔迹", server: false });',
     ["§B 空集"]),  # 本地改串 ⇒ 全字段比对先红（原因串那条断言读的是 s.body 即服务端 ✓）
    # ⑤ 回执里的 `seq` 不回显目标
    ("seq_echo_wrong",
     "      out.seq = target;\n      out.server = false;",
     "      out.seq = 0;\n      out.server = false;",
     ["§A revert_to_seq"]),
    # ⑥ 工具级 `ok:false` 回到一律 200（与服务端 400 分歧）
    ("status_mapping_off",
     "      // **逐条失败时 `ok:false` ⇒ 状态照服务端映射（400 ✓）** ✗ 不是 200 ✓。\n      return json(out, p2ToolStatusOf(out));",
     "      return json(out);",
     ["§D 起点之前逐条失败：状态码应两边相同"]),
    # ⑦ 入口声明校验被关掉（缺 seq／null／字符串／小数／未知键静默通过）
    ("validation_off",
     'const specBad = p2ValidateArgsFor("revert_to_seq", body);',
     "const specBad = null;",
     ["§E1 缺 seq"]),
    # ⑧ 跨求值起点检查被关掉（`__p2RevertBatch` 共用这段）
    ("origin_check_off",
     "if (typeof seq === \"number\" && seq <= origin) {",
     "if (false) {",
     ["§D 起点之前逐条失败"]),
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

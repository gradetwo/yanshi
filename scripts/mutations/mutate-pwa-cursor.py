#!/usr/bin/env python3
"""**变异验证**（PWA 本地层 ✓）：把 `web/api-local.js` 的每条修复改坏，
看 `scripts/tool-pwa-api-behaviour.mjs` 里**该红的那条**是不是真红。

规矩（`AGENTS.md`）：变异粒度是**断言**；跑完**必须还原**（`finally` ✓）；
还原后复测必须全绿 ⇒ 证明变异真的被撤掉了 ✓。

`web/` 是 `crates/yanshi-http/assets/` 的**生成拷贝**（判据读的是 `web/` ✓）
⇒ 变异只动 `web/`，还原 = 从源头重新同步 ⇒ 不会污染单一真源 ✓。
"""

import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent)
WEB = REPO / "web/api-local.js"
SRC = REPO / "crates/yanshi-http/assets/api-local.js"
ORIGINAL = WEB.read_text(encoding="utf-8")

MUTATIONS = [
    # ⑧ 游标越过未交付的评论（= 修复前的原代码 ✓）
    ("cursor_over_undelivered",
     """      const page = comments.slice(0, limit);
      const nextSince = page.length ? page[page.length - 1].seq : since;""",
     """      const page = comments.slice(0, limit);
      const nextSince = comments.length ? comments[comments.length - 1].seq : since;""",
     ["照 next_since 轮询必须收齐 5 条"]),
    # ⑨ 「被你夹了」不回显 ⇒ 只有 ⑨ 该红
    ("clamp_never_reported",
     "                    limit_clamped: requestedLimit > limit,",
     "                    limit_clamped: false,",
     ["被夹过要留痕"]),
    # ⑩ 读不出就静默回落（= 修复前 `Number(x) || 0` 的行为 ✓）
    ("silent_fallback",
     """    if (!Number.isFinite(n) || !Number.isInteger(n) || n < 0) {""",
     """    if (false) {""",
     ["since_seq:-1 必须被拒绝并点名参数"]),
]


def run_criterion():
    proc = subprocess.run(["node", "scripts/tool-pwa-api-behaviour.mjs"],
                          cwd=REPO, capture_output=True, text=True)
    return proc.returncode, (proc.stdout + proc.stderr)


def main():
    bad = []
    try:
        for name, old, new, expected in MUTATIONS:
            assert ORIGINAL.count(old) == 1, f"{name}: 锚点出现 {ORIGINAL.count(old)} 次（必须恰好 1 次）✗"
        for name, old, new, expected in MUTATIONS:
            WEB.write_text(ORIGINAL.replace(old, new, 1), encoding="utf-8")
            code, out = run_criterion()
            hits = [e for e in expected if e in out]
            missing = [e for e in expected if e not in hits]
            if code == 0:
                print(f"  {name}: 判据**没红**（退出码 0）✗")
                bad.append(name)
            elif missing:
                print(f"  {name}: 红了，但缺 expected 断言 {missing} ✗（输出尾部：{out[-200:]}）")
                bad.append(name)
            else:
                print(f"  {name}: 退出码 {code}，红在该红的地方 ✓（{hits}）")
    finally:
        WEB.write_text(ORIGINAL, encoding="utf-8")

    code, out = run_criterion()
    print(f"  还原后复测：退出码 {code}（应为 0）")
    if code != 0:
        bad.append("restore-check")
        print(out[-300:])
    if SRC.read_text(encoding="utf-8") != WEB.read_text(encoding="utf-8"):
        # 缓存戳归一化后仍应一致 ⇒ 这里只做**字面**比较的提示，不判失败 ✓
        print("  ⊘ 提示：源头与 web 字面不同（缓存戳差异属正常 ✓，由 tool-web-assets-parity 负责判定）")
    print(f"\n变异：{len(MUTATIONS) + 1 - len(bad)}/{len(MUTATIONS) + 1} 通过")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""**变异验证**（判据第四层 ✓）：把每一条修复改坏，看**该红的那条测试**是不是真红。

规矩（`AGENTS.md`）：
- 变异粒度是**断言**，不是判据 ⇒ 每条断言配一个变异 ✓；
- 变异前先问「这条断言读的是**哪条路径**」⇒ 在**那条路径**上改 ✓；
- 跑完**必须还原**（`finally` ✓），还原后再测一次应当全绿 ✓。

用法：`python3 tools/mutate-read-cursor.py`（在仓库根跑也行，路径自己算）。
"""

import subprocess
import sys
from pathlib import Path

REPO = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent)
TOOLS = REPO / "crates/yanshi-server/src/tools.rs"

# ① 游标越过未交付的评论（= 修复前的原代码 ✓）⇒ 该红：polling 与 ①′
CURSOR_OVER_PAGES = (
    """    let truncated = count > limit;
    comments.truncate(limit);
    let returned = comments.len();
    let next_since = comments
        .last()
        .and_then(|item| item.get("seq"))
        .and_then(Value::as_u64)
        .unwrap_or(since);
    comments.reverse();
""",
    """    let truncated = count > limit;
    let next_since = comments
        .last()
        .and_then(|item| item.get("seq"))
        .and_then(Value::as_u64)
        .unwrap_or(since);
    comments.reverse();
    comments.truncate(limit);
    let returned = comments.len();
""",
)
# ①″ 交付顺序不变，但游标取**截断之前**的最大 ⇒ 未交付的那几条被越过 ⇒ 两条都该红
CURSOR_BEFORE_TRUNCATE = (
    """    let truncated = count > limit;
    comments.truncate(limit);
    let returned = comments.len();
    let next_since = comments
        .last()
        .and_then(|item| item.get("seq"))
        .and_then(Value::as_u64)
        .unwrap_or(since);
    comments.reverse();
""",
    """    let truncated = count > limit;
    let next_since = comments
        .last()
        .and_then(|item| item.get("seq"))
        .and_then(Value::as_u64)
        .unwrap_or(since);
    comments.truncate(limit);
    let returned = comments.len();
    comments.reverse();
""",
)
# ①‴ 只坏「满页信号」⇒ `truncated` 恒假 ⇒ 只有 ①′ 该红 ✓
TRUNCATED_ALWAYS_FALSE = (
    """    let truncated = count > limit;
    comments.truncate(limit);
""",
    """    let truncated = false;
    comments.truncate(limit);
""",
)
# ② 不回显 limit／游标 ⇒ 该红：echoes 与 returns_a_cursor
NO_LIMIT_ECHO = (
    """        "limit": limit,
        "limit_requested": requested_limit,
        "limit_clamped": requested_limit as usize > limit,
        "next_since": next_since,
""",
    "",
)
# ②′ 「被你夹了」与「还有下一页」混成一句 ⇒ 只该红 echoes（证明这两条断言读不同路径 ✓）
CLAMP_NEVER_REPORTED = (
    '        "limit_clamped": requested_limit as usize > limit,\n',
    '        "limit_clamped": false,\n',
)
# ③ 读不出就静默回落缺省（= 修复前 `optional_u64(...).unwrap_or(...)` 的行为 ✓）
SILENT_FALLBACK = (
    """    Err(YanshiError::new(
        ErrorCode::InvalidArgument,
        ErrorContext::detail(format!(
            "参数 {key} 必须是不小于 0 的整数（它是计数／序号，不能是负数或小数），得到 {value}；\\
             要「从头开始」就**别传这个参数**（缺省即 0），传 0 也可以"
        )),
    ))""",
    "    Ok(None)",
)
# ③′ 整数形态的浮点（`5.0`）读不出来 ⇒ 只该红 float 那条 ✓
FLOAT_IGNORED = (
    """    if let Some(number) = value.as_f64() {
        if number.fract() == 0.0 && number >= 0.0 && number < u64::MAX as f64 {
            return Ok(Some(number as u64));
        }
    }
""",
    "",
)

MUTATIONS = [
    ("cursor_over_undelivered", CURSOR_OVER_PAGES, ["polling_the_comment_cursor_delivers_every_comment"]),
    # 交付的仍是「最早一页」，但游标取**截断之前**的最大 ⇒ 两条都该红 ✓
    ("cursor_before_truncate", CURSOR_BEFORE_TRUNCATE, ["polling_the_comment_cursor_delivers_every_comment", "a_full_page_of_comments_says_there_are_more"]),
    # 只坏「满页信号」这一条断言 ⇒ 只有 ①′ 该红（证明它与轮询不漏读的是**不同路径** ✓）
    ("truncated_always_false", TRUNCATED_ALWAYS_FALSE, ["a_full_page_of_comments_says_there_are_more"]),
    ("no_limit_echo", NO_LIMIT_ECHO, ["find_atom_echoes_the_limit_it_applied", "find_atom_returns_a_cursor_so_the_rest_is_reachable"]),
    ("clamp_never_reported", CLAMP_NEVER_REPORTED, ["find_atom_echoes_the_limit_it_applied"]),
    ("silent_fallback", SILENT_FALLBACK, ["a_negative_cursor_is_refused_not_treated_as_zero"]),
    ("float_ignored", FLOAT_IGNORED, ["an_integer_written_as_a_float_is_still_honoured"]),
]

ORIGINAL = TOOLS.read_text(encoding="utf-8")


def run_tests():
    proc = subprocess.run(
        ["cargo", "test", "-p", "yanshi-server", "--test", "read_cursor_resume"],
        cwd=REPO, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    failed = [line.split()[1] for line in out.splitlines()
              if line.startswith("test ") and "... FAILED" in line]
    compile_error = "error[" in out or "error: could not compile" in out
    return failed, compile_error, out


def main():
    results = []
    try:
        for name, (old, new) in [(m[0], m[1]) for m in MUTATIONS]:
            assert ORIGINAL.count(old) == 1, f"{name}: 锚点出现 {ORIGINAL.count(old)} 次（必须恰好 1 次）✗"
        for name, (old, new), expected in MUTATIONS:
            TOOLS.write_text(ORIGINAL.replace(old, new, 1), encoding="utf-8")
            failed, compile_error, out = run_tests()
            if compile_error and not failed:
                print(f"  ⊘ {name}: 编译就坏了（不算变异证据）\n{out[-400:]}")
                results.append((name, failed, expected, "compile-broken"))
            else:
                missing = [t for t in expected if t not in failed]
                verdict = "红在该红的地方 ✓" if not missing else f"没红：{missing} ✗"
                print(f"  {name}: 红 {sorted(failed)} ⇒ {verdict}")
                results.append((name, failed, expected, "ok" if not missing else "FAILED-TO-RED"))
        # 还原并复测（证明变异真的被撤掉了 ✓）
        TOOLS.write_text(ORIGINAL, encoding="utf-8")
        failed, compile_error, out = run_tests()
        print(f"  还原后复测：失败 {failed or '无'}（应当为空）")
        results.append(("restore-check", failed, [], "ok" if not failed and not compile_error else "NOT-RESTORED"))
    finally:
        TOOLS.write_text(ORIGINAL, encoding="utf-8")

    bad = [r for r in results if r[3] not in ("ok",)]
    print(f"\n变异：{len(results) - len(bad)}/{len(results)} 通过")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()

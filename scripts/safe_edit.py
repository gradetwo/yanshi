#!/usr/bin/env python3
"""安全地做**结构化文本替换** ✓ —— 本仓库开发脚本的公共小工具。

为什么需要它 ✓：本会话我用 `open(path, "w")` 改源码时，脚本中途 `write(None)` 抛错 ✗，
而 **`open(..., "w")` 会先截断文件** ✗ ⇒ 一个 2999 行的源文件被清空 ✓（幸好那只是 worktree ✓）。

规则（由本模块强制 ✓）：
1. **先读全文** ✓，在内存里替换 ✓；
2. **每一处都必须命中** ✓，否则**拒绝写入** ✗（不再出现"以为改了其实没改" ✗）；
3. 写**临时文件** ✓，再 `os.replace` **原子替换** ✓（失败也不会留下半个文件 ✓）；
4. 可选**写后校验**（关键词计数 ✓）。

用法：
    python3 -c "import sys; sys.path.insert(0,'scripts'); from safe_edit import safe_replace; ..."
或直接在脚本里 `from safe_edit import safe_replace`（把 `scripts` 加进 `sys.path` ✓）。
"""

from __future__ import annotations

import os
import tempfile


def safe_replace(path: str, pairs, expect: dict | None = None) -> str:
    """把 `pairs`（`(old, new)` 列表）逐一替换进 `path` ✓，返回替换后的全文。

    * 任一片段**未命中** ⇒ 抛 `SystemExit` 且**不写入** ✓；
    * `expect`：`{片段: 至少出现次数}` ✓，用于"写后校验关键改动确实落地" ✓。
    """
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    original = text
    for old, new in pairs:
        if old not in text:
            raise SystemExit(f"未命中片段（拒绝写入 {path}）：{old[:70]!r}")
        text = text.replace(old, new, 1)
    if text == original:
        raise SystemExit(f"替换后内容未变化（{path}）")
    if expect:
        for fragment, count in expect.items():
            found = text.count(fragment)
            if found < count:
                raise SystemExit(f"写后校验失败：{fragment!r} 应至少 {count} 处，实得 {found}")
    directory = os.path.dirname(os.path.abspath(path))
    # **保留原文件权限** ✓ —— `mkstemp` 产出的是 0600 ✓，直接改名会把可执行脚本变成
    # 不可执行 ✗（本会话真实踩到：`scripts/with-temp-server.sh` 在替换后"权限不够" ✓，
    # 而上一轮提交里那个 `mode change 100755 => 100644` 也是同一个原因 ✓）。
    mode = os.stat(path).st_mode
    fd, tmp = tempfile.mkstemp(dir=directory, suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(text)
        os.chmod(tmp, mode)
        os.replace(tmp, path)
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise
    return text


if __name__ == "__main__":  # 自检：临时文件往返一次 ✓
    import tempfile as _tmp

    with _tmp.NamedTemporaryFile("w", suffix=".txt", delete=False) as handle:
        handle.write("alpha beta\n")
        probe = handle.name
    try:
        safe_replace(probe, [("beta", "gamma")], expect={"gamma": 1})
        with open(probe, encoding="utf-8") as handle:
            assert handle.read() == "alpha gamma\n"
        try:
            safe_replace(probe, [("不存在的片段", "x")])
        except SystemExit:
            pass
        else:
            raise AssertionError("未命中片段时必须拒绝写入")
        print("  safe_edit 自检通过 ✓")
    finally:
        os.unlink(probe)

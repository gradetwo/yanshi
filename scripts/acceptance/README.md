# 真机验收脚本 ✓（不是单元测试 ✓）

这些脚本用 **CDP + headless Chromium** 对着**真正跑着的编辑器**做验收 ✓ ——
目标是"内核先行→工具放行→确定性测试→**真实浏览器验收**"里的最后一步 ✓。

```bash
# ① 起一个带调试端口的 Chromium（profile 放**大分区** ✓ —— /tmp 常常只有 2GB ✗）
chromium --headless=new --remote-debugging-port=9335 \
  --user-data-dir=/home/crow/yanshi-tmp/cdp-profile --no-first-run --disable-gpu &

# ② 起一个被测实例（用临时 workspace ✓，别动线上那份 ✓）
cargo build --release -p yanshi-http
./target/release/yanshi-serve --bind 127.0.0.1:8215 --root /tmp/yanshi-accept &

# ③ 跑脚本 ✓（页面侧代码放在单独 .js 里 ✓ —— **不要用模板字符串** ✗：
#    本会话被"模板字符串里的反引号"绊倒过四次 ✓，从结构上根除 ✓）
YANSHI_CDP=9335 YANSHI_PATH="/?doc=<docId>&token=<token>" \
  node scripts/acceptance/run-page.mjs scripts/acceptance/ui-toggles.js 8215
```

| 脚本 | 验什么 |
|---|---|
| `ui-toggles.js` ✓ | 图标/快捷键提示 ✓、两列工具栏 ✓、隐藏左栏与右栏窗口 ✓、工作区预设 ✓、全屏画布 ✓、光标处右键快捷面板 ✓ |
| `paint-plain-brush.js` ✓ | **普通笔刷**在"**还没有图层的**文档"上落笔 ✓（回归用 ✓：修前服务端墨量为 0 ✗、修后 > 0 ✓） |
| `paint-medium-tool.js` ✓ | **介质工具**（`data-tool="medium_dab"` ✓）落笔 ✓ —— 注意必须**先选那个工具** ✗，否则测的是另一条路径 ✓ |
| `draw-shape-without-layer.js` ✓ | 把文档的图层**删光**再画矩形 ✓ —— 验"失败必须留痕" ✗（日志里应出现明确原因 ✓） |
| `probe-hash.js` ✓ | 画布整幅哈希与"有墨"像素数 ✓（**只读** ✓） |

**量墨请用文档空间** ✓：`render_region {region, raw:true}` ⇒ 取 `raw_url` 数像素 ✓
—— 数**视口画布**的墨会随缩放/平移抖动 ✗（我为此浪费过一轮 ✓）。

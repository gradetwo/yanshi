#!/usr/bin/env bash
# **扫一张 PNG 里有没有"单通道爆满、其余近 0"的可疑像素** ✓
#   （YANSHI_ISSUES §11／§12 报的那种 `[255,5,1]`／`[253,8,3]` ✗）
#
# **∴ 为什么需要它 ✓**：手采样 12 个点**看不出一块 1300 像素的红**✗ ——
#   我为此白试了三次 ✓。**∴ 全幅逐像素扫**才是对的仪器 ✓。
#
# **用法**：scripts/scan-suspicious-pixels.sh <png> [阈值高位] [阈值低位]
#   ⇒ 默认 `max>=200 且 min<=30` ✓（§11/§12 的样本都落在这个区间 ✓）
# **退出码**：0 = 没有可疑像素 ✓；1 = 有（打印前 10 个的坐标与颜色 ✗）；3 = 跑不了
set -u
PNG="${1:-}"
HI="${2:-200}"
LO="${3:-30}"
[ -n "$PNG" ] || { echo "用法: $0 <png> [hi] [lo]"; exit 3; }
[ -f "$PNG" ] || { echo "  ✗ 文件不存在：$PNG"; exit 3; }
command -v convert >/dev/null 2>&1 || { echo "  ✗ 需要 ImageMagick 的 convert"; exit 3; }

W=$(identify -format "%w" "$PNG" 2>/dev/null) || { echo "  ✗ identify 读不了 $PNG"; exit 3; }
H=$(identify -format "%h" "$PNG" 2>/dev/null) || exit 3
echo "  扫描 ${PNG}（${W}×${H}，判定 max>=${HI} 且 min<=${LO}）✓"

convert "$PNG" -depth 8 rgb:- | python3 -c "
import sys
W=$W; H=$H; HI=$HI; LO=$LO
raw=sys.stdin.buffer.read()
n=len(raw)//3
bad=[]
for i in range(n):
    r,g,b=raw[i*3],raw[i*3+1],raw[i*3+2]
    if max(r,g,b)>=HI and min(r,g,b)<=LO:
        bad.append((i%W, i//W, (r,g,b)))
        if len(bad)>=10: break
tot=len(bad)
print(f'  **可疑像素：{len(bad) if tot<10 else \"10+\"}**（前 {len(bad)} 个）')
for x,y,c in bad: print(f'     ({x},{y}) ⇒ {c}')
sys.exit(1 if bad else 0)
"
rc=$?
[ $rc -eq 0 ] && echo "  ✓ 没有可疑像素" || echo "  ✗ 发现可疑像素（见上）"
exit $rc

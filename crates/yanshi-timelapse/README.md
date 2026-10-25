# yanshi-timelapse

将 `.yanshi` 工程文件导出为绘画过程视频（从第一笔开始）。

## 实测性能

| 版本 | 耗时 | 说明 |
|---|---|---|
| Python 原版 | 113.2秒 | PIL LANCZOS 每帧缩放 |
| Rust 单线程 pipe | **5.0秒** | 直接 pipe 给 ffmpeg, ultrafast preset |

测试工程：`parrot-4k-1000plus.yanshi`（1050 atoms, 838 有效笔触, 4K）

输出：1080p, 30fps, 84帧, H.264

## 使用

```bash
# 需要安装 ffmpeg
# 只有输入文件是必填的，其它都有默认值
cargo run --release -p yanshi-timelapse -- <输入.yanshi.zip> [输出.mp4] [每N个atom一帧] [起始atom] [结束atom]

# 示例
cargo run --release -p yanshi-timelapse -- in.zip
# → 输出: in-timelapse.mp4, 每10个atom一帧, 从开头到结尾

cargo run --release -p yanshi-timelapse -- in.zip out.mp4
# → 指定输出文件名

cargo run --release -p yanshi-timelapse -- in.zip "" 1
# → 每个atom一帧 (最精细, 最慢), 输出用默认名

cargo run --release -p yanshi-timelapse -- in.zip "" 5 100 500
# → 从第100个atom到第500个, 每5个一帧
```

## 参数

| 参数 | 必填 | 默認 | 說明 |
|---|---|---|---|
| 输入.yanshi.zip | ✅ | - | 工程文件 |
| 输出.mp4 | ❌ | `<輸入名>-timelapse.mp4` | 輸出視頻 |
| 每N个atom一帧 | ❌ | 10 | 数字越小越精细 |
| 起始atom | ❌ | 0 | 从第几个开始 |
| 结束atom | ❌ | 全部 | 到第几个结束 |

注意：指定起始位置时，前面的 atoms 仍会被应用（以建立正确的画布状态），只是不渲染成帧。

## 退出码

| 码 | 含义 |
|---|---|
| `0` | 出片成功 |
| `1` | 渲染/编码失败 |
| `2` | **输入包不完整**：原子引用的 blob 不在包里 ⇒ **拒不出片**（从前是 `rc=0` ⇒ 产出一条看起来正常的空影片）；用 `export_project` 且带 `include_bitmaps: true` 重打一份完整包 |


## 原理

1. 从 zip 读取 `.yanshi` (tar 格式)
2. 解析 `atoms.jsonl`, 过滤 `revert` 掉的笔触
3. 提取 blob, nearest 降采样到目标分辨率
4. 按顺序整数 alpha 合成到画布
5. 每 N 个 atom, 把 RGB 帧直接 pipe 给 ffmpeg (rawvideo)
6. ffmpeg 用 `ultrafast` preset 硬编码 H.264

## 优化记录

- v1: Python + PIL LANCZOS = 113.2s
- v2: Python 直接1080p合成 = 82.0s (1.38x)
- v3: Rust 单线程 = 14.4s (7.9x)
- v4: Rust + pipe to ffmpeg = 5.0s (22.6x)
- v5: Rust + rayon = 6.4s (反而更慢, 线程开销 > 收益, 已废弃)

# `.yanshi` 工程包格式（供 timelapse / 导入端对齐）

**⇒ 本文是**唯一权威**（第 1344 轮，由**本仓 `export_project` 的真实输出**核对而来，不是推测）。**

## 一、包是**裸 tar**（不压缩）

**⇒ `yanshi_server::archive::read_tar` 直接读 ⇒ **不是** `.tar.gz` ✓**

## 二、顶层条目

| 条目 | 必需 | 说明 |
|---|---|---|
| `atoms.jsonl` | **是** | 每行一个原子；**没有它就报「不像一个 .yanshi 工程包」** |
| `meta.json` | **是**（timelapse 用）| 读 `width`/`height`（**画布尺寸**），可选 `doc_id` |
| `blobs.encoding` | 有 blob 时**必需** | 内容就是 `zlib`（**只认这一个值**；缺了会被**拒绝**导入）|
| `blobs/sha256/<2>/<2>/<64 位 hex>` | 有图时必需 | 见第三节 |
| `render.seq` / `BUILD-INFO` / `README.txt` | 否 | timelapse 计为「忽略条目」 |

## 三、★ blob 的**命名与编码**（最容易错的地方）

**⇒ 实测（第 1344 轮，本仓导出的包里取一个 blob）：**

| 项 | 值 |
|---|---|
| 路径 | `blobs/sha256/ba/b8/bab8751e816e…822f` |
| **文件字节数** | **86**（原 PNG **75** ⇒ **文件里是**压缩后**的字节）|
| **文件名里的哈希** | **`bab8751e…822f` = `sha256(压缩前的明文字节)`** ✓ |
| `sha256(压缩后的字节)` | `823c2fd6…` ⇒ **与文件名不同** |

**⇒ ⇒ 所以规则是：**
> **文件名 = `sha256(明文)`；文件内容 = `zlib(明文)`。**

**⇒ 为什么**（源码 `crates/yanshi-server/src/service.rs:2177`）：
> **先按 `blobs.encoding` 解码、再对「解出来的明文」核哈希** —— 哈希的定义就是**对明文**算的。

**⇒ ⇒ 而**用「压缩后哈希」命名的后果**（外部实测踩到）✗：
1. 解压后算哈希 ⇒ **与文件名不符** ⇒ 报 `⚠️ blob xxx 的实际哈希是 yyy`
2. blob 被记到**错误的键**上 ⇒ 原子引用的那个哈希**查不到**
3. ⇒ `import_image` 找不到图 ⇒ **什么都不画** ⇒ **全白帧**（1612 帧均值 255.0）

## 四、`blobs.encoding` 的合法取值

**⇒ 本版本**只有 `zlib`**（`service.rs:59` 的 `PACKAGE_BLOB_ENCODING`）。**

**⇒ ⇒ 三种情形的行为（都是**显式**的，不猜）：**
| 包里 | 行为 |
|---|---|
| 有 blob ＋ `blobs.encoding = zlib` | ✓ 接受，逐个解压并核明文哈希 |
| 有 blob ＋ 别的值（如 `none`）| **拒绝**（`invalid_argument`，说明「只认 zlib」）|
| 有 blob ＋ **没有** `blobs.encoding` | **拒绝**（「不给兜底，否则坏字节会被悄悄导入」）|
| **没有** blob | 不要求清单 ✓ |

## 五、`import_image` 原子**能被 timelapse 回放**（实测 ✓）

**⇒ 用上面那个包跑 `yanshi-timelapse proj.yanshi out.mp4 1`：**

```
画布 64×64 ⇒ 输出 64×64（缩放 1.000）｜blob 1 个（编码 zlib）｜忽略条目 3
4 帧｜apply 0..3｜replay+编码 0.7s
```

**⇒ 末帧实测：**均值 252.6｜非白像素 **79/4096**｜`(12,12) = [199,57,64]`**（我画的是 `[200,60,60]`）✓**

**⇒ ⇒ 所以 `import_image` 型笔迹**可以**出视频 ⇒ 不必改成「真 `stroke` atom」的画法 ✓**

## 六、★ 两条**自己踩到**的 `put_blob` 陷阱（写在这里省别人的时间）

| 陷阱 | 现象 | 正确做法 |
|---|---|---|
| **相对路径** | `path: "in.png"` ⇒ `读不到 in.png` | **用**工作区内**的绝对路径**（相对路径按**进程 CWD** 解析，不是工作区根）|
| **工作区外** | 绝对路径在工作区外 ⇒ `path 必须落在工作区根目录之内` | 先把文件放进**工作区** |

**⇒ 返回字段是 `blob_hash`，带 `sha256:` 前缀**（如 `sha256:bab8751e…`）⇒ 而**包里路径**要**去掉前缀**，只用 64 位 hex ✓

## 七、最小复现配方（照抄即可）

```bash
# 1) 起服务（工作区、导出目录都设好）
YANSHI_EXPORT_DIR=$WS/exports yanshi-serve --root $WS --bind 127.0.0.1:PORT &
# 2) 建文档 + 图层
curl -X POST localhost:PORT/api/documents -d '{"doc_id":"d1","width":64,"height":64}'
curl -X POST "localhost:PORT/api/tools/create_layer?doc=d1&token=$T" -d '{"layer_id":"L","name":"L"}'
# 3) 放图（**绝对路径且在工作区内**）
curl -X POST "localhost:PORT/api/tools/put_blob?doc=d1&token=$T" \
  -d "{\"path\":\"$WS/in.png\",\"mime_type\":\"image/png\"}"    # ⇒ blob_hash
# 4) 建 import_image 原子
curl -X POST "localhost:PORT/api/tools/import_image?doc=d1&token=$T" \
  -d "{\"layer_id\":\"L\",\"bitmap\":{\"blob_hash\":\"$HB\",\"mime_type\":\"image/png\"},\"region\":{\"x\":10,\"y\":10,\"w\":8,\"h\":8}}"
# 5) 导出**工程包**（不要自己拼 tar）
curl -X POST "localhost:PORT/api/tools/export_project?doc=d1&token=$T" -d '{"path":"proj.yanshi"}'
# 6) 出视频
yanshi-timelapse $WS/exports/proj.yanshi out.mp4 1
```

// **★ PWA 的持久化层（IndexedDB）✓ ★**（第 613 轮 ✓；**部署矩阵 §4 ④ ✓**）
//
// **∴ 为什么需要它 ✗**：**本部署**没有服务器端 ✗**（**用户要求 ✓**）⇒
//   **∴ 服务端原本负责的三件事必须由浏览器自己扛 ✗**：
//     **① 工程文件／原子日志的持久化 ✓**；**② blob（**PNG／wasm 资源 ✓**）的托管 ✓**；
//     **③ 缩略图与渲染快照的缓存 ✓**。
//
// **∴ 设计原则（**与目标第 6 条一致 ✓**）**：
//   * **快照必须带**序号 ＋ 格式版本**✗**（`seq` ＋ `FORMAT_VERSION` ✓）⇒
//     **∴ 过期或缺失就**重算 ✗**，**绝不许拿旧图冒充 ✓**；
//   * **∴ 本文件**不做渲染 ✗**（**渲染在内核里 ✓**）⇒ **∴ 它只存取 ✓**；
//   * **∴ 与现有 WEB **零耦合**✗**：**它不调用任何服务端接口路径 ✓**（**判据会断言这一点 ✓**）。
//
// **⚠️ 现状** ✗**：**这是**最小可用实现 ✗**（**库表 ＋ 读写 ＋ 序号校验 ✓**），
//   **∴ 尚未接进 `web/index.html` 的界面流程 ✗**（**那一步在 viewer 静态化之后 ✓**）。

/** **★ 库表结构版本 ✓ ★**：**改动结构必须**同时**改它 ✗**（**否则会静默读到旧结构 ✓**）。 */
export const DB_VERSION = 1;
/** **★ 快照格式版本 ✓ ★**：**渲染快照与它不匹配 ⇒ 必须重算 ✗**（**目标第 6 条 ✓**）。 */
export const FORMAT_VERSION = 1;
const DB_NAME = "yanshi-online";

/** 打开数据库（首次会建表）。 */
export function open() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      // **原子日志**：**按文档主键 ＋ 序号自增 ✓** ⇒ **∴ 支持"回到此处"✓**。
      if (!db.objectStoreNames.contains("atoms")) {
        const s = db.createObjectStore("atoms", { keyPath: "id" });
        s.createIndex("by_doc_seq", ["doc", "seq"], { unique: false });
      }
      // **blob**：**按内容哈希存 ✓**（**与服务端的 blob 语义一致 ✓**）。
      if (!db.objectStoreNames.contains("blobs")) {
        db.createObjectStore("blobs", { keyPath: "hash" });
      }
      // **文档元数据**：**含 `seq` 与 `format` ✓** ⇒ **∴ 快照是否过期可判 ✓**。
      if (!db.objectStoreNames.contains("docs")) {
        db.createObjectStore("docs", { keyPath: "doc" });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export const tx = (db, store, mode) => db.transaction(store, mode).objectStore(store);
export const wrap = (req) => new Promise((res, rej) => { req.onsuccess = () => res(req.result); req.onerror = () => rej(req.error); });

/** 写一条原子（**自动编号 ✓**）。 */
export async function putAtom(db, doc, atom) {
  const meta = (await wrap(tx(db, "docs", "readonly").get(doc))) || { doc, seq: 0 };
  const seq = meta.seq + 1;
  const rec = { id: `${doc}:${seq}`, doc, seq, atom };
  await wrap(tx(db, "atoms", "readwrite").put(rec));
  await wrap(tx(db, "docs", "readwrite").put({ ...meta, doc, seq, format: FORMAT_VERSION }));
  return seq;
}

/** 取某文档的全部原子（**按序号 ✓**）。 */
export async function atomsOf(db, doc) {
  const all = await wrap(tx(db, "atoms", "readonly").getAll());
  return all.filter((r) => r.doc === doc).sort((a, b) => a.seq - b.seq);
}

/**
 * **★ 取快照 ＋ 校验 ✓ ★**（**目标第 6 条的重点 ✗**）：
 * **∴ `seq` 落后 或 `format` 不匹配 ⇒ 返回 `null` ✗** ⇒ **∴ 调用方必须重算 ✓**。
 * **∴ 绝不许返回一张"看起来能用"的旧图 ✗**。
 */
export async function readSnapshot(db, doc, expectedSeq) {
  const meta = await wrap(tx(db, "docs", "readonly").get(doc));
  if (!meta) return null;
  if (meta.format !== FORMAT_VERSION) return null;
  if (meta.seq !== expectedSeq) return null;
  if (!meta.snapshot) return null;
  return meta.snapshot;
}

/** 写快照（**同时写 `seq` ＋ `format` ✓**）。 */
export async function writeSnapshot(db, doc, seq, snapshot) {
  const meta = (await wrap(tx(db, "docs", "readonly").get(doc))) || { doc };
  await wrap(tx(db, "docs", "readwrite").put({ ...meta, doc, seq, format: FORMAT_VERSION, snapshot }));
}

/** 存 blob（**按内容哈希 ✓**）。 */
export async function putBlob(db, hash, bytes) {
  await wrap(tx(db, "blobs", "readwrite").put({ hash, bytes }));
  return hash;
}

/** 取 blob。 */
export function getBlob(db, hash) {
  return wrap(tx(db, "blobs", "readonly").get(hash)).then((r) => (r ? r.bytes : null));
}

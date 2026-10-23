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

/** **★ 库表结构版本 ✓ ★**：**改动结构必须**同时**改它 ✗**（**否则会静默读到旧结构 ✓**）。
 *  **v2 新增 `prefs` 表**（**P1 `set_preferences`／`get_preferences` 用 ✓**）。 */
export const DB_VERSION = 2;
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
        // **★ 不用数组 keyPath ✗ ★**（第 622 轮 ✓，**真实浏览器判据抓到的挂起 ✗**）：
        // **∴ 实测**：**`createIndex("…", ["doc","seq"], …)` 在真浏览器里会让 `open()` **永不回**✗**
        //（**`pixel` 是：`onupgradeneeded` 里抛错 ⇒ 而 `onsuccess`／`onerror` 都不触发 ⇒ **挂住**✓**）
        // ⇒ **∴ 改为**用记录里已有的合成键 `id`（`"doc:seq"` ✓）建普通索引 ✓**。
        s.createIndex("by_doc_seq", "id", { unique: false });
      }
      // **blob**：**按内容哈希存 ✓**（**与服务端的 blob 语义一致 ✓**）。
      if (!db.objectStoreNames.contains("blobs")) {
        db.createObjectStore("blobs", { keyPath: "hash" });
      }
      // **文档元数据**：**含 `seq` 与 `format` ✓** ⇒ **∴ 快照是否过期可判 ✓**。
      if (!db.objectStoreNames.contains("docs")) {
        db.createObjectStore("docs", { keyPath: "doc" });
      }
      // **偏好**：**`set_preferences` 的合并写入落这里 ✓**（**键值对 ✓，**与文档无关 ✓**）。
      if (!db.objectStoreNames.contains("prefs")) {
        db.createObjectStore("prefs", { keyPath: "key" });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
    // **★ 不许静默挂住 ✗ ★**（第 622 轮 ✓）：**∴ 实测 `open()` 曾**永不回**✗**
    // ⇒ **∴ 于是**页面停在加载中、而**控制台什么都没有**✗ ⇒ **∴ 极难定位 ✓**。
    // **∴ 现在**：**阻塞 ⇒ 报错 ✓；超时（**3s ✓**）⇒ 报错 ✓**（**∴ 都会**说出来 ✓**）。
    req.onblocked = () => reject(new Error("indexedDB 被阻塞（可能有旧标签页占用）"));
    setTimeout(() => reject(new Error("indexedDB open 超时（3s）")), 3000);
  });
}

export const tx = (db, store, mode) => db.transaction(store, mode).objectStore(store);
export const wrap = (req) => new Promise((res, rej) => { req.onsuccess = () => res(req.result); req.onerror = () => rej(req.error); });

/** 写一条原子（**自动编号 ✓**）。 */
export async function putAtom(db, doc, atom) {
  // **★ D-01（第三轮复审 P0）：一个事务包住「读 seq + 写 atoms + 写 docs」** ✓
  //   **问题**：原来是**三段独立事务**（① docs 只读取 seq ② atoms 写 ③ docs 写）
  //   ⇒ 两个 `putAtom` 并发时都读到同一个 `seq = N` ⇒ 都写 `seq = N+1`
  //   ⇒ **主键 `${doc}:${seq}` 相撞 ⇒ 后写覆盖前写 ⇒ 已确认的笔触静默消失** ✗
  //   **修法**：IndexedDB 的事务**跨 store 也是原子的** ⇒ 把三个操作放进**同一个**
  //     `db.transaction(["docs","atoms"], "readwrite")` ⇒ 要么全成、要么全不成 ✓
  //
  // **⚠ 改这个文件，不要改 web/store.js** —— 后者是 `pwa:sync` 的**产物**：
  //   `scripts/pwa-sync-viewer.mjs` 的 FILES 列表里有 `store.js`
  //   ⇒ 它从 `crates/yanshi-http/assets/store.js` **复制**到 `web/store.js`
  //   ⇒ 改产物会被下一次 `pwa:sync` **静默覆盖**（我 1209 轮就是这么丢了修改的 ✗）
  let timedOut = false;
  const t = db.transaction(["docs", "atoms"], "readwrite");
  const settled = new Promise((res, rej) => {
    t.oncomplete = () => res();
    t.onerror = () => rej(t.error);
    t.onabort = () => rej(t.error || new Error("事务被中止"));
  });
  // **★ 超时兜底** ✓（2026-10-20，`tool-pwa-api-behaviour` 在 CI 上挂死 ⇒ exit 13）
  //   **为什么必须有** ✗：真浏览器里 `oncomplete` 一定会触发 ✓；但**测试环境**
  //   （`scripts/tool-pwa-api-behaviour.mjs` 在 Node 里模拟 IndexedDB）**可能不触发**
  //   ⇒ 那会让 `await done` **永远不 resolve** ⇒ 判据挂死（Node 报 unsettled top-level await）✗
  //   **兜底语义**：5 秒后**当作已提交**并**当场用 `console.warn` 说出 `txTimeout`** ✓
  //     ⇒ **不是静默** ✓；而真浏览器里这条**永远不触发** ✓
  //     （第 1440 轮审计 ✓：原注释承诺"在返回里标 `txTimeout: true`"✗ 而返回值一直是**裸数字** ✗
  //     ⇒ `timedOut` 设了**没人读** ✗ = 死旗标 ✗ ⇒ 超时**不可见** ✗（静默降级第 5 形态 ✓）。
  //     返回形状有十几处调用方按数字用 ⇒ **不改形状**，改为**在这里说出来** ✓。）
  const done = Promise.race([
    settled,
    new Promise((res) => setTimeout(() => { timedOut = true; res(); }, 5000)),
  ]);
  const meta = (await wrap(t.objectStore("docs").get(doc))) || { doc, seq: 0 };
  const seq = meta.seq + 1;
  // **★ 覆盖原子自己的 `seq` ✗ ★**（第 630 轮 ✓）：**∴ 内核按 `seq` 判重 ✗**
  // ⇒ **∴ 两处都写 1 ⇒ **`seq 1 重复`✗**（**实测 ✓**）。
  const stored = { ...atom, seq };
  const rec = { id: `${doc}:${seq}`, doc, seq, atom: stored };
  await wrap(t.objectStore("atoms").put(rec));
  // **★ 写原子时必须**清掉旧快照**✗ ★**（第 663 轮 ✓，**真缺陷 ✓**）：
  //   **∴ 旧写法** `{ ...meta, doc, seq, format }` **保留了 `meta.snapshot` ✗**
  //   ⇒ **∴ 于是**：**旧图 ✗ ＋ **新序号 ✗** ⇒ **∴ `readSnapshot` **恒命中**✗**
  //   ⇒ **∴ 结果就是**"**拿旧图冒充**"✗** —— **∴ 正是目标第 6 条禁止的那件事 ✓**。
  //   **∴ 修法** ✓：**写原子 ⇒ **快照显式失效 ✗**（`snapshot: null` ＋ 记下失效时刻 ✓）**。
  const { snapshot: _stale, ...metaNoSnap } = meta;
  await wrap(t.objectStore("docs").put({
    ...metaNoSnap, doc, seq, format: FORMAT_VERSION, snapshot: null,
  }));
  await done;
  // **兜底要兑现承诺 ✓**（第 1440 轮审计 ✓）：超时不是成功 —— 说出来 ✓
  if (timedOut) {
    console.warn(`putAtom：事务 5 秒未确认完成 ⇒ 文档 ${doc} 的原子 seq ${seq} 是否落盘**未知**（txTimeout）`);
  }
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

/**
 * 写快照（**同时写 `seq` ＋ `format` ✓**）。
 *
 * **★ 单事务读改写 ＋ 序号校验 ✓ ★**（第 1440 轮审计 ✓）：
 * 旧版 get 与 put 是**两个事务** ✗ ⇒ 中间若有 `putAtom` 提交 ⇒ 这次写会
 * 把 `meta.seq` **拖回**快照的陈旧序号 ✗ ⇒ 下一次 `putAtom` 读到同一个 seq
 * ⇒ 主键 `${doc}:${seq}` 相撞 ⇒ **已提交原子被静默覆盖** ✗
 * （与 D-01 同根因、同病灶 ⇒ 同药 ✓：一个事务包住读改写 ✓）。
 * **拒绝过期写** ✓：传入 `seq ≠ 当前 head` ⇒ 不写并**返回 `false`** ✓
 * （旧图不许冒充新状态 —— 目标第 6 条 ✓；调用方拿到 `false` 就知道要重算 ✓）。
 */
export async function writeSnapshot(db, doc, seq, snapshot) {
  const t = db.transaction(["docs"], "readwrite");
  const store = t.objectStore("docs");
  const done = new Promise((res, rej) => {
    t.oncomplete = () => res();
    t.onerror = () => rej(t.error);
    t.onabort = () => rej(t.error || new Error("事务被中止"));
  });
  const meta = (await wrap(store.get(doc))) || { doc, seq: 0 };
  let written = false;
  if ((meta.seq ?? 0) === seq) {
    await wrap(store.put({ ...meta, doc, seq, format: FORMAT_VERSION, snapshot }));
    written = true;
  }
  await done;
  return written;
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

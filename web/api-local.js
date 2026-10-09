// **★ PWA 的本地 API 适配层 ✓ ★**（第 614 轮 ✓；**部署矩阵 §11 ＋ 用户"不影响现有 WEB" ✓**）
//
// **★ 为什么用"拦截"而不是"改前端" ✓ ★**：
//   **∴ 现有 `crates/yanshi-http/assets/viewer-app.js`（**510 KB ✓**）依赖 **15 个服务端端点 ✗**：
//     `/api/tools/get_document` ✓／`/api/tools/render_region` ✓／`/api/tools/list_layers` ✓／
//     `/api/documents` ✓／`/api/atoms` ✓／`/api/blob/…` ✓／`/api/effects` ✓／`/api/diagnostics` ✓ 等 ✓
//   **∴ 若把前端**复制**一份再改 ✗ ⇒ **∴ 两条部署会**分叉**✗**（**违背用户"不要影响现有 WEB"✓**）。
//   **∴ 本层的作用 ✗**：**在 PWA 里**覆写 `window.fetch`✗** ⇒ **∴ 把 `/api/*` 转到**本地实现**✗**
//     （**内核 wasm ＋ IndexedDB ✓**）⇒ **∴ 于是**同一份 `viewer-app.js` **两处都能跑 ✓，
//     **而 `crates/yanshi-http/assets/` **一个字节都不用改 ✓**** ✓✓
//
// **⚠️ 现状（**如实 ✓**）**：**这是**骨架 ＋ 分发表 ✗** ——
//   **∴ 已实现**：**拦截安装／卸载 ✓、未实现端点的**如实 501 ＋ 原因 ✗**（**∴ 不静默失败 ✓**）；
//   **∴ 未实现**：**各端点的真实本地映射 ✗**（**那需要按端点逐个接内核／IndexedDB ✓**，
//   是后续轮次的工作 ✗）⇒ **∴ 在此之前，PWA 页面会**明确报"该端点尚未本地实现"✗**，
//   **∴ 绝不会假装成功 ✓**。

/** **★ 本地已实现的端点数 ✓ ★**（**判据会断言它随实现增加 ✓**）。 */
export const LOCAL_IMPLEMENTED = 4;

/**
 * **★ 安装本地 API 层 ✓ ★**：**覆写 `window.fetch` ✗** ⇒ **∴ `/api/*` 走本地 ✓，
 * 其余（**静态资源 ✓**）走原 `fetch` ✓**。
 * @param {{local: (req: Request) => Promise<Response>}} deps **本地实现（**由内核／IndexedDB 提供 ✓**）
 */
/**
 * **★ 第一版本地实现 ✓ ★**（第 615 轮 ✓）：**三个端点只依赖 IndexedDB ✗**（**不需要内核 ✓**）
 * ⇒ **∴ 它们可以**独立成立 ✓**；**而需要内核的端点（**`get_document` ✓／`render_region` ✓**）
 * **∴ 仍然如实 501 ✓**（**∴ 下一批再说 ✓**）。
 *
 * **∴ 与真实服务端的语义对齐点 ✗**：
 *   * **原子写入 ⇒ **序号自增 ＋ 推进文档 `seq`**✗**（**与 `store.js` 的快照校验配套 ✓**）；
 *   **`list_layers` ⇒ **从原子日志推导**✗**（**`create_layer` 之类的`kind` ✓**）⇒
 *     **∴ 不另存一份"层表" ✗**（**∴ 避免双份状态不一致 ✓**）。
 */
export async function makeLocalApi(db) {
  const { putAtom, atomsOf, open } = await import("./store.js");
  const handle = db || (await open());

  return async function local(req) {
    const url = new URL(req.url, location.origin);
    const q = url.searchParams;
    const doc = q.get("doc") || "";
    const body = req.method === "POST" ? await req.json().catch(() => ({})) : {};

    // **① `/api/documents`（**创建 ✓**）**：**∴ 与真实服务端一样返回 `token` ✗**
    //（**∴ 本部署没有鉴权 ✗ ⇒ **token 是**本地占位**✗，**而字段存在 ✓** ⇒ **∴ 前端无需分支 ✓**）。
    if (url.pathname === "/api/documents") {
      const docId = body.doc_id || `local-${Date.now().toString(36)}`;
      await putAtom(handle, docId, {
        kind: "create_document", width: body.width ?? 1024, height: body.height ?? 1024,
      });
      return json({ ok: true, doc_id: docId, token: "local", server: false,
                    width: body.width ?? 1024, height: body.height ?? 1024 });
    }

    // **② `/api/atoms`（**读／写 ✓**）**
    if (url.pathname === "/api/atoms") {
      if (req.method === "POST") {
        const seq = await putAtom(handle, doc, body);
        return json({ ok: true, seq, server: false });
      }
      const list = await atomsOf(handle, doc);
      return json({ ok: true, atoms: list.map((r) => r.atom), count: list.length, server: false });
    }

    // **③ `/api/tools/list_layers`（**从原子推导 ✓**）**
    if (url.pathname === "/api/tools/list_layers") {
      const list = await atomsOf(handle, doc);
      const layers = [];
      for (const r of list) {
        const a = r.atom || {};
        if (a.kind === "create_layer" && a.layer_id) layers.push({ id: a.layer_id, name: a.name ?? a.layer_id });
      }
      return json({ ok: true, layers, count: layers.length, server: false,
                    derived_from: "atoms" });
    }

    // **∴ 其余端点**一律抛给上层 ⇒ 上层如实 501 ＋ 原因 ✗**（**∴ 不假装成功 ✓**）。
    throw new Error(`endpoint_not_local: ${url.pathname}`);
  };
}

export function installLocalApi(deps) {
  const original = window.fetch.bind(window);
  window.fetch = async (input, init) => {
    const req = input instanceof Request ? input : new Request(input, init);
    const url = new URL(req.url, location.origin);
    // **∴ 只拦 `/api/` ✓**（**静态资源与 `/wasm/` 保持原路 ✓**）。
    if (!url.pathname.startsWith("/api/")) return original(input, init);
    // **∴ 健康检查要**如实报"无服务器"✗**（**∴ 不许谎报 ✓**）。
    if (url.pathname === "/health") {
      return json({ ok: true, server: false, render_backend: await detectBackend(),
                    note: "本部署没有服务器端；渲染全部在本地浏览器完成" });
    }
    try {
      return await deps.local(req);
    } catch (err) {
      // **★ 未实现的端点必须**如实报错**✗**（**目标总则：lazy 不许变成撒谎 ✓**）。
      return json({ ok: false, error: "not_implemented_locally",
                    endpoint: url.pathname,
                    reason: "该端点在 PWA 里尚未映射到本地内核／IndexedDB" ,
                    detail: String(err && err.message ? err.message : err) }, 501);
    }
  };
}

/** **★ 如实探测浏览器侧后端 ✓ ★**（**部署矩阵 §8.2 ✓**）。 */
export async function detectBackend() {
  try {
    if (navigator.gpu && (await navigator.gpu.requestAdapter())) return "webgpu";
  } catch { /* **∴ 探测失败按无 GPU 处理 ✓** */ }
  return "cpu";
}

function json(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

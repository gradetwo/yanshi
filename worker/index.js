// **★ YanShi PWA 的 Workers 入口 ✓ ★**（第 608 轮 ✓；**部署矩阵 §8 ✓**）
//
// **∴ 本文件**不做任何渲染计算 ✗**（**用户澄清：PWA 只有离线客户端渲染 ✓，**没有服务器端 ✓**）。
// **∴ 它的职责只有两件 ✗**：
//   **① 静态资源**由 `[assets]` 绑定提供（**HTML／JS／WASM／manifest／sw.js ✓**）；
//   **② 对 SPA 路由兜底**（**`not_found_handling = "single-page-application"` ✓**）。
//
// **∴ 因此**这里是一个**最小直通**✗**：**把请求交给 `ASSETS` ✓** ——
// **∴ 若将来要加"**同步／分享**"这类少量职责 ✗，**才在这里加分支 ✓**（**∴ 但要记住：**它仍然没有 GPU ✗**，
// **∴ 所以任何重渲染都必须留在浏览器 ✓**）。

export default {
  async fetch(request, env) {
    // **∴ 静态资源直通 ✓**；**`ASSETS` 由 `wrangler.toml` 的 `[assets]` 绑定 ✓**。
    return env.ASSETS.fetch(request);
  },
};

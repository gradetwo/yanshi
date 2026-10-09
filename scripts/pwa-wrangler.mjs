// **★ 用**本仓库或全局的** wrangler 执行子命令 ✗ ★**（第 224 轮 ✓）：
//   **∴ 为什么包一层 ✗**：**`npm run` 的 PATH 里**有 `node_modules/.bin` ✗**
//   ⇒ **∴ 所以**优先用**仓库本地**的 wrangler ✓**（**∴ 版本随仓库固定 ✓）；
//   **∴ 本地没有**才退回**全局命令 ✓**（**∴ 而** `pwa:check-wrangler` **已经保证**它存在 ✓）** ✓✓
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";

const local = join("node_modules", ".bin", process.platform === "win32" ? "wrangler.cmd" : "wrangler");
const cmd = existsSync(local) ? local : "wrangler";
const r = spawnSync(cmd, process.argv.slice(2), { stdio: "inherit" });
process.exit(r.status ?? 1);

// **★ 部署前的 wrangler 检查 ✗ ★**（第 224 轮 ✓；**用户报"未找到命令" ✓**）：
//   **∴ 为什么要它 ✗**：**`wrangler` **不是** Node 内建 ✗**（**∴ 它**需要单独安装 ✓）
//   ⇒ **∴ 而**裸跑 `wrangler deploy` **只会**丢一句 `command not found` ✗**
//     ⇒ **∴ 用户**看不出**该装什么 ✓**
//   ⇒ **∴ 现在**：**先探测 ✗** ⇒ **∴ 有**就继续 ✗；**没有**就**打印三种装法 ＋ 退出 ✓**** ✓✓
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";

const local = join("node_modules", ".bin", process.platform === "win32" ? "wrangler.cmd" : "wrangler");
if (existsSync(local)) {
  console.log(`  wrangler：本仓库的 ${local} ✓`);
  process.exit(0);
}
let globalOk = false;
try {
  execFileSync("wrangler", ["--version"], { stdio: "pipe" });
  globalOk = true;
} catch (e) {
  globalOk = false;
}
if (globalOk) {
  console.log("  wrangler：全局命令 ✓");
  process.exit(0);
}
console.error("✗ 找不到 wrangler（Cloudflare 的部署工具）");
console.error("  它不在本仓库的依赖里，也不在 PATH 上。任选一种装法：");
console.error("      1) 装到本仓库（推荐，版本随仓库固定）：");
console.error("             npm install");
console.error("      2) 装到全局：");
console.error("             npm install -g wrangler");
console.error("      3) 不安装，直接用 npx（每次会下载）：");
console.error("             npx wrangler@4 deploy");
console.error("  装好后重跑：npm run pwa:deploy");
process.exit(1);

#!/usr/bin/env node
// **空笔刷库启动即报错（B3）**：`resolve_assets_dir` 找不到资产目录时，
//   MCP 服务端启动即失败（退出码非 0 + 报错列出找过的路径），不再静默以 0 支笔刷跑起来；
//   `list_brushes` 在 0 支时回执顶层给 `brushes_available: false`。
//
// **判据**：
//   ① 裸二进制启动失败：把 `yanshi-mcp` 拷进空临时目录（旁无 `assets/`，
//      与仓库资产目录无任何相对路径关系）下跑 ⇒ 退出码非 0 + 报错含找过的路径；
//   ② 有 assets ⇒ 启动正常 + `list_brushes` 的 `count > 0`
//      且 `brushes_available === true`（只断 > 0，不断死笔刷数，笔刷数会变）；
//   ③ 只有 `textures/`、没有 `brushes/` 的资产目录 ⇒ 启动正常 +
//      `list_brushes` 的 `brushes_available === false` 且 `count === 0`。
//
// **变异**：把启动失败改成警告继续 ⇒ ① 红；`brushes_available` 恒 `true` ⇒ ③ 红。
//
// 用法：node scripts/tool-empty-brush-library.mjs
//   （`run-criteria-local.sh` 会先重建 `target/release/yanshi-mcp` 并 export
//   `YANSHI_MCP_BIN`；本判据自己 spawn 二进制，不用它起的 HTTP 服务。）
import { spawn } from "node:child_process";
import { copyFileSync, chmodSync, mkdtempSync, mkdirSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const BIN = resolve(process.env.YANSHI_MCP_BIN || "target/release/yanshi-mcp");
const REPO_ROOT = resolve(dirname(BIN), "..", "..");
if (!existsSync(BIN)) {
  console.error(`  ⊘ 不可判：找不到 MCP 二进制（${BIN}）⇒ 判据前提不成立`);
  process.exit(3);
}
if (!existsSync(join(REPO_ROOT, "assets", "brushes"))) {
  console.error(`  ⊘ 不可判：${REPO_ROOT}/assets/brushes 不存在 ⇒ ② 臂的前提不成立`);
  process.exit(3);
}

const fails = [];
const check = (ok, msg) => {
  if (!ok) fails.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};

// **spawn 的环境必须清掉 `YANSHI_ASSETS_DIR`**：外面若有人设了它指向真资产目录，
//   ① 臂的"裸二进制"就不再裸 ⇒ 会假绿。
const cleanEnv = () => {
  const env = { ...process.env };
  delete env.YANSHI_ASSETS_DIR;
  return env;
};

const children = [];
process.on("exit", () => {
  for (const child of children) {
    try {
      child.kill("SIGKILL");
    } catch {
      /* 已经没了 */
    }
  }
});

// 起一个 MCP 子进程，返回 { child, stderr, waitForExit, call }。
//   `call` 发 tools/call 并等回包；`waitForExit(ms)` 等进程自己退出，超时返回 null。
const startMcp = (bin, args, cwd) => {
  const child = spawn(bin, args, { cwd, env: cleanEnv(), stdio: ["pipe", "pipe", "pipe"] });
  children.push(child);
  let stderr = "";
  let stdoutBuf = "";
  const seen = new Map();
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  child.stdout.on("data", (chunk) => {
    stdoutBuf += chunk;
    let index;
    while ((index = stdoutBuf.indexOf("\n")) >= 0) {
      const line = stdoutBuf.slice(0, index);
      stdoutBuf = stdoutBuf.slice(index + 1);
      try {
        const message = JSON.parse(line);
        if (message.id !== undefined && message.id !== null) seen.set(message.id, message);
      } catch {
        /* 忽略非 JSON */
      }
    }
  });
  const exited = new Promise((done) => {
    child.on("exit", (code, signal) => done({ code, signal }));
  });
  const send = (object) => child.stdin.write(`${JSON.stringify(object)}\n`);
  const waitFor = async (id, ms = 30000) => {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      if (seen.has(id)) return seen.get(id);
      if (child.exitCode !== null) return null;
      await new Promise((r) => setTimeout(r, 100));
    }
    return null;
  };
  let nextId = 100;
  const call = async (tool, toolArgs) => {
    const id = ++nextId;
    send({ jsonrpc: "2.0", id, method: "tools/call", params: { name: tool, arguments: toolArgs ?? {} } });
    return waitFor(id);
  };
  const initialize = async () => {
    send({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "judge", version: "1" } },
    });
    const reply = await waitFor(1);
    if (!reply) return false;
    send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
    return true;
  };
  const waitForExit = async (ms = 15000) => {
    const timeout = new Promise((done) => setTimeout(() => done(null), ms));
    return Promise.race([exited, timeout]);
  };
  return {
    child,
    call,
    initialize,
    waitForExit,
    stderr: () => stderr,
    kill: () => {
      try {
        child.kill("SIGKILL");
      } catch {
        /* 已经没了 */
      }
    },
  };
};

// tools/call 回包的 result 文本是 JSON 字符串，解出来；拿不到就返回 null。
const resultJson = (reply) => {
  const text = reply?.result?.content?.[0]?.text;
  if (typeof text !== "string") return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
};

// ① 裸二进制：拷进空临时目录再跑，启动必须失败。
const bareDir = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-b3-bare-")));
const bareBin = join(bareDir, "yanshi-mcp");
copyFileSync(BIN, bareBin);
chmodSync(bareBin, 0o755);
const bare = startMcp(bareBin, [], bareDir);
const bareExit = await bare.waitForExit();
if (bareExit === null) {
  bare.kill();
  check(false, `① 裸二进制 15s 内没退出（还在跑 ⇒ 启动失败没生效）`);
} else {
  const errText = bare.stderr();
  check(
    bareExit.code !== 0 && bareExit.code !== null,
    `① 裸二进制退出码=${bareExit.code}（要求非 0）`,
  );
  check(
    errText.includes("找过") && errText.includes(bareDir),
    `① 报错含找过的路径（stderr 前 200 字：${errText.slice(0, 200).replace(/\n/g, " ")}）`,
  );
}

// ② 有 assets：仓库根下跑，启动正常 + list_brushes 计数 > 0。
const live = startMcp(BIN, ["--doc", "b3probe", "--width", "64", "--height", "64"], REPO_ROOT);
if (!(await live.initialize())) {
  check(false, `② 有 assets 的服务端没响应 initialize（stderr：${live.stderr().slice(0, 200).replace(/\n/g, " ")}）`);
} else {
  const reply = await live.call("list_brushes", {});
  const listed = resultJson(reply);
  if (!listed || typeof listed.count !== "number") {
    check(false, `② list_brushes 没回可读的计数（${JSON.stringify(reply)?.slice(0, 160)}）`);
  } else {
    check(listed.count > 0, `② 有 assets 时 list_brushes 的 count=${listed.count}（要求 > 0）`);
    check(
      listed.brushes_available === true,
      `② 有 assets 时 brushes_available=${listed.brushes_available}（要求 true）`,
    );
  }
}
live.kill();

// ③ 只有 textures/ 的资产目录：启动正常，但 0 支笔刷。
const thinDir = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-b3-thin-")));
const thinBin = join(thinDir, "yanshi-mcp");
copyFileSync(BIN, thinBin);
chmodSync(thinBin, 0o755);
const thinAssets = join(thinDir, "fake-assets");
mkdirSync(join(thinAssets, "textures"), { recursive: true });
const thin = startMcp(
  thinBin,
  ["--assets-dir", thinAssets, "--doc", "b3probe", "--width", "64", "--height", "64"],
  thinDir,
);
if (!(await thin.initialize())) {
  check(false, `③ textures-only 资产目录的服务端没响应 initialize（stderr：${thin.stderr().slice(0, 200).replace(/\n/g, " ")}）`);
} else {
  const reply = await thin.call("list_brushes", {});
  const listed = resultJson(reply);
  if (!listed || typeof listed.count !== "number") {
    check(false, `③ list_brushes 没回可读的计数（${JSON.stringify(reply)?.slice(0, 160)}）`);
  } else {
    check(listed.count === 0, `③ textures-only 时 list_brushes 的 count=${listed.count}（要求 0）`);
    check(
      listed.brushes_available === false,
      `③ textures-only 时 brushes_available=${listed.brushes_available}（要求 false）`,
    );
  }
}
thin.kill();

// **临时目录由 `trackTemp` 在退出时清理** ✓（`scripts/lib/temp-dirs.mjs` ✓）。
if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（空笔刷库启动即报错）`);
  process.exit(1);
}
console.log("\n  ✓ 空笔刷库启动即报错：裸二进制失败 / 有 assets 正常 / 0 支回执三臂全对");

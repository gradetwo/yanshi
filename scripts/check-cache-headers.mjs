#!/usr/bin/env node
// **`web/` 每个资源目录都必须有显式缓存规则，且「原地重写」的目录不许 immutable**（05 复审 M-1）
//
// **⇒ 背景**：`_headers` 只给 5 个内容寻址目录配了 immutable，其余 6 个
//   （textures/samples/fonts/brush-previews/showcase/palettes，合计约 28 MB）
//   落在 `/*` 的 no-cache 下 ⇒ 每次访问都回源校验 ✗（M-1）；
//   而它们**文件名按资源名、不按内容哈希** ⇒ 也不能 immutable ✗
//   （c5abb674 一次原地重写全部 brush-previews ⇒ immutable 会让改版最长一年看不到）。
//
// **⇒ 判什么**（开放集，不是写死六条）：
//   ① `web/` 下**每个子目录**都要有 `/X/*` 规则 ⇒ 新加目录忘了配就红；
//   ② 在原地重写名单（IN_PLACE）上的目录：规则存在、有 `max-age`、**不含 `immutable`**；
//   ③ 其余目录：规则存在且有 `Cache-Control`。
//
// **⇒ 变异**：删掉 /textures/* 段 ⇒ ② 红 ✓；把它改回 immutable ⇒ ② 红 ✓；
//   临时 mkdir web/new_dir ⇒ ① 红 ✓（三条都实测红过再回绿）。
import { readFileSync, readdirSync, existsSync } from "node:fs";

const IN_PLACE = ["textures", "samples", "fonts", "brush-previews", "showcase", "palettes"];

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};

let text;
try {
  text = readFileSync("web/_headers", "utf8");
} catch {
  console.error("  ⊘ 判据前提不成立：读不到 web/_headers（不是产品问题）");
  process.exit(3);
}
if (!existsSync("web")) {
  console.error("  ⊘ 判据前提不成立：没有 web/ 目录");
  process.exit(3);
}

// 解析成 { "/X/*": "Cache-Control 值" }；注释行不参与。
const rules = {};
let current = null;
for (const line of text.split("\n")) {
  if (line.trim() === "" || line.trim().startsWith("#")) continue;
  if (/^\s/.test(line)) {
    if (current) {
      const m = line.trim().match(/^Cache-Control:\s*(.+)$/i);
      if (m) rules[current] = m[1].trim();
    }
    continue;
  }
  current = line.trim();
}

const dirs = readdirSync("web", { withFileTypes: true })
  .filter((e) => e.isDirectory())
  .map((e) => e.name)
  .sort();

// ① 开放集：每个 web/ 子目录都要有规则
const unruled = dirs.filter((d) => !(("/" + d + "/*") in rules));
check(
  unruled.length === 0,
  "web/ 每个子目录都有 /X/* 缓存规则",
  unruled.length ? `缺：${unruled.join("、")}` : `${dirs.length} 个目录全覆盖`,
);

// ② 原地重写名单：有规则、有 max-age、不含 immutable
for (const d of IN_PLACE) {
  const cc = rules["/" + d + "/*"];
  check(
    cc !== undefined && /max-age=\d+/.test(cc) && !/immutable/i.test(cc),
    `${d}/（原地重写、稳定文件名）⇒ 有 max-age 且**不** immutable`,
    cc ?? "（没有规则）",
  );
}

// ③ 其余目录至少被显式管到（①已断存在性，这里断都有 Cache-Control 值）
const others = dirs.filter((d) => !IN_PLACE.includes(d));
const noCC = others.filter((d) => rules["/" + d + "/*"] === undefined);
check(noCC.length === 0, "其余目录的规则都带 Cache-Control", noCC.length ? `缺：${noCC.join("、")}` : others.join("、"));

if (failures.length > 0) {
  console.error(`✗ 缓存头判据红 ${failures.length} 条`);
  process.exit(1);
}
console.log(`✓ web/_headers 缓存规则齐（immutable=${others.length}｜小时级 TTL=${IN_PLACE.length}）`);

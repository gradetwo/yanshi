#!/usr/bin/env node
// 站点自检（无依赖）。能红的地方：**每一处以 / 开头的引用都必须真有对应文件**。
// 这正是最容易悄悄坏掉的一类问题（改了文件名 / 忘了拷资源 ⇒ 线上 404，而页面看着正常）。
import { readFileSync, existsSync, statSync } from "node:fs";
import { join, dirname } from "node:path";

const PUB = "public";
const pages = ["public/index.html", "public/en/index.html", "public/404.html"];
let bad = 0, checked = 0;

for (const page of pages) {
  if (!existsSync(page)) { console.error(`✗ 缺少页面：${page}`); bad++; continue; }
  const html = readFileSync(page, "utf8");

  // 1) 本地引用存在性（href/src 以 / 开头，或相对本页）
  const refs = [...html.matchAll(/(?:href|src)="([^"]+)"/g)].map((m) => m[1]);
  for (const ref of refs) {
    if (/^(https?:|mailto:|#)/.test(ref)) continue;
    const target = ref.startsWith("/") ? join(PUB, ref) : join(dirname(page), ref);
    checked++;
    if (!existsSync(target)) { console.error(`✗ ${page} 引用了不存在的文件：${ref}`); bad++; }
  }

  // 2) 语言互链必须双向存在
  const need = page.includes("/en/") ? '/"' : '/en/"';
  if (!html.includes('hreflang=')) { console.error(`✗ ${page} 缺少 hreflang 互链`); bad++; }
  if (page === "public/index.html" && !html.includes('href="/en/"')) { console.error("✗ 中文页没有指向 /en/ 的链接"); bad++; }
  if (page === "public/en/index.html" && !html.includes('href="/"')) { console.error("✗ 英文页没有指回 / 的链接"); bad++; }
  void need;

  // 3) 邮箱必须出现在每一页
  if (!html.includes("yanshi@wangda.today")) { console.error(`✗ ${page} 没有联系方式`); bad++; }
}

// 4) 404 页存在（wrangler 的 not_found_handling = "404-page" 依赖它）
if (!existsSync("public/404.html")) { console.error("✗ 缺少 public/404.html"); bad++; }

// 5) 两个语言页的 <h2> 小节数量应一致（双语的粗糙一致性检查）
const sections = (p) => (readFileSync(p, "utf8").match(/<h2>/g) || []).length;
const zh = sections("public/index.html"), en = sections("public/en/index.html");
if (zh !== en) { console.error(`✗ 双语小节数不一致：zh=${zh} en=${en}`); bad++; }

// 6) 站点里不该有 JS（本站是纯静态、零脚本）
for (const page of pages) {
  if (/<script/i.test(readFileSync(page, "utf8"))) { console.error(`✗ ${page} 含 <script>（本站约定零脚本）`); bad++; }
}

if (bad) { console.error(`\n✗ 自检未通过：${bad} 处问题（共检查 ${checked} 处引用）`); process.exit(1); }
console.log(`✓ 自检通过：${pages.length} 页、${checked} 处本地引用都存在，双语小节数 ${zh}，零 <script>`);

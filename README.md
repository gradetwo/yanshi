# yanshi.wangda.today

本站点分支（`site`）。**这是一条 orphan 分支：只有网站，没有 Rust 源码。** 静态页面托管在
Cloudflare Workers 上。

- 中文页：`/`（`public/index.html`）
- 英文页：`/en/`（`public/en/index.html`）

## 目录

```
public/                 ← wrangler 发布的目录，也是站点根
  index.html            中文页
  en/index.html         英文页
  styles.css            单一样式表（含深色模式；无外部字体、无 CDN）
  404.html robots.txt sitemap.xml
  favicon.ico
  assets/brand/         品牌图形（拷贝自 main 分支，见下）
wrangler.toml           部署配置（`name` 改成你的 Worker 名）
scripts/check.mjs       自检（可红）
```

## 部署

```sh
npx wrangler deploy            # 发布到 Cloudflare Workers
npx wrangler dev               # 本地预览
npx wrangler versions upload   # 只上传一个预览版本，不切线上
```

没有 `main` 字段，所以不打任何 Worker 脚本，只发布 `public/` 下的文件。

自定义域在控制台绑定：**Workers & Pages → 选中该 Worker → Settings → Domains & Routes →
Add → Custom Domain → `yanshi.wangda.today`**。DNS 由 Cloudflare 接管，不需要另配 CNAME。

## 维护时守住这几条

1. **零脚本**：`public/` 里不放 JavaScript，也不引外部字体或 CDN。页面是纯 HTML + 一份 CSS。
2. **双语对等**：改中文页就同步改英文页。两页的 `<h2>` 小节数由自检比对。
3. **每页都要有联系方式**（自检会拦）。
4. **品牌图形是副本，源头在 `main` 分支的 `assets/brand/`**。更新方式：

   ```sh
   MAIN=/path/to/yanshi        # 指向 main 分支的工作区
   cp "$MAIN"/assets/brand/svg/*.svg            public/assets/brand/svg/
   cp "$MAIN"/assets/brand/png/favicon-16.png \
      "$MAIN"/assets/brand/png/favicon-32.png \
      "$MAIN"/assets/brand/png/favicon-180.png \
      "$MAIN"/assets/brand/png/yanshi-icon-512.png \
      "$MAIN"/assets/brand/png/logo-horizontal-560.png   public/assets/brand/png/
   cp "$MAIN"/assets/brand/png/favicon.ico      public/favicon.ico
   ```

5. 改完跑自检：

   ```sh
   node scripts/check.mjs
   ```

   它会检查：每处以 `/` 开头的引用是否真有对应文件（改了文件名、忘了拷资源这类问题最
   容易悄悄上线成 404）、两页是否互相链接、每页是否有联系方式、双语小节数是否一致、
   是否混进了 `<script>`。

## 文字口径

- 务实、简短，不写没验证过的能力，不用夸张形容词。
- 「当前状态」一节只写事实：版本号、没有发行版、未实现的工具组、没有的纹理预设、
  浏览器端内核属于构建产物。
- 事实来源是主仓库的 `README.md` / `README.zh-CN.md`、
  `docs/design/yanshi-v1.0-draft4.md`（权威设计文档）与 `Cargo.toml`。**改了主仓库的
  事实，就要回来改这里。**
- 联系方式：`yanshi@wangda.today`。安全漏洞请直接发邮件，不要开公开 issue。

## 许可

MIT，与主仓库一致。 © 2026 The Yanshi Authors

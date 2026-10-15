# 发布打包：一条实测教训（静态包 ＋ GPU ⇒ 段错误）

## 一、问题（2026-10-15，v0.2.0 打包时实测）

用 `scripts/package-release.sh`（**缺省带 GPU** ✓）打出静态包并**真跑**：

```
$ ./yanshi.sh --bind 127.0.0.1:7975 --root /tmp/unproot2 --doc boot --width 320 --height 240
偃师 Yanshi 服务已启动：http://127.0.0.1:7975
bash: 409221 段错误（核心已转储）
```

**这不是"没起来"** ✗ —— 它**打印了启动成功**然后**立刻 SIGSEGV** ✓（所以只看日志会以为正常 ✓，
必须**真的请求一次** `/health` 才能发现 ✗）。

## 二、取证（`coredumpctl`，两次同样崩溃 ⇒ 是模式不是偶发）

```
Signal: 11 (SEGV) si_code: SEGV_MAPERR
Stack trace of thread 409226:
  #0..#9  libgallium-26.2.4-arch3.1.so / libEGL_mesa.so.0
  #5      driCreateNewScreen3 (libgallium-26.2.4-arch3.1.so)
  #10     .../bin/yanshi-serve + 0x787a92      <- 只有这一帧是 yanshi
```

**证据说的** ✓：崩溃发生在 **Mesa 的 EGL 建屏**（`driCreateNewScreen3` ✓）里，
**由 yanshi 调进去** ✓；15 秒内同一二进制崩了**两次** ✓。

## 三、控制变量实验（结论就是这样钉住的）

| 变体 | 静态 | GPU | 结果 |
|---|---|---|---|
| 缺省（脚本默认 ✓） | ✓ | ✓ | **SIGSEGV**（Mesa EGL） ✗ |
| `--no-gpu` | ✓ | ✗ | **正常服务** ✓（`/health` 有响应 ✓、首页 750 KB ✓、无新 core dump ✓）|
| 本机开发二进制（`target/release` ✓） | ✗ 动态 | ✓ | **正常** ✓（本轮跑了数十次 ✓）|

## 三之二、★★ 磁盘充足后重测：信号变了，但**照样崩** ⇒ 结论比第一版更硬 ★★

第一版验完就碰上机器磁盘写满（`ENOSPC`）⇒ 当时我怀疑"**其实只是磁盘满**" ✗。
**空间恢复后原样重测**（同一个官方产物 ✓），结果：

| 变体（磁盘都充足 ✓） | 静态 | GPU | 结果 |
|---|---|---|---|
| **官方发布产物**（CI 用 `d8384df` 构建 ✓） | ✓ | ✓ | **`SIGFPE` / `FPE_INTDIV`** ✗ |
| **`--no-gpu`** 静态包（本地用 `b0b31a28` 构建 ✓） | ✓ | ✗ | **正常服务** ✓（`/health` ✓、rss 3.84 MB ✓、无新 core ✓）|
| 本机开发二进制（动态 ✓） | ✗ | ✓ | **正常** ✓ |

**新的取证（这次的栈比第一版清楚得多）**：
```
Signal: 8 (FPE) si_code: FPE_INTDIV          <- 整数除零
  #0  __libc_early_init (libc.so.6 + 0x1ad33b)
  #1  dl_open_worker_begin (yanshi-serve + 0xc84537)
  #2  _dl_catch_exception   #3 dl_open_worker   #4 _dl_catch_exception
  #5  _dl_open              #6 dlopen_doit      #7 _dl_catch_exception
  #8  _dl_catch_error
```

⇒ **结论（证据支持，磁盘变量已排除）** ✓：
**静态链接（static-pie）的进程去 `dlopen` 系统图形栈** ✗ ⇒ 被加载的那个库里**自带/假定**的 glibc 初始化
（`__libc_early_init` ✓）在**本进程是静态 glibc** 的情况下跑 ⇒ **整数除零** ✗ ⇒ 进程死 ✓。
**GPU 路径正是那个 dlopen 的来源** ✓（`--no-gpu` 不走它 ⇒ 一切正常 ✓）。

**关于第一版** ✗：我把因果写成"**静态＋GPU 与 Mesa 不兼容**" ✓，方向对 ✓，但**归因过窄** ✗
（真正的不兼容在**静态 glibc ＋ dlopen 外部库**这一层 ✓，Mesa 只是**当时**崩出来的那一个 ✓）；
而磁盘满**改变了它崩在哪**（第一次是 Mesa 里的 SEGV ✓）✗，**没有**改变它**会崩** ✓。

## 四、顺带排除的一项（有人怀疑是"机器上自动跑的 server 干扰" ✓）

机器上**确实**有一个在跑的东西：`systemd --user` 的 `yanshi-serve.service`（**inactive/dead** ✓，
描述是回环 **8110** ✓），以及 **`0.0.0.0:8080` 被某个进程监听** ✓（`ss` 看不到持有者 ⇒ 非本用户/root ✓）。
**但它不是原因** ✓：在上面那张表里，两次实验**它都在**（常量 ✓），
而 `--no-gpu` 变体**正常服务** ✓ ⇒ 差异**只可能**来自 GPU 代码路径 ✓。

## 五、影响与建议

* **影响**：`release.yml` 缺省产的 **Linux 静态包**在**有 Mesa GPU 驱动**的机器上**起不来** ✗。
  CI 里那一档之所以"绿" ✗，是因为它**只打包、不运行**二进制 ✗（**发布流水线的缺口** ✓）。
* **立刻可用的绕法** ✓：用 `--no-gpu` 打静态包（**实测可用** ✓，本轮已给 v0.2.0 补上传一个 ✓），或用动态构建 ✓。
* **已上传的补救** ✓：`v0.2.0` 的 Release 里除原有产物外，另加
  `yanshi-0.2.0-b0b31a28-x86_64-unknown-linux-gnu-static-nogpu.tar.gz` ✓（**实测能起服务** ✓）；
  原产物**保留** ✓（透明 ✓）—— 请按名字取用 ✓。
* **该修的** ✗：让 `release.yml` **至少跑一次** `--version` ＋ 一次 `/health` 冒烟 ✓（成本极低 ✓），
  并决定静态包是否还要缺省带 GPU ✗（这是一次**产品决策** ✓：GPU 加速 vs 静态可分发 ✓）。

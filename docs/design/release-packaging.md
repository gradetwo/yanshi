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

⇒ **结论（证据支持）** ✓：**静态链接（static-pie）＋ GPU/EGL 路径** ⇒ 与 Mesa 驱动不兼容 ⇒ 段错误 ✗。
**推断（未证实）** ✗：Mesa 的 DRI/EGL 假定宿主是**动态链接**的 ✓；
**静态链接把它 dlopen 进来的方式**让它建屏时踩了空指针 ✗。

## 四、顺带排除的一项（有人怀疑是"机器上自动跑的 server 干扰" ✓）

机器上**确实**有一个在跑的东西：`systemd --user` 的 `yanshi-serve.service`（**inactive/dead** ✓，
描述是回环 **8110** ✓），以及 **`0.0.0.0:8080` 被某个进程监听** ✓（`ss` 看不到持有者 ⇒ 非本用户/root ✓）。
**但它不是原因** ✓：在上面那张表里，两次实验**它都在**（常量 ✓），
而 `--no-gpu` 变体**正常服务** ✓ ⇒ 差异**只可能**来自 GPU 代码路径 ✓。

## 五、影响与建议

* **影响**：`release.yml` 缺省产的 **Linux 静态包**在**有 Mesa GPU 驱动**的机器上**起不来** ✗。
  CI 里那一档之所以"绿" ✗，是因为它**只打包、不运行**二进制 ✗（**发布流水线的缺口** ✓）。
* **立刻可用的绕法** ✓：用 `--no-gpu` 打静态包（**实测可用** ✓），或用动态构建 ✓。
* **该修的** ✗：让 `release.yml` **至少跑一次** `--version` ＋ 一次 `/health` 冒烟 ✓（成本极低 ✓），
  并决定静态包是否还要缺省带 GPU ✗（这是一次**产品决策** ✓：GPU 加速 vs 静态可分发 ✓）。

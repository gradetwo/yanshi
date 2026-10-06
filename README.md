<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/svg/logo-horizontal-dark.svg">
    <img src="assets/brand/svg/logo-horizontal.svg" alt="Yanshi" width="320">
  </picture>
</p>

# Yanshi 偃师

[中文](README.zh-CN.md) | **English**

**Yanshi is an AI native headless painting engine — a canvas where people and AI create together.**

- **Operations are atoms.** Every edit is an atom appended to a log, and the picture is the fold of that log.
- **One kernel, two hosts.** The same Rust kernel runs natively on the server and as WebAssembly in the browser,
  so local rendering and server rendering are one implementation, not two approximations.
- **A package is self-contained.** A `.yanshi` package carries the atom log and every blob it references, and
  importing it elsewhere reproduces the picture.

An MCP server and a web viewer sit on top of that engine, so an agent and a person work on the same document.

## Quick start

**Requirements**

- **Rust** — the stable toolchain pinned by [`rust-toolchain.toml`](rust-toolchain.toml), including the
  `rustfmt` and `clippy` components.
- **`make`** and a POSIX shell — every entry point below is a Make target that calls `scripts/`.
- Optional, to get **client-side rendering** (the browser kernel):
  `rustup target add wasm32-unknown-unknown` and `wasm-bindgen-cli` **0.2.129**
  (`cargo install wasm-bindgen-cli --version 0.2.129 --locked`). Its version must match the
  `wasm-bindgen` crate, or the generated glue will not load the `.wasm`.
  **Without both, the server still starts** and the viewer renders on the server side.
- Optional, only to run the criteria: **Node** and **chromium**
  (the runner skips browser criteria when chromium is absent).

**Build and run**

```bash
git clone https://github.com/gradetwo/yanshi.git && cd yanshi
make dev        # build the WASM kernel if the toolchain is present, build the server, start it
                # ⇒ open http://127.0.0.1:8110/  (the page obtains its capability token)
```

Data lives in `~/.local/share/yanshi/workspace` by default. To change the port or the data root, pass
**environment variables** — `make dev` does not forward Make variables into its recipe, while
`make serve` passes `--port` explicitly:

```bash
PORT=9000 make dev                 # `make dev PORT=9000` would NOT reach the recipe
ROOT=/tmp/yanshi PORT=9000 make dev
make serve PORT=9000
```

Then:

```bash
make check      # fmt + clippy + tests — the three gates to run before pushing
make test       # workspace tests (the fast set); make test-heavy runs the long #[ignore]d jobs
make serve      # rebuild -p yanshi-http and serve (run this before verifying tools with curl)
make release    # build the release package
make help       # every target
```

## Credits


The project is MIT (see `LICENSE`), with two notable exceptions and additions.

`assets/brushes` holds 196 brushes from **mypaint-brushes 2.0.2**, which is **CC0 1.0**; the upstream `COPYING` is kept
there as `LICENSE-CC0.txt`.

Paper and canvas textures are being sourced from CC0 providers rather than from MyPaint's own assets, whose
GPL-2.0-or-later licence would have put a second licence in the tree; the earlier copy was removed for that reason,
so everything shipped today is MIT or CC0.

`assets/palettes` holds **Open Colors** (15 families, 132 colours), which is MIT, and is the palette set the
project owner chose to replace MyPaint's own. sK1's public domain palettes are still to be located, since they are
not in the repository that was checked.

Brush engine came from **Hokusai** (Apache-2.0 or MIT) as described above.
- **Rust libraries**: `serde` and `serde_json` (MIT / Apache-2.0), `sha2` (MIT / Apache-2.0), `thiserror` (MIT / Apache-2.0), `proptest` (MIT / Apache-2.0), and `libm` (MIT) — the last one so that the server and the browser share one implementation of the maths and stay bit-identical.
- **People**: a place is kept here for the contributors and reviewers to be listed. Volunteer to be named.

## Where the name comes from

Yanshi is the artificer in the *Liezi*, "Questions of Tang", who builds a mechanical performer. The passage:

> 周穆王西巡狩，越昆侖，不至弇山。反還，未及中國，道有獻工人名偃師，穆王薦之，問曰：「若有何能？」偃師曰：「臣唯命所試。然臣已有所造，願王先觀之。」穆王曰：「日以俱來，吾與若俱觀之。」翌日，偃師謁見王。王薦之曰：「若與偕來者何人？」對曰：「臣之所造能倡者。」穆王驚視之，趨步俯仰，信人也。巧夫，顉其頤，則歌合律；捧其手，則舞應節。千變萬化，惟意所適。王以為實人也，與盛姬內御並觀之。技將終，倡者瞬其目而招王之左右侍妾。王大怒，立欲誅偃師。偃師大懾，立剖散倡者以示王，皆傅會革木膠漆白黑丹青之所為。王諦料之，內則肝膽心肺脾腎腸胃，外則筋骨支節皮毛齒髮，皆假物也，而無不畢具者。合會復如初見。王試廢其心，則口不能言；廢其肝，則目不能視；廢其腎，則足不能步。穆王始悅而歎曰：「人之巧乃可與造化者同功乎！」詔貳車載之以歸。夫班輸之雲梯，墨翟之飛鳶，自謂能之極也。弟子東門賈、禽滑釐聞偃師之巧以告二子，二子終身不敢語藝，而時執規矩。

> (After [Chinese Text Project, *Liezi*, "Tang Wen"](https://ctext.org/liezi/tang-wen/zhs); transcribed here in simplified characters.)

What is worth borrowing from the story is not the cleverness but that **the construction and the behaviour are separable**. Opened up, the singing, dancing figure is leather, wood, glue, lacquer and pigment. Take away its heart, liver or kidney and it cannot speak, see or walk; **put it back together and it is as it was**.

That is the part this project matches:

- **The log is the construction.** Every edit is an atom appended to the log, and the picture is the result of folding it.
- **Taken apart and reassembled, the result is the same.** A `.yanshi` package carries the atom log and every blob it references, and importing it into another workspace is byte for byte identical.
- **One kernel produces the behaviour.** The native build for the server and the WebAssembly build for the browser are the same implementation, not two approximations of each other.

The passage ends with the king marvelling that a person's craft could rival nature. **We do not claim that.** This project writes down how the tool is built, and it writes down what it cannot do as well as what it can.

## Logo

The mark is a **slingshot**: a forked frame, two rubber bands and a red pellet. It takes its inspiration from a line in the film *Who Cares* (《谁说我不在乎》):

> 抽出裤衩里的猴皮筋，做成弹弓打你们家玻璃。

> (Take the rubber band out of your underpants and make a slingshot to break your window.) [Clip](https://www.bilibili.com/video/BV1Rw411c7Bx/) on bilibili; the film's rights belong to its rights holders.

What it borrows is the gesture in a joke, **making something usable out of whatever is at hand**, which is the same attitude the project takes: write the construction down, use what works, and say so when it does not.

## Documentation

- [Design document](docs/design/yanshi-v1.0-draft4.md) — the specification.
- [Tool examples](docs/design/tool-examples.md) — a copyable call for every tool.
- [Implementation notes](docs/design/implementation-notes.md) — module map, decisions where the
  specification is silent, measured performance data, known deviations.
- [docs/tools.md](docs/tools.md) — tools, adjustments, filters, retouch, masks, collaboration.
- [scripts/README.md](scripts/README.md) — acceptance scripts, including the browser UI check
  that asserts a stroke stays on the canvas after a commit.
- Repository layout: `crates/yanshi-core` (atoms, log, fold, CAS), `-render` (compute kernel),
  `-server` (document service, tool layer), `-http` (transport, viewer, `yanshi-serve`),
  `-mcp`, `-wasm` (browser kernel); `scripts/`, `deploy/`, `docs/design/`.
- [SECURITY.md](SECURITY.md) — threat model and reporting.

Detailed operational documentation — release packaging, viewer controls, brushes, textures, palettes, compatibility and the rest — lives in [docs/guide.md](docs/guide.md).

## License

MIT. See [LICENSE](LICENSE).

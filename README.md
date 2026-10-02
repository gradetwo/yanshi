# Yanshi 偃师

[中文](README.zh-CN.md) | **English**

Yanshi is a headless image editor engine: an append-only atom log, a folded state, a pure-Rust
render compute kernel, a zero-dependency HTTP/WebSocket server, and a minimal web viewer. The
kernel compiles natively for the server and to WebAssembly for the browser, so optimistic local
rendering and server rendering are one implementation.

[docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) is the authoritative
specification (Chinese); the code must not diverge from it silently.

## Static builds and storage portability

`scripts/package-release.sh --static` produces a fully static binary that references no GLIBC version, which matters
because the dynamic build inherited GLIBC 2.43 from its build machine and failed to start on Debian 12. The script
prints the glibc requirement of whatever it packages and refuses a static package that still references one.

Where a filesystem cannot fsync, such as 9p or some network mounts, content-addressed writes now degrade rather than
fail, and `/health` reports `blob_fsync: unsupported` so the weaker durability guarantee is visible.

## Automation and a blank canvas

`window.yanshi` exposes a stable hook for scripts: `state()` reports the current tool, colour, size, opacity, medium,
layer and document, and `setColor`, `setSize`, `setOpacity`, `setMedium` and `setTool` change them through the same
controls and events the interface uses. `new_document` creates a blank canvas, and because a document id is the unit
of persistence, a new canvas means a new id; asking to recreate an id that is already open is refused with that
advice rather than quietly clearing it.

`--profile` accepts core, history, changeset, retouch, semantic, conflict, annotation, collab and structure, all
listed in `--help`; semantic is reserved and not developed by decision.

## Error messages you can act on

A refusal for a missing layer, object, selection, mask or style also lists the ids that do exist, capped at eight with
an ellipsis, and says plainly when there are none, so a caller can correct itself without another listing call.

## Undo and redo

Ctrl or Cmd with Z undoes, and adding Shift redoes, alongside the buttons in the activity panel.

## Which build am I running

`yanshi-serve --version` and `yanshi-mcp --version` print the version, the short commit and the build time, and
`GET /health` reports the same values, so a running server can be identified without asking anyone. A commit with a
`-dirty` suffix means the tree had uncommitted changes when it was built. Release tarballs carry the commit in their
file name and include a `BUILD-INFO` file with the version, commit, target, build time and compiler.

## Exporting a PNG

`export_png` renders the whole document or a region and writes a PNG to a path, optionally rescaling first, so an
agent gets a file rather than base64 and is not limited by the inline image cap of 512 pixels.

    {"path": "/tmp/canvas.png", "max_edge": 2048}

`width` and `height` may be given together instead of `max_edge`, and `filter` chooses nearest or bilinear scaling.

## Stroke pressure

A stroke's `points` accept an optional third element, the pressure from 0 to 1, so `[[x, y, 0.2], [x, y, 0.9]]`
draws a line that thickens and thins; omitting it means full pressure and renders exactly as before. The kernel has
always supported this, including the `{"x": .., "y": .., "pressure": ..}` object form, and the server-side tool
description did not mention it, which is why generated artwork tended to come out as uniform noodles.

    {"layer_id": "L", "data": {
      "points": [[20, 40, 0.15], [60, 44, 0.9], [100, 48, 0.35]],
      "size": 24, "color": [0.8, 0.2, 0.1, 1.0]}}

## Shape geometry

A shape's geometry is `{"kind": "rect" | "ellipse" | "polygon", ...}` with either a `bbox` of `x`, `y`, `w` and `h`
or `points`. Anything else is refused with a message naming the accepted forms, rather than returning success and
committing an object that renders nothing. A `bbox` may be an object or an array of four numbers, the array being
normalised internally. `create_layer` returns the `layer_id` it created.

## Rendering and determinism

The compute kernel is CPU code shared by the client and the server and is bit-exact, which the design calls the D0
baseline and treats as the only authority; the compositing tier, previews and thumbnails are D1 and are allowed a
one least-significant-bit difference. There is no GPU backend: the design makes GPU acceleration optional and scoped
to the compositing tier alone, and adding one would mean taking on a dependency, which this project does not do. The
CPU path currently serves both tiers within budget, and a GPU backend could be added behind the compositing tier
later without changing the architecture.

## Path operators

The object panel drives the path operators from design 792: reverse, close, join, merge, split and boolean, the
last with union, intersect, subtract and xor modes. The unary operators act on the first checked object, while join,
merge and boolean need two, taking the first as the target and the second as the other. Choosing a binary operator
with fewer than two objects is refused in the interface with a message saying how many are needed, rather than
sending a request that cannot succeed. A boolean replaces its inputs with the merged result, so the object count
goes down rather than up.

## Roles

Opening a document takes an optional role: `POST /api/documents?role=viewer` issues a read-only capability token,
`editor` (the default) may change the document, and `owner` additionally may revert other actors' atoms. The role is
enforced at the tool endpoint using each tool's own `mutating` flag, so a read-only token can call read tools but
any tool that changes the document is refused with a permission error naming the role and the tool. An unknown role
is refused rather than quietly treated as an editor, since that would turn a read-only link into a writable one.

## Release package

`scripts/package-release.sh` builds the release binaries and packs them with the runtime assets they need:
the two executables, the browser-side WASM kernel and the medium plugins. It writes a versioned tarball and a
checksum file under `dist/`.

    scripts/package-release.sh            # build and pack into dist/
    tar -xzf dist/yanshi-*-x86_64-unknown-linux-gnu.tar.gz
    cd yanshi-*-x86_64-unknown-linux-gnu
    ./yanshi.sh --root ./workspace --bind 127.0.0.1:8110

Then open <http://127.0.0.1:8110/>. Without `--root` the server is purely in memory, so nothing persists and a
restart starts empty.

The package is laid out as `bin/` for the executables and `share/yanshi/` for the WASM kernel, the medium plugins
and the brand assets, with `yanshi.sh` as a thin wrapper. The wrapper matters: the server's defaults for those asset
directories are relative to the current directory, so running `bin/yanshi-serve` directly from elsewhere would find
neither the kernel nor the plugins, leaving the viewer degraded and `/mediums/*.wasm` returning 404 while the server
itself still starts. The wrapper derives the paths from its own location instead, so the tree works from anywhere.

## Install

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh    # Rust stable 1.85+
```

Optional, for the browser kernel (the version must match the `wasm-bindgen` crate):

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

Without them the server still runs; the viewer then renders on the server side.

## Build and run

```bash
make run          # build the WASM kernel if the toolchain is present, build the server, start it
                  # then open http://127.0.0.1:8110/ (the page obtains its capability token)
make build        # server only
make build-wasm   # WASM kernel only
```

```bash
make run PORT=9000 ROOT=/tmp/yanshi                 # PORT / ROOT (default ~/.local/share/yanshi/workspace)
cargo run --release -p yanshi-http -- --no-wasm     # server only, no browser kernel
make help                                           # all targets
```

## Test

```bash
make test         # workspace test suite
make ci           # fmt, clippy, tests, WASM runtime smoke check
```

Long jobs (perf budgets, 100k-atom fold fuzz, 4K profile) are `#[ignore]`d and run on GitHub:

```bash
gh workflow run heavy.yml && gh run list            # manual only; logs are artifacts
cargo test --release --workspace -- --ignored --nocapture   # or run them locally
```

## Use

```bash
cargo run --release -p yanshi-mcp                   # MCP over stdio (agent integration)
cargo run --release -p yanshi-mcp -- --list-tools
```

```bash
# open a document; the response carries the capability token
curl -s -X POST http://127.0.0.1:8110/api/documents -d '{"doc_id":"demo","width":1024,"height":1024}'

# call a tool (token via Authorization: Bearer or ?token=)
curl -s -X POST "http://127.0.0.1:8110/api/tools/create_layer?doc=demo&token=$TOKEN" -d '{"layer_id":"layer_1"}'
```

The tool and effect inventory is in [docs/tools.md](docs/tools.md).

## Viewer controls

Drag to paint; `＋ 图层` adds a layer; `撤销` / `重做` undo and redo repeatedly (multi-level). Wheel zooms around the cursor,
middle-drag pans, `+` / `-` / `0` zoom in, out and fit, and `1:1` shows one document pixel per CSS
pixel, `导出 PNG` downloads the full-resolution render, the 打开 dialog lists the server's documents and imports local images (PNG/JPEG/WebP are decoded
by the browser and uploaded as raw pixels; over the tool API, `POST /api/blob` also accepts a **PNG**
directly and normalises it to raw pixels with the repository's own decoder, since the project takes no
external dependencies - JPEG and WebP are refused there with an explanation rather than stored as-is),
the history panel lists the atom log with kind and actor
filters plus a jump-back action, and the effects panel applies any adjustment or filter to the
current layer (names come from the kernel, parameters default to the kernel's own values).

Recent panels close the same kind of gap - a tool that existed with unit tests but no way to reach it. A PSD
chosen in the import dialog is routed to the server, which reads the flattened composite and says so in the
log; layer structure and masks are not imported, and the contract is read-only. The `标注` panel creates,
edits, resolves and deletes annotations and draws clickable pins on the canvas. The `对象` panel lists the
current layer's objects, turns one into a live-linked instance, groups several together, rotates, scales or
moves them, and converts any object into a shape or a path so it can be kept as vector work. The history panel marks checkpoints and jumps
back to them; the log is append-only, so nothing is lost and the canvas can move forward again. Effects can be
edited after they are added, by clicking one in the list, which loads its parameters and updates that same
object instead of stacking another. A The history panel now inspects an atom: clicking one shows the payload of what it actually changed, which needed a new `get_atom` tool, since the log and search tools return metadata only by design. A `评论` panel posts comments and lists them with their author and text, which needed a new tool: comments could always be written but no tool could read them back, a gap a browser check exposed and `list_comments` closes. Annotations can be rejected as well as resolved, and a suggestion can be previewed first: preview validates every patch step and reports it without applying anything, so the effect count stays put until accept is pressed. Every toolbar button carries an inline SVG icon and its keyboard shortcut in the tooltip, and a guard test keeps it that way after the annotation tool shipped without an icon and rendered as an invisible button. `medium_stroke` paints with a medium plugin from a server-side or MCP session, not only in the browser:

    {"layer_id": "L", "medium": "oil", "points": [[20, 30, 0.2], [60, 36, 0.9], [100, 42, 0.5]],
     "size": 24, "color": {"r": 210, "g": 80, "b": 40, "a": 255}}

Points take an optional pressure and the object records the plugin id and version. Six medium plugins ship as dependency-free wasm modules under assets/mediums, example, oil, watercolour, marker, pencil and pixel, and the object records the medium by id and version so upgrading a plugin cannot change how older documents render. The object panel can restyle a stroke, changing its colour, size and opacity, which the update_stroke tool had no tests for until this round added two. A 变更集 section in the history panel groups a run of edits so they can be undone together, and the served tool profile list was missing the changeset and conflict groups even though its own comment says the web editor enables the full set, so the button reported an unknown tool until that was fixed, while the semantic group stays deliberately absent. A `建议` panel lists suggestions from section 12.6 with their status and priority, and accepting one replays its patch in order and resolves the annotation it came from, which is the other half of the review loop the annotation panel begins. The `存储 / 维护` panel reports the blob store's three
lifecycle tiers from design 6.3 - active, history and orphan - and can reclaim orphans past the TTL or demote
history to cold storage, both only after an explicit confirmation, since deletion is irreversible; the report
alone is always safe and never deletes anything.

Strokes accept an `appearance` block (the design's `advanced.appearance`): `size_curve`, `opacity_curve`
and `pressure_curve` shape the stroke, `dynamics` with `seed` adds deterministic jitter, scatter, size
and angle variation, a procedural `noise` or `grain` `texture` modulates the ink, and `paint_load`, `wetness` and `mixing`
give a wet brush: ink runs out along the stroke, dragging drains faster than dabbing, and mixing pulls
the colour already under the brush into the tip. Without an
appearance block a stroke renders exactly as before, byte for byte.

The toolbar also drives the pixel tools: brush and eraser, fill layer, eyedropper, clone stamp and
heal (Alt+click sets the source), smudge, the three liquify modes, and rect or ellipse masks with a
feather setting (drag a shape to create the mask and attach it to the current layer).

Object groups follow design section 9.4: `create_group`, `add_to_group`, `remove_from_group` and
`set_group_transform` move a set of objects together, and a group transform is recorded on the group while
the members carry the resulting geometry, so rendering, bounding boxes, hit testing and dirty planning all
stay consistent without special cases.

`选区` drags a rectangular selection that constrains everything painted afterwards - strokes, erasing,
shapes, fills, text, liquify and retouch - per pixel, so nothing outside it is touched, and `清除选区`
removes it, after which the same objects render unconstrained again because the constraint is
recomputed from the log. `文本`
places a text object at the clicked point. Text stays an editable object in the log - editing its data
through `supersede` changes the render - and the kernel rasterises it with an embedded open-source
font: a built-in 5×7 ASCII font plus a 1-bit 16×16 atlas generated from Noto Sans CJK (SIL OFL 1.1),
covering ASCII, 6763 GB2312 level-1 and level-2 hanzi (3755 common plus 3008 less common) and the GB2312 symbol rows. Provenance, format
and the regeneration command live in `assets/fonts/`. `刷新` re-renders from the server and `一致性自检` runs the kernel-versus-server comparison.

## Desktop entry (Linux: Omarchy / Hyprland)

```bash
cp deploy/systemd/yanshi-serve.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now yanshi-serve
```

```lua
-- ~/.config/hypr/bindings.lua
o.bind("SUPER + ALT + Y", "Yanshi", { webapp = "http://127.0.0.1:8110/?doc=yanshi", focus = true })
```

Uses `systemctl --user`, so Linux only; on macOS the server runs the same way (`make run`).
Details: [deploy/omarchy/README.md](deploy/omarchy/README.md).

## Documentation

- [Design document](docs/design/yanshi-v1.0-draft4.md) — the specification.
- [Implementation notes](docs/design/implementation-notes.md) — module map, decisions where the
  specification is silent, measured performance data, known deviations.
- [docs/tools.md](docs/tools.md) — tools, adjustments, filters, retouch, masks, collaboration.
- [scripts/README.md](scripts/README.md) — acceptance scripts, including the browser UI check
  that asserts a stroke stays on the canvas after a commit.
- Repository layout: `crates/yanshi-core` (atoms, log, fold, CAS), `-render` (compute kernel),
  `-server` (document service, tool layer), `-http` (transport, viewer, `yanshi-serve`),
  `-mcp`, `-wasm` (browser kernel); `scripts/`, `deploy/`, `docs/design/`.
- [SECURITY.md](SECURITY.md) — threat model and reporting.

## Status

The atom log and fold, CPU rendering (the compute kernel is bit-exact; blur-family filters are
allowed ±1 LSB by an approved scope decision), server rendering with thumbnails and tiles, the tool
layer, HTTP/WebSocket with capability tokens, the annotation channel and suggestion loop, and the
browser WASM kernel are implemented. GPU compositing (feasibility measured), plugin hosting and
collaboration beyond one server are not.

## License

MIT. See [LICENSE](LICENSE).

# Yanshi 偃师

[中文](README.zh-CN.md) | **English**

<img src="assets/brand/svg/logo-horizontal.svg" alt="Yanshi" width="360" />

> An AI-native collaborative painting and design engine

Yanshi replaces the traditional panel-and-mouse-trajectory GUI with document state, semantic commands,
editable objects, multi-level previews, a versioned append-only atom log, and collaboration primitives —
so that humans and AI agents work on one shared object model: humans through fine-grained tools,
AI through high-level semantic tools.

The repository currently ships the **headless parts of Phase 0 / Phase 1 / Phase 2**: the core engine,
the render compute kernel, the headless server and tool protocol, a dependency-free HTTP/WebSocket
transport, a minimal web viewer, and a **browser-side WASM compute kernel with local optimistic rendering**.

```bash
cargo test --workspace                 # 322 tests
cargo run -p yanshi-http --bin yanshi-serve -- --root ./workspace
# open http://127.0.0.1:8080/ — the page obtains a capability token via POST /api/documents
```

## Design document

- [docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) — the frozen design document
  v1.0-draft4 (**Chinese**). It is the authoritative definition of every semantic and protocol; code must not
  silently diverge from it, and behaviour changes must update the document in the same commit.
- [docs/design/implementation-notes.md](docs/design/implementation-notes.md) — implementation notes:
  module-to-section map, decisions where the document is silent, measured performance data, known
  limitations, the not-yet-implemented list, and acceptance carriers (test ↔ document requirement).

## Status

| Layer | State | Notes |
|---|---|---|
| `yanshi-core` core engine | ✅ | append-only atom log (client ULIDs for idempotency + server-authoritative `seq`), fold evaluation with cascading revert invalidation, `state@seq` and `declare_head`, logical snapshots, three-tier Blob CAS lifecycle and GC |
| `yanshi-render` compute kernel | ✅ | D0 CPU bit-exact: f16 linear tiles, premultiplied blending, brush stamping, shapes, adjustments/filters, dual dirty propagation, tiered thumbnails, dependency-free deterministic PNG |
| `yanshi-server` server semantics | ✅ | document service, Job protocol (TTL/cancel/polling), capability tokens, control-flow vs data-flow broadcast boundaries, append-only annotation channel, file persistence, the 27 core tools plus profile-gated groups (49 tools in total with every implemented group enabled) |
| `yanshi-http` transport & viewer | ✅ | dependency-free HTTP/1.1 + RFC 6455 (hand-written SHA-1 handshake, frame codec, fragmentation, ping/pong), keep-alive connection reuse, minimal single-page web viewer |
| `yanshi-mcp` | ✅ | MCP stdio (`initialize` / `tools/list` / `tools/call` / `ping`), profile layering, submit-and-poll |
| `yanshi-wasm` browser compute kernel | ✅ | wasm32 build, local incremental folding + optimistic rendering, pending-stroke overlay, LRU tile pool with a 90% watermark fallback |
| Desktop entry | ✅ | systemd user service + Omarchy web app + `SUPER + ALT + Y` (see [deploy/omarchy/README.md](deploy/omarchy/README.md)) |

**Phase 2 exit criteria verified in a real Chromium**:

| Exit criterion | Result |
|---|---|
| Client and server CPU paths are bit-exact | ✅ SHA-256 of the local WASM-rendered PNG equals the server `blob_hash` |
| First-stroke presentation < 16 ms | ✅ measured **6.2 ms** (incremental stamping path), and one stroke yields exactly one atom |

Planned but unimplemented work is listed in the roadmap below; this README never describes an
unimplemented capability as available.

## Repository layout

```
yanshi/
├── assets/brand/             # Brand assets: nine source SVGs + render.sh (icons/favicons/logos)
├── deploy/                   # Deployment: systemd user service + Omarchy/Hyprland desktop entry
├── crates/
│   ├── yanshi-core/          # Core engine: atom log, fold evaluation, state, Blob CAS
│   ├── yanshi-render/        # Render compute kernel: D0 CPU baseline, tiles, dirty, thumbnails, PNG
│   ├── yanshi-server/        # Server semantics: document service, jobs, tokens, broadcast, annotations, 27 tools
│   ├── yanshi-http/          # Dependency-free HTTP/1.1 + WebSocket transport and the minimal web viewer
│   ├── yanshi-wasm/          # WASM bindings for the compute kernel (browser-side optimistic rendering)
│   └── yanshi-mcp/           # MCP stdio server (JSON-RPC over stdio)
├── docs/design/              # Frozen design document, implementation notes, revision history
├── .github/workflows/ci.yml  # CI: fmt / clippy / test / long-running fuzz
├── CONTRIBUTING.md
└── LICENSE                   # MIT
```

## Getting started

Rust stable 1.85 or newer is required.

```bash
cargo test --workspace                  # unit + property tests + Phase 0 exit cases
cargo test --workspace --release -- --ignored   # 100k-atom fold fuzz + render/server perf budgets
cargo run -p yanshi-core --example quickstart    # end-to-end: submit → revert → time travel → GC
cargo run -p yanshi-render --example render_demo # render sample: writes PNGs to target/render-demo/
cargo run -p yanshi-mcp -- --list-tools          # list the MCP tool set (JSON)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

> Long-running fuzz tests are `#[ignore]`d by default; CI runs them with `--ignored`
> (the `fuzz` job in `.github/workflows/ci.yml`).

### Dependency-free HTTP/WebSocket server + minimal web viewer

```bash
# loopback 8080; --no-wasm / --no-brand disable the WASM kernel and brand assets
cargo run -p yanshi-http --bin yanshi-serve -- --bind 127.0.0.1:8080 --root ./workspace
# open http://127.0.0.1:8080/ — the page fetches a capability token and connects over WebSocket
```

* **Local optimistic rendering**: the browser loads `yanshi_wasm.wasm` (the same Rust compute kernel the
  server uses). While dragging, only the **new stroke segment** is stamped onto cached tiles; on pointer-up
  a single atom is submitted asynchronously and the authoritative server state then corrects the client.
* **Open is an image**: the page paints the server's cached HEAD render first (measured 199ms for a
  512² document, 434ms for 1024²) and warms the WASM kernel in the background, so the first frame never
  waits for client-side folding; the kernel takes over as soon as it is ready.
* **Consistency self-check**: the “consistency self-check” button compares the SHA-256 of the locally
  rendered PNG against the server blob hash.
* Building the WASM bundle: `cargo build -p yanshi-wasm --target wasm32-unknown-unknown --release` plus
  `wasm-bindgen --target web --out-dir crates/yanshi-wasm/pkg --no-typescript <wasm>`. When the bundle is
  missing, the viewer degrades to server-side rendering automatically.

### Using it over MCP (agent integration)

`yanshi-mcp` is an MCP stdio server: one JSON-RPC message per line; local processes are exempt from
authentication (12.7).

```bash
# in-memory, default document `default` (1024×1024)
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | cargo run -q -p yanshi-mcp

# persist to ./workspace and expose the annotation and history tool groups
cargo run -q -p yanshi-mcp -- --root ./workspace --doc demo --profile core,annotation,history
```

A typical call sequence (10.1 responses carry `atom_id` / `seq` / `preview.thumb_url`):

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"create_layer","arguments":{"layer_id":"layer_1"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"draw_stroke","arguments":{
  "layer_id":"layer_1","data":{"points":[[40,40],[400,300]],"size":12,"color":{"r":40,"g":40,"b":60,"a":255}}}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"render_region","arguments":{
  "region":{"x":0,"y":0,"w":256,"h":256},"include_image":true}}}
```

### Using it over HTTP / WebSocket

```bash
# 1) open or create a document → capability token and URL (12.7)
curl -s -X POST http://127.0.0.1:8080/api/documents \
     -d '{"doc_id":"demo","width":1024,"height":1024}'
# {"ok":true,"doc_id":"demo","token":"<64 hex>","url":"/?doc=demo&token=..."}

# 2) call a tool (token via Authorization: Bearer or ?token=)
curl -s -X POST "http://127.0.0.1:8080/api/tools/draw_stroke?doc=demo&token=$TOKEN" \
     -d '{"layer_id":"layer_1","data":{"points":[[40,40],[400,300]],"size":12,
          "color":{"r":40,"g":40,"b":60,"a":255}}}'

# 3) render a region and fetch the PNG back from the CAS (thumb_url is rewritten to a GET-able URL)
curl -s -X POST "http://127.0.0.1:8080/api/tools/render_region?doc=demo&token=$TOKEN" \
     -d '{"region":{"x":0,"y":0,"w":256,"h":256}}'

# 4) submit an atom with a client-generated ULID (idempotent retries; the optimistic-rendering entry point)
curl -s -X POST "http://127.0.0.1:8080/api/atoms?doc=demo&token=$TOKEN" \
     -d '{"id":"01J...","kind":"create_layer","actor":"human:web","session":"s",
          "timestamp":1,"payload":{"layer_id":"layer_1"}}'
```

The WebSocket endpoint (`ws://127.0.0.1:8080/ws?doc=demo&token=$TOKEN`) pushes along the 6.8 boundary:
**control flow** (whole atoms) is broadcast globally, while **data flow** (tiles, thumbnails) is filtered by
the subscribed viewport. MCP stdio never receives pushes; it polls `get_log` / `get_job` / `get_render_status`.

#### Colour notation (one convention for the whole tool layer)

| Form | Meaning |
|---|---|
| `[r,g,b]` / `[r,g,b,a]` with every component ≤ 1 | straight linear |
| `[r,g,b,a]` with any component > 1 | sRGB bytes 0–255 (equivalent to `{"r":…}`) |
| `{"r":0-255,"g":…,"b":…,"a":…}` | sRGB bytes (`a` defaults to 255) |
| `"#RRGGBB"` / `"#RRGGBBAA"` | sRGB hex |

Invalid colours are rejected in the tool layer (`invalid_argument`) and never reach the atom log.

#### Colour grading and filters (`retouch` group)

```bash
# adjustment: brightness_contrast / saturation / invert / levels
curl -s -X POST "http://127.0.0.1:8080/api/tools/add_adjustment?doc=demo&token=$TOKEN" \
     -d '{"layer_id":"layer_1","adjustment_type":"saturation","params":{"amount":1.6}}'
# filter: box_blur / gaussian_blur / brightness_contrast / saturation / invert
curl -s -X POST "http://127.0.0.1:8080/api/tools/add_filter?doc=demo&token=$TOKEN" \
     -d '{"layer_id":"layer_1","filter_name":"gaussian_blur","params":{"sigma":4.0}}'
```

Effects apply to **everything below them in the same layer**, so both tools place the new object at the top
of the layer unless you pass `z_index`. Adjustments run in linear light (inverting sRGB byte 128 yields 229,
not 127). Unimplemented effect names and out-of-range parameters are rejected with `invalid_argument`, and
`list_effects` reports the supported names.

### Desktop entry (Omarchy / Hyprland)

The client *is* the web editor, so no native GUI toolkit is involved: run the server as a systemd user
service, register it as a web app with Omarchy's own `omarchy-webapp-install` (`chromium --app`), and bind
a key. Full steps and measured results live in **[deploy/omarchy/README.md](deploy/omarchy/README.md)**.

```bash
cargo build --release -p yanshi-http
cp deploy/systemd/yanshi-serve.service ~/.config/systemd/user/
systemctl --user enable --now yanshi-serve     # loopback 8110, starts with the session
assets/brand/render.sh                         # render icons/favicons from the brand SVGs
omarchy-webapp-install "Yanshi" "http://127.0.0.1:8110/?doc=yanshi" deploy/icons/yanshi.png
# append to ~/.config/hypr/bindings.lua:
#   o.bind("SUPER + ALT + Y", "Yanshi", { webapp = "http://127.0.0.1:8110/?doc=yanshi", focus = true })
hyprctl reload && hyprctl configerrors         # expect: ok / empty
```

## Brand assets

`assets/brand/svg/` holds the nine source SVGs (horizontal/vertical/primary logos, light and dark icons,
favicon, ultra-mini, mono) and is the single source of truth for brand assets.
`assets/brand/render.sh` uses `rsvg-convert` to produce the sizes the product actually ships
(desktop icon 256/512, favicons 16/32/180 and `.ico`, README and documentation logos).
The server exposes them at `/favicon.svg`, `/favicon.png`, `/favicon.ico` and `/brand/{file}` (allowlisted).

## Design highlights

- **Atom log**: append-only — nothing is deleted or mutated. Client ULIDs make submission idempotent;
  the server `seq` is the only authoritative total order. `parents` records causality for auditing but never
  participates in ordering.
- **Fold evaluation**: atoms are scanned linearly by `seq`, maintaining a live atom chain per object;
  `revert` invalidates cascades, and later atoms that depended on a reverted atom are skipped with a
  `cascade_invalidation` warning; `reapply` restores only the target atom — the cascade chain must be
  resubmitted explicitly.
- **state@seq and declare_head**: `state@seq_n := fold(A_n, base_state(H_n))`, where `H_n` is the latest
  `declare_head` at or before `n`. `declare_head` is a heavy atom that uniformly implements time travel,
  `revert_to` and `restore_checkpoint`; it triggers a snapshot and full tile invalidation while the log stays
  append-only.
- **Three-tier Blob CAS lifecycle**: active (referenced by the current HEAD fold, hot storage), historical
  (referenced by some atom but not by the current state; cold archive with zstd, always retrievable), orphan
  (uploaded but never referenced; collected after a 7-day TTL). The GC root set is the reference closure of
  the whole log; the active Manifest produced by snapshots only marks hot/cold migration and is *not* a root.
- **Determinism tiers**: D0 (CPU compute kernel) is the bit-exact baseline; D1 (composition backend,
  previews, thumbnails) may differ by ±1 LSB; D2 (plugins, external services) may differ. The compute kernel
  is the single authoritative source.
- **Broadcast boundary**: control flow (whole atoms) is broadcast globally because client folding needs every
  atom; data flow (tile bitmaps, thumbnails, blob bytes) is filtered by viewport subscription or fetched on
  demand.
- **Tool exposure layering**: the 27 core tools are registered by default; extension groups are enabled via
  the `profile` parameter, keeping the agent's function-selection burden and token cost down.
- **Render compute kernel (D0)**: `yanshi-render` implements compositing, brush stamping, coverage
  rasterisation, adjustments/filters and bitmap patches in pure scalar CPU `f32`; in-memory tiles are f16
  linear premultiplied and output pixels are derived from those f16 tiles; all randomness comes from the atom
  `seed`, so replays are bit-identical. Tile size, cache eviction and coverage clipping never change pixels
  (property tests compare byte for byte).
- **Dual dirty propagation**: geometry dirty takes the union of object bounding boxes; structure dirty is
  computed through the dependency closure (objects above in the same layer, instance masters, group members).
  The invalidated tile set must cover every changed pixel; when in doubt the range is widened.
- **Thumbnails and export**: doc/layer/object/history/selection tiers, 32×32 incremental block updates,
  aspect-ratio fitting; PNG uses a dependency-free deterministic encoder (WebP/AVIF belong to the transport
  layer and are not implemented yet). The document-level thumbnail is pinned to HEAD (recomputed when stale),
  which is what makes “open is an image” show the current frame.
- **Dependency-free transport and minimal viewer**: HTTP/1.1 and RFC 6455 are hand-written on `std::net`
  (own SHA-1 handshake, frame codec with mask validation, fragmentation, ping/pong, 16 MiB frame cap) with
  keep-alive connection reuse; tokens travel via the `Bearer` header or the query string;
  `yanshi://blob/<hash>` is rewritten to a GET-able URL carrying the token. The single-page viewer offers
  brush/rectangle/ellipse/eraser, undo/redo, region previews, a thumbnail panel, a control-flow log and the
  consistency self-check.
- **Headless server and tool layer**: validate → authoritative `seq` → incremental fold → dual dirty →
  control-flow broadcast → Job → snapshot; the 27 core tools are registered by default and extension groups
  are profile-gated; `batch` shares one changeset; a sampling-replace conflict auto-creates a conflict layer
  and returns `conflict_layer_id`; annotations live in a separate append-only channel; capability tokens
  protect HTTP/WS but exempt stdio; MCP stdio submits and polls, while atoms are persisted as JSONL plus CAS
  and a render cache so a restart resumes instantly.
- **WASM compute kernel and local optimistic rendering**: the same compute kernel as the server (D0
  bit-exact); local incremental folding + tile invalidation + viewport linkage; the LRU tile pool has a hard
  cap and auto-evicts at a 90% watermark without any JS call, plus `evict_outside_viewport` for JS-driven
  eviction. The in-progress stroke lives in a **local pending overlay** that never touches the atom log; it is
  submitted on pointer-up and then corrected by the authoritative server state.
- **License**: MIT.

## Roadmap

| Phase | Summary | Status |
|---|---|---|
| Phase 0: technical validation | fold-engine prototype, algebraic property tests and fuzz, CPU D0 baseline renderer, Blob CAS race and three-tier lifecycle/GC prototype, liquify and WebGPU feasibility | ✅ done |
| Phase 1: atom core + fold + server-side rendering | atom model and append-only log, ULID idempotency, authoritative seq, fold evaluation with cascading invalidation, state@seq, Blob CAS submission ordering, layer isolation and tile chunking, server CPU rendering, 27 core tools, Job protocol, capability tokens, broadcast boundaries, HTTP/WS transport, minimal web viewer | ✅ done |
| Phase 2: WASM core + WS collaboration + local optimistic rendering | WASM compute kernel, control/data-flow separated WS broadcast, local optimistic rendering, WASM LRU pool and viewport linkage, Job protocol, import_image | ✅ done (L3/L4 caching and cross-fade correction outstanding) |
| Phase 3: basic retouch + GPU compositing + general brushes | GPU composition backend, general raster brushes and style system, clone/heal/patch plus basic liquify and colour grading, checkpoints and history browsing, conflict handling and the resolve_conflict macro, AI semantic tools | in progress — colour grading and filters are done (`add_adjustment` / `add_filter` / `update_*` / `list_effects` in the `retouch` group); retouch and liquify are still `Primitive::Unsupported` in the kernel, so no empty tools are exposed |
| Phase 4a / 4b: annotations / AI annotation parsing | separate annotation channel with CRUD and visualisation; AI parses annotations, proposes suggestions, accept/reject flow | 4a channel + CRUD done, 4b planned |
| Phase 5: plugins + advanced features | WASM plugin sandbox and capability model, instance and group references, advanced path editing, owner/editor/viewer permissions | planned |

## Tests and acceptance

| Command | Coverage |
|---|---|
| `cargo test --workspace` | 322 tests: core engine (atoms/log/fold/state@seq/snapshots/Blob CAS/conflicts), render kernel (brush/shape/adjustment/filter/dirty/thumbnail/PNG/colour/coverage clipping/incremental stamping), server (document service/Job/tokens/broadcast/annotations/persistence/tool layer), transport (HTTP routing/keep-alive/auth/WASM and brand asset hosting), MCP stdio, WASM kernel (incremental folding/overlay/watermark), plus property tests for the five 5.3 invariants and D0 determinism |
| `cargo test --workspace --release -- --ignored` | 100k-atom fold fuzz (Phase 0 exit criterion), render perf budgets (8.5 / 14.10), overdraw and tile hit rate, server view-mode open < 100 ms, coverage-clipping budget |
| `cargo doc --workspace --no-deps` | no rustdoc warnings (`missing_docs` is enabled) |

The mapping from design document to tests, the measured performance data and the known limitations live in
[docs/design/implementation-notes.md](docs/design/implementation-notes.md).

## Contributing

Contributions are welcome — please read [CONTRIBUTING.md](CONTRIBUTING.md) first. Before submitting:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass, and new fold or GC semantics must come with property tests. Commit messages are in
English.

## License

Released under the [MIT license](LICENSE).

Copyright (c) 2026 The Yanshi Authors

# Yanshi 偃师

[中文](README.zh-CN.md) | **English**

Yanshi is a headless image editor engine. It keeps an append-only atom log, evaluates state by
folding that log, renders through a pure-Rust compute kernel, and serves everything over a
zero-dependency HTTP/WebSocket transport with a minimal web viewer.

The same kernel compiles natively (server) and to WebAssembly (browser), so optimistic local
rendering and server rendering are one implementation with one set of results.

[docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) is the authoritative
specification (Chinese). The code must not diverge from it silently.

## Install

Rust stable (1.85 or newer) and `make`:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

For the browser kernel, add the WebAssembly target and `wasm-bindgen-cli` (the version must match
the `wasm-bindgen` crate):

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

Both are optional. Without them the server still runs and the viewer renders on the server side.

## Build and run

```bash
make run          # build the WASM kernel when the toolchain is present, build the server, start it
                  # then open http://127.0.0.1:8110/ (the page obtains its capability token)
make build        # server only
make build-wasm   # WASM kernel only
```

`make run` takes `PORT` and `ROOT` (default `~/.local/share/yanshi/workspace`):

```bash
make run PORT=9000 ROOT=/tmp/yanshi
```

Server without the browser kernel:

```bash
cargo run --release -p yanshi-http -- --root ./workspace --no-wasm
```

`make help` lists every target.

## Test

```bash
make test         # workspace test suite; seconds to a couple of minutes
make ci           # fmt, clippy, tests, WASM runtime smoke check
```

Long jobs — performance budgets, the 100k-atom fold fuzz, the 4K profile — are marked `#[ignore]`
and run on GitHub instead of locally:

```bash
gh workflow run heavy.yml
gh run list
```

`.github/workflows/ci.yml` runs the fast checks on every push; `.github/workflows/heavy.yml` runs
the long suite nightly and on demand, and uploads its log as an artifact. To run the long suite
locally anyway: `cargo test --release --workspace -- --ignored --nocapture`.

## Tools and effects

The tool layer registers 27 core tools; enabling every implemented group gives 65 tools in total.
Groups: core, history, retouch, annotation, collab, structure (`--profile` selects them).

Adjustments (12): brightness_contrast, saturation, invert, levels, exposure, white_balance, curves,
hsl, posterize, color_balance, split_toning, vibrance. `levels` accepts a per-channel `channel`.

Filters (13): box_blur, gaussian_blur, motion_blur, sharpen, clarity, dehaze, film_grain, noise,
vignette, glow, brightness_contrast, saturation, invert.

Retouch and liquify: clone_stamp, heal_stamp, smudge, patch, liquify_push, liquify_twirl,
liquify_pinch. Masks: rect, ellipse, polygon, with feather.

### MCP (agent integration)

```bash
cargo run --release -p yanshi-mcp            # stdio, in-memory, default document
cargo run --release -p yanshi-mcp -- --list-tools
```

### HTTP / WebSocket

```bash
# open or create a document; the response carries the capability token
curl -s -X POST http://127.0.0.1:8110/api/documents \
     -d '{"doc_id":"demo","width":1024,"height":1024}'

# call a tool (token via Authorization: Bearer or ?token=)
curl -s -X POST "http://127.0.0.1:8110/api/tools/create_layer?doc=demo&token=$TOKEN" \
     -d '{"layer_id":"layer_1"}'

# render a region; thumb_url points at the blob in the CAS
curl -s -X POST "http://127.0.0.1:8110/api/tools/render_region?doc=demo&token=$TOKEN" \
     -d '{"region":{"x":0,"y":0,"w":512,"h":512}}'
```

## Desktop entry (Omarchy / Hyprland)

```bash
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/yanshi-serve.service <<'EOF'
[Unit]
Description=Yanshi server
After=network.target

[Service]
ExecStart=%h/yanshi/target/release/yanshi-serve --bind 127.0.0.1:8110 --root %h/.local/share/yanshi/workspace --doc yanshi
WorkingDirectory=%h/yanshi
Restart=on-failure
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now yanshi-serve
```

```lua
-- ~/.config/hypr/bindings.lua
o.bind("SUPER + ALT + Y", "Yanshi", { webapp = "http://127.0.0.1:8110/?doc=yanshi", focus = true })
```

## Repository layout

```
crates/yanshi-core/     atoms, log, fold, state@seq, snapshots, Blob CAS, conflicts, annotations
crates/yanshi-render/   render kernel: brush, shapes, adjustments, filters, dirty, tiles, PNG
crates/yanshi-server/   document service, jobs, tokens, broadcast, tool layer
crates/yanshi-http/     HTTP/WebSocket transport, viewer, yanshi-serve binary
crates/yanshi-mcp/      MCP stdio server
crates/yanshi-wasm/     browser kernel (wasm-bindgen)
scripts/                acceptance scripts (demo, pixel self-check, wasm smoke, perf probes)
docs/design/            design document and implementation notes
```

## Documentation

- [Design document](docs/design/yanshi-v1.0-draft4.md) — the specification (Chinese).
- [Implementation notes](docs/design/implementation-notes.md) — module map, decisions where the
  specification is silent, measured performance data, known deviations.
- [scripts/README.md](scripts/README.md) — how to run the acceptance scripts.
- [SECURITY.md](SECURITY.md) — threat model and reporting.

## Status

Implemented: the atom log and fold with the five invariants under property tests, deterministic CPU
rendering (the compute kernel is bit-exact; blur-family filters are allowed ±1 LSB by an approved
scope decision), server-side rendering with thumbnails and tiles, the tool layer and its profiles,
the HTTP/WebSocket transport with capability tokens, the annotation channel with the AI suggestion
loop, and the browser WASM kernel.

Not implemented: GPU compositing (measured feasibility is recorded in the notes), plugin hosting,
and collaboration transport beyond a single server.

## License

MIT. See [LICENSE](LICENSE).

# Yanshi 偃师

[中文](README.zh-CN.md) | **English**

Yanshi is a headless image editor engine: an append-only atom log, a folded state, a pure-Rust
render compute kernel, a zero-dependency HTTP/WebSocket server, and a minimal web viewer.

The kernel compiles natively for the server and to WebAssembly for the browser, so optimistic
local rendering and server rendering are one implementation.

[docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) is the authoritative
specification (Chinese); the code must not diverge from it silently.

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
gh workflow run heavy.yml && gh run list            # nightly + on demand; logs are artifacts
cargo test --release --workspace -- --ignored --nocapture   # or run them locally
```

## Tools and effects

27 core tools are registered; enabling every implemented group gives 65 tools in total. Groups:
core, history, retouch, annotation, collab, structure (`--profile`).

Adjustments (12): brightness_contrast, saturation, invert, levels, exposure, white_balance, curves,
hsl, posterize, color_balance, split_toning, vibrance. `levels` takes a per-channel `channel`.

Filters (13): box_blur, gaussian_blur, motion_blur, sharpen, clarity, dehaze, film_grain, noise,
vignette, glow, brightness_contrast, saturation, invert.

Other: clone_stamp, heal_stamp, smudge, patch, liquify_push, liquify_twirl, liquify_pinch; masks
(rect, ellipse, polygon, feather); comments, annotations and the AI suggestion loop.

## Use it

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

## Desktop entry (Linux: Omarchy / Hyprland)

The instructions below use `systemctl --user`, so they apply to Linux only. On macOS the server
runs the same way (`make run`); only the autostart wiring differs.

```bash
cp deploy/systemd/yanshi-serve.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now yanshi-serve
```

```lua
-- ~/.config/hypr/bindings.lua
o.bind("SUPER + ALT + Y", "Yanshi", { webapp = "http://127.0.0.1:8110/?doc=yanshi", focus = true })
```

Details: [deploy/omarchy/README.md](deploy/omarchy/README.md).

## Layout

```
crates/yanshi-core/     atoms, log, fold, state@seq, snapshots, Blob CAS, conflicts, annotations
crates/yanshi-render/   render kernel: brush, shapes, adjustments, filters, dirty, tiles, PNG
crates/yanshi-server/   document service, jobs, tokens, broadcast, tool layer
crates/yanshi-http/     HTTP/WebSocket transport, viewer, yanshi-serve binary
crates/yanshi-mcp/      MCP stdio server
crates/yanshi-wasm/     browser kernel (wasm-bindgen)
scripts/  deploy/  docs/design/
```

## Documentation

- [Design document](docs/design/yanshi-v1.0-draft4.md) — specification.
- [Implementation notes](docs/design/implementation-notes.md) — module map, decisions where the
  specification is silent, measured performance data, known deviations.
- [scripts/README.md](scripts/README.md) — acceptance scripts.
- [SECURITY.md](SECURITY.md) — threat model and reporting.

## Status

Implemented: atom log and fold (five invariants under property tests), CPU rendering (the compute
kernel is bit-exact; blur-family filters are allowed ±1 LSB by an approved scope decision), server
rendering with thumbnails and tiles, the tool layer and profiles, HTTP/WebSocket with capability
tokens, the annotation channel and suggestion loop, and the browser WASM kernel.

Not implemented: GPU compositing (feasibility measured and recorded in the notes), plugin hosting,
collaboration beyond a single server.

## License

MIT. See [LICENSE](LICENSE).

# Yanshi 偃师

[中文](README.zh-CN.md) | **English**

Yanshi is a headless image editor engine: an append-only atom log, a folded state, a pure-Rust
render compute kernel, a zero-dependency HTTP/WebSocket server, and a minimal web viewer. The
kernel compiles natively for the server and to WebAssembly for the browser, so optimistic local
rendering and server rendering are one implementation.

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

Drag to paint; `＋ 图层` adds a layer; `撤销` / `重做` undo and redo. Wheel zooms around the cursor,
middle-drag pans, `+` / `-` / `0` zoom in, out and fit, and `1:1` shows one document pixel per CSS
pixel. `刷新` re-renders from the server and `一致性自检` runs the kernel-versus-server comparison.

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

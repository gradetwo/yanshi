# Medium plugins

Brush media beyond the generic raster brush - oil, watercolour, marker, pencil, pixel, vector - are
**WASM plugins** in this project's design. `example-dab.wasm` is a minimal, dependency-free reference
plugin that shows the ABI and doubles as the loader's self-test.

## Capability boundary

A host loads a plugin with `WebAssembly.instantiate` and **refuses it unless it imports nothing**, which
is what actually enforces the design's "no network, no system clock" rule: a module with no imports
cannot reach anything the host did not hand it. Randomness is not the plugin's own either - the host
passes a seed, and the same seed must produce the same bytes.

## ABI (version 1)

Exports:

| export | signature | meaning |
|---|---|---|
| `yanshi_abi_version` | `() -> u32` | must equal `1` |
| `yanshi_max_dab` | `() -> u32` | largest `size` the plugin will accept, for host quotas |
| `yanshi_dab_ptr` | `() -> u32` | address of the plugin's RGBA output buffer in its linear memory |
| `yanshi_dab` | `(seed: u32, size: u32, hardness_milli: u32) -> u32` | renders one dab, returns bytes written |

The host reads `size * size * 4` bytes from `yanshi_dab_ptr` after calling `yanshi_dab`, with `size`
capped by `yanshi_max_dab`. Float results are **D2** by the design's tolerance levels: they need not be
bit-identical across platforms, but they must be deterministic for a given seed and build.

## Building

```sh
cargo build -p yanshi-medium-example --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/yanshi_medium_example.wasm assets/mediums/example-dab.wasm
```

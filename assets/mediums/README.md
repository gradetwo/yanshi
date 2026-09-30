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

## Plugins in this repository

All of them are dependency-free `cdylib`s built for `wasm32-unknown-unknown`, import nothing from the
host, take their randomness only from the injected seed, and report their own `max_dab`.

| file | id | ABI | what makes it that medium |
|---|---|---|---|
| `example-dab.wasm` | `example-dab` | 1 | minimal reference: a soft round tip, used to pin the ABI itself |
| `oil.wasm` | `oil` | 2 | bristle channels with a seed-fixed direction, paint load running out along a stroke, wet-on-wet mixing against the colour already under the tip, a slightly heavier rim where paint piles up |
| `watercolor.wasm` | `watercolor` | 2 | an irregular angle-dependent boundary, a deposition band near the edge, translucent washes and paper grain |
| `marker.wasm` | `marker` | 2 | a flat chisel nib whose angle is fixed per stroke, so its width changes as the stroke turns, ink that darkens where it overlaps ink already on the canvas, and a faint bleed past the nib edge |
| `pixel.wasm` | `pixel` | 2 | a hard-edged square nib with **no anti-aliasing at all**: coverage is binary, the alpha takes exactly one value and the colour is exactly the tip colour, with no grain and no mixing. It is deliberately independent of the seed, since a square nib has nothing random about it, and the ABI check asserts that rather than the opposite |
| `pencil.wasm` | `pencil` | 2 | a soft round tip whose darkness follows `pressure^1.5`, graphite grain that makes light pressure read as broken grit rather than even translucency, and almost no mixing, because a dry medium does not pull the colour underneath up into the tip |

`marker` and `pencil` are registered in the viewer and in `scripts/medium-abi-check.mjs`, so they go
through the same boundary checks as the others: zero imports, ABI version, identical output for the same
seed and different output for a different one, quota enforcement, and the v2 context inputs.

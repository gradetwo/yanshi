# Sample artwork

Each sample below is a document painted **in this app, through its own UI, by an agent** - the same
way a user would. That is deliberate: painting a real picture exercises the whole stack, so the samples
double as acceptance tests. Several bugs found in this project were found exactly this way (a stroke
landing at an offset, a canvas that stopped refreshing after a heavy operation, a medium whose canvas
stayed blank). They are ordinary documents: open them from `打开…` → **示例作品**, look at them, keep
painting in them, or copy them and use them as a starting point.

| document | what it is | which features it exercises |
|---|---|---|
| `sample-oil` | a landscape in oil | the **oil** medium plugin: bristle texture, paint load running out along a stroke, wet-on-wet mixing |
| `sample-watercolor` | hills and a lake in watercolour | the **watercolour** medium plugin: irregular bleeding edges, edge deposition, translucent washes |
| `sample-brush` | foliage and flowers | the built-in brush with an `appearance` block: size/opacity/pressure curves, jitter and scatter dynamics, procedural `noise`/`grain` textures, wet paint (`paint_load`, `wetness`, `mixing`) |
| `sample-reference` | a feature reference sheet | text (Latin, digits and **CJK** through the embedded OFL atlas), shapes, selection-constrained painting, layer masks, object and layer moves, PNG export |

## How the media work

The media are **WASM plugins**, not parameter presets: the host compiles them, refuses any plugin that
imports a host function (so a plugin cannot reach the network or the clock), checks the ABI version,
injects the seed, and caps the output by the plugin's self-reported maximum. Each plugin's `id` and
`version` are recorded **with the atom**, so upgrading a plugin never rewrites how an existing document
renders. See `assets/mediums/README.md` for the ABI and the source of the two reference plugins.

## Recreating them

They were painted by driving the real UI over the DevTools protocol (pointer events on the canvas plus
the app's own tool calls). If a sample is ever lost, it can be repainted the same way; the point is not
the exact pixels but that the flows they exercise keep working.

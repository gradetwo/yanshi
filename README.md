<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/svg/logo-horizontal-dark.svg">
    <img src="assets/brand/svg/logo-horizontal.svg" alt="Yanshi" width="320">
  </picture>
</p>

# Yanshi 偃师

[中文](README.zh-CN.md) | **English**

**Interface language**: the viewer is bilingual. The top bar carries an `EN` / `中文` toggle, and `?lang=en` or `?lang=zh` picks a language for a link; the choice is remembered. Names that come from the server (document ids, layer names, brush names) are shown as they are.

Yanshi is a headless painting engine: an append-only atom log, a folded state, a pure-Rust
render compute kernel, a zero-dependency HTTP/WebSocket server, and a minimal web viewer. The
kernel compiles natively for the server and to WebAssembly for the browser, so optimistic local
rendering and server rendering are one implementation.

[docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) is the authoritative
specification (Chinese); the code must not diverge from it silently.

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

## Enabling all tool groups

`--profile all` enables every implemented group in one word, which is 114 tools against the 46 that core alone gives;
it deliberately excludes semantic, which is reserved by decision. Both `yanshi-serve` and `yanshi-mcp` list the
values in `--help`. Layer blend modes are validated against the renderer's own list, so an unknown mode is refused
with the available names instead of being written to the log and silently ignored.

## Grain, stroke outlines, and a note on honesty

`appearance.texture` adds a paper-like grain: `"noise"` or `"grain"`, or an object with `kind`, `scale`, `strength`
and `seed`. These approximate watercolour paper and canvas, and can be tuned, but there is no dedicated rough-paper
or canvas-weave preset. Shapes take `stroke_width` and `stroke_color`:

    {"layer_id": "L", "data": {
      "geometry": {"kind": "rect", "x": 20, "y": 20, "w": 120, "h": 90},
      "color": {"r": 200, "g": 180, "b": 140, "a": 255},
      "stroke_width": 4, "stroke_color": {"r": 40, "g": 40, "b": 40, "a": 255}}}

## Batching without previews

`batch` accepts `silent: true` to skip the preview that each nested write call would render, which matters when a
large area is hatched with hundreds of strokes and none of the intermediate previews will ever be looked at. It
defaults to false and only affects previews; the atoms and the drawing are unchanged.

## Broken colour

`appearance.dynamics.color_jitter` from 0 to 1 lets the tip colour wander slightly around the given colour, stamp by
stamp, which is what oil painters call broken colour. It is off by default and the perturbation is skipped entirely
at zero, so existing documents render exactly as before.

### Importing a project package

`import_project` is the other half of the export added earlier, so backing up a document, moving it to another
machine or reproducing someone else's problem all close the loop. It reads the same uncompressed tar the exporter
writes, with a reader written by hand for the same reason the writer was: tar is a header and the bytes, no
compression library needed, and any system's tar can inspect it.

Three rules come from earlier lessons. It never overwrites: an existing document id is refused, because losing a
document is irreversible. Every blob is verified against the hash in its path after being written to the content
addressed store, so a package whose bytes are damaged is refused rather than half imported. And when something is
missing the error says what the package does contain, so the reader can see whether they were handed the wrong file.
A checksum failure is reported as a checksum failure, and a rejected import leaves no document directory behind.

The test that matters exports a document containing a shape and a medium stroke, imports it into a separate workspace,
renders both and compares them byte for byte. An import that reports success while having lost content is the same
class of defect as a tool that accepts input and does nothing with it.

## Project packages

`export_project` writes a `.yanshi` package: an uncompressed tar containing the append-only atom log, the document
metadata, the content-addressed blobs the log references, and a render of the current head taken at export time. Any
system `tar` can list and extract it. Use it instead of copying a document directory by hand, because the cached
`render.png` on disk may be stale.

## Medium texture and mixing

`medium_stroke` takes `texture` from 0 to 1, which flattens the oil plugin's bristle and grain so a large area can be
covered smoothly; 0 is the default and is byte-identical to before, so nothing you already painted changes. The
server now samples the canvas under the brush before painting, so the plugin mixes the tip colour with what is
already there, as the browser does.

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

## Copyable call examples

Every tool that carries one has a verified argument object in
[docs/design/tool-examples.md](docs/design/tool-examples.md), generated from the running server's own
catalogue so it cannot drift from what the tools accept. All of them are exercised against a live
server before they are kept, and a check mode fails when the document is stale.

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
or `points`. The `bbox` may be an object, a four number array, or omitted in favour of `x`, `y`, `w` and `h` written
directly on the geometry; all three spellings draw, because normalisation happens before validation. Anything else is refused with a message naming the accepted forms, rather than returning success and
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
enforced in the tool layer using each tool's own `mutating` flag, so a read-only token can call read tools but any
tool that changes the document is refused with a permission error naming the role and the tool. It sits in the layer
every entry point passes through rather than at one endpoint, because the check was once made at the HTTP endpoint
only and the WebSocket path, which sets its own context, bypassed it entirely. An unknown role is refused rather than
quietly treated as an editor, since that would turn a read-only link into a writable one.

## Release package

`make release` builds the release binaries and packs them with the runtime assets they need: the two executables, the
browser-side WASM kernel and the medium plugins. It writes a versioned tarball and a checksum file under `dist/`.
`make help` lists the other entry points, and `make check` runs the format, lint and test gates.

The package is **static by default**, because a dynamically linked binary inherits its build machine's glibc and then
refuses to start on older distributions. Use `make release-dynamic` only if you specifically need dynamic linking.

The browser-side kernel is a build artefact, so a fresh clone does not have it. Rather than failing, the packaging
warns and produces a package anyway, because the viewer falls back to server-side rendering; run `scripts/dev.sh`
(building the kernel additionally needs `wasm-bindgen-cli`) to include it.

On macOS there is one prerequisite for the WASM half of the build. rustup's `rust-lld` is linked against
`@rpath/libLLVM.dylib`, which the toolchain does not ship, so every `wasm32-unknown-unknown` build stops with a screen
of dyld search paths; that is a packaging problem in the toolchain and not in this repository. Homebrew's `lld` fixes
it, and it is **its own formula**: the `llvm` formula contains clang and the llvm tools but no linker at all, so
`brew install llvm` is not enough. The packaging script finds it and uses it, so two commands are all it takes:

    brew install lld
    make release

To confirm the linker landed where the script looks, `ls -l /opt/homebrew/opt/lld/bin/` should list `lld` and
`wasm-ld` (they are the same binary under two names). `YANSHI_WASM_LINKER=/path/to/lld make release` names one
explicitly, and `rustup update` often clears the failure as well, because toolchain builds ship the library
differently. Without a usable linker the release still completes using the plugin assets committed in
`assets/mediums`, which may lag the sources, and the script says exactly that instead of leaving you to read dyld
paths.

    make release                          # build and pack into dist/
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

`export_png` renders the whole document, or a rectangle of it, or a single layer, and writes it to a path at any
size without going through base64 or the size limit of an inline preview. Exporting a layer ignores whether it is
hidden, because naming a layer to export is an explicit request, and it deliberately bypasses the region byte cache:
that cache is keyed by region and version but not by layer, so reusing it could return another layer's pixels, which
is the classic cache-key-missing-a-dimension bug and one that only shows up on a cache hit.

New, open, import and export live in a single File menu in the header rather than scattered through the information
panel, which is where a person expects them and which keeps the panel for information. The controls are the same
nodes moved into the menu, so their ids, listeners and behaviour are unchanged, and the project packaging card that
used to occupy a tab of its own now sits there too; a tab that ends up with no cards is hidden rather than left
empty. The script requires all five controls to be inside the menu, none of them to remain in the panel, the export
inside the menu to actually write an image rather than merely be clickable, and Escape or a click outside to close
it.

The right column is split into five tabs, drawing, history, assets, file and diagnostics, which holds the same
seventeen cards as before and shows only the ones belonging to the active tab. The cards are moved rather than
rebuilt, so their listeners, selected options and drawn thumbnails survive a switch, and the grouping lives in the
markup as a data-panel attribute rather than being guessed from order. The page itself no longer scrolls at all;
each panel scrolls inside itself, and the canvas stage is the scroll container for a canvas too large to fit. Fitting
caps the display size to the available area so no scrollbar appears, while zooming past fit lifts that cap, both the
inline size and the stylesheet's maximum width, because the latter alone silently clamped the canvas back to the
stage and made the inline width pointless. Two rounds were spent on this for a measurement reason worth recording:
the first reading was taken during startup, when the canvas is still the 300 by 150 default and the viewport is still
the initial 1024 square, so it looked as though the computed size never reached the element. The tab panes also have to be built before the asset dock records where
the palette and texture cards live, otherwise closing the dock returns them to a node that no longer holds them,
which the layout script checks.

The brush area can be collapsed to a single toggle, which only hides the controls and leaves the chosen brush,
colour and smoothing untouched, so folding it away is never a way to lose a setting. The palette and texture cards
float above the canvas from the same row, by button or by key: P brings the palette up, T the texture, Escape puts
it back, and a backslash folds the brush area. Whichever card a key asked for gets a visible outline, because a
key that appears to do nothing is indistinguishable from a broken one. The dock carries an auto close box, on by
default, so picking a colour puts it away, and turning it off is how you keep it open through several swatches.

Both texture and gradient can be limited to the current selection with a checkbox; the region comes from the
selection the server reports rather than from anything guessed locally, and the viewer syncs the selection's shape and
not just its description, so a selection made through MCP or another client is drawn and honoured here too. If the
checkbox is set and there is no selection, the tool call is refused with a message saying so, because quietly filling
the whole canvas instead would be the kind of silent substitution this project keeps having to remove.

Picking a colour from a palette can be sent to the brush colour, the gradient start or the gradient end, chosen with a
selector next to the palette, and the panel says which of the three it wrote to. The default remains the brush colour,
and the brush colour still goes through the same setter as everywhere else, so there is one notion of the current
brush colour rather than two that drift apart.

The brush picker groups the brushes by the source recorded in their names and offers a search box, and it loads the
list when the user first reaches for it rather than on page load. Beside it, a brush library shows the brushes as a
browsable list where every row carries a preview painted by that brush through the same server call the stroke path
uses, so what the list shows is what the brush does rather than an illustration that can drift from it. Previews are
lazy and cached: only rows scrolled into view are painted, three at a time, which came to nine server strokes for a
list of one hundred and ninety nine brushes. Clicking a row goes through the same setter as the picker and as MCP, so
there is still one notion of the current brush, and the panel reads the same filter as the picker instead of keeping
a second one. Rows are deduplicated by name, because favourites and recents are a second entry point to a brush
rather than a second brush, which reads as a duplicate in a browsable list even though it is right in a select. That promise was in the tooltip long before it was
true: nothing actually loaded the list on interaction, so a person opening the picker saw only the built-in brush.
My earlier verification of the picker had called the loader itself, which is how the gap survived being tested. The
trigger now lives in its own small script next to the markup, because the page's main script is not the global scope
and the functions defined there are not reachable by name from anywhere else, which is what made three earlier attempts
fail silently.

### Undo in gestures, not atoms

`undo_last` and `redo_last` undo and redo the last few gestures. A gesture is every atom sharing one object id, so a
stroke that produced several atoms is undone whole rather than leaving half of it behind. Structural atoms, such as
creating or deleting a layer, are not touched: naming a layer is not a mark, and a request to undo the last stroke must
not quietly delete a layer and its contents, so they stay with `revert` and `revert_changeset`. Atoms that are no
longer alive are skipped and counted, which makes the pair idempotent and keeps it from reporting success for a revert
that changed nothing.

The viewer's undo and redo buttons call these tools and display the counts the server reports, not counts kept locally.
A local stack drifts as soon as the page is reloaded or another client edits the document, and then the interface
claims three steps are available while nothing can be undone.

### Project packages in the viewer

The export and import tools existed with no way for a person to reach them, so the viewer now has a card for both. The
path is a path on the server, not a file chooser in the browser, and the card says so, because the opposite assumption
would be the natural one. Importing a package creates a new document and refuses to overwrite an existing one, so the
card reports the new document id and the exact command to mint a token for it, since tokens are issued per document and
the one the viewer already holds will not open it.

A first version of the export line printed question marks for the atom and blob counts because I wrote the field names
from memory and the tool does not return those; a line reading "? atoms, ? blobs" is worse than no line at all, so the
card now reports only the fields the tool actually returns.

### Favourite and recent brushes

Workspace preferences are a small key and value store, reachable through get_preferences and set_preferences, so that
what a person prefers lives in the tool layer and can be read from MCP as well as from the interface; a browser's local
storage would be invisible to everything else. Writing merges, so naming one key leaves the others alone, and a null
value deletes a key rather than leaving an empty shell behind. File backed workspaces write them to preferences.json
and reopening the workspace brings them back, which is the whole point of a favourite; an in-memory workspace says that
it will not persist, since a write that quietly evaporates is the same defect as one that never happened.

The brush picker puts a starred group and a recently used group above the groups by source, with the selected brush
recorded on every change and a button that toggles the current brush into or out of the favourites. Groups that end up
empty are hidden, and the search filter still hides the groups that no longer match.

Laying a texture no longer accumulates layers: the viewer removes the texture layer it created last time before
asking for a new one, so clicking repeatedly leaves exactly one texture background rather than one per click, which is
what a user saw after clicking five times. The tool keeps its own contract of creating a layer and moving it to the
bottom, and the replacement logic stays in the viewer rather than being pushed into the tool.

Applying a texture also reports that it is working, because measurement showed the canvas still had no ink one second
after the click and only filled about six seconds later, once the server had rendered the texture and the preview had
come back. A control that appears to do nothing for six seconds reads as broken, so the panel now says what it is doing
straight away and then reports the result.

### Solve it like the leaders do

When something is hard, look at how the leading tools in the field solve it before inventing an approach. MyPaint,
Krita, Photoshop and Procreate have each spent years on brushes, palettes, selection and undo, and their answers are
usually both simpler and better informed than a guess made here. This is written down as a rule because the opposite
happened: the brush colour problem was solved by copying MyPaint's own model, where the colour is the `color_h`, `color_s`
and `color_v` settings, rather than by inventing a mechanism.

`brush_stroke` takes an optional colour and overrides those three settings, so a MyPaint brush can finally be painted in
any colour, which is what MyPaint itself does when you pick a brush and then pick a colour. Before this there was no
single tool with both MyPaint physics and a caller-chosen colour: one had the physics without the colour and the others
had the colour without the physics. Verified by measuring the pixels a red stroke leaves: the values come out at two
hundred and twenty, thirty one, thirty one, against the two hundred and twenty, thirty, thirty that were asked for.

### Descriptions say when to use a tool

Tool descriptions now open with the situation they are for and name the neighbours they compete with, because a model
chooses between tools by reading them. An external test report showed the cost of the opposite: the four stroke tools
said how to call them but not when to use which, so a model used the simplified medium interface for four revisions
before discovering that the MyPaint brush tool was the one it wanted. The asset listing now also names the kinds it
takes, with counts, and says that there is no separate listing tool to look for.

## Compatibility

By the owner's ruling on 2026-10-02, backward compatibility with older versions and older data is not a concern at
this stage. Old documents, old logs and old packages may be discarded by default, and supporting them must not shape
current decisions; the question will be reopened only if the owner asks. This supersedes a working assumption used
until now, in which new switches defaulted off and were skipped entirely so that existing documents stayed
byte-identical: that constraint is lifted, so how existing documents render may change, and on-disk formats, atom
kinds and package layouts may change freely.

Determinism is not affected. The same input producing the same output within a given build is a property of the code,
not a promise about data written by earlier versions, and the tests that assert it stay.

## Dependencies

The project used to take exactly one dependency, `wasm-bindgen`, and no others. By decision it now also depends on
**Hokusai**, a pure Rust brush engine inspired by libmypaint (`hokusai` with its default `myb-json` and `tile-mem`
features, plus `thiserror` pulled in by those crates). The reason is concrete: Hokusai reads libmypaint `.myb`
brushes, so the CC0 brush pack vendored under `assets/brushes` can be used as authored rather than approximated, and
it is pixel aligned with libmypaint on 188 of 196 stock brushes. It is Apache-2.0 or MIT, unsafe free, and builds for
wasm32. The `tiny-skia` feature is deliberately not enabled. Everything else stays dependency free: the HTTP stack,
the renderers, the storage layer and the medium plugins are still written by hand.

## Credits and licences

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

## Paper and canvas textures

Textures are CC0 and live in a per workspace cache rather than in the repository, because the smallest useful variant
from ambientCG is thirteen megabytes and committing a set would bloat the tree. `scripts/fetch-textures.sh --root
<workspace>` downloads the colour map of a few paper and cardboard assets into `<workspace>/textures/` and writes a
`NOTICE.md` recording the source and the CC0 licence. The `list_textures` tool then reports what is cached, with each
entry marked usable or not, since the pixel decoder handles PNG only. Because that tool is in the tool layer, MCP and
the viewer both reach it, which is the rule for every capability here.

## Importing brushes, textures and palettes

`import_asset` copies a file into the workspace's asset cache and is the single entry point for both MCP and the
viewer, which is why it accepts two sources: `path` for a file already on the server, which suits MCP, and `blob` for
a file the browser has uploaded first, since a browser cannot name a server path. Both funnel into one kernel method,
so validation, directory layout and overwrite semantics exist once. `list_assets` reports what is available for a
kind, with `source` distinguishing assets shipped with the release from ones imported or fetched locally, and
`usable` saying whether the format can actually be used, since the pixel decoder handles PNG and the brush engine
reads `.myb`. Importing over an existing name is refused unless `overwrite` is set, and names may not contain path
separators. The release package carries `textures`, `brushes` and `palettes` under `share/yanshi`, which the wrapper
points at with `--assets-dir`.

## Palettes

Two sets ship under `assets/palettes`. **Open Colors** is MIT and is the one the project owner chose to replace
MyPaint's own: fifteen families and a hundred and thirty two colours. The **sK1 Project multiformat palette
collection** contributes forty one palettes as `.gpl`, which the sK1 author released into the public domain with the
plain statement that the files may be used for any purposes; the notice in that directory records the canonical
address, the mailing list post it comes from, and the mirror the files were fetched through, whose own readme
disclaims any licensing. Only the `.gpl` variant is kept: the collection's `.skp`, `.xml`, `.ase`, `.jcw`, `.cpl`
and `.soc` files have no parser here, and shipping files nothing can read is the same defect as advertising a
greyscale PNG as usable.

The texture panel shows a thumbnail for each texture, served by a `/textures/<file>` route that resolves names
in the same order as the tools do, cache before bundled, so the picture shown and the pixels applied cannot disagree.
The route accepts only a plain `.png` name, refusing path separators and other extensions, and returns a specific
status for a bad name and a missing file rather than a generic failure.

## Backgrounds that are not a brush

Two tools exist for the problem of covering a large area, which brushwork handles badly: a stroke leaves its edges and
its sampling noise behind, which is how a background becomes a row of stamps or a set of bands. `texture_background`
tiles a paper or canvas texture, and `gradient_fill` lays a linear or radial gradient. A gradient is a pure function
of its two colours and its direction, so it has no edges, no noise and no randomness, and the same arguments always
produce the same pixels, which a test asserts by requiring two runs to yield the same content addressed blob. The
panel offers two colour pickers, a type and an angle.

Worth recording from checking the result: a render of a sky gradient with a soft radial light appeared to have a faint
horizontal band and a vertical edge, and measurement showed both to be imagination, with neighbouring rows and
neighbouring rows and columns differing by three out of two hundred and fifty five. That is Mach banding, an artefact of the eye
rather than the renderer, and it is the reason the habit here is to look first and then measure before believing it.

## File names must differ by more than case

No two paths in the repository may differ only in case, because macOS and Windows filesystems are case insensitive
and the pair would silently overwrite each other on checkout, leaving a clone quietly missing files that are present
in the repository. The brush pack shipped two such pairs upstream, `Pen.myb` with `pen.myb` and `Knife.myb` with
`knife.myb`, four genuinely different brushes; they are renamed from the `parent_brush_name` recorded inside each
`.myb`, and the mapping is written down in `assets/brushes/CASE-NOTES.md`. A test walks the whole repository and
fails on any directory holding two names that differ only by case, since this class of defect is invisible on Linux
and would otherwise surface only for the people it breaks.

## Painting with MyPaint brushes

Hokusai gives the project the libmypaint brush format, and `brush_stroke` drives a `.myb` brush from the tool layer,
so MCP and the viewer both reach every one of the hundred and ninety six CC0 brushes vendored under `assets/brushes`.
The viewer has a brush picker beside the medium picker; choosing a brush routes the brush tool's strokes to that
tool, so the same set is reachable from the interface and from MCP. A brush is named, not pathed, and resolves through
the same precedence as listing: something imported into the
workspace cache wins over the shipped copy, which is how a user replaces a brush without touching the repository.
`size` overrides the brush's own radius if given. Control points are interpolated at two pixels because Hokusai
expects a stream of pointer positions, and feeding it only sparse points produces detached stamps, the same failure
that was reported for the medium path. The stroke is rendered into a Hokusai surface, converted from its fix15 tiles
to RGBA, stored as a blob and committed through the same `import_image` path the medium strokes use, so both engines
share one commit implementation. The same stroke twice produces identical pixels, which is asserted.

## Palettes and texture backgrounds

`list_palette_colors` reads a palette from either the shipped set or the workspace cache and returns each colour as
components and as hex, so a caller can either paint with it or drop it into a colour input. It understands the GIMP
palette text format used by the sK1 collection and the JSON shape Open Colors uses, and it reports how many colours
the file held against how many were returned, because a palette truncated to five hundred without saying so reads as
a palette that small. A file that cannot be parsed at all is refused rather than reported as empty.

Both have panels in the viewer: the palette panel lists the palettes, draws their colours as swatches and sets the
brush colour when one is clicked, and that one colour is what the stroke, shape and fill tools all read, so picking a
swatch changes what every drawing tool paints with rather than only the brush. A texture can be laid over the whole
canvas as a background or into a named region as a patch; a patch is not pushed to the bottom of the stack, because a
patch that moved itself underneath everything would be wrong far more often than right, and the response says which of
the two happened. Tiling is anchored to the document origin, so a patch and a full background line up rather than
showing a seam. and the texture panel picks a texture and a mode and applies it, showing any
warning the tool returned. Capabilities live in the tool layer so that MCP and the viewer share them, and the panels
exist so that the viewer is not the poor relation.

`texture_background` lays a texture over the canvas in one of three ways: tiled, which is the default because the
CC0 paper and cardboard textures are seamless and tiling avoids any resampling, stretched to the full canvas, or
scaled to cover and centre-cropped, the last two reusing the project's own bilinear resampler. When no layer is
named it creates one and moves it to the bottom, since a background belongs underneath, and it warns when a layer
that already has content was named instead, because measurement showed that within a single layer an imported bitmap
ends up above the objects already there.

Assets are found by resolving a list of candidates, not by trusting a relative default, and the chosen directory
is printed at startup. The default was the relative path `assets`, so starting the server from anywhere other than the
repository root silently produced empty lists for brushes, palettes and textures, which reads as a feature that was
never built rather than as files that were not found. If nothing is found the message names every path that was tried.

### Choosing what to build for

`make release` builds for the current platform, which is the default. `make release TARGET=<triple>` builds for a
specific one, and `make release-all` walks a list of common release targets, skipping any that is not installed with
a message saying how to add it and printing a summary of what succeeded, what was skipped and what failed; any
failure makes the run exit non-zero, because reporting success after a failure is the same defect as accepting input
and doing nothing. `make targets` lists which targets this machine can build, and `YANSHI_RELEASE_TARGETS` narrows or
extends the list for the all-targets mode.

Each target is packaged by re-running the same script with `--target`, so a cross build goes through exactly the same
checks as a native one, and the static and glibc logic keys off the target rather than the host, which matters because
cross compiling to Linux still wants the static build. Cross compilation also needs a linker for the target, which
Rust will report if it is missing.

## Packaging on macOS and other non-Linux systems

On macOS, rustup's `rust-lld` can fail with `Library not loaded: @rpath/libLLVM.dylib`, which stops every
`wasm32-unknown-unknown` build, the kernel and the six medium plugins alike. That is a packaging problem in the
toolchain, not in this repository. The workaround is Homebrew's `lld`, installed as its own formula, because the
`llvm` formula does not ship a linker: `brew install lld` puts `lld` and `wasm-ld` in `/opt/homebrew/opt/lld/bin`
(or under `/usr/local` on Intel), and the packaging script looks there, then at the `llvm` paths, then asks
`brew --prefix` for both formulae, so `make release` then just works. You can also name a linker yourself with
`YANSHI_WASM_LINKER=/path/to/lld make release`. `lld` is preferred over `wasm-ld` only because rustc passes
`-flavor wasm` and the multi flavour driver is the one that certainly accepts it, though the two are the same
binary behind different names. `rustup update` often clears the failure as well, since toolchain builds ship the
library differently. If the plugins cannot be rebuilt for any reason, the release still completes and packages the
plugin assets committed in `assets/mediums`, which may lag the sources, and the script says so when it does.


The glibc machinery is Linux only, and it used to run unconditionally: on macOS `ldd` and `objdump` do not exist, and
static linking is not the right answer there in the first place, since `-C target-feature=+crt-static` is a workaround
for a glibc problem that macOS does not have. The packaging now detects the host and, off Linux, defaults to a dynamic
build, skips the glibc check, and says why rather than failing. It also says plainly that a package with no GLIBC
symbols is not thereby portable, since portability is decided by the target system. Two other GNU-only assumptions
were removed while checking: `find -printf` in the texture fetch script and a hard dependency on `sha256sum`, which on
macOS is spelled `shasum -a 256`; the checksum helper picks whichever exists, and the command printed for the user
matches the platform.

## If the interface looks older than the checkout

The viewer page is compiled into the binary, so a server process started before a change will keep serving the old
interface no matter what the repository contains. `make dev` compares the running instance's reported commit with the
checkout and refuses to stand aside quietly when they differ, explaining that the page is compiled in and how to
either stop the old process or run on another port. A systemd user service on the same port makes this recur, so its
unit file is worth checking if the identity keeps going stale.

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

Where the controls are, since a few are easy to miss: the tool rail down the left holds the brush, shapes, text and
the rest, each with its icon and shortcut; the option row above the canvas holds the tool's parameters, including the
**medium** picker (example, oil, watercolour, marker, pencil, pixel), which is the control for painting with a plugin
rather than the built-in brush; the layer list on the right carries a per-layer eye and lock; and the buttons above
the canvas include `Export PNG`. Hiding the last visible layer blanks the canvas, which is what hiding means rather
than a fault; the eye toggles visibility and the lock only prevents editing. Exporting a project, as opposed to a
PNG, is available over the tool API as `export_project` and does not yet have a button in the viewer.

Drag to paint; `＋ Layer` adds a layer; `Undo` / `Redo` undo and redo repeatedly (multi-level). Wheel zooms around the cursor,
middle-drag pans, `+` / `-` / `0` zoom in, out and fit, and `1:1` shows one document pixel per CSS
pixel, `Export PNG` downloads the full-resolution render, the Open dialog lists the server's documents and imports local images (PNG/JPEG/WebP are decoded
by the browser and uploaded as raw pixels; over the tool API, `POST /api/blob` also accepts a **PNG**
directly and normalises it to raw pixels with the repository's own decoder, since the project takes no
external dependencies - JPEG and WebP are refused there with an explanation rather than stored as-is),
the history panel lists the atom log with kind and actor
filters plus a jump-back action, and the effects panel applies any adjustment or filter to the
current layer (names come from the kernel, parameters default to the kernel's own values).

Recent panels close the same kind of gap - a tool that existed with unit tests but no way to reach it. A PSD
chosen in the import dialog is routed to the server, which reads the flattened composite and says so in the
log; layer structure and masks are not imported, and the contract is read-only. The `Annotations` panel creates,
edits, resolves and deletes annotations and draws clickable pins on the canvas. The `Objects` panel lists the
current layer's objects, turns one into a live-linked instance, groups several together, rotates, scales or
moves them, and converts any object into a shape or a path so it can be kept as vector work. The history panel marks checkpoints and jumps
back to them; the log is append-only, so nothing is lost and the canvas can move forward again. Effects can be
edited after they are added, by clicking one in the list, which loads its parameters and updates that same
object instead of stacking another. A The history panel now inspects an atom: clicking one shows the payload of what it actually changed, which needed a new `get_atom` tool, since the log and search tools return metadata only by design. A `Comments` panel posts comments and lists them with their author and text, which needed a new tool: comments could always be written but no tool could read them back, a gap a browser check exposed and `list_comments` closes. Annotations can be rejected as well as resolved, and a suggestion can be previewed first: preview validates every patch step and reports it without applying anything, so the effect count stays put until accept is pressed. Every toolbar button carries an inline SVG icon and its keyboard shortcut in the tooltip, and a guard test keeps it that way after the annotation tool shipped without an icon and rendered as an invisible button. `medium_stroke` paints with a medium plugin from a server-side or MCP session, not only in the browser:

    {"layer_id": "L", "medium": "oil", "points": [[20, 30, 0.2], [60, 36, 0.9], [100, 42, 0.5]],
     "size": 24, "color": {"r": 210, "g": 80, "b": 40, "a": 255}}

Points take an optional pressure and the object records the plugin id and version. Six medium plugins ship as dependency-free wasm modules under assets/mediums, example, oil, watercolour, marker, pencil and pixel, and the object records the medium by id and version so upgrading a plugin cannot change how older documents render. The object panel can restyle a stroke, changing its colour, size and opacity, which the update_stroke tool had no tests for until this round added two. A changeset section in the history panel groups a run of edits so they can be undone together, and the served tool profile list was missing the changeset and conflict groups even though its own comment says the web editor enables the full set, so the button reported an unknown tool until that was fixed, while the semantic group stays deliberately absent. A `Suggestions` panel lists suggestions from section 12.6 with their status and priority, and accepting one replays its patch in order and resolves the annotation it came from, which is the other half of the review loop the annotation panel begins. The `Storage / maintenance` panel reports the blob store's three
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

`Selection` drags a rectangular selection that constrains everything painted afterwards - strokes, erasing,
shapes, fills, text, liquify and retouch - per pixel, so nothing outside it is touched, and `Clear selection`
removes it, after which the same objects render unconstrained again because the constraint is
recomputed from the log. `Text`
places a text object at the clicked point. Text stays an editable object in the log - editing its data
through `supersede` changes the render - and the kernel rasterises it with an embedded open-source
font: a built-in 5×7 ASCII font plus a 1-bit 16×16 atlas generated from Noto Sans CJK (SIL OFL 1.1),
covering ASCII, 6763 GB2312 level-1 and level-2 hanzi (3755 common plus 3008 less common) and the GB2312 symbol rows. Provenance, format
and the regeneration command live in `assets/fonts/`. `Refresh` re-renders from the server and `Consistency check` runs the kernel-versus-server comparison.

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

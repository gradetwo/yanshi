# Tools and effects

The tool layer registers **43 core tools**, and with every implemented group enabled there are **99 tools in total** (both numbers are asserted against the registry by `tool_inventory.rs`; keeping them on this one line means adding a tool edits one place, and the anchors `core tools` / `tools in total` must stay unbroken because that is what the test parses).
(`analyze_image`, `inpaint_region`, `generate_mask_from_prompt`, `semantic_replace`, `vectorize_stroke`,
`apply_style_transfer`) is **reserved but not implemented** by the project owner's decision - the provider
seam and the guarantees it owes are written down in [semantic-tools.md](semantic-tools.md), and no code,
registration or network path exists yet;
select them with `--profile` (HTTP and MCP).

The most recent core tools come from design section 9: the object-group operations `create_group`,
`add_to_group`, `remove_from_group` and `set_group_transform`, `create_instance`, which resolves its
master's geometry at render time, `detach_instance`, `link_to_master`, `get_resolved_state`, `update_sync_policy`, `update_override` and `get_dependency_graph`

Five `history` group read tools join the existing ones: `get_object_history` reports an object's currently
effective atom version chain, `find_atom` searches the log with `object_id` and `layer_id` filters and
reports the total match count rather than only the page it returns, `get_diff` reports the atom-level diff
between two sequence numbers along with a by-kind tally and the objects and layers touched, and
`get_ancestors` and `get_descendants` walk the reference graph in the two directions. The design lists these
names without semantics, so the split is recorded: an object's own atom chain belongs to
`get_object_history`, while the two graph walks answer "what do I depend on" and "what depends on me",
which keeps them from overlapping.

Path objects exist as `ObjectType::Path`, with `data.nodes` holding `{x, y, in, out}` points and cubic Bezier
segments between them plus a `closed` flag, sharing the stroke's style fields. Rendering flattens the nodes
into a polyline and reuses the stroke primitive, so brush parameters, appearance, selection clipping, dirty
planning and hit testing are inherited rather than reimplemented, while resolution independence comes from
geometry living in the log being re-rasterised per view. A segment with zero handles emits only its start
point, which is what makes `convert_to_path` keep the picture identical pixel for pixel; subdividing straight
segments looked harmless but shifted the sampling phase and changed 3042 bytes. `convert_to_shape` turns a path, or a stroke, into a polygon shape, taking its vertices from the same
flattening the renderer uses so that the shape's outline and what the renderer draws cannot drift apart, and
keeping the style fields while replacing `nodes` or `points` with `geometry`. The polygon closes implicitly,
which is what makes the outline a filled region, and the two atoms share one changeset so the conversion is
withdrawable in one action. The `path_edit` operator of the same name delegates to this implementation rather
than growing a second one. `path_edit` accepts both kinds and their operations differ: reversing a path swaps each node's handles, since reversing only the nodes
would silently deform the curve, and closing a path sets the flag rather than appending a node. `split` cuts
an open path at a node index into two, each half keeping the handle that belongs to it so the halves together
reproduce the original curve, and it refuses a closed path because a loop needs two cuts and the design does
not say how the second is given. `merge` joins two paths and aligns the tangents at the seam along the
neighbouring segments, which `join` deliberately does not do, and it refuses strokes rather than silently
behaving like `join`. Both drop a duplicated node when the endpoints coincide and report it as
`seam_deduplicated`, so the seam leaves no zero-length segment. `boolean` completes the operator list with
`union`, `intersect`, `subtract` and `xor`, taking its geometry from `yanshi_render::polygon`: a
Greiner-Hormann implementation chosen for predictable size and behaviour, with one traversal shared by all
four modes. Degenerate inputs are refused rather than guessed, because that algorithm is known to fail on
them - intersections landing on a vertex, collinear overlapping edges, and zero-area polygons all return an
explicit error naming the reason, and the module records that a sweep-line algorithm is the deliberate
upgrade path if such inputs ever need to work. Curves are discretised at the same subdivision the renderer
uses, so the result contains straight segments only and no longer carries Bezier handles. A result with
several rings, which xor of two overlapping shapes naturally produces, becomes one shape object per ring,
and the new shapes plus the two retired inputs share a single changeset so one withdrawal restores
everything.

The dangling-changeset tools of design 12.4 sit in the changeset group, which is where the design's own name
for them, an independent dangling changeset, points. The design describes the behaviour without naming tools,
so the surface is recorded: `submit_offline` submits the atoms appended during an offline window, `list_stashes`
supplies the branch comparison the design says the editor shows, `apply_stash` is the force-apply choice and
`discard_stash` the discard choice. The third choice, regenerating against head, is the caller's own work, so
no tool pretends to do it. The whole batch is validated against a copy of the state, folded atom by atom, before
anything is appended, because the design requires the atoms and their blobs to be packed together and
committing one at a time would leave the earlier ones in the log when a later one fails. On failure the batch
goes to a stash with the validator's reason, and stashes are persisted under the workspace so an offline window
survives a restart. Discarding drops only the pending replay and never the blobs, since design 6.3 keeps stash
blobs at the history level; there is no collector yet, and when one arrives stashes must count as roots.

`get_atom` closes the general form of the gap the comment panel exposed. Only four read-only tools in the whole
project returned payloads, while the log and atom-search tools return metadata only by design, being the polling
channel and a search respectively, so the interface could list what happened but could not answer what any single
atom actually changed. The new tool takes an atom id and returns its kind, actor, session, timestamp, message,
changeset, parents and full payload, with a clear refusal when the id is unknown rather than an empty record. The
kernel already had the by-id lookup, so this only exposes it to the tool layer, and the history panel can now show
what a selected atom did.

The annotation panel can also reject an annotation, which moves its status to rejected rather than merely
removing it from view, and the suggestion panel can preview a suggestion. Preview validates each patch step,
reporting tool names, targets and change classes without applying anything, which the browser check confirms by
showing the validation and leaving the effect count at zero until accept is pressed, at which point it becomes one.

Every toolbar button is checked to have a non-empty icon by a guard test that reads the viewer source, because
the renderer falls back to an empty svg and produces a button that is both invisible and unclickable. That had
already happened: the annotation tool I added two rounds ago shipped without an icon, which I found by
cross-checking the tool table against the icon table rather than by looking at the page, and the guard was then
proven non-vacuous by removing the icon again and watching it fail with the tool named.

The plugin medium mechanism was verified end to end this round rather than assumed. The viewer offers six
mediums, example, oil, watercolour, marker, pencil and pixel, each a dependency-free cdylib compiled to
wasm32-unknown-unknown and committed under assets/mediums, and choosing oil with the medium tool paints through the
plugin and records the medium as id oil at version two on the object. Recording the plugin id and version with the
object is exactly what the design requires so that upgrading a plugin cannot silently change how older documents
render.

medium_stroke paints through a medium plugin from the server or MCP side, which until this round was only
possible in the browser. The plugins are dependency-free Rust, so adding an rlib target lets the server link them,
and because all six exported the same C symbol names they originally collided; the exports are now split by target,
so the wasm build keeps the published ABI names while the native build gets unique ones and all six can live in one
binary. The stroke renders into a straight RGBA patch through the same import path the browser uses, so replay,
undo and the record of the plugin id and version on the object all come for free, and an upgrade still cannot change
how an older document renders. Points accept an optional pressure, and the server-side medium does not read the
canvas back for mixing, which is stated rather than glossed over.

export_png renders the whole document or a region, optionally rescaling it, encodes it with the repository's own
PNG encoder and writes it to a path, returning the path, the size and the byte count. It exists because the inline
image option on the render tool caps at 512 pixels and otherwise returns a yanshi blob URL, which is a pseudo
protocol that nothing outside the process can fetch, leaving no practical way to obtain a file. It does not change
the document, so it is not a mutating tool, but it does write to the filesystem, which is stated here rather than
implied.

Shape geometry is validated in the shared draw path, alongside the existing colour and medium validation, so a
shape whose geometry cannot be parsed is refused with a message naming the accepted forms instead of returning
success and committing an object that renders nothing. The accepted geometry is a kind of rect, ellipse or polygon
with either a bbox of x, y, w and h or points, an array bbox is normalised to the object form so a spelling that
passes validation also draws, and a rejected shape leaves no object behind. create_layer returns the layer id it
created, which previously took a second listing call to discover.

The object panel can also restyle a stroke, changing its colour, size and opacity through update_stroke, which
had no test coverage at all until this round added two: one checks that the object data and the rendered pixels
both change, and the other that a partial core block merges rather than replacing the fields it does not mention.

`list_comments` closes an asymmetry in the collaboration channel that a browser check exposed. Comments could
always be written, but no tool could read them back: the log and atom-search tools return metadata only, by
design, since they are the polling channel and a search respectively, so the comment text was stored in the
payload and unreachable. The sibling channels both had their readable half already, in `list_annotations` and
`list_suggestions`, so comments were the one that was missing it. The new tool lists comments newest first with
their text, author, session, timestamp and target object, supports `since_seq` for incremental polling and
filtering by author or by the object commented on, and mirrors the response shape of its siblings.

`import_psd` implements the read-only PSD import that design chapter 17 lists as a later item. The contract is
read-only and composite-only: it takes the already-flattened image the file carries, does not import the layer
structure, masks or blend modes, and never writes a PSD back. The result goes out as a single raster patch, so it
travels the same downstream path as importing a PNG and is revertible and replayable like anything else. It is
blob-first like every other import: the PSD bytes are already uploaded, the decoded RGBA is written to the store,
and only then is the atom committed. Unsupported files are refused with specific reasons rather than being
half-drawn, covering version two, sixteen and thirty-two bit depths, non-RGB colour modes including CMYK and
grayscale, ZIP compression, and truncated data, because half a picture is worse than a clear error.

`resample` completes the retouch group. The design lists the name without semantics, so the choice is
recorded: it resamples a raster object's pixels to a new size, accepting only raw RGBA bitmaps, and produces a
new blob plus a supersede, which keeps it non-destructive since the old atom stays in the log and remains
revertible. The default filter is bilinear because medium brushwork is continuous tone, and pixel art should ask
for nearest explicitly. The object's medium descriptor is preserved, because design 11.1 wants the plugin id and
version recorded with the atoms, and a `resampled_from` record is added alongside it so that nobody later
mistakes these pixels for what the plugin originally painted.

`blob_gc` implements the blob lifecycle of design 6.3 in the history group. It classifies every blob in the
store into the design's three levels - active, referenced by the current folded state; history, referenced by
some atom in a log but not in the current state; and orphan, uploaded but never referenced - and reports counts,
bytes and the hash lists for each. The root set is the closure of every atom reference in every log plus every
stashed atom's references, because the design says GC never deletes a blob any log atom refers to, and stashed
atoms are roots so that offline edits survive until reconnection. Deletion is off by default and requires both
dry_run false and confirm true, since it is irreversible, and it only ever touches orphans past the seven-day
TTL. Cold archival with zstd is not implemented, because that is a dependency decision this project has so far
avoided and it is recorded rather than taken silently.

`resolve_conflict` completes the `conflict` group and follows design 12.3 literally, which is the clearest
section of the design: it is a composition macro rather than a new atom type, so the folder is untouched and
the macro expands into atoms that already exist. `keep_ours` withdraws the opponent's atom and puts our
submission back on the formal layer, `keep_theirs` withdraws ours, `discard` withdraws both, and `merge`
changes no content at all because by then the caller has already submitted its own edit, so the tool only
closes the conflict. The conflict layer is tombstoned afterwards while its `metadata.conflict` mark stays for
audit. Two translations are recorded, because the design's table names operations this codebase does not have:
atoms cannot be tombstoned or have their layer changed, since the log is append-only, so "make their atom
ineffective" is a `Revert`, and "move our atom to the formal layer" is moving the object when the submission
created one and re-committing the same kind and payload onto the formal layer when it is a pixel patch.

`begin_transaction` and `commit_transaction` also live in that group. The design lists them beside the
changeset tools and says nothing else at all, not even how the two differ, so the difference is recorded here:
a transaction is a changeset that also rolls back. If a mutating tool call fails while a transaction is open,
the atoms already committed inside it are withdrawn and the transaction closes, and the error response reports
what was rolled back so the caller does not mistake it for a single failed step. Whether a call is mutating
comes from the tool's own spec rather than a second list, so a failed read never rolls anything back - that
would discard a caller's work because a lookup failed. Because a transaction is also a changeset, its atoms
stay revertible as a group, and the check sits at the single point every tool call passes through.

`begin_changeset`, `commit_changeset` and `abort_changeset` complete that group. `begin_changeset` opens a
changeset for the document and session, and every commit from then on joins it automatically, because the
auto-join lives at the single point all commits pass through rather than in each tool; `commit_changeset`
closes it while keeping the atoms, and `abort_changeset` withdraws them, reusing the same implementation as
`revert_changeset` so the two cannot drift. The design names these tools without saying where an open
changeset lives or what happens on repeated `begin`, so both choices are recorded: the state lives in the
workspace keyed by document and session, and a second `begin` is an error rather than silently reusing or
replacing the open one. Aborting withdraws rather than deletes, since the log is append-only, which also means
that the abandonment itself can be withdrawn.

`get_changesets` and `revert_changeset` live in the `changeset` group, which is not part of the default
profiles, so the counts above are unchanged. `get_changesets` lists the changesets in the log and
`revert_changeset` withdraws one by committing a `Revert` for each of its atoms, with those reverts
themselves grouped into one changeset, so withdrawing is itself withdrawable and the design's identity in
section 793, `revert(revert(x)) ≡ reapply(x)`, holds end to end. That identity was not always reachable:
commit validation used to accept only atoms with a state effect, which refused both `revert` and `reapply` on
a revert atom while the fold had already implemented the semantic, and the gap was recorded here as a
reported limitation. The project owner decided to relax the validation, so it now accepts a state effect or a
revert or reapply, and still refuses collaboration atoms, which produce no state effect and are not history
actions either. `skipped_history_atoms` is therefore always zero and remains only for compatibility.

**Path objects** now exist as `ObjectType::Path`: `data.nodes` is a list of `{x, y, in, out}` points with
cubic Bezier segments between them, plus a `closed` flag, sharing the stroke's style fields. The design
specifies neither the model nor how a path rasterises, so both choices are recorded. Rendering flattens the
nodes into a polyline and reuses the stroke primitive, so brush parameters, appearance, selection clipping,
dirty planning and hit testing are inherited rather than reimplemented, while resolution independence comes
from geometry living in the log and being re-rasterised per view. A segment with zero handles emits only its
start point, which is what makes `convert_to_path` keep the picture identical pixel for pixel; subdividing
straight segments looked harmless but shifted the sampling phase and changed 3042 bytes. `convert_to_path`
turns a stroke into a path by taking its sample points as nodes with empty handles and grouping the new path
and the retired stroke into one changeset. `path_edit` now accepts both kinds, and their operations differ:
reversing a path also swaps each node's `in` and `out` handles, since reversing only the nodes would silently
deform the curve, and closing a path sets the flag rather than appending a node.

Layer locking is now enforced rather than merely recorded: the commit layer refuses any atom that changes
an object whose own lock or whose layer's lock is set, and refuses creating objects on a locked layer, while
`locked` and `visible` remain changeable so a lock can always be released; a layer's own management atoms,
such as renaming, opacity, ordering, stay allowed, since the lock is over content, not over the layer record.
`duplicate_layer` copies a layer with its live objects, sharing the same blobs since the log is content
addressed, and places the copy directly above the original by submitting a complete z-order, because giving
it a bare `z_index + 1` would collide with the layer above and leave the copy's position decided by id. The
web layer panel lists layers top first with per-row visibility and lock toggles and buttons for adding,
duplicating, deleting and moving, keeping the existing select as the single source of selection so the panel
and the rest of the viewer cannot drift apart.

`transform_object` and `restore_object` join the `structure` group. `transform_object` is the
human-readable counterpart to `move_object`: it takes exactly one of a rotation in degrees, a scale, or a
translation, plus an optional anchor defaulting to the object's bounding box centre, and composes onto the
existing transform unless `compose: false` asks for a replacement; it commits through the existing `Move`
atom with an absolute matrix, so rendering, bounding boxes, dirty planning and hit testing need no new path.
`restore_object` brings a deleted object back through history, reverting the tombstones that removed it and
grouping those reverts into one changeset, rather than creating a fresh object with the same id, which would
throw away its atom lineage. One caveat is recorded in the tests: withdrawing that restore is the second
place where the validator refusing to revert a revert bites, so the tool reports the atoms it skipped instead
of pretending.

`path_edit` lives in the `structure` group and implements the three of design 792's operations that fit
the existing stroke geometry without inventing a path object model: `reverse`, `close` and `join`. The
other five - `split`, `merge`, `boolean`, `convert_to_shape`, `convert_to_path` - need a path model the
design does not yet specify, and are refused with an explanation rather than silently accepted. (`override` and sync policies other than `all` are not implemented yet and
are rejected rather than ignored).

This file is checked by `crates/yanshi-render/tests/doc_consistency.rs` (every adjustment and
filter name) and `crates/yanshi-server/tests/tool_inventory.rs` (the counts), so it cannot drift
from the kernel and registry silently.

## Adjustments (12)

brightness_contrast, saturation, invert, levels, exposure, white_balance, curves, hsl, posterize,
color_balance, split_toning, vibrance.

`levels` takes an optional per-channel `channel` (`rgb` / `r` / `g` / `b`). `posterize` takes
`levels` 2..64. `color_balance` takes `shadows` / `midtones` / `highlights` as `[r,g,b]` triples.
`split_toning` takes `shadows` / `highlights` triples plus `balance` and `amount`. `vibrance`
boosts low-saturation pixels more than saturated ones.

## Filters (13)

box_blur, gaussian_blur, motion_blur, sharpen, clarity, dehaze, film_grain, noise, vignette, glow,
brightness_contrast, saturation, invert.

`dehaze` requires `air: [r,g,b]`; `estimate_dehaze` (read-only) suggests it. `film_grain` takes
`amount`, `size` (1..8) and a `seed`; grain is keyed on document coordinates, so tile-composed and
whole-region renders agree.

## Retouch, liquify, masks

clone_stamp, heal_stamp (colour-matched), smudge, patch (captures `source_region` as a blob and
lands it at `target`), liquify_push (pushes along `direction`), liquify_twirl (rotates),
liquify_pinch (contracts; a negative strength expands).

Masks take a `shape` of `rect`, `ellipse` or `polygon`, with `feather`.

## Storage maintenance

`collect_garbage` runs the design's orphan collection (6.3): the root set is the whole log's
reference closure plus the active manifest, so anything a revert, a time traveller or a Stash
references is never touched. It defaults to a **dry run** that only reports counts and bytes per
lifecycle tier (active / historical / orphan / expiring); pass `confirm: true` to actually reclaim
orphans older than the TTL (7 days by default; pass `ttl_seconds` to tighten it, which is how
render-produced orphans - written seconds ago - become reclaimable). Render previews, exports and pixel self-checks
write blobs that no atom references, so this is the tool that reclaims them.

## Collaboration (Phase 4b)

`create_annotation` / `update_annotation` / `resolve_annotation` / `reject_annotation` /
`delete_annotation` / `list_annotations` / `get_annotation` over an append-only annotation channel,
plus the AI suggestion loop: `suggest` (a patch plus a `priority`), `preview_suggestion`
(validation and estimated affected scope, no side effects), `accept_suggestion` (replays the patch
through the tool dispatch table), `accept_suggestions` / `reject_suggestions` (batches that do not
abort on a single failure), `reject_suggestion` (records the reason) and `list_suggestions`
(status, conflicts, `since_seq` polling).

## 选区（设计 4.4；语义由用户确认为「约束落笔」）

| 工具 | 说明 |
|---|---|
| `create_selection` | 创建选区（`shape` + `feather` + `invert` + `mode`：`new`/`add`/`subtract`/`intersect`），**约束之后新落笔的像素范围**；`linked_layer` 可只约束某图层 |
| `delete_selection` | 删除选区（tombstone）：删除后落笔不再受约束，**已画内容保持不变** |
| `list_selections` | 列出选区（设计 4.4 的 `shape/feather/mode/invert/linked_layer/refined_edges`）|

设计只给了选区的**数据模型**与类型清单 ✓、未规定它对落笔的作用 ✗；用户确认采用
**路线 A「约束落笔」**（只影响新落笔、不改写已有内容、可撤销 ✓），已记入 implementation-notes。

## 修改对象内容（设计邻近，设计未规定工具名）

| 工具 | 说明 |
|---|---|
| `replace_object_data` | 替换对象的 `data`（走设计 5.2 的 **`supersede`** ✓）：文本换文字、笔触换点列/颜色等，**对象保持可编辑** ✓ |

设计把"修改对象内容"交给 `supersede` ✓，但 **10.x 的工具清单没有列出对应入口** ✗；
`update_object` 只覆盖 6 个属性键（`visible/locked/z_index/layer_id/metadata/type` ✓）。
此前只能自行构造原子 ✓，本工具把那条路补成正式入口 ✓。设计未规定处 → 已记入 implementation-notes ✓。

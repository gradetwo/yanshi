# Tools and effects

The tool layer registers **39 core tools**. With every implemented group enabled there are
**90 tools in total**. Groups: `core`, `history`, `retouch`, `annotation`, `collab`, `structure`; the design's `semantic` group
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
segments looked harmless but shifted the sampling phase and changed 3042 bytes. `path_edit` accepts both
kinds and their operations differ: reversing a path swaps each node's handles, since reversing only the nodes
would silently deform the curve, and closing a path sets the flag rather than appending a node. `split` cuts
an open path at a node index into two, each half keeping the handle that belongs to it so the halves together
reproduce the original curve, and it refuses a closed path because a loop needs two cuts and the design does
not say how the second is given. `merge` joins two paths and aligns the tangents at the seam along the
neighbouring segments, which `join` deliberately does not do, and it refuses strokes rather than silently
behaving like `join`. Both drop a duplicated node when the endpoints coincide and report it as
`seam_deduplicated`, so the seam leaves no zero-length segment.

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

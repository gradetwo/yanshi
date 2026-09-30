# Tools and effects

The tool layer registers **27 core tools**. With every implemented group enabled there are
**69 tools in total**. Groups: `core`, `history`, `retouch`, `annotation`, `collab`, `structure`;
select them with `--profile` (HTTP and MCP).

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

# Tools and effects

The tool layer registers **27 core tools**. With every implemented group enabled there are
**65 tools in total**. Groups: `core`, `history`, `retouch`, `annotation`, `collab`, `structure`;
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

## Collaboration (Phase 4b)

`create_annotation` / `update_annotation` / `resolve_annotation` / `reject_annotation` /
`delete_annotation` / `list_annotations` / `get_annotation` over an append-only annotation channel,
plus the AI suggestion loop: `suggest` (a patch plus a `priority`), `preview_suggestion`
(validation and estimated affected scope, no side effects), `accept_suggestion` (replays the patch
through the tool dispatch table), `accept_suggestions` / `reject_suggestions` (batches that do not
abort on a single failure), `reject_suggestion` (records the reason) and `list_suggestions`
(status, conflicts, `since_seq` polling).

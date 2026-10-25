---
name: yanshi-vector-tracing
description: 用 yanshi 的 MCP 工具把位图插画/标志复现成干净的矢量图 —— 语义合并、分层着色、软特征重建，最后 export_svg 出图。适用于把位图插画或 logo 转成矢量、或诊断「描出来又脏又碎」的问题。不适用于照片修图（那是 yanshi-photo-styling），也不适用于从零作画（那是 yanshi-painting）。
---

# Yanshi Vector Tracing — Raster Illustration / Logo Reproduction

Reproduce a raster illustration or logo as clean, vector-style artwork in yanshi (path shapes + masks), and export a faithful SVG. Distilled from production runs; measured evidence for each run lives in that run's own notes, not in this document.

## Core Principles

- **Visual judgment is the acceptance test — and it is the only pass.** Metrics (mean diff, IoU, dirty-point counts, curve-level distances) are supporting evidence only, and they can never overrule the eye: numbers all green but the picture wrong is a fail; numbers slightly off but the picture right is judged together with the eye. When a version scores better but looks dirtier or more fragmented, ship the cleaner one. The reviewing agent must personally zoom-render and compare against the reference before every delivery — never delegate the verdict to numbers.
- **Semantic merging is a visual decision, not an algorithm.** Programmatic segmentation produces raw fragments only. A reviewer who has *looked* at the reference groups fragments into semantic parts (a sofa = backrest top, two back cushions, two pillows, one seat, front rail, plinth, armrests…). More parts is not more accurate: over-segmented runs look dirty; merge until every part is a thing a human would name. When in doubt, merge.
- **Dirt comes from granularity and boundary noise, not from the tools.** Scratchy internal lines = fragments kept as shapes, or outlines fitted too tightly. Fix by merging harder and fitting looser — not by adding detail.
- **Vector-first from the start: draw with vectors and curves, not by converting pixels.** Look at the reference, recognize each long curve, and place anchors at its curvature extrema — then draw a long Bézier through them. A path is constructed from what the eye recognizes, not fitted point by point from a pixel mask or a skeleton: mask boundaries are pixel staircases and skeleton centerlines wobble pixel to pixel, so any path derived from them inherits the wobble no matter how much it is smoothed afterward. Smoothing a pixel chain (RDP, Chaikin, Catmull-Rom) only rounds the staircase — it never makes it a vector. Pixels are only for measuring and verifying — color, area, position, metrics — never for generating paths (see Phase 3).
- **Structure first, shading second, as separate passes.** Never bake shading into the structural pass. Shading is purely additive layers on top of a frozen structure.
- **Every color is sampled from the reference.** Base color = part's pixel mean; shading colors = the part's measured light/dark ends. Never invent a color, never reuse a neighboring part's tone because it "looks close".
- **Soft features are rebuilt, never traced.** Tracing a soft feature (blush, glow, soft shadow) yields gray mud or hard stickers. Rebuild it generatively (see Phase 5).
- **Outlines are strokes, not seams.** A black outline is a line, not the gap between two filled shapes. Never fake it with a dark underlay silhouette that colored shapes grow into: the seam width is set by segmentation error, not by the reference, and it comes out heavy. Draw the outline as its own stroked path (see Phase 3).
- **Shadow boundaries are colour changes, not lines.** A shadow's edge is a change of fill colour, never a stroked path drawn along it — drawing one invents a line the reference does not have. Before adding any stroke, sample the reference along the stroke's path: if the reference shows no dark line there, the stroke does not exist. Only silhouette edges and genuine ink lines get strokes (see Phase 3).
- **Shading that defines the form stays.** "Clean beats strong" applies to decorative shading only. When a soft shadow gives a part its volume, dropping it changes what the part is — keep it as mid-tone flat steps or a true gradient, sampled from the reference (see Phase 4).
- **Negative space is real holes.** Use even-odd paths; never paint white cover shapes. But verify the "hole" is actually closed in the source — letter counters may be open self-winding contours, and connected components that look joined may be separate. Reproduce what is measured, not what is assumed.

## Phase 0: Setup

- Canvas = the reference's own pixel dimensions (crop the reference to the artwork first if it sits inside a larger screenshot).
- Sample the palette up front: for logos, the exact dominant color by pixel census. For illustrations, per-part means come later, per part.
- Keep a run directory with: reference, segmentation data, the merge table (JSON), build script, exports, and a per-version REPORT. Version the artwork (v-next) instead of overwriting — comparisons need the previous version.

## Phase 1: Segmentation (program only)

- Fine over-segmentation is fine here — it is raw material, not the artwork. Produce a numbered fragment map image so the merge reviewer can see every fragment's outline and location.
- Segmentation is for measuring, not for drawing. Pixel masks tell you a part's color, area, and position; they never generate a path — contours and centerlines are drawn by recognizing curves (see Phase 3).
- **Never classify edges with a single brightness or single-channel threshold.** Anti-aliased edges live between colors: a single-threshold barrier eats shadow tones that sit just under the cut. Use a color-aware barrier — exclude pixels that belong to a color class first (e.g. orange-class by a large R−G gap) and only then threshold. After segmentation, check every part's pixel area against the reference mask for that part: a part that lost more than 10% of its area is re-segmented, never patched by growing the shape — growing bloats the outline it touches.
- A "black" background in a PNG may be alpha = 0, not black pixels: the artwork is the alpha mask. Threshold the alpha channel, not brightness.

## Phase 2: Semantic Merge Table (visual decision)

- The reviewer studies the fragment map against the reference and writes the merge table: each semantic part = {name, fragment ids, draw order}. Parts are things a human would name, at the coarsest granularity that still reads correctly.
- Sanity budget: a single prop (sofa, chair, vehicle) should land around 10–16 parts. If the table has 25+, merge again.
- Small scratch/detail fragments inside a part are absorbed into it — they must not survive as independent shapes.
- **Small dark features are parts, not dirt.** An eye's dark patch, a nose, a brow — small and hard to fill is not a reason to drop one. Draw it as a thin stroked path or as its own small shape (see Phase 3). Dropping one requires the triptych to show the face still reads identically.

## Phase 3: Structural Build

- Author every filled shape as `draw_shape {kind:"path", d:"…"}` (SVG d strings are the most compact reliable authoring form). Fills are for bodies.
- **Outlines are separate stroked paths, never filled shapes and never seams.** Call `draw_shape` with a path and no fill color, giving the stroke through the data object's stroke width and stroke color fields, so the line is pure geometry at a measured width. Do not use `stroke_path` for outlines: it bakes brush physics (pressure taper, texture) into what must be a clean geometric line — it is for lines that are brush strokes in the reference, not for cartoon outlines.
- **Calibrate the stroke weight against the reference.** Count dark pixels (max channel below ~130) in the reference and in the render: if the render exceeds the reference by more than 10%, thin the strokes and re-render. The number is a warning light, not the verdict — the eye decides (see Stop Rule).
- **Stroke centerlines are constructed the same way.** Look at the reference, recognize the line, place anchors at its curvature extrema, and draw a long Bézier through them — never fit a centerline point by point from a skeleton, and never simplify-then-fit a pixel chain. A skeleton wobbles pixel to pixel; stamping a stroke along it stamps the wobble into the edge. Where several stroked lines meet and the reference is a solid dark junction, fill the junction as a solid dark shape under the strokes; stroked lines alone leave 1 px white gaps between them.
- **Seam killing:** draw back-to-front; each filled shape's contour is expanded ~1–1.5 px so later shapes overlap earlier ones and no background/dark line leaks between neighbors. The outer silhouette is *not* expanded — it must stay registered to the reference. Filled neighbors overlap *beneath* the outline stroke, which is drawn last, on top, at its measured width — never grow a fill into the outline to fake the line. Where two fills meet, one must tuck *under* the other by several px: abutting edges leave a white gap when the two contours miss by even 1 px. After rendering, count background-coloured pixels in every joint region — a joint that shows background is a fail, not a residual.
- **Contour construction is recognition-first, not fitting-first.** Look at the reference and recognize each long curve of the part; place anchors at its curvature extrema and draw a long Bézier through them — few control points, each spanning a real stretch of curve. Never fit a fill contour point by point from a mask boundary, and never simplify-then-fit a pixel chain (RDP or any other): a path built from pixels is a pixel path, however much it is smoothed. **Measure first, then place anchors:** before placing anchors for a part with tight proportions, measure the reference's key-point coordinates — each feature's bbox, position and size. Place anchors on the measured numbers, never on the eye's estimate: eyeballed anchors misjudge widths and heights. Check the drawn curve against the reference (overlay, zoom) and move the anchors until it registers; a smooth curve that misses the reference by a pixel beats a jagged one that hugs it. Segment count is a dirt proxy: a smooth curve is a few long segments; a path built from many tiny segments reads jagged, however closely it follows the reference.
- Boolean composition (`combine_shapes` union/subtract) is available for genuinely holed parts. Caution: its path-input coverage had a bbox (x,y,w,h)-vs-corners bug, fixed upstream in merge d31eeb6e (2026-10-21); on older builds feed combine inputs as polygons. `create_selection` accepts raw `d` only with that same fix merged.
- Text in the artwork: use real text (`draw_text`), never traced letterforms, unless the task is a logo (then letters are outlines — Phase "Logo variant" below).
- QA gate: render, build a triptych (reference | previous | current) including a zoomed crop of the densest region, and pass your own visual check before proceeding. Regions outside the edited bbox must be pixel-identical to the previous version.

## Phase 4: Shading Pass (additive only)

Two techniques, both required for volume:

1. **Nested tone steps** — per volumetric part, add 1–2 inset shapes whose contours are the part's own smoothed contour offset inward; colors = the part's measured light-end and dark-end samples. Center light, edges sink.
2. **Masked glazing** — soft shadows/highlights via path masks with multi-tier feathering (the contact-shadow method: 3 tiers minimum, more for wider falloff). Use for cushion undersides, junctions, contact shadows, and warm/cool shifts on bodies (e.g. warm-white belly).

Rules: colors sampled per region; a shading layer that pixel-sampling proves does not exist in the reference (no local darkening, or light from the opposite side) must be removed, not faked. If a layer introduces a hard edge or dirt, feather it more, reduce tiers' contrast, or drop it — clean beats strong. Re-run the triptych QA after each group of layers.
- **Form-defining shading is not decorative shading.** A soft shadow that gives a part its volume must be kept: add 1–2 mid-tone flat steps sampled from the gradient's measured light and dark ends, or a true gradient fill where the shape supports it. Dropping it is allowed only if the triptych shows the part reads the same volume without it. "Clean beats strong" governs decoration; it never licenses removing volume.

## Phase 5: Soft-Feature Rebuild (blush, glows, auras)

- Delete any traced version of the feature entirely. Rebuild generatively from measurements: footprint (center, radii, tilt) fitted to the reference's visible extent; color extrapolated from the sampled center color along its saturation falloff vector (pigment color is deeper than the rendered center, which is already diluted by the skin/base beneath).
- Construction: ≥4 concentric tiers of low-opacity ellipses/shapes + path-mask feathering, outermost tier fading to invisibility against the base color.
- Acceptance: crop the feature at high zoom against the reference — hue, position, size, and falloff softness must all match; render-verify the composited center color against the reference center.
- Draw facial features (eyes, beak) *above* the soft feature so it never tints them.

## Stop Rule (Iteration Discipline)

- "Diminishing returns" is not a stop rule — it is what the last versions always feel like. Iterate until every item below passes.
- **The first item is visual inspection, and it is the only pass.** Look at the 1:1 comparison and the 3× zooms yourself, item by item — every key part: does it look like the reference, is the expression right, are there too many lines, do the color blocks join up. Every numeric item below is supporting evidence only (辅助): numbers all green but the eye sees something wrong is a fail; numbers slightly off but the eye sees nothing wrong is judged together with the eye. **Numbers never overrule the eye.** Curve-level comparison against a known SVG (Hausdorff distance, control-point deviation, segment counts) is a diagnostic — it tells you where a shape deviates, by how much, and in which direction, so you know what to fix. It never grants a pass.
- Stop only when the visual inspection passes and every supporting item below is in line; "looks like the same artwork at a glance" is necessary and not sufficient:
  - Key parts, one by one at zoom: each key part reads identically to the reference — not "close", identical.
  - Eyes get their own 3× check — never passed on the thumbnail alone. At 3× zoom, each eye reads identically to the reference: eye-white length and sharpness of the tips, pupil outline shape, size relative to the eye white, and position, upper-lid line and lash-flick length and curve, lower-lid shadow. Measure a pupil or any small dark feature by its outline, never by its darkest core: a small dark core inside a larger feature is not the feature. A size error invisible at thumbnail is obvious at 3×. Check each eye separately — a pass on one eye says nothing about the other.
  - Edge smoothness (visual): at 3× zoom, long edges show no steps and no wobble. Where it can be measured, the rendered edge's second-order differences diagnose the wobble — they guide the fix, they do not pass the version.
  - Stroke weight (supporting): render dark-pixel count within ±10% of the reference (see Phase 3 calibration).
  - Per-part shape check (supporting): each key part's bbox and area in the render within ±10% of the reference's measured bbox/area — not just the whole-image pixel totals. Pixel totals alone cannot catch a distorted part: a part can have the right area but sit in the wrong place, or the right area but the wrong shape. Check bbox (x, y, width, height) *and* shape per part, at zoom.
  - Shading present wherever the reference has form-defining shading (see Phase 4).
  - No part lost more than 10% of its reference area (supporting, see Phase 1).
- If a version fails exactly one item, fix that item and re-render — do not declare the run done and file the failure as a "known residual".

## Logo Variant

- Single flat color: census the exact value; do not eyeball it.
- Icon + wordmark are both traced outlines (no live text). Counters and cutouts are even-odd holes; verify openness/connectedness per letter from the source mask before choosing path topology.
- Deliver with transparent background; render comparisons on white (or the source's bg).
- Segment budget still applies: clean geometric logos want long smooth curves.

## Deliverables Checklist (all of them, every version)

- [ ] Full-canvas PNG
- [ ] Triptych PNG (reference | previous | current), including the key zoomed region
- [ ] Overlay PNG (render at 50% over the reference) — contours must coincide, not just look similar side by side
- [ ] Numeric gate alongside the eye (supporting only): render dark-pixel count within ±10% of the reference, and every part's area within ±10% of its reference mask. Numbers support the eye; they do not replace it, and they never overrule it (see Stop Rule).
- [ ] SVG exported via `export_svg` — **copy it out of the run's exports dir into the delivery folder in the same step** (this was forgotten once; it is the deliverable users ask for by name)
- [ ] Project file (.yanshi)
- [ ] REPORT in the run dir: merge table, per-part segment counts, sampled colors, shading-layer inventory, metrics as supporting evidence, known residuals stated honestly

## Known Failure Modes

- **Dirty/scratchy result** → merge granularity too fine or fit tolerance too tight. Merge harder, refit looser.
- **Gray muddy soft features** → the feature was traced. Rebuild it (Phase 5).
- **Hard-edged shadow blocks** → flat shapes used as shadows. Use mask-tier feathering.
- **Dark seams between flat parts** → shapes abut instead of overlap. Apply the Phase 3 expansion rule.
- **Better metrics, worse picture** → trust the picture. State the tradeoff in the report instead of hiding it.
- **Heavy outlines** → outlines faked as seams between filled shapes or as a dark underlay silhouette. Draw them as stroked paths (Phase 3).
- **Lost small features** → a small dark feature dropped because filling it swallowed its neighbor. Draw it as a stroke or a small shape; never drop it silently.
- **Missing volume / flat parts** → form-defining shading dropped as "clean beats strong". Keep it as mid-tone steps (Phase 4).
- **Eaten edges** → a single-threshold barrier eats shadow tones and small parts. Use a color-aware barrier and the per-part area check (Phase 1).
- **Jagged edges / wobbly strokes** → the path was built from pixels: mask boundary as a staircase, skeleton centerline as a pixel chain. Rebuild by recognizing the curve and drawing it as a long Bézier with few control points — smoothing a pixel chain does not make it a vector.

---
name: yanshi-vector-tracing
description: 用 yanshi 的 MCP 工具把位图插画/标志复现成干净的矢量图 —— 语义合并、分层着色、软特征重建，最后 export_svg 出图。适用于把位图插画或 logo 转成矢量、或诊断「描出来又脏又碎」的问题。不适用于照片修图（那是 yanshi-photo-styling），也不适用于从零作画（那是 yanshi-painting）。
---

# Yanshi Vector Tracing — Raster Illustration / Logo Reproduction

**Version: v5.10** — cold-start round 11 backfill (Appendix R11). Promotes 5 rules to the body (P-A call formats: draw_shape/draw_text argument shapes + pre-timing draw_shape probe; P-B shared-edge overlap neutral point by two-point interpolation; P-C CR anchors equi-arc-length resampled ≤ ~5 px, joints not by corner-peak method; P-D text-class images take the outline-tracing branch; P-E eps by minimum curvature radius, not part size). Corrects the v5.9 Phase 3 draw_shape authoring form, which the 76154e1 engine rejects. P-A, P-B, W8-2 (v5.7–v5.8) unchanged; all other v5.9 rules unchanged.

Reproduce a raster illustration or logo as clean, vector-style artwork in yanshi (path shapes + masks), and export a faithful SVG. Distilled from production runs; measured evidence for each run lives in that run's own notes, not in this document.

## Core Principles

- **Visual judgment is the acceptance test — and it is the only pass.** Metrics (mean diff, IoU, dirty-point counts, curve-level distances) are supporting evidence only, and they can never overrule the eye: numbers all green but the picture wrong is a fail; numbers slightly off but the picture right is judged together with the eye. When a version scores better but looks dirtier or more fragmented, ship the cleaner one. The reviewing agent must personally zoom-render and compare against the reference before every delivery — never delegate the verdict to numbers.
- **The agent doing the work checks its own render — every round, before reporting.** At the end of each round, before reporting, the executing agent looks at the engine render at 1:1 and at 3× with its own eyes, region by region, against the reference. Numbers only diagnose; they never gate acceptance, and the visual verdict is never pushed to the coordinator — the coordinator's check is a second look, not the first.
- **Semantic merging is a visual decision, not an algorithm.** Programmatic segmentation produces raw fragments only. A reviewer who has *looked* at the reference groups fragments into semantic parts (a sofa = backrest top, two back cushions, two pillows, one seat, front rail, plinth, armrests…). More parts is not more accurate: over-segmented runs look dirty; merge until every part is a thing a human would name. When in doubt, merge. The unit of a merge is a physical part: one physical part is one semantic object, drawn with its true physical outline — never split one physical part by color or shading, and never merge two different physical parts into one. A part's shading gradient is a method gap, recorded honestly; it is never faked by cutting the part into color bands.
- **Dirt comes from granularity and boundary noise, not from the tools.** Scratchy internal lines = fragments kept as shapes, or outlines fitted too tightly. Fix by merging harder and fitting looser — not by adding detail.
- **Measure with pixels, draw with vectors.** Anchor positions come from measurement, not from the eye: take the curvature extrema of the part's measured mask or contour, and place the anchors there — then draw a long Bézier through the measured points. Two ways to fail, and both are banned: fitting a path point by point from a pixel chain (mask boundary or skeleton, RDP or any other) gives hundreds of tiny segments — a pixel path, however much it is smoothed; placing anchors by eye without measuring gives smooth curves in the wrong places — the eye misjudges proportions, small features, and letterforms. Pixels measure — position, extrema, width, color, area — and vectors draw: few control points, each spanning a real stretch of curve, through measured points (see Phase 3).
- **Structure first, shading second, as separate passes.** Never bake shading into the structural pass. Shading is purely additive layers on top of a frozen structure.
- **Every color is sampled from the reference.** Base color = part's pixel mean; shading colors = the part's measured light/dark ends. Never invent a color, never reuse a neighboring part's tone because it "looks close". For a part whose color varies smoothly, a single flat fill takes the median of the visible region that best distinguishes the part from its background — not the part's overall median: the overall median can coincide with the background's tone and make the part invisible against it.
- **Soft features are rebuilt, never traced.** Tracing a soft feature (blush, glow, soft shadow) yields gray mud or hard stickers. Rebuild it generatively (see Phase 5).
- **Outlines are strokes, not seams.** A black outline is a line, not the gap between two filled shapes. Never fake it with a dark underlay silhouette that colored shapes grow into: the seam width is set by segmentation error, not by the reference, and it comes out heavy. Draw the outline as its own stroked path (see Phase 3).
- **Shadow boundaries are colour changes, not lines.** A shadow's edge is a change of fill colour, never a stroked path drawn along it — drawing one invents a line the reference does not have. Before adding any stroke, sample the reference along the stroke's path: if the reference shows no dark line there, the stroke does not exist. Only silhouette edges and genuine ink lines get strokes (see Phase 3).
- **Shading that defines the form stays.** "Clean beats strong" applies to decorative shading only. When a soft shadow gives a part its volume, dropping it changes what the part is — keep it as mid-tone flat steps or a true gradient, sampled from the reference (see Phase 4).
- **Negative space is real holes.** Use even-odd paths; never paint white cover shapes. But verify the "hole" is actually closed in the source — letter counters may be open self-winding contours, and connected components that look joined may be separate. Reproduce what is measured, not what is assumed.

## Phase 0: Setup

- Canvas = the reference's own pixel dimensions (crop the reference to the artwork first if it sits inside a larger screenshot).
- Sample the palette up front: for logos, the exact dominant color by pixel census. For illustrations, per-part means come later, per part.
- **"Ground truth" means an original artwork file, and the reference must be rendered from it — verify the pairing before using it as a diagnostic control.** A ground-truth SVG is the artwork's own original vector file, and its PNG is rendered from that same file. Before using a claimed ground-truth SVG as a curve-diagnostic control, render it at the reference PNG's size and pixel-compare: if the render does not match the PNG, the file is not that PNG's ground truth — judge against the PNG only. A target with no vector original (a photograph, a raster-only illustration) has no ground truth: judge it against the PNG only, and no curve diagnostics against a reference SVG exist for it.
- **Ground-truth pairing lays the source background first.** Compositing a transparent background to RGB turns it black and invents false differing pixels (one such trap once produced hundreds of thousands of false positives) — lay the source background before any pixel comparison.
- Keep a run directory with: reference, segmentation data, the merge table (JSON), build script, exports, and a per-version REPORT. Version the artwork (v-next) instead of overwriting — comparisons need the previous version. Build chains fail loudly: every step checks its exit code — on a shell chain use `set -o pipefail`, because a pipe to `tail` masks a failed step and the chain runs on as if it had succeeded.
- **A document id is append-only across sessions.** `new_document` with an existing doc_id reopens the old content — it does not clear it. Before re-driving a document, delete `mcp-root/docs/<doc_id>` or use a fresh doc_id; otherwise the new drive stacks on top of the old one (a re-drive that should have been 85 ops came out as 170 ops with 6 images). This is the append-only atom-log design, not a bug — the procedure is: delete or rename, then drive.
- **Drive in byte-budgeted batches — the server's stdio line limit is 64 KiB, by design.** One line over the limit kills the server process outright (no clean error receipt; every later call fails; recovery is a restart + a full re-drive). So: chunk `batch` calls by serialized bytes, each packet ≤60 KiB — the budget is bytes, not call count (a 393-op drive packs to ~170 KB, 2.6× the limit and dies if sent at once; a small drive packs to ~32 KB and goes in one packet). Batching saves almost no time (~0.06 s per 100 ops) — chunk for survival, not speed. Use the environment package's guarded client (`mcp.py`): it measures each line before sending, auto-splits an over-limit batch, and refuses a single call that is itself over the limit (a lone oversized path must be split into smaller shapes by hand — the client cannot split one call). Never hand-roll a send that bypasses the guard.
- **Install the standard font package before promising a typeface.** Font inventories differ per machine (one machine's `list_fonts` showed 35 fonts including the display families, another may not have them) — never assume a font exists because it existed on another box. Run the environment package's `install-fonts.sh` first (it installs the standard list — 6 families — into `~/.fonts` + `fc-cache` and verifies each one), then check `list_fonts` to confirm the family you need is there before building the text (see Text Assembly for the Bold procedure).
- **The environment package's mincheck does not cover `draw_shape` — run one real `draw_shape` probe before timing starts.** Before `init`, drive one minimal-document `draw_shape` call in the verified form (see Phase 3) and confirm the part lands in the export (count `d` strings, read the render pixels). Five round-11 agents (github, butterfly, owl, seal, logo) each had to add this probe themselves after mincheck passed without it. A green mincheck is not proof that `draw_shape` drives.
- **Receipts: read `ok`/status, not the presence of sha256.** A correct `export_png` or `export_svg` call's receipt always carries sha256 (50 correct PNG calls, 0 missing — the earlier "occasionally missing" reading is closed: it was a misread; the SVG export carries the field too). A failed call's receipt is an error JSON with no sha256. So a missing sha256 does not mean a failed render — check `ok` first; only a failed `ok` (or a missing output file) is a failure.
- **An export receipt's sha256 covers the pre-assembly export.** If the delivery is assembled afterwards (text assembly, gradient matching), the receipt hash is not the delivered file's hash — verify the delivered file by direct hash, never trust the receipt hash alone (measured: receipt `fc15357e…` vs delivered `5ddec839…` on a two-colour document; the file itself rendered correctly).

## Phase 1: Segmentation (program only)

- Fine over-segmentation is fine here — it is raw material, not the artwork. Produce a numbered fragment map image so the merge reviewer can see every fragment's outline and location.
- Segmentation is for measuring — and measurement is what places the anchors. Pixel masks tell you a part's color, area, position, and its contour's curvature extrema; the extrema are the anchor positions. Masks never generate a path point by point — the path is a few long Béziers through the measured extrema (see Phase 3).
- **Never classify edges with a single brightness or single-channel threshold.** Anti-aliased edges live between colors: a single-threshold barrier eats shadow tones that sit just under the cut. Use a color-aware barrier — exclude pixels that belong to a color class first (e.g. orange-class by a large R−G gap) and only then threshold. After segmentation, check every part's pixel area against the reference mask for that part: a part that lost more than 10% of its area is re-segmented, never patched by growing the shape — growing bloats the outline it touches. The area guard compares against the reference mask in pixels, never against a fitted path's area — a fitted curve can overshoot its mask, so a path-area baseline both hides real loss and raises false alarms.
- **Blend colours between near-identical flat colours form filaments that a per-class census reads as a third class.** Filter tiny connected components by area before trusting a per-class census — a census taken before filtering invents a class that does not exist.
- **Fine-point density is an acceptance item of its own — decide it before any batch drop.** Before batch-dropping sub-10 px components, measure their density first (per-class census at 50% coverage, counting only solid-core spots with the anti-aliasing spectrum trimmed, per 1000 px²) and decide explicitly whether that density is itself an acceptance item. When it is, a batch area cutoff must not silently erase the whole population — a real population measured 0.302/1000 px², and a batch drop erased 145/145, an erasure visible at 1:1 and 3×. Measure first, then decide what may be dropped; a filter applied before the density decision destroys the evidence it was meant to judge.
- **A class's bbox and extrema are polluted by the 1 px anti-aliased edge spectrum.** Row-scan min/max can be skewed by tens of px by single-pixel spectra — trim the edge spectrum (ignore 1 px-wide spectral outliers) before reading a class's bbox or extrema.
- **A polygon fill overcounts area on thin strokes — never use it for area measurement.** Measured overcount +27% on a thin-stroke case (the earlier 7–20% range understated it). Use the reference renderer or point-wise distance instead.
- **A part with no pixels in the reference cannot be measured — and cannot be drawn.** If a candidate part has zero pixels in the reference render, exclude it from the trace set and record the exclusion with the reason. Drawing it would mean copying coordinates that were never measured, which the measure-first rule forbids.
- **Fine features are measured at 50% coverage, not at a tight color cut.** All thresholds are taken at the 50% coverage point — for every area and contour measurement, not only fine features. Locate every edge by dense normal profiles (~0.2 px sampling) read to the 50% crossing, not by a threshold-mask pixel count.** Set the threshold at the midpoint between the feature's fill color and its background: a tight threshold eats fine lines and shrinks bright areas by ~1 px. A tiny hole counts as a real spot only if its interior holds solid pixels of the opposite color — test solidity, not area: an area cutoff keeps anti-aliased specks and drops real pinpricks. And contour extraction splits a peninsula into separate rings at necks of ~2 px or less — do not trust ring counts there; verify narrow necks by eye at zoom. Small-shape tips are systematically cut short by thresholds (~2–4 px) — record that as a measurement floor, not a defect to fix by generic extrapolation, which is unreliable on small complex shapes. The same floor applies to hairlines below ~1 px wide: their ink level can be reproduced, but their length cannot be fully recovered without over-inking — a lower threshold buys length at double width and darker ink, so record the shortfall as a residual and do not chase it.
- On steep edges (|dx/dy| > ~2), a thresholded scanline reading is a row average, biased 2–3 px from the true path — judge steep edges by the path geometry at the row center, or by path-level Hausdorff, not by scanline edges.
- **Two measurement traps on anti-aliased edges.** (a) Nearest-class labelling mislabels dark-stroke anti-aliased pixels as the bright class (measured: 54% of a 2,627 px AA band mislabelled) — gate labels by reconstruction residual and largest connected component, not by nearest class alone. (b) A butt-cap endpoint's column-wise centreline is biased ~+4.8 px (larger than the earlier ~2 px, and it grows with line width and cap angle) — fit the tangent from interior points and extrapolate; the tangent-extrapolated error was only −0.7 px.
- A "black" background in a PNG may be alpha = 0, not black pixels: the artwork is the alpha mask. Threshold the alpha channel, not brightness.
- **Measure white gaps against the engine render, and bound every measurement mask to the region it belongs to.** A white-gap mask's connected components must be restricted to their own region — a component that spans two regions mixes two different gaps into one number. A local proxy render overestimates white gaps by ~7–10% against the engine — use the proxy only for fast screening; every number that decides anything comes from the engine render.
- **A pattern that drifts as it runs must be measured the way it runs.** If a set of stripes shifts position row by row, measure each stripe's centerline per row (a per-row scan) — a single row's measurement drawn as straight bars is wrong. The same holds for any repeating mark whose position is a function of the row (or column) it sits on: measure the function, don't copy one sample.

## Phase 2: Semantic Merge Table (visual decision)

- The reviewer studies the fragment map against the reference and writes the merge table: each semantic part = {name, fragment ids, draw order}. Parts are things a human would name, at the coarsest granularity that still reads correctly.
- Select and merge by what a fragment *is*, after looking — never by color value or pixel adjacency alone. A selection, mask, or path is drawn around a semantic identity (one physical part), and the measurement mask for a part is cut by that same identity: two different objects that happen to share a color are never merged by a color threshold.
- Sanity budget: a single prop (sofa, chair, vehicle) should land around 10–16 parts. If the table has 25+, merge again.
- Small scratch/detail fragments inside a part are absorbed into it — they must not survive as independent shapes.
- **Small dark features are parts, not dirt.** An eye's dark patch, a nose, a brow — small and hard to fill is not a reason to drop one. Draw it as a thin stroked path or as its own small shape (see Phase 3). Measure a small dark feature's top-row pixel run width before deciding its form — a flat top a dozen px wide is a different shape from a pointed one. Dropping one requires the triptych to show the face still reads identically.
- **A physical part split by occlusion is still one part.** When an occluder cuts one physical part's visible region into disconnected fragments, never emit one fill per fragment — measure the part's true outer outline, bridging across the occluded zone along the measured course (see Phase 3 "Hidden geometry"), draw ONE fill, and let the occluder cover the bridge afterward. One fill per visible fragment reads as separate flat plates, not as one part.
- **Two-tone line art has its own main structure.** Where the artwork is a single connected ink network over a background, the legal main structure is three layers — the background color shaped to the silhouette, foreground islands of the background color, and closed ink pieces that are disconnected from the main network — not a failed semantic merge. Do not force it into a strokes-and-parts decomposition.

## Phase 3: Structural Build

- Author every filled shape as `draw_shape` with `data={geometry:{kind:"path", d:"…"}, color, ...}` — `kind` lives inside the `geometry` object, and the d string lives inside it too. A top-level `kind` is rejected, a top-level `d` is rejected, and a bare d string as the whole `geometry` value is rejected (`geometry 必须是对象`). The structured form `data={geometry:{kind:"path", subpaths:[...]}, color}` is also accepted. Fills are for bodies. (Round-11 evidence: gt1, butterfly, penguin — d-string-in-geometry, all calls ok with parts landing in the export; seal — subpaths form. This corrects the v5.9 line, which wrote the top-level form `{kind:"path", d:"…"}` as the most reliable authoring form; that form is rejected on the 76154e1 build.)
- **The path data lives inside `data.geometry` — a top-level `d` is rejected.** `draw_shape` with `d` at the top level returns an explicit invalid_argument naming the missing parameter; read the error first, it states what is missing.
- **`draw_shape` calls need `layer_id` (e.g. layer_default).** A missing `layer_id` is rejected with invalid_argument naming it — a minimal-document probe does not warn about it; the first drive does. Read the error, add the parameter, re-drive.
- **Colour space of a numeric colour depends on its input form (`draw_shape`, verified).** A numeric array with any component > 1 is sRGB bytes (0–255, round-trip identical — send the sampled bytes unchanged). An array with all components ≤ 1 is linear-light passthrough: sending sRGB/255 brightens it, dark colours by ~2–2.7×. A hex string and a `{r, g, b, a}` object are always sRGB bytes. Scope: verified for `draw_shape` only. For `draw_text` only the hex form is verified sRGB — the numeric-array form is NOT separately verified, do not assume it. Untested: alpha < 1, the explicit `color_space` parameter, brush-stroke colours.
- **The engine's path parser rejects the arc command — pre-convert every arc to cubics.** `A` is refused (silently, or with an error — either way the shape is missing from the export). Write every arc and rounded corner as cubic Béziers (κ=0.5523), at most 45° per segment, with the sweep start angle normalized to (−180°, 0] so the arc does not flip when its endpoints cross 180°. After driving, verify every part actually landed in the exported SVG by counting `d` strings — never trust the call-ok count alone.
- **A flat-topped arc splits at the vertex, with the vertex tangent constrained horizontal, then fits halves — but only when a single cubic cannot hold it.** Split only when the single cubic's max error exceeds ~0.5 px; a single cubic that already meets the bar needs no split (an unneeded split is a new joint to maintain).
- **Segmented arc fitting is enabled only when it improves the fit by more than ~1 px.** Fitting an arc in segments instead of one circle always improves the residual a little — that alone is not evidence the contour needs segments. Enable segmented fitting only when the improvement exceeds ~1 px (a non-circular contour improved by 9.21/5.59 px; a true circle improved by only ~0.01–0.02 px, which is overfitting anti-aliasing noise). Below the threshold, keep the single-arc fit.
- **Outlines are separate stroked paths, never filled shapes and never seams.** A `draw_shape` call's data is `{geometry, color, stroke_width, stroke_color}` — there is no `fill` key: the fill colour IS `color`, and an unknown key such as `data.fill` is silently accepted and silently ignored (the call still returns ok). A stroke-only path omits `color` (or sets it null) — it then exports with `fill="none"`. But a stroke-only rect/ellipse/polygon cannot be written by omission: for those kinds the fill flag is unconditionally on and the colour defaults to black, so omitting `color` fills the shape black with no warning in the receipt — draw such a shape as a `path` kind instead, or set `fill="none"` on the assembled/exported SVG by hand and verify the export. Probe the behaviour, never trust the receipt: drive one stroke-only shape of each kind in a minimal document, export, and read the exported `fill` attribute plus the render pixels. The stroke itself is given through the data object's stroke width and stroke color fields, so the line is pure geometry at a measured width. Do not use `stroke_path` for outlines: it bakes brush physics (pressure taper, texture) into what must be a clean geometric line — it is for lines that are brush strokes in the reference, not for cartoon outlines. Exception: when a shape's fill and stroke are one path in the reference, draw them in one call — `color` and stroke fields together. Splitting them doubles the object count for no visual gain; the separate-outline rule is for an outline that is its own line, not for the stroke of the filled shape itself.
- An outline sits outside the fill it bounds: its centerline is the fill's edge plus half the stroke width — never centered on the edge, never inside the fill. Sample an arc by arc length, not by point count, so the curve keeps its shape along its whole run.
- A black line between two colors is its own stroked object: a fill patch cannot substitute for it, and a fill patch of one side's color drawn over the line erases the line. A solid dark junction where several lines meet is a fill, not a stroked patch — a stroke's endpoints pushed into a junction only move the gap around. Before deleting a line that looks wrong, check whether it is also the only cover for something else (a line whose end covers a junction top is load-bearing): delete it only after its cover is replaced.
- **Calibrate the stroke weight against the reference.** Count dark pixels (max channel below ~130) in the reference and in the render: if the render exceeds the reference by more than 10%, thin the strokes and re-render. The number is a warning light, not the verdict — the eye decides (see Stop Rule). The dark-pixel predicate only applies inside its color range — on a mid-tone color it matches nothing, silently; state the applicable range, or use a hue-relative ink definition, and treat a zero-match predicate as a broken measurement, not a zero.
- **The engine probe is the only width validator — a local rasterizer systematically overestimates the engine.** A local rasterizer's line rendering and a distance-field estimate both read wider than the engine (round caps by ~5–7 px): never set a width from them. Set every width with a minimal-document engine probe — the stroke alone in its own document, its edge measured row by row from the engine render — and sweep widths, taking the one whose free edge lands in [0, +1] px of the reference edge. If a distance field is used at all, the allowed width is twice the distance to the nearest non-dark pixel; reversed, it collapses to the lower bound. The engine quantizes stroke width to even numbers and renders a hard edge with one anti-aliasing level — expect that, don't fight it. A width that falls between two even steps has no stroke representation — draw it as a fill band of centerline ± half-width instead of choosing between the two neighboring even widths. The same stroke renders ±1–2 px differently in different documents, so a probe within −1 px of the target is a pass — do not chase the last pixel.
- **The engine stroke has no linecap parameter.** The exported SVG carries no `stroke-linecap`, so the same file renders round ends in the engine PNG but flat ends in a browser/rasterizer (measured on one stroke: engine ink bbox x 91..308 vs cairosvg x 100..299). When the reference has flat ends, draw the stroke as a fill band, not as a stroke. The same geometry renders systematically differently across rasterizers (sub-pixel coverage, edge placement): judge against ground truth rendered in the SAME rasterizer, and run a dual-rasterizer comparison first whenever the attribution (drawing vs renderer) is in doubt.
- **Dark-pixel counts on the engine PNG are not a geometry verdict.** The engine's anti-aliasing is non-linear with feature size: the ground-truth path itself loses ~5% of its dark pixels in the engine on large smooth fills, while a 6 px thin band reads +5.1% dark in the engine vs +0.4% in a reference rasterizer (cairosvg); the same geometry reads 799 missing px in cairosvg vs 2,489 in the engine under a <128 threshold. Judge the geometry on a reference-rasterizer render of the delivered SVG; use the engine PNG only to check for visible defects. An engine dark-pixel deficit (or excess) on its own never fails a part and never justifies re-drawing — confirm the geometry in the reference rasterizer first. (R8: tiger + gt1 + seal + wordmark.)
- **A tapering stroke end is shortened, not capped.** Measure the end's width profile over its first few columns first: if the end tapers (first-column width ≤ half the cap width), shorten the centerline by ~1/4 of the line width at each end as a starting point and bisect once — a round cap on a tapering end reads as a stub.
- **Stroke width is measured by closed-loop render measurement, never by run widths.** A per-point vertical run width taken from the reference is unreliable — anti-aliased edges undercount by 0.5–1 px, junctions cross-talk, outliers run several-fold — so render the band and compare it against the reference instead.
- **Decompose a stroke deficit before widening anything.** If the already-covered bands render at or above the reference, the deficit is in uncovered ink — patch it with fills built from the measured uncovered clusters; widening the strokes would fatten lines the reference does not have. Small clusters below the measurement floor (a few px) are left as a residual, not chased.
- **Stroke width follows the dark band's local width, not its offset and not a band-width average.** Measure the dark band's width at each point along the stroke and set the width to that local width; then validate point by point that the stroke stays inside the reference's dark mask (plus an anti-aliasing tolerance), and narrow or move back whatever spills out. A stroke widened to a mean width overshoots wherever the local band is narrower than the mean — that overshoot is a new black line the reference does not have. Offset alone is a weak signal: a centerline a few px from the nearest dark pixel can still be 10–15 px off the band's center — check the centerline against the band's center, not against the nearest dark pixel.
- **Stroke centerlines are constructed the same way.** Take the line's measured course from the pixels first — sample where the centerline runs and where it bends — then draw it as a long Bézier through the measured points. Never fit a centerline point by point from a skeleton, and never simplify-then-fit a pixel chain: a skeleton wobbles pixel to pixel, and stamping a stroke along it stamps the wobble into the edge. Never place the anchors by eye alone either — an unmeasured centerline drifts off the line it is meant to follow. Order a centerline by walking the local direction, not by shortest path: a shortest path cuts the corner inside a tight U-bend. Walk along the measured direction, split at turning-angle anchors (> ~35°) before fitting, and set the leak-repair threshold above ~4 px — a lower threshold fires on edge pixels and fits wild loops. Where several stroked lines meet and the reference is a solid dark junction, fill the junction as a solid dark shape under the strokes; stroked lines alone leave 1 px white gaps between them.
- **Refinement splicing uses the per-edge shortest span, not the shortest whole segment.** A shortest-whole-segment span covering most of a ring flips direction — splice per edge.
- **Bind a defect to its stroke with a separate probe per stroke.** The union's edge only tells you which row bulges; a probe of each stroke on its own tells you which stroke binds that row — probe strokes one at a time when a defect must be attributed.
- **Check downstream before burying a cap.** When a centerline is extended to bury a round cap, check within one radius downstream of the endpoint for a white gap — otherwise the cap bridges across the gap and hides it. A probe must check for overflow clusters around the window, not just the edge it was aimed at.
- **Split without dropping samples, and change width without refitting.** When a stroke is split by allowed width, a tail segment with fewer than 4 samples must not be dropped — it is often the key narrow segment; split a short segment from the original cubic by de Casteljau (centerline deviation ≤0.1 px), never by refitting. When the allowed widths across a stroke vary little (spread ≤10 px), keep the original path and change the width only — zero fitting error.
- **A seam probe measures the neighboring columns too.** When measuring a 1 px gray seam, measure the target column and its ±1 neighbors — a fix can move the seam one column over, and a probe that reads only the target column misses it. A seam probe always outputs three columns × the row range.
- **A patch that meets a stroke must overlap it, never abut it.** A patch edge joined to a stroke edge with zero overlap leaves the boundary column partly covered — a 1 px gray seam. Expand the patch ~1 px toward the stroke side (or dilate the missing cluster by 1 px when fitting); any such change needs a full overflow re-check.
- **Rebuild a ring stroke from the boundary of the fills it encloses.** Take the guide from the outer ring of the enclosed fills' union — a guide fitted from thin air drifts off the dark band by ~10 px and the search then bites the wrong band. A solid dark region is a socket fill, not a ring: when the dark area is a solid patch (an eye socket, a solid junction), fit a dark fill to the region's mask and draw the bright features on top of it — a ring around solid black carves a false wedge out of it. A stroke that a fill-boundary guide drops (sharp turns, short runs) is recovered by scanning the dark mask directly for the band center.
- **A line-like dark feature too fat for a stroke is fitted as a fill.** Fit the fill from the feature's connected dark region at the visual bandwidth (including the anti-aliased edge), not from the dark core alone — the core is narrower than the visible band. Before deleting a fat stroke, measure the missing black it was covering (reference dark minus render dark) and prepare patch fills first: deleting it exposes the gap it hid. When a bright small feature is covered by legitimate dark strokes, do not move the dark strokes — draw the bright feature as a top fill, after the strokes.
- **Seam killing:** draw back-to-front; each filled shape's contour is expanded ~1–1.5 px so later shapes overlap earlier ones and no background/dark line leaks between neighbors. The outer silhouette is *not* expanded — it must stay registered to the reference. Filled neighbors overlap *beneath* the outline stroke, which is drawn last, on top, at its measured width — never grow a fill into the outline to fake the line. Where two fills meet, one must tuck *under* the other by several px: abutting edges leave a white gap when the two contours miss by even 1 px. After rendering, count background-coloured pixels in every joint region — a joint that shows background is a fail, not a residual.
- **A polygon-vertex miter outset divides by sin(β/2), not cos.** 1/cos over-outsets by several px on smooth vertices — use sin(β/2) for the outset distance.
- **An offset normal must be tested per edge, inside/outside — never assumed from the centroid direction.** On a concave edge or a U-shaped frame the centroid sits in the hollow and the 'away from centroid' normal points into the material (measured: a U-frame's inner edge shortened, area 26,400 vs 32,000 with a per-edge tested normal). Test each edge's normal against the mask before offsetting.
- **Concave-vertex compensation is triggered by the measurement method, not by the angle.** A compensation that pushes a concave vertex outward assumes the measured edge sits inside the true edge. When the edges are already measured to sub-pixel accuracy against ground truth, there is no shrinkage to correct and the same compensation overshoots on every notch (5/5, sharpest 1.92 px) — do not apply it. When the edges come from a threshold mask, which shrinks them by ~0.5 px, the same compensation is a correction, not an overshoot. Decide from how the edges were measured, never from the vertex angle alone; verify any compensation against the render first.
- **Where two parts share one edge, only one side draws it.** Adjacent parts that share an edge must not each draw their own approximation of it — two approximations never coincide, and the mismatch reads as a white seam at 3×. Draw the shared edge once (or let the upper part cover the lower), overlapping 0.5–1 px to cover the seam. Where two fills share an edge, the lower fill's edge presses 1–2 px inward under the upper fill — two abutting fills leave a hairline gap. **The overlap's neutral point is not 0.5 — find it by two-point interpolation.** The outset that zeroes the net area error differs per colour pair (measured: black 0.4, grey 0.5 on one shared edge); do not assume 0.5. Probe two outset values, interpolate to the zero-net-area point, and use that. (Round-11 evidence: fox, tiger.)
- **Draw order is part of the decomposition.** A later path of a different color cuts the earlier one — a bright part drawn after a dark part punches a hole in it. Plan the order so the cuts land where the reference has them; a hole in the wrong place is a draw-order error, not a shape error. Draw order must follow occlusion: overlapping regions change colour with draw order (measured: a red/blue overlap flips colour when the order flips) — the occluded part is drawn first, the occluder after it. Inside one object, order follows occlusion: the occluded part is drawn first, the occluder after it, and a highlight or decorative stroke after the body it belongs to — a part's visible region must never be covered by a later part of the same object without reason; check at 1:1. Order by geometric containment, not by a contour-tree depth: a two-level hierarchy (RETR_CCOMP) does not express cross-part containment — a black island inside a hole is ordered by whether one contour geometrically contains the other, measured, not by its tree depth (tiger: 26 small black parts inside the giant gray hole were drawn in the wrong order, −4,996 px; logo: a white hole covered a black island inside it). Recompute levels by geometric containment (or use RETR_TREE and verify); never trust a two-level depth. (R8: tiger + logo.)
- **Contour construction is measure-first, draw-vector.** Measure the part's contour from the pixels — key-point coordinates, bbox, and the contour's curvature extrema — and place anchors on the measured extrema. Draw a long Bézier through them — few control points, each spanning a real stretch of curve. **The boundary, stated once:** what is banned is fitting a path point by point from a pixel chain (mask boundary or skeleton, RDP or any other): a path built from hundreds of tiny segments is a pixel path, however much it is smoothed. What is NOT banned — and is required — is taking anchor positions from measurement: a curvature extremum measured from the mask *is* the anchor's position. Placing anchors by eye without measuring is banned the same way: the eye misjudges widths, heights, and small features. Check the drawn curve against the reference (overlay, zoom) and move the anchors until it registers; a smooth curve that misses the reference by a pixel beats a jagged one that hugs it. Segment count is a dirt proxy: a smooth curve is a few long segments; a path built from many tiny segments reads jagged, however closely it follows the reference. A smooth arc needs few anchors: a long Bézier through measured interior points (e.g. at one-third and two-thirds along the arc) is enough — never add anchors to pad the count. On a smooth arc, prefer one cubic to a split — a split leaves a kink at the joint; split only where the curve's character changes. And a long unsampled gap must be constrained by intermediate points — a single free curve across it goes wild even when its fit residual is tiny. Anchors measured from pixels sit on pixel centers; a uniform sub-pixel offset (up to ~0.5 px) across all parts is the measurement grid, not a registration error — do not chase it. After drawing a curve, check it against the measured points and add anchors where it deviates — smoothing erases peninsulas narrower than ~3 px. When an edge has internal structure (a bulge or a hollow), two-point fitting misses it — sample every ~5 px and fit by least squares. For a corner, fit a circle first and check the residual — if the residual exceeds ~1 px, the corner is not an arc: use the measured corner chain instead.
- **Small curved features are hand-fitted, never fitter output.** For a feature under ~100 px, never ship a dense fitter's output as the final path — hand-fit long Béziers through the measured curvature extrema (peak, endpoints, inflection points) and check the outline against the measured mask at 3×. A fitter's dense output on a small feature reads as angular corners at zoom, and a fitter may silently stop short of the feature's true edge. When an automated fit polygonizes a small feature, the failure is sampling density, not the angle threshold — densify the samples (or lower eps) instead of tuning the corner angle; angle tuning cannot fix sparse points.
- **Fitting helpers have silent failure modes — validate the output, not the residual.** A closed-loop fitter can collapse a loop to a single point while reporting near-zero offset (a point's distance to the nearest center is ~0 by construction): always validate the fitted path's bounding box and length against the source loop, and roll back on a collapse. An open-run fitter's step can drop the extension endpoint — use step 1 for short guides. A dense fill fitter fits only the outer ring / largest connected component — fit a multi-component mask component by component and check coverage per component. (In this run's toolkit these are the dense loop fitter fit_loop_dense, the open-run fitter fit_open_run, and the dense fill fitter fit_fill_dense.)
- **Segment starts are the running current point.** A curve segment starts where the path currently is, not where the fitting function assumed it starts — after fitting, check every segment's start against the running current point; a mismatch silently accumulates error toward the tip. Between two fitted segments on either side of a flat edge, draw the flat edge explicitly so every segment starts where its control points assumed. Do not default to a small round cap on a tip: measure the flat-top width first — wider than ~8 px, draw the flat top explicitly with rounded transitions on both sides (same for flat bottoms).
- **Overlapping parts of the same color** are split along the visible arcs of their union; where one part hides under another, bridge the hidden crossing with a straight segment between the measured shared vertices — never guess the hidden edge. Verify each part on its own first (render it alone, or compare it alone against its reference outline) before judging the union: an error hidden inside the union is invisible in the combined render. And check every bridge endpoint twice: a bridge that starts at the wrong vertex silently distorts the next curve, because the curve's control points were computed from the intended start.
- **Hidden geometry is extrapolated along the measured course, never guessed.** Where a part tucks under another, its hidden edge continues the fitted curve into the hidden zone: on steep edges extrapolate with the average direction of the two tangents that meet at the hidden vertex — a single tangent lands in the wrong place, a cubic extrapolation overshoots; across a hidden hollow bridge with a tangent at one-third of the chord; at a hidden tip, split the curve so the tip is a measured point rather than an extrapolated one. Judge the hidden part by whether the visible edge lands right: a deviation that exists only behind another part is not a visible error — do not chase hidden-zone numbers as if they were. Fit a symmetric hidden peak under a symmetry constraint — a free fit sags off the true peak. An edge with no raster signal (dark-on-dark, or hidden under a same-color part) is constructed from its measured endpoints and the course of the neighboring curves — never left to a single free curve across a long stretch.
- **A slit in a closed ring is traversed twice.** To cut a notch into a closed outline, walk the slit out and back along the same line — a single traversal closes the ring with a straight line and the winding cancels the region the slit was meant to open.
- **Zero-width spikes leave no fill signal — find them by residual.** Dilate the traced dark mask and subtract it from the reference dark mask: the remainder is outline excursions the trace missed. Splice each real spike in as an out-and-back excursion. But a 2–3 px thin strip (registration residue) looks like a short spike — splice only strips of line width ≥ ~3.4 px, or verify by hand.
- **Simplify a closed ring from antipodal points.** Split the ring at two opposite points first — with start = end the initial chord degenerates and every corner is lost.
- **Chain boundaries come from the contour's own corners — on smooth shapes the cut is lower.** A turning-angle cut tuned for spiky shapes keeps no corners at all on smooth ones (a circle's corners all turn shallow), leaving one giant chain per loop that no fitter can follow — and a single-corner loop degenerates to a zero-length segment whose fit residual reads as 0.0, a silent wrong shape. Take the boundaries from the simplification corners themselves (a lower cut, ~15°, or a minimum-boundaries rule) — never fit a whole closed loop as one chain — and a fit that emits a straight segment must report its true deviation, never 0.0 by construction. On a smooth closed curve there may be no curvature-extremum joint at all — do not force one by the corner-peak method (it misfires there); an equal-arc-length CR pass through the resampled contour holds the curve in one go. (Round-11 evidence: gt1.)
- **Cut chains on a dual baseline so corners are not absorbed.** An 8 px baseline can read a real corner as ≤12° when it spans the previous corner and absorbs it, so a curve then bows across the true corner by several px. Cut when the 8 px baseline angle exceeds ~20°, or when the adjacent-point angle exceeds ~35° and both adjacent segments are ≥4 px (on a sparse chain the adjacent points are themselves the shape). Tighten the fit tolerance to ~0.6 px when corners matter.
- **Detail zones get upgraded measurement.** The ~20° anchor threshold is the default; in a detail-dense zone drop it to ~12°, draw sharp angles as straight segments rather than quadratics, and measure small features (<10 px) coordinate by coordinate. Add an alarm: if a bright area or a fill's area drifts more than 5% from its reference mask, the fit has systematically shrunk — investigate before continuing.
- **Contour smoothing (eps, corner_angle) is chosen by the part's minimum curvature radius, not by part size.** For `approxPolyDP` + Catmull-Rom: set eps from the tightest curve the part must hold — a part whose smallest radius is small takes a small eps even when the part is large (a large part at eps 2.0–3.5 polygonizes its small-radius curves; round-11 seal needed eps 0.8 on large parts and 0.6 on the head to pass 3×). A 200 px-class long arc still leaves ~1 px flats at eps 1.2 — tighten further or add anchors there. corner_angle 30° default; a tufted or feathered edge that must stay round takes corner_angle 45° with eps ~3.5. A detail-dense zone (high perimeter per area) takes eps one tier lower than the part's radius tier — the radius tier alone eats fine texture there. eps too large eats small features outright (eps 4.0 rounded a small feature off, ~636 px lost) — if a small feature vanishes, lower eps before anything else. eps too small on a large part keeps pixel staircases — raise it until the 3× edge is smooth. After simplification, check the part's four cardinal extrema (top/bottom/left/right) against the vertex set and insert any missing extremum as an anchor — simplification eats shallow extrema and the curve then goes flat where the reference peaks. Check both directions at 3× after every change. (B14 note: the corner_angle here is the simplification threshold — which vertices survive. The run-split threshold for fitting a smooth closed outline is a separate rule — see below.) (Round-11 evidence: seal, tiger.)
- **A run-split threshold must exceed the run's largest chord angle — it splits runs, it never drops points.** When fitting a smooth closed outline, split the run into fitting segments at a threshold above the largest chord angle inside that run (near 50° covers the 35–50° band; a 45° threshold still fails where the run's chord angle reaches 48°). The threshold decides where a run is split for fitting — it discards no vertices and no points, and it is not a drop filter. Do not confuse it with the contour-smoothing corner_angle above, which decides which vertices survive simplification.
- **Catmull-Rom parameterization is chosen, not defaulted.** When anchor spacing is uneven, use centripetal Catmull-Rom (α=0.5) — a uniform parameterization drags the control points across a long span and bows the curve off the reference by several px. Catmull-Rom is for long arcs, not a default: a part built mostly of straight edges and sharp corners takes a straight-edge polygon instead, decided by probe (on such a part the polygon measures better than the smoothed curve). For a smooth arc, split a segment only when the cubic's midpoint deviates from the measured point by more than ~2 px — splitting by feel facets the arc. **Resample CR anchors at equal arc-length spacing, no more than ~5 px apart.** A sparse vertex set plus CR leaves ~1 px flats on 200 px-class long arcs; resampling the measured contour at ~5 px arc-length intervals before CR closes them. (Round-11 evidence: owl, gt1.)
- **A fill can have several loops — fit every one.** A shape may carry an outer loop plus inner loops (a ring, a shape with holes, a band that closes on itself). Fit every loop from the measured contour and emit them as subpaths of one d (even-odd) — never keep only the longest loop and drop the rest. Before drawing an outer contour, count the closed connected components of each color inside the body: each one is either a hole of some part (an even-odd subpath) or an independent part of its own — drawing only the outer contour fills them all in with the body color.
- **A short chain that bows must not be replaced by its chord.** When a short chain's samples deviate from its chord by more than ~0.25 px, route through the farthest sample so the side keeps its bow — a straight segment is only safe when the samples actually lie on the chord. On a thin loop, replacing both sides by the same chord collapses the polygon to zero width and it renders nothing. And a fit's reported residual is only honest when each sample is measured against the segment it belongs to — measuring all samples against every sub-segment inflates the number and sends the fix to the wrong place.
- **Stacked parts that share one contour are drawn as complete shapes.** Where two parts meet along one measured edge, draw each part whole and reuse the shared edge — do not stop a part at the visible boundary where the next part covers it; the shared edge is measured once and used by both.
- **A concave boundary makes the backing vulnerable.** Where a union's contour dips inward at a part boundary, a backing shape assumed covered can show through a 1 px gap — check concave dips explicitly and patch with a backing shape spanning the region. The rule is "concavity makes the backing vulnerable", not "concavity = hole".
- **Mirrored or reversed chains get a mechanical continuity check.** Before drawing, verify the key vertices appear in traversal order — a reversed chain that folds back on itself is invisible in the d string and only shows up as a collapsed part in the render. When a symmetric pair measures ~1 px apart, use the exact mirror of one side for the other, so the chain closes.
- Boolean composition (`combine_shapes` union/subtract) is available for genuinely holed parts. Caution: its path-input coverage had a bbox (x,y,w,h)-vs-corners bug, fixed upstream in merge d31eeb6e (2026-10-21); on older builds feed combine inputs as polygons. `create_selection` accepts raw `d` only with that same fix merged.
- **A measured contour is not the rendered position — verify thin features against the engine render.** The engine's rasterization can shift a thin polygon by ~1 px from its coordinates (a rectangle renders true while a thin spike needs a +1 px shift to align with the reference mask): compensate the exported coordinates by the measured shift, and judge a fine feature by the engine render — never by a local rasterizer, whose anti-aliasing differs and whose 1 px differences are not a verdict. Two related floors: the engine's raster spread is sub-pixel (~0.3 px measured) — never compensate it with whole-pixel erosion (1 px erosion overcorrects ~3×). And a mask vertex from a strict threshold sits ~0.5–1 px inside the true vector edge (an exact-color edge reads ~1–2 px inside the visual edge) — compensate small shapes outward by ~1 px, and judge steep edges by the engine render + 3× inspection, never by a scanline distance number.
- **A sub-pixel offset between measurement and render is a property of the measurement chain — verify it with a rectangle probe before applying any uniform shift; never apply +0.5 blindly.** The earlier +0.5 convention was measured on one chain (synthetic circle Δ=(−0.505,−0.512); synthetic rectangle (−0.5,−0.5) in two renderers; a real logo's path centroid vs PNG centroid Δ=(−0.516,−0.614), residual ≤0.11 px after +0.5). A minimal rectangle probe on the current build can show no shift at all: two rectangles drawn at known coordinates (one at half-pixel, one at integer) rendered exactly on their draw coordinates, and removing a uniform +0.5 then moved every edge ~0.5 px closer to the true edge (letter IoU 0.956/0.941→0.974/0.965). So: first probe, then decide. A uniform +0.5 translation is not an outset — on top/left edges it moves the edge inward, not outward. What a contour needs is an outset along the outward normal, sized by how the contour was sampled: a binary-mask contour insets ~0.5 px and needs ~0.5 px of outward-normal outset; a 50%-coverage contour needs ~+0.5 px of outset (a +0.2 px outset is insufficient there). Do not chase a residual below ~0.1 px as a registration error. (N1 revised, round 7: three image classes.)
- **A thin stroke that renders thin is corrected by outlining outward 0.5 px — not by translating the shape.** Outer ring outward, hole ring toward the hole's center — both fatten the ink. Translating the shape moves it off its measured position and measures worse.
- **A 1 px-wide tip is a rasterization-fragile point — the eye at 3× decides, a single mask pixel never gates.** Sub-pixel alignment response at a 1 px tip saturates rather than rising monotonically, so lengthening it may not land at all and may overshoot instead; if the 3× view reads the tip as equal in length and shape, a missing mask pixel is not a defect. A worry about a fix (e.g. "lengthening will overshoot") is tested by probing, not assumed — what decides is whether the coverage lands.
- **Probes run in independent documents, at true positions.** Variants stacked in one document on a positional grid do not render as they do at their true positions — the engine's banded rasterization depends on absolute position, so a grid probe can mislead by 2×. Probe each variant in its own document at its true coordinates. In a new session, create a new document before exporting — otherwise the export renders the blank default document.
- **Export paths are relative or inside the export directory.** The export call accepts only a relative path (relative to the server's working directory) or a path inside the `YANSHI_EXPORT_DIR` directory; an absolute path outside it is rejected. A relative export lands under the server cwd's `exports/` and the receipt hash matches the file on disk — but an absolute path pointing inside `exports/` is itself rejected, while an absolute `/tmp` path is allowed.
- Text in the artwork: use real text, never traced letterforms — except a text-class image (a logo or wordmark), whose letters are outlines drawn from measured contours (Phase "Logo variant" below). The `<text>` route is used only after the candidate font passes an internal-shape probe of the letterforms (inner-stroke gaps, serif cuts) — a bbox pass is not enough: a font whose bbox matches can still have its inner strokes in the wrong place. With a true font from the OFL allowlist, `draw_text` + `export_svg` already deliver real `<text>`; builtin text and non-allowlisted families still export as raster and must be assembled (see Text Assembly). (Round-11 evidence: wordmark — Libertinus inner strokes wrong despite a bbox pass, W failed on the `<text>` route and passed on outlines; logo — letters traced as outlines, 18 paths / 0 text, passed.)
- QA gate: render, build a triptych (reference | previous | current) including a zoomed crop of the densest region, and pass your own visual check before proceeding. Regions outside the edited bbox must be pixel-identical to the previous version. A flat extremal edge must cross the last row's pixel center, or that row renders empty — verify the rendered rows after drawing, not just the path.

## Text Assembly

- **Read the export receipt before assembling text — since draw-text ③ (2026-10-27) `export_svg` emits real `<text>` itself.** `draw_text` with a true font from the OFL allowlist records text provenance and the export inlines the font subset as `@font-face`; the receipt counts the three destinations (texts_vector / texts_raster_fallback / texts_builtin_raster) and warns per object with the reason. Text still rasterizes into `<image>` in three cases: `font=builtin` (bitmap glyphs, no outlines), a family outside the allowlist (font_license_unknown), and a non-translation object transform (text_transform_not_translation). Assemble manually (below) only for those; when the receipt reports every text as texts_vector, the raw engine export already carries real text.
- **`draw_text` takes its arguments inside a `data` envelope — top-level parameters are rejected.** Call as `draw_text {doc_id, layer_id, data:{text, font, size, color, position:[x, y]}}`. Top-level `text`/`font`/`size`/`color`/`position` are rejected outright (round-11 penguin: all 21 text calls failed, then passed after switching to the envelope). Check the receipt's echoed `position` to prove the argument was read (see the position-semantics rule below).
- **Text fidelity with embedded subset fonts must be checked in a conformant renderer (browser), not a rasterizer.** A rasterizer that cannot load embedded subset fonts silently falls back — thin glyphs, tofu boxes for non-Latin scripts (measured: a subset-embedded CJK line rendered as tofu in cairosvg and correct bold in Chromium; Latin ink 9,144 px vs 17,094 px, ratio 0.53). If the renderer's glyphs look thin or tofu appears, re-check in the browser before concluding the text is wrong. Text colour: only the hex form is verified sRGB — see the colour-space rule in Phase 3; do not carry the numeric-array rule over from `draw_shape` untested. A numeric-array colour on `draw_text` silently renders black (measured: `[255,0,0]` ink (18.7,18.7,18.7) vs hex (255,12.5,12.5)) — use hex for text colours.
- **Assembly procedure (only for the raster-fallback cases above).** (1) Drive the text with `draw_text` in the engine doc as usual — the raster placeholders give you the text's position and a rendered copy to measure against. (2) Export the engine SVG and locate the raster `<image>` elements (one per text call — identify each by its `y` attribute, which is the text's row). (3) Replace each image with real `<text>`: display lettering gets one `<text>` per letter; a CJK line gets one `<text>` per line. (4) Verify: assert 0 `<image>` remain and the `<text>` count equals the text inventory; render the assembled SVG locally (cairosvg) and check display letters' bboxes against the reference within ≤4 px and CJK lines pixel-identical to the reference. Measure with tight windows — a wide window's neighbouring pixels contaminate the measurement and fake a size error. The engine PNG and the assembled SVG's text layer are verified separately: the engine render never shows the real text.
- **The engine export normalizes number formats in `d` (`226.0` → `226`), so exact matching against a parts table fails after assembly — normalize number formats before matching.**
- **Letter placement (display lettering).** Measure each letter's reference bbox (x, y, w, h). The letter's centre x is the reference bbox centre (`text-anchor="middle"`); baseline y = glyph_top + K·size, where glyph_top is the letter's measured ink top and K is the ink-top-to-baseline fraction for that letter in that font. `font-weight="700"`, `font-size` = the calibrated size. K is per letter per font — measure it for the font you use (baseline − ink top, divided by size); never reuse another font's K. Measured K for Fredoka Bold: C 0.60; o/n/a/s/r 0.42–0.43; g 0.43; t 0.57; ! (Titan One) 0.70. The assembled `<text>` is rendered by a different renderer than the engine, and the two rulers differ: the same nominal size renders wider and taller in the assembly renderer — re-measure advance and height with the assembly renderer (one isolated render per text class) and fit width and height separately with scaleX/scaleY. And the engine scales glyphs by line height (ascent − descent), not by the nominal size: the assembled font-size is the effective size (nominal size × upem / line height), and the effective size (or the line-height ratio) is recorded in the text provenance.
- **`draw_text` position semantics (needed to get from engine coordinates to assembly coordinates).** `position` is the **em-box top-left**, not the glyph box: the rendered glyph box sits offset from it by about +2·(size/120) horizontally and +2.5·(size/120) vertically (CJK about +3). A letter's centre x = draw_x + 2·(size/120) + render_w/2, where render_w is the rendered letter width. For CJK lines: x = the em-box left (the `draw_text` position), baseline y = glyph_top + 0.62·size. Both `[x, y]` and `{x, y}` are accepted on the true-font path (the object form used to be silently ignored — fixed 2026-10-27); the receipt echoes the normalized `position` so you can prove the argument was read.
- **Font calibration (method).** For each candidate font, render the letters and measure each letter's bbox error against the reference; pick the font with the smallest per-letter error; correct remaining narrow letters with a per-letter `transform` scale on the assembled `<text>` (never by stretching the font). `textLength` is ignored by some renderers, so it cannot be the correction mechanism. Validated picks so far: rounded bubble display lettering → Fredoka Bold (per-letter error ≤3% for most letters; o +10%, r −23%, t −27% — corrected with per-letter scale; the exclamation is not Fredoka's shape, use Titan One for it); CJK bold → Noto Sans CJK SC Bold. If the reference's letterforms are hand-drawn rather than a font, the closest font + per-letter scale is the ceiling — record it as a font ceiling, don't chase it by tracing letters. **Non-font glyphs are a separate branch:** when every candidate font's single-letter bbox error exceeds the threshold (e.g. width >25%), do not force `<text>` — trace the measured outlines as a Logo-variant shape and record the font ceiling honestly; a forced `<text>` with a heavy horizontal squeeze thins the strokes and reads false at 3×.
- **When a display face's cap/x-height ratio differs from the reference's by >1.3×, one size cannot match both height and width — size per glyph by the geometric mean of the height and width ratios.** Per-glyph geometric-mean sizing splits the error between the two axes.
- **Font procurement — Bold is a family, not a weight.** The machine may have only a couple of dozen fonts; check `list_fonts` first, and if the family you need is missing, install it (Google Fonts OFL) into `~/.fonts` + `fc-cache`. The engine matches family names exactly: a variable font's Bold must be made into its own family first — with `fonttools`, pin the variable font to `wght=700` (instancer) and rename the family (name IDs 1/16/4/3) to e.g. "Fredoka Bold". Passing "Fredoka Bold" as the font parameter is rejected unless a family is literally named that. Same for CJK: extract the Bold face from the system TTC (e.g. face index 2 of NotoSansCJK-Bold.ttc) and rename its family to "Noto Sans CJK SC Bold".

## Phase 4: Shading Pass (additive only)

Two techniques, both required for volume:

1. **Nested tone steps** — per volumetric part, add 1–2 inset shapes whose contours are the part's own smoothed contour offset inward; colors = the part's measured light-end and dark-end samples. Center light, edges sink.
2. **Masked glazing** — soft shadows/highlights via path masks with multi-tier feathering (the contact-shadow method: 3 tiers minimum, more for wider falloff). Use for cushion undersides, junctions, contact shadows, and warm/cool shifts on bodies (e.g. warm-white belly). (Note: path-mask feathering rasterizes into an embedded image on export — in a tracing deliverable it is not the softening mechanism; see the rule below.)
- **In a tracing deliverable, softening is low-opacity overlay tiers — the only vector-safe softening method.** `draw_shape` with an alpha color exports as a vector path with opacity; `fill_region` feathering, `smart_glaze`, `smart_edge_soften`, and `gradient_fill` all rasterize into an embedded image on export and must not be the softening mechanism in a tracing deliverable. Build every soft transition as ≥3 concentric tiers of low-opacity shapes (same form, growing outward, opacity stepping down), never as a feathered region.
- **Tier targets are measured, not guessed.** A contact shadow is 3 tiers of growing ellipses, alpha 45/22/8 (a smaller secondary shadow 40/20/7). If the centre reads too strong, lower the composite opacity, not the tier count — fewer tiers harden the falloff. And a same-color stroke feathered over a hard edge (width ~10, alpha ~77) only weakens the edge — it is a mitigation, never the fix for a sharp moving highlight; the fix is the dome representation above.

Rules: colors sampled per region; a shading layer that pixel-sampling proves does not exist in the reference (no local darkening, or light from the opposite side) must be removed, not faked. If a layer introduces a hard edge or dirt, feather it more, reduce tiers' contrast, or drop it — clean beats strong. Re-run the triptych QA after each group of layers.
- **Nested tone steps never split the part.** A bright band or dark face on one physical part is a layer laid over the part's base fill (an inset shape or a low-opacity overlay tier), never a reason to cut the part into separate fills — cutting by tone is what makes a part read as flat plates. The part stays one fill; the shading is additive on top of it.
- **Small 3D props are drawn facet by facet.** For a small three-dimensional prop (a chest, a box, a cup), draw each face as its own measured flat fill — bright top, dark underside, side faces, shadow line under a band — and draw the hardware (clasps, gems, bands, buckles) as separate small measured shapes, back to front. Check at 3× that every face the reference shows is present; "reads as the prop at 1:1" is not the bar, every measured face is. A face's color is measured across the face — a single-pixel edge color is anti-aliasing, not a face; painting it as a face puts a false patch on the artwork.
- **Form-defining shading is not decorative shading.** A soft shadow that gives a part its volume must be kept: add 1–2 mid-tone flat steps sampled from the measured light and dark ends, or a true gradient where the field supports it (see the next rule). Dropping it is allowed only if the triptych shows the part reads the same volume without it. "Clean beats strong" governs decoration; it never licenses removing volume.
- **A true gradient is used only where the field is a measured smooth field — and it is an assembled-SVG gradient, never the engine's gradient tool.** Use it when the color field varies smoothly along one axis: measure the field along its gradient axis (value as a function of position along the axis, nearly independent of the cross-axis coordinate), and express it as one multi-stop linearGradient along that measured axis, with stops from smoothed measured values (bin medians, three-point smoothing, outliers beyond ~8 units replaced — never encode sensor noise as gradient jitter). The gradient lives in the assembled/exported SVG: the engine document holds a flat mid-tone color for the part, and assembly matches the part to its gradient by exact normalized path data (a bbox-and-color match collides with same-colored flat strips — verify the match count and assert the gradient parts matched). Clamp beyond the last stop — never extrapolate past the measured field. This does not contradict the vector-safe rule above: the engine's `gradient_fill` tool rasterizes and is never the mechanism; the multi-stop gradient in the exported SVG is. Acceptance for a gradient field: row-mean absolute difference against the reference, plus the widest ≥5-unit vertical run — a true horizontal line is a run tens of px wide, while a smooth vertical slope of ~2–3 units and narrow runs at a moving slanted edge are benign; do not confuse them.
- **Stacked hard-edged translucent bands faking a smooth field leave step edges at 3× — use one shape with a multi-stop gradient.** At matched peak value, 6 hard-edged bands showed 5 boundary jumps of ~10 units each (mean 10.0, zero inside the bands) and 5 visible concentric rings at 3×; a single multi-stop gradient's widest adjacent step was 1 unit and showed no rings. This is the negative diagnostic of the gradient rule above: if the field is smooth, draw it as one gradient, never as stacked translucent bands.
- **Recognize a dome field before choosing a representation — the recognition is the method, not the gradient.** The signals, checked before any band is drawn: (a) the part's color field varies smoothly along one axis — value as a function of position along the axis, nearly independent of the cross-axis coordinate (a measured S-curve, not a step); (b) the bright peak is a few px wide and drifts ~1 px per row — a highlight that moves as it runs down the part; (c) the part's form is a dome — a rounded top with a bright plateau falling to a darker base, judged from the silhouette and the field together. When those signals are present, go straight to the smooth-gradient dome representation (multi-stop linearGradient along the measured slanted axis, stops from smoothed measured values, edge-falloff covers measured per side after the occlusion check). Do not start with 1 px bands + pinned edges: the band representation has a proven limit at a sharp moving highlight, and the four pinning variants are dead ends — starting from bands costs a full round and ends at the same gradient. The failure mode to watch for is drawing a flat slab with a bright stripe where the reference has a rounded dome: if the bright region's silhouette is a dome, the shape itself must be a dome — a slab reads flat and angular at 3×, and no amount of feathering on the stripe fixes the form.
- **Bands have a representation limit at a sharp moving highlight — when pinning fails, change the representation.** A highlight peak a few px wide that moves ~1 px per row defeats 1 px bands with pinned edges: pinned-edge consistency and vertical continuity cannot both be met. Four pinning variants are proven dead ends — pinning to the profile midpoint (consistency returns in full), narrowing the columns (continuity worsens), adaptive splitting (does not converge), birth feathering (both worsen) — do not retry them. Switch the representation to the smooth-gradient form above; do not try another pinning variant.
- **A transition boundary is measured point by point, never fitted by a three-point line.** A boundary that curves (a U-shaped boundary can sit 10 rows off its straight fit) makes a straight-fitted base wrong by ~19 units at the edge. Measure the boundary position at intervals along its run and use the measured curve; a correction band (next rule but one) is the patch for a straight model, not a substitute for measuring.
- **Check occlusion before drawing an edge falloff.** A darkening at a contour may be the occluder's transition, not the part's own edge falloff — verify which side is occluded before drawing a cover there; a cover drawn on an occluded side paints a falloff the part does not have. A falloff cover strip stops exactly at the contour — extending it past the contour reads as a bright halo on a dark background.
- **A correction band is a general construction.** To repair a local field error without touching the existing layers, draw a band of per-row 1 px pieces with a horizontal multi-stop gradient, its inner end pinned to the existing value for an exact join, placed after the layer being corrected and before the target layer. Width is tens of px (an inner band ~25 px where a model fails, an outer blend band ~20 px where a neighbor's field must meet the edge). Existing strips are untouched, so the seam metrics do not move. Stops come from smoothed measured values along each row, and the band's inner edge uses the d=2 measured value at the contour (d=1 is anti-aliasing-contaminated; the renderer's own anti-aliasing reproduces the edge).
- **A smoothly varying field is cut into fixed strips, not per-row greedy segments.** Per-row greedy segmentation moves the joints row to row, and in steep zones the joints line up into visible horizontal steps. Take the union of per-row joints, cluster them into fixed cuts (a cut must appear in a substantial share of rows, with a maximum column width cap), so each column is one vertically continuous 1 px strip chain and each strip's bottom equals the next strip's top at every point — horizontal steps can then only appear at true feature edges or occlusion boundaries. Keep a diagonal gradient inside each column for the cross-column gradient, so adjacent columns agree at their shared edge. Internal strip edges overlap 0.5 px (the outer silhouette edge is never extended); a column's first edge extends 0.5 px inward only where the reference's neighboring pixel is bright — i.e. the neighbor is another part of the same object — never onto a dark exterior, where it would read as a bright edge.
- **Assembly matching breaks near-ties on color.** When two gradient parts occupy nearly the same box with nearly equal area, "smallest area" misjudges the match; prefer the candidate whose flat color equals the engine fill, and fall back to smallest area only for split fragments, whose resampled color matches no part's flat color.

## Phase 5: Soft-Feature Rebuild (blush, glows, auras)

- Delete any traced version of the feature entirely. Rebuild generatively from measurements: footprint (center, radii, tilt) fitted to the reference's visible extent; color extrapolated from the sampled center color along its saturation falloff vector (pigment color is deeper than the rendered center, which is already diluted by the skin/base beneath).
- Construction: ≥4 concentric tiers of low-opacity ellipses/shapes, outermost tier fading to invisibility against the base color. Size the footprint from the reference's measured halo extent — not from the feature's core: a blush's visible wash is far wider than its deepest core, and sizing by the core leaves a small hard sticker. Step the opacity down tier by tier and verify the falloff against the reference's measured falloff (e.g. a scanline's color-difference profile) at zoom. Path-mask feathering is not the softening mechanism here — it rasterizes on export (see Phase 4).
- **Tier targets (measured).** A blush is 6 tiers, radii 0.30/0.45/0.60/0.74/0.87/1.00 of the footprint, per-tier alpha 69/56/43/33/23/13 (0–255), so the stacked composite opacity is ~0.65 at the centre fading to ~0.05 at the edge. If the centre reads too strong, lower the composite opacity, not the tier count — fewer tiers harden the falloff.
- **What decides whether a soft radial falloff rings at 3× is the rendered value step per tier, not the tier count.** At a fixed step, 6/8/10 tiers ring identically (step ~13 → boundary jump ~10, rings plainly visible; step ~6.5 → jump ~5, faint but visible including 8 tiers; step ~2.6 → jump ~2, no rings). Size the tier count from the step and the falloff depth: a shallow falloff needs ~8–10 tiers at step ≤3, a deep one proportionally more (a 78-unit falloff needed 30 tiers at step 2.6). And the nominal alpha step is not the rendered step — layered alpha stacks amplify it ~2× (a 6-tier schedule's nominal step measured 17–26 after rendering), so measure the step on the render, not on the schedule. Step ≤3 is necessary, not sufficient: faint layering can remain visible at 3× even at small steps, so the 3× check is still the verdict.
- Acceptance: crop the feature at high zoom against the reference — hue, position, size, and falloff softness must all match; render-verify the composited center color against the reference center.
- **Sample a soft feature's center away from neighboring strong colors.** Exclude neighboring strong-color parts' masks before sampling the center color, and if the center lands on a strong color, shift away from it until the target color's share in the sampling window is adequate — a center sampled on a neighboring strong part tints the whole feature with that part's color.
- Draw facial features (eyes, beak) *above* the soft feature so it never tints them.

## Stop Rule (Iteration Discipline)

- "Diminishing returns" is not a stop rule — it is what the last versions always feel like. Iterate until every item below passes.
- **The first item is visual inspection, and it is the only pass.** Look at the 1:1 comparison and the 3× zooms yourself, item by item — every key part: does it look like the reference, is the expression right, are there too many lines, do the color blocks join up. Check semantic structure first, seams second: ask of every part "does it read as the thing it is" (a backrest reads as one board, a cushion as one cushion, a chest as a 3D chest) before counting white gaps or black overreach — a version with zero gaps and a misread structure is a fail, and a gap-count pass must never be reported as a structure pass. Every numeric item below is supporting evidence only (辅助): numbers all green but the eye sees something wrong is a fail; numbers slightly off but the eye sees nothing wrong is judged together with the eye. **Numbers never overrule the eye.** Curve-level comparison against a known SVG (Hausdorff distance, control-point deviation, segment counts) is a diagnostic — it tells you where a shape deviates, by how much, and in which direction, so you know what to fix. It never grants a pass. Sampling protocol: sample both curves densely (≤1 px per point, or ≥150–200 points per segment), per subpath, never interpolating across subpaths — sparse sampling and cross-subpath interpolation both invent phantom distances. Validate the diagnostic tool itself first against a known-coincident curve pair (it must read zero) — a diagnostic that has not been falsified is not a measurement. And compare visible geometry only: when the reference draws one shape under another, the reference path's extent includes occluded regions — compare the rendered visible shapes (or clip the reference path to what is visible), never a raw path extent against a visible-pixel measurement; the difference between the two is the occluded part, not an error. Report curve diagnostics split by visible and hidden: a headline worst distance dominated by hidden geometry says nothing about the visible render — judge the visible parts on their own numbers, and the hidden parts only by whether the visible edges land right. For a layered diagram, a single part's curve distance is diagnostic only — the part as drawn is not the part as visible — so acceptance is the union render's pixels plus 1:1/3× visual inspection; state in the notes which parts' distances are naturally large. Do not chase the worst point at a sharp tip: a tip vertex can read ~3 px while the raster tip is correct — anchor by scanline, not by the report's worst point. Tips are fixed by intersecting the fitted edges, not by extremal pixels — anti-aliasing retracts a tip by ~4 px. That rule applies where the sides meet at a real angle; on a near-parallel hairpin the intersection is ill-conditioned and the ~4 px retraction is the measurement floor — a blanket outward extension misfires there, so record it and move on. When a tip is the endpoint of a fitted edge, constrain that edge's end tangent to the measured direction — a free fit can have small residuals and still end blunt. A diagnostic's worst point can also be a construction artifact of the reference path — an out-and-back jump between two loops of one path that sticks out a few pixels but has zero render width. Verify the worst point against the render before fixing anything: if the rendered shape is right, the slit is neither drawn nor chased.
- **Falsify before fixing.** A "defect" whose measured missing area is 0 px is a misread, not a defect — measure first, and do not "fix" what the measurement says is not there. A difference that appears in one rasterizer and not another is a renderer difference, not a drawing error — confirm it by rendering the same geometry in both before re-drawing anything.
- **Inspection images lie about scale — verify an apparent defect at the pixel level before believing it or fixing it.** A dot, fringe, or point that appears only in a scaled-up view must be checked against pixel values, not against the scaled display: count the pixels, do not trust the zoom. Zoomed inspection images must use one consistent interpolation (smooth) — mixing smooth and nearest-neighbour zooms invents edges that are not in the render. (Round-10 evidence: two image classes — a scaled display invented a dot that a strict pixel count showed as 0; a nose tip read differently under different zoom interpolations.)
- Stop only when the visual inspection passes and every supporting item below is in line; "looks like the same artwork at a glance" is necessary and not sufficient:
- **Run the whole-image glance verdict explicitly before declaring a version done.** At 1:1, look at the whole image once: does it read as the same artwork at a glance. A glance fail fails the version, and zone scores cannot overturn it. A glance pass does not grant a pass — zones must still pass one by one. (Round-9 evidence: penguin, fox, owl, gt3, seal all ran it explicitly; round 8's penguin passed zones but failed the glance, and the zone scores were wrong to stand.)
  - Key parts, one by one at zoom: each key part reads identically to the reference — not "close", identical.
  - Eyes get their own 3× check — never passed on the thumbnail alone. At 3× zoom, each eye reads identically to the reference: eye-white length and sharpness of the tips, pupil outline shape, size relative to the eye white, and position, upper-lid line and lash-flick length and curve, lower-lid shadow. Measure a pupil or any small dark feature by its outline, never by its darkest core: a small dark core inside a larger feature is not the feature. A size error invisible at thumbnail is obvious at 3×. Check each eye separately — a pass on one eye says nothing about the other.
  - Edge smoothness (visual): at 3× zoom, long edges show no steps and no wobble. Where it can be measured, the rendered edge's second-order differences diagnose the wobble — they guide the fix, they do not pass the version.
  - Stroke weight (supporting): render dark-pixel count within ±10% of the reference (see Phase 3 calibration).
  - Two-sided gap metrics (supporting): count white gaps (render light where the reference is dark) and black overreach (render dark where the reference is light) side by side, per region. Both must come down together — a version that closes white gaps by adding black the reference does not have has traded one defect for another.
  - **Gap metrics over overlapping zones double-count.** Partition gap zones disjointly before summing, or report per-zone only — one zone's numbers can exactly equal the sum of its sub-zones.
  - Per-part shape check (supporting): each key part's bbox and area in the render within ±10% of the reference's measured bbox/area — not just the whole-image pixel totals. Pixel totals alone cannot catch a distorted part: a part can have the right area but sit in the wrong place, or the right area but the wrong shape. Check bbox (x, y, width, height) *and* shape per part, at zoom. Compare each part's outline as it was drawn, not as the union render shows it — a hole a later part filled, or a bump a later part covered, inflates that part's measured deviation without being that part's error.
  - **Regress position first, then compute the global IoU.** Per-part bbox-aligned IoU hides drift — fit a position regression on measured anchors first, then the global IoU. For a ground-truth image this is the first step, before any IoU is read. Anchors come from 50%-coverage masks only; drop outliers beyond ~3 px before regressing; adopt the fitted shift only after the IoU confirms it improves — a fit can chase noise (one fit lowered the IoU instead of raising it), anchors on flat edges are unstable, and a non-robust fit can report a false error. Never adopt a shift on the fit residual alone.
  - Shading present wherever the reference has form-defining shading (see Phase 4).
  - No part lost more than 10% of its reference area (supporting, see Phase 1).
- If a version fails exactly one item, fix that item and re-render — do not declare the run done and file the failure as a "known residual".

## Iteration Economy (Time Discipline)

- **Diagnose by falsification before fixing.** Before any fix, run a minimal control probe that can prove the diagnosis wrong — a full-chain render showing the supposed defect does not exist, a small probe testing the mechanism. A misdiagnosis costs a full fix loop; a falsification costs one probe. Diagnosis is the largest single segment of a round (about a third) and the most experience-dependent — its patterns (cap burial, curved boundaries, falsified probe methods, per-stroke attribution) are what this skill records.
- **Falsification covers parameter changes too, not just diagnoses.** Any eps / stroke-width / threshold change gets a minimal probe first — one part, one render, the changed parameter only — and the full chain rebuilds only after the probe confirms it. An unprobed parameter change is how a fix creates its own defect (an eps raised to smooth one edge ate a small feature, ~636 px lost, and cost a whole fix loop to undo). The probe is seconds; the loop it prevents is minutes.
- **Small loops instead of full-chain loops.** Anything a minimal-document engine probe can verify (seconds each) is probed first; a full-chain rebuild + drive + render (minutes each) runs only for final confirmation. One fix loop costs a full chain — keep loops to 0–1 per round by probing first.
- **Probes run in parallel — with a runner, not by hand.** Independent-document probes are independent — run them across processes or machines. Serial variants are the expensive part, not the probe itself. The pattern: collect the probe variants into a list (each: its own document, its own coordinates, its own output file), and run the list through a small runner (e.g. `xargs -P` / a multiprocessing pool) instead of one shell loop — font calibration runs its candidate fonts in parallel processes, a width sweep runs its widths in parallel processes. A serial run of 25 variants costs ~15 minutes; the same list in parallel costs a third to a fifth of that. List the probes before running them — the list is itself the precondition for parallel execution: probes invented one at a time run serially however good the runner is. Diagnosis is not parallelized — it is one brain's judgment work, and splitting it across agents creates merge cost; only probes (independent measurements) parallelize.
- **Eye check first.** Confirm the defect is visible at 3× before acting — do not fix what 3× cannot see.
- **Log nine segments, every round — live, at each switch, never reconstructed afterwards.** Startup, measure/diagnose, probe, build chain, drive+render, numeric compare, visual check, fix loops, wrap-up — write the row when the segment ends, at the switch. A table reconstructed from memory after the run stops being complete (a log whose table stopped at segment 4 while the run had 9 is the failure mode). Report the total, the per-segment split, and the comparison with the previous round (which segment grew or shrank, and why).
- **Cold start and warm iteration are timed separately.** Cold start is a fresh agent + image + skill, with no old scripts, to the final SVG. Compilation and environment setup do not count; timing starts when the environment is ready. Never rebuild the engine per round — use the prebuilt release package.
- **Script the delivery check.** Verifying the deliverables (files present, sizes, hashes, embedded-image count, comparison images complete) is a script, not a manual per-item check — manual checking is half the wrap-up time.
- **Know the floor.** A complex multi-part illustration with text and soft edges has a cold-start floor of ~25–30 minutes (read the skill + look at the picture ~2', measured work with tools ~10–12', build chain ~4', drive+render ~5' for 2–3 versions, eye check ~5–8', 1–2 fix loops ~5–10', wrap-up ~3'). A simpler single-logo image floors much lower (~10–15'). Report time per image-complexity band, not as one number. Below the floor you are trading quality for time — don't.

## Logo Variant

- Single flat color: census the exact value; do not eyeball it.
- Icon + wordmark are both traced outlines (no live text). Counters and cutouts are even-odd holes; verify openness/connectedness per letter from the source mask before choosing path topology. Offset direction is opposite for outer and inner rings: the outer ring expands outward and the hole ring contracts toward the hole's center — both fatten the ink. Expanding a hole ring thins the ink instead. **Letterforms are built from measured contours, never drawn from impression:** measure each letter's bbox, stem positions and widths, and counter shapes from the mask first, then draw the outline through the measured points. A letter drawn from memory of what the letter looks like has the wrong proportions — it may read at thumbnail size and break at zoom.
- Deliver with transparent background; render comparisons on white (or the source's bg).
- Segment budget still applies: clean geometric logos want long smooth curves.

## Deliverables Checklist (all of them, every version)

- [ ] Full-canvas PNG
- [ ] Triptych PNG (reference | previous | current), including the key zoomed region
- [ ] Overlay PNG (render at 50% over the reference) — contours must coincide, not just look similar side by side
- [ ] Numeric gate alongside the eye (supporting only): render dark-pixel count within ±10% of the reference, and every part's area within ±10% of its reference mask. Numbers support the eye; they do not replace it, and they never overrule it (see Stop Rule).
- [ ] SVG exported via `export_svg` — **copy it out of the run's exports dir into the delivery folder in the same step** (this was forgotten once; it is the deliverable users ask for by name). After exporting, confirm the file exists (`ls`) before copying it out — the receipt can arrive 1–3 s before the file lands on disk; if it is missing, wait 2 s and retry.
- [ ] Background rect removed when the deliverable needs a transparent background: the engine's document exports a white full-canvas background rect — remove it and re-render to verify (cairosvg white-background render numerically unchanged; transparent-render corner alpha = 0). (W8-2, promoted in v5.8: wordmark rounds 8–9, github round 9, logo round 9.)
- [ ] Assembled SVG (if the artwork has text): 0 `<image>`, `<text>` count equals the text inventory; assembled SVG locally rendered and checked (display-letter bboxes ≤4 px vs reference, CJK lines pixel-identical) — see Text Assembly.
- [ ] Project file (.yanshi)
- [ ] REPORT in the run dir: merge table, per-part segment counts, sampled colors, shading-layer inventory, metrics as supporting evidence, known residuals stated honestly
- [ ] Delivery-check script passed (files / sizes / hashes / embedded-image count / comparison images complete — see Iteration Economy)

## Known Failure Modes

- **Dirty/scratchy result** → merge granularity too fine or fit tolerance too tight. Merge harder, refit looser.
- **Gray muddy soft features** → the feature was traced. Rebuild it (Phase 5).
- **Hard-edged shadow blocks** → flat shapes used as shadows. Use low-opacity tier overlays (vector-safe); mask-tier feathering rasterizes on export in a tracing deliverable (see Phase 4).
- **Dark seams between flat parts** → shapes abut instead of overlap. Apply the Phase 3 expansion rule.
- **Better metrics, worse picture** → trust the picture. State the tradeoff in the report instead of hiding it.
- **Heavy outlines** → outlines faked as seams between filled shapes or as a dark underlay silhouette. Draw them as stroked paths (Phase 3).
- **Lost small features** → a small dark feature dropped because filling it swallowed its neighbor. Draw it as a stroke or a small shape; never drop it silently.
- **Missing volume / flat parts** → form-defining shading dropped as "clean beats strong". Keep it as mid-tone steps (Phase 4).
- **Eaten edges** → a single-threshold barrier eats shadow tones and small parts. Use a color-aware barrier and the per-part area check (Phase 1).
- **Jagged edges / wobbly strokes** → the path was built from pixels: mask boundary as a staircase, skeleton centerline as a pixel chain. Rebuild by recognizing the curve and drawing it as a long Bézier with few control points — smoothing a pixel chain does not make it a vector.
- **Distorted shapes / lost small features / wrong proportions** → anchors were placed by eye without measurement: smooth edges, wrong geometry. Rebuild by measuring the contour's curvature extrema and placing anchors on them — a smooth curve in the wrong place is worse than a jagged one in the right place.
- **Horizontal spike lines across a smooth curved highlight** → the band-plus-pinning representation is at its limit. Switch to the smooth-gradient representation (Phase 4); do not try another pinning variant.
- **White seam where two parts share an edge** → both sides drew the shared edge. Draw it on one side only, overlapping 0.5–1 px (Phase 3).
- **1 px gray seam where a patch meets a stroke** → zero-overlap abutment. Overlap the patch ~1 px into the stroke side (Phase 3).
- **Thin polygon renders ~1 px left of its coordinates** → engine rasterization offset. Compensate the exported coordinates and verify against the engine render (Phase 3).
- **Export rejected, or a blank default document rendered** → absolute path outside the export directory, or a new session that never created a document. Use a relative path or a path inside the export directory, and create a new document before exporting in every new session (Phase 3).
- **Flat slab where the reference has a dome** → the part's form was misread; the bright field was drawn as a stripe on a slab instead of a smooth-gradient dome. Recognize the dome field first (Phase 4) and rebuild the part as a dome.
- **Text ships as raster `<image>`** → the assembly step was skipped; `draw_text` always rasterizes on export. Assemble real `<text>` (Text Assembly) and assert 0 `<image>`.
- **Re-drive stacks the old version on top (ops doubled)** → the same doc_id reopened the old content. Delete `mcp-root/docs/<doc>` or use a fresh doc_id before re-driving (Phase 0).
- **Font rejected / Bold not found** → the engine matches family names exactly; a Bold that isn't its own family doesn't exist. Make the Bold family with fonttools (Text Assembly).
- **Server exits mid-drive / every call after a big drive fails** → a line exceeded the 64 KiB stdio limit (the server dies on an over-limit line, no clean error). Chunk `batch` by bytes (≤60 KiB per packet) and drive through the guarded client — never send an over-limit line by hand (Phase 0).
- **Export receipt has no sha256** → a failed call's receipt has none — check `ok`/status first; a missing sha256 alone is not a failure (Phase 0).
- **Stripes drawn straight where the reference's stripes drift** → the pattern was measured from one row. Measure drifting patterns row by row (Phase 1).
- **Arc command rejected / a part missing from the export** → the engine's path parser rejects `A`; pre-convert arcs to cubics (≤45° per segment, sweep start normalized to (−180°, 0]) and count `d` strings in the export — the call-ok count alone does not prove the part landed (Phase 3).
- **A `fill` key in `draw_shape` data is silently ignored** → the call returns ok, the shape exports with `fill="none"` and renders blank. The fill colour is `color`, not `fill` (Phase 3).
- **Stroke-only shape exports filled black** → a rect/ellipse/polygon drawn without `color` fills black by default (only a `path` kind honours a missing `color` as no fill). Draw it as a `path`, or set `fill="none"` on the assembled SVG and verify the export (Phase 3).
- **`draw_text` call ok but the text lands at the origin** → both position forms are accepted now, so check the receipt's echoed `position`: it echoes what was parsed, proving the argument was read (Text Assembly). Empty/畸形 values still fall back to `(0, 0)`.
- **Curve diagnostic reports a huge distance on a render that looks right** → the diagnostic compared full reference paths (including occluded regions) against visible-shape traces, or sampled too sparsely — compare visible geometry only, densely sampled per subpath, and falsify the diagnostic tool itself first (Stop Rule). Split the error into normal and tangential components: a normal offset is a shape error, a tangential offset is usually a parametrization or sampling artifact. Locate edges by alpha-coverage subpixel measurement (dense normal profiles to the 50% crossing), never by counting pixels in a threshold mask — a threshold mask cannot resolve a sub-pixel edge.

---

## Appendix R3 — Cold-start round 3 additions (v5.2, 2026-10-27)

Source: cold-start round 3, 7 images, skill v5.1 + engine main 68adb93
(receipts ①+②, draw_text ③). Round-2 → round-3 times: ~210' → ~134'
(−36%). Quality: round 3 7/7 execution-agent self eye PASS; round 2
independent eye 6/7 — the two pass rates are NOT directly comparable
(different verifiers). Full per-image record: BACKFILL.md in the round-3
work directory.

Generalization gate (unchanged): a rule counts only after verification on
≥2 image classes; rule text contains no image name, part name, or colour
value. A档 = meets the gate, merged into the body. B档 = single-class,
appendix only. C档 = decided, rule in the body.

### A档 — 已验证（≥2 类图），已并入正文（v5.2）

| 编号 | 规则 | 并入位置 |
|---|---|---|
| A1 | Thresholds at 50% coverage for every area/contour measurement; edges located by dense normal profiles to the 50% crossing | Phase 1 (50% coverage rule) |
| A2 | Same geometry renders differently across rasterizers — judge against the ground truth rendered in the SAME renderer; dual-rasterizer comparison first when attribution is in doubt | Phase 3 (engine-probe rule) + Stop Rule |
| A3 | Split curve diagnostics into normal/tangential components; locate edges by AA-coverage subpixel profiles, never by threshold-mask pixel counts | Stop Rule + Phase 1 |
| A4 | Stroke width between representable quanta → fill band (confirmation of the existing Phase 3 even-step rule; no body change) | — (confirmation) |
| A5 | A fresh session exporting without creating a document renders the blank default doc (confirmation of the existing Phase 0/Phase 3 rule; no body change) | — (confirmation) |

### B档 — 待第二类图验证（只记附录，不进正文）

**B1. Anti-aliased blend colours of near-identical pure colours get misclassified into a third class.** Blend colours between two near-identical pure colours form filaments that read as a third class. Countermeasure: filter tiny connected components by area before trusting a per-class census. → v5.3: promoted to Phase 1.

**B2. Per-class area/bbox is polluted by the 1 px AA edge spectrum.** Row-scan min/max can be skewed by tens of px by single-pixel spectra. Countermeasure: trim the edge spectrum (e.g. ignore 1 px-wide spectral outliers) before reading a class's bbox/extrema. → v5.3: promoted to Phase 1.

**B3. A polygon-vertex miter outset's denominator is sin(β/2), not cos.** 1/cos over-outsets by several px on smooth vertices (verified: 15° vertex 0.505 px, 90° corner 0.707 px). Use sin(β/2). → v5.3: promoted to Phase 3.

**B4. Overlapping-zone gap metrics double-count.** One zone's numbers can exactly equal the sum of its sub-zones. Countermeasure: partition gap zones disjointly before summing, or report per-zone only. → v5.3: promoted to the Stop Rule.

**B5. Fine-dot density is an independent acceptance item.** When batch-dropping <10 px isolated spots, decide explicitly whether fine-dot density is an acceptance item of its own — a batch area cutoff must not silently decide it. Measure it: per-class census at 50% coverage, count real fine dots (maxdim <10 px, solid core, AA spectrum trimmed) per 1000 px². → v5.4: promoted to Phase 1. Second class: constructed starfield (152 solid-circle stars; census recovers 152/152; real fine dots 145, density 0.302/1000 px²; batch-dropping erases 145/145) vs round-4 butterfly (2043 sub-10 px spots all AA artifacts, density 0) — same census method, opposite outcomes.

**B6. Refinement splicing uses per-edge shortest span, not shortest whole segment.** A "shortest whole segment" span flips direction when it covers most of a ring. Fix by per-edge shortest-span splicing. → v5.3: promoted to Phase 3.

**B7. A polygon fill overcounts area on thin strokes — banned for area measurement.** Measured overcount +27% on a thin-stroke case (v5.3 correction — the 7–20% range in the v5.2 appendix understated it). Use the reference renderer or point-wise distance instead. → v5.3: promoted to Phase 1.

**B8. Regress position first, then compute the global IoU.** Per-part bbox-aligned IoU hides drift — fit an affine position regression on measured anchor points first, then the global IoU. Anchors come from 50%-coverage masks only; drop outliers beyond ~3 px before regressing; adopt the fitted shift only after the IoU confirms it improves — a fit can chase noise, anchors on flat edges are unstable, and a non-robust fit can report a false error. → v5.3: promoted to the Stop Rule (standard first step for a ground-truth image).

**B9. A flat-topped arc splits at the vertex, with the vertex tangent constrained horizontal, then fits halves.** A single cubic cannot hold both a flat-topped arc's vertex and its shoulders at subpixel precision (pinning the vertex worsened shoulders 0.8→1.3 px). Split at the vertex, constrain the vertex tangent horizontal, fit halves (maxerr 0.09–0.62 px). Split only when the single cubic's max error exceeds ~0.5 px — a single cubic that already meets the bar needs no split (weak/conditional promotion). → v5.3: promoted to Phase 3.

**B10. Analytic vertex compensation overshoots at concave vertices.** Compensation of 0.5/sin(half-angle) overshoots where the vertex is concave (measured shrink, visible at 3×). When the fitted line already hugs the true edge, use the raw fitted vertex; verify any compensation against the render first. The trigger is the measurement method, not the angle: with subpixel edge measurement and no threshold shrinkage, compensation necessarily overshoots (displacement >1 px when the outer wedge angle ψ<60°); with threshold-mask measurement (~0.5 px shrinkage) the same compensation is a correction. → v5.4: promoted to Phase 3. Second class: constructed 5-notch figure (concave interior angles φ=210–330°; raw fit error ≤0.07 px; compensation overshoots 5/5, 0.49–1.92 px; measured bisector displacement matches analytic 0.5/sin(ψ/2) within 0.04 px) vs round-4 wordmark (9 closer, 0 overshoot under threshold-mask measurement).

**B11. A thin stroke that renders thin is corrected by outlining outward 0.5 px — not by translating the shape.** Outer ring outward, hole ring toward the hole's center — both fatten the ink; translating the shape moves it off its measured position and measures worse (v5.3 correction of the v5.2 'offset by half a pixel' wording, which did not say which direction). → v5.3: promoted to Phase 3.

**B12. Stacked hard-edged translucent bands faking a smooth field leave step edges at 3× — use one shape with a multi-stop gradient.** This confirms the existing Phase 4 gradient rule on a new illustration class (the rule itself was distilled from earlier dome work on the same image family — confirmation, not an independent second class for the rule). → v5.3: stays in the appendix; see R4 for what verification is still missing.

**B13. A soft radial falloff needs ≥8–10 tiers with small alpha steps; 6 tiers with ~13-unit steps show rings at 3×.** Tension with the existing Phase 5 blush rule (6 tiers, alpha 69/56/43/33/23/13): the blush tiers use smaller alpha steps and a different radius schedule, and the 6-tier rule was measured on blush. Needs a second class and a direct comparison against the Phase 5 schedule before any body change. → v5.3: stays in the appendix; see R4 for what verification is still missing.

**B14. Fitting a smooth closed outline with a hard-corner threshold near 35° facets large shapes at 3× — use a run-split threshold near 50°.** The threshold must exceed the largest chord angle in the smooth run (~50° covers the 35–50° band); 45° fails where a run's chord angle reaches 48°. This is a run-split threshold, not a drop filter (the drop implementation was disproved by render: area −9.5%, max deviation 32 px). Do not read it as '45° ≡ 50°'. → v5.4: promoted to Phase 3. Second class: constructed arcs (full circle R=100, chord 40°; segment R=140, chord 48° — 30°: area −8.09%/−15.21%; 45°: the 48° segment still fails at −15.21%; 50°: 0.00%/0.00%) vs round-4 tiger (30° vs 50° showed no visible polygonization at 3× and 50° measured worse throughout — a single-class 45° application). Note: this threshold is different from the Phase 3 contour-smoothing corner_angle (30° for large parts) — that one decides which vertices survive simplification; this one decides where a smooth run splits. Do not conflate them.

**B15. When a display face's cap/x-height ratio differs from the reference's by >1.3×, one size cannot match both height and width — use per-glyph geometric-mean sizing.** Per-glyph geometric-mean sizing splits the error. → v5.3: promoted to Text Assembly (second class: display lettering; third verification: a second text class).

**B16. Text fidelity with embedded subset fonts must be checked in a conformant renderer (browser), not a rasterizer.** A rasterizer that cannot load embedded subset fonts silently falls back (thin glyphs, tofu boxes for non-Latin scripts). Verified by rasterizer-vs-browser renders of the same file (rasterizer: thin fallback + tofu; browser: correct). → v5.3: stays in the appendix; see R4 for what verification is still missing.

**B17. A concave boundary makes the backing vulnerable — patch with a backing shape.** When a union's contour dips inward at a part boundary, a backing shape assumed covered can show through a 1 px gap; patch with a backing shape spanning the region. Wording correction (v5.3): 'concavity makes the backing vulnerable', not 'concavity = hole'. → v5.3: promoted to Phase 3.

**B18. Seemingly circular contours may not be ideal circles — fit arcs per segment.** Top/bottom fitted centres/radii can differ by ~1.3 px; fit arcs per ~45° segment through measured 1/3, 2/3 points, and use measured arcs when the fit is off by >~1 px. Segmented fitting is enabled only when it improves the fit by >~1 px — below that it overfits AA noise. → v5.4: promoted to Phase 3. Second class: constructed ellipses (axis ratios 1.5 + boundary 1.3; single-circle mean|res| 9.41/5.69 px; 8×45° segmented 0.20/0.11 px; improvement 9.21/5.59 px; segmented radii 59→126 change systematically, centres move by the curvature law — not AA-noise overfitting) vs round-4 owl's true circle (single-circle mean|res| 0.193 px; segmented improvement only ~0.01–0.02 px; radii 63.0–67.9, centres ±3 px random scatter).

**B19. Ground-truth pairing must lay the source background first.** Trap: transparent→RGB turns black and once gave 225,662 false-positive differing pixels. Procedural note for the Phase 0 ground-truth rule. → v5.3: promoted to Phase 0.

**B20. Stroke width follows the dark band's local width, not its offset and not a band-width average.** Cross-verifies A1 from the rendering side. The v5.2 '~0.25 px inward' sub-assertion is withdrawn in v5.3 — it was the measuring agent's own sign error in its probe function; re-measurement shows 0.00 px deviation. No new body text — the main rule is already in Phase 3 verbatim.

### C档 — 已裁决（2026-10-27）

**C1. Colour space of numeric colours — decided.** The round-3 probe and the
earlier probe tested different input forms and both were right in their own
form (independent re-probe: 20/20 cells agreeing across three renderers,
old and new engine binaries byte-identical; report:
`C1-REPROBE-REPORT.md` in the round-3 work directory). The rule is now in
Phase 3: an array with any component > 1 is sRGB bytes; all components
≤ 1 is linear-light passthrough; a hex string and a `{r, g, b, a}` object
are always sRGB bytes. Scope: verified for `draw_shape` only. For
`draw_text`, the hex form is verified sRGB; the numeric-array form is NOT
separately verified. Untested: alpha < 1, the explicit `color_space`
parameter, brush-stroke colours.

---

## Appendix R4 — Cold-start round 4 additions (v5.3, 2026-10-27)

Source: cold-start round 4, 9 images, skill v5.2 + engine 48f3190
release package (with raw). Independent coordinator eye check restored:
8/9 pass (the penguin NOT PASS — same as round 2). Round-4 total ~153'
(target <100' missed; same-7-images ~135', flat vs round 3). Full
record: REPORT.md in the round-4 work directory.

Generalization gate (unchanged): a rule counts only after verification on
≥2 image classes; rule text contains no image name, part name, or colour
value. Evidence classes below are provenance, not rule text.

### P0 — `draw_shape` fill-field reading corrected (Phase 3)

The v5.2 body implied `draw_shape` data has a fill field ("no fill
color", "fill and stroke fields together"). It does not: the engine
contract is `data = {geometry, color, stroke_width, stroke_color}`
(`crates/yanshi-render/src/object.rs::parse_shape`, main be9ed57) —
`data.fill` is silently accepted and silently ignored (call returns ok,
shape exports with `fill="none"`, renders blank). The fill colour IS
`color`. A stroke-only path omits `color`; a stroke-only rect/ellipse/
polygon cannot be written by omission (fill flag unconditionally on for
non-path kinds, colour defaults to black — `tools_io.rs` notes has_fill
is true for all other kinds). The Phase 3 outline rule now states the
contract, the correct writings, and the probe. The engine silently
accepting unknown fields is an engine-side issue (loud rejection would
be the fix); this round corrects the skill only.

### Promoted to the body (13)

| 编号 | 并入位置 | 佐证图类 |
|---|---|---|
| B1 | Phase 1 | fox (round 3) + owl (round 4) |
| B2 | Phase 1 | fox (round 3) + owl (round 4) |
| B3 | Phase 3 | fox (round 3) + owl (round 4) |
| B4 | Stop Rule | fox (round 3) + owl (round 4) |
| B6 | Phase 3 | wordmark (round 3) + gt3 (round 4) |
| B7 | Phase 1 | wordmark (round 3) + gt3 (round 4); magnitude corrected to +27% |
| B8 | Stop Rule | wordmark (round 3) + all GT images (round 4); anchor-robustness caveat added |
| B9 | Phase 3 | gt3 (round 3) + wordmark (round 4); weak/conditional — trigger-gated (single-cubic maxerr > ~0.5 px) |
| B11 | Phase 3 | gt3 (round 3) + wordmark (round 4); remedy corrected to outward outset 0.5 px, not translation |
| B15 | Text Assembly | penguin (round 3) + wordmark (round 4) + penguin re-verified (round 4, third verification) |
| B17 | Phase 3 | penguin (round 3) + fox (round 4); wording corrected to "concavity makes the backing vulnerable" |
| B19 | Phase 0 | github (round 3) + all GT images (round 4) |
| B20 | — (already in Phase 3 verbatim; no new body text) | github (round 3) + gt3 (round 4); the ~0.25 px sub-assertion is withdrawn |

### Held in the appendix (7) — what verification is still missing

**B5. Fine-dot density as an independent acceptance item.** Round-4
evidence: the density term is NOT TESTABLE on the verification image —
all 2043 sub-10 px spots were AA artifacts (exact-colour masks 0), so
batch-dropping lost zero real features; the "explicit decision" half is
proven, the density half is not. Still missing: an image class with
non-zero real fine-dot density (e.g. pointillist / starfield ground
truth).

**B10. Analytic vertex compensation overshoots at concave vertices.**
Round-4 evidence: 15 concave vertices compensated 0.52–1.12 px, 9 closer
to the edge, 6 neutral, 0 overshoot — overshoot not reproduced. Still
missing: a targeted counter-example class where compensation measurably
overshoots a concave vertex.

**B12. Stacked hard-edged translucent bands faking a smooth field leave
step edges at 3× — use one shape with a multi-stop gradient.** Round-4
evidence: penguin round 3 + penguin round 4 (different parts) — same
image, not two independent classes; the rule confirms the Phase 4
gradient rule as a negative diagnostic. Still missing: a second image
class with layered translucent bands faking a smooth field.

**B13. A soft radial falloff needs ≥8–10 tiers with small alpha steps;
6 tiers with ~13-unit steps show rings at 3×.** Round-4 evidence:
penguin only; the key quantity was measured (value step ≤3 units; 8
tiers at step ~6–7 still ring at 3×; 10 tiers at step 2–3 largely
smooths) — but step ≤3 is necessary, not sufficient: faint concentric
layering can remain visible at 3×. Still missing: a second image class
with a soft radial falloff over a wide field, plus a direct comparison
against the Phase 5 6-tier schedule.

**B14. Fitting a smooth closed outline with a hard-corner threshold
near 50°.** Round-4 evidence: 30° vs 50° showed no visible
polygonization at 3× and 50° measured worse throughout; the angle must
be a run-split threshold, not a drop filter (the drop implementation was
disproved by render: area −9.5%, max deviation 32 px). A single-class
45° application does not confirm the general rule. Still missing: an arc
whose chord angle falls in the 35–50° band.

**B16. Text fidelity with embedded subset fonts must be checked in a
conformant renderer (browser), not a rasterizer.** Round-4 evidence:
penguin only (rasterizer-vs-browser renders of the same file:
thin fallback + tofu vs correct). Still missing: a second image class
with embedded subset-font text, checked in a conformant renderer
against a rasterizer.

**B18. Seemingly circular contours may not be ideal circles — fit arcs
per segment.** Round-4 evidence: the verification disc is a true circle
(single-circle fit mean|res| 0.193 px, max 0.53 px); 8-segment fitting
improved only ~0.01–0.02 px (far below the ~1 px enable threshold) and
overfit AA noise; fitted centres scattered ±3 px, radii 63.0–67.9 px.
Still missing: a non-circular / elliptical disc class.

### Honest notes (v5.3)

- B12 / B13 / B16 stay in the appendix in v5.3: their second
  verification is the same image in a second round (different parts),
  not an independent image class — the ≥2-independent-classes gate is
  not met. Promotion is deferred to the next version, after an
  independent second image class verifies each rule; see the held
  table above.
- B9 is a weak/conditional promotion (trigger-gated). B13 (held) — its
  step ≤3 is necessary, not sufficient (faint layering can remain
  at 3×).
- Round-4 quality: 8/9 independent eye PASS (penguin NOT PASS);
  round-4 time ~153' vs the <100' target (missed).


---

## Appendix R5 — Cold-start round 5 additions (v5.4, 2026-10-27)

Source: cold-start round 5, 9 images, skill v5.3 + engine 48f3190
release package (with raw). Independent coordinator eye check: 6/9
pass (butterfly, owl, penguin NOT PASS). Same-7-images time 91.3'
(target ≤120' met; round 4 ~135', round 3 ~134'); 9-image total
121.2'. Full record: REPORT.md in the round-5 work directory;
probe record: `probes/PROBES-REPORT.md`.

Generalization gate (unchanged): a rule counts only after verification
on ≥2 image classes; rule text contains no image name, part name, or
colour value. Evidence classes below are provenance, not rule text.

### Promoted to the body (4)

| 编号 | 并入位置 | 佐证图类 |
|---|---|---|
| B5 | Phase 1 | butterfly (round 4, density 0 — all AA) + constructed starfield (round 5 probe, density 0.302/1000 px², batch-drop erases 145/145) |
| B10 | Phase 3 | wordmark (round 4, 9 closer under threshold-mask measurement) + constructed 5-notch figure (round 5 probe, 5/5 overshoot, raw fit ≤0.07 px) |
| B14 | Phase 3 | tiger (round 4, single-class 45° application) + constructed arcs (round 5 probe, 40° circle + 48° segment; only 50° covers the band) |
| B18 | Phase 3 | owl true circle (round 4, segmented improvement ~0.01–0.02 px, overfit) + constructed ellipses (round 5 probe, ratios 1.5 + 1.3, improvement 9.21/5.59 px) |

### Held in the appendix (3) — what verification is still missing

**B12. Stacked hard-edged translucent bands faking a smooth field leave
step edges at 3× — use one shape with a multi-stop gradient.** Round-5
status: unchanged (penguin only). Still missing: a second image class
with layered translucent bands faking a smooth field. Second-class
idea (probe report): butterfly wing gradient zone or lion background
halo, re-cut into 5–7 hard-edged translucent bands vs a single-shape
multi-stop gradient; or a constructed SVG (radial-gradient base +
concentric translucent rings).

**B13. A soft radial falloff needs ≥8–10 tiers with small alpha steps;
6 tiers with ~13-unit steps show rings at 3×.** Round-5 status:
unchanged (penguin only — the round-5 penguin's ring-free blush is the
same image, not a second class). Still missing: a second image class
with a soft radial falloff over a wide field, plus a direct comparison
against the Phase 5 6-tier schedule. Second-class idea (probe report):
reuse the B12 constructed image — tier count (6 vs 8 vs 10) × alpha
step (~13 vs ~6–7 vs 2–3) matrix, count visible rings at 3×, and compare
side by side with the Phase 5 6-tier blush schedule.

**B16. Text fidelity with embedded subset fonts must be checked in a
conformant renderer (browser), not a rasterizer.** Round-5 status:
unchanged (penguin only). Still missing: a second image class with
embedded subset-font text, checked in a conformant renderer against a
rasterizer. Second-class idea (probe report): construct an SVG with a
subset-embedded system font (non-Latin text e.g. Chinese “测试” +
Latin text), render in cairosvg vs browser/Chromium and compare glyph
by glyph for thin fallback or tofu; or use the GT wordmark (embedded-
font case).

### New one-image candidates (held — what validation each lacks)

**N1. Half-pixel grid convention (github, round 5).** Pixel-measurement
coordinates → renderer coordinates need +0.5 px; unrecorded, it caused
a systematic −0.5 px whole-image shift (LSQ fit dx=−0.50/dy=−0.53,
local IoU 0.9905→0.9991). Lacks: a second image class showing the same
systematic shift fixed by the same +0.5 convention.

**N2. `draw_shape` has no linecap parameter (gt1, round 5).** The engine
stroke defaults to round caps and the exported SVG carries no
`stroke-linecap`, so the same file renders round ends in the engine PNG
but flat ends in cairosvg/browser; workaround was drawing the S-curve
as a fill band. Lacks: a second image class with a flat-ended stroke
showing the same divergence.

**N3. Offset normal must be tested per edge, inside/outside (owl,
round 5).** offset_poly using "away from centroid" for the normal
fails on concave edges of a U-shaped frame (the centroid sits in the
hollow, the normal points into the material and shortens the inner
edge); fix: per-edge `pointPolygonTest` normal + 4 px miter cap
(v5 mean|d| 2.43→1.30). Also: uniform Catmull-Rom loops at dense
vertices (control points 6.5 px off chord); centripetal CR fixes it
(but was not the seam's main cause). Lacks: a second image class with
a concave / U-shaped frame where the same fix applies.

**N4. Receipt-hash scope (butterfly, round 5).** The export receipt
sha256 covers the pre-assembly export; the assembly step modifies the
file, so the receipt hash ≠ the delivered file hash. A scope note, not
a file error (the file was verified by direct hash + cairosvg
re-render). Lacks: confirmation on a second image that assembly always
changes the hash (or a rule: verify the delivered file by direct hash,
never trust the receipt hash alone).

**N5. Timer jump detection / daemon-held T0 (whole round 5).**
`time.monotonic()` resets across exec calls on this VM (VM reboots
mid-round; observed resets 7103.76→~130, 7101.33→~302, 7088→~170,
7130.92→~130–260, 7093.99→~286, 7102.8→~136; `timer.py now` gave
absurd values like T0+−113.4m). The round-5 brief's "monotonic anchor,
no wall-clock subtraction" is not executable on this VM — the round
switched to dual-clock recording with wall-clock fallback on absurd
monotonic values. Candidate: timer.py jump detection or a daemon-held
T0_mono. Lacks: a jump-detecting timer.py validated across a reboot
(round 6).

**N6. gt1 measurement traps (gt1, round 5; recorded in NOTES).**
Nearest-class labels mislabel dark-stroke AA pixels as bright class
(fix: t + residual gate + largest connected component); butt-cap
endpoint column-wise centreline is biased ~2 px (fit the tangent from
interior points and extrapolate). Lacks: a second image class with the
same traps.

**N7. `draw_shape` `d` must sit inside `data.geometry`; export
relative paths land under the server cwd (wordmark, round 5).** A
top-level `d` is rejected with an explicit error message; export
relative paths land under the server process cwd's `exports/`.
Lacks: a second image confirming the same error / landing behaviour.

**N8. Penguin round-5 gaps (penguin, round 5).** (a) `draw_text` colour
given as a numeric array silently renders black — hex is required
(skill C1 already says the draw_text numeric-array form is "not
separately verified"; this is the first measured case). (b) The engine
export normalizes `226.0` to `226` in `d`, so exact matching against
parts.json fails — post-assembly surgery must normalize number formats
first. (c) The rendered glyph bottom sits ~5 px higher than the
reference glyph, so speckles must be re-verified against the rendered
ink, not trusted from the reference mask. (d) Draw order must follow
occlusion (armrest behind seat, or the seat cuts the armrest straight
across). Lacks: a second image class for each of (a)–(d).

### Honest notes (v5.4)

- B12 / B13 / B16 stay in the appendix in v5.4: no second image class
  has verified any of them yet. Promotion is deferred to the next
  version, after an independent second image class verifies each rule.
- B14's promotion carries a wording note (see the body text): write
  "threshold must exceed the largest chord angle in the run; ~50°
  covers the 35–50° band", NOT "45° ≡ 50°".
- B10's promotion carries a trigger note: the trigger is the
  measurement method (subpixel fit already on the edge → compensation
  overshoots; threshold-mask shrinkage → the same compensation
  corrects), not the angle itself.
- Round-5 quality: 6/9 independent eye PASS (butterfly, owl, penguin
  NOT PASS); round-5 time 91.3' same-7 (target ≤120' met). The penguin
  is still NOT PASS (5/10 checklist items) — texture improved
  (speckle, blush), soft shading did not (armrests/seat still flat
  slabs, chest still a flat icon).
- Round-5 timing-method correction: `time.monotonic()` resets across
  exec calls on this VM, so the brief's "monotonic anchor, no
  wall-clock subtraction" was replaced mid-round by dual-clock
  recording with wall-clock fallback; see REPORT.md §4. The v5.4 timer
  candidate (N5) is a tooling fix, not a skill rule.


---

## Appendix R6 — Cold-start round 6 additions (v5.5, 2026-10-27)

Source: cold-start round 6, 9 images, skill v5.4 (packaged 24241b4) +
engine 48f3190 release package (with raw). Independent coordinator eye
check: 8/9 pass (penguin NOT PASS — 3/10 checklist items; the
execution agent's self-check claimed 10/10 and was overturned, same
lesson as rounds 4 and 5). Same-7-images time 103.9' (target ≤91.3'
missed by 12.6'; round 5 91.3'); 9-image total 133.4' (round 5
121.2'). Full record: REPORT.md in the round-6 work directory; probe
record: `probes/PROBES-REPORT.md`.

Generalization gate (unchanged): a rule counts only after verification
on ≥2 image classes; rule text contains no image name, part name, or
colour value. Evidence classes below are provenance, not rule text.

### Promoted to the body (13)

| 编号 | 并入位置 | 佐证图类 |
|---|---|---|
| B12 | Phase 4 | penguin (rounds 3–5, layered bands) + constructed radial halo (round 6 probe: 6 bands, 5 jumps ~10 vs single gradient max step 1) |
| B13 | Phase 5 | penguin (rounds 3–5, blush tiers) + constructed tier×step matrix (round 6 probe: 6/8/10 tiers × steps 13/6.5/2.6; step, not count, decides; 30-tier deep falloff) — wording rewritten: step-based, not "≥8–10 tiers" |
| B16 | Text Assembly | penguin (rounds 3–5, rasterizer vs browser) + constructed subset-font SVG (round 6 probe: CJK tofu in cairosvg, correct in Chromium; Latin ink ratio 0.53) |
| N1 | Phase 3 | github (round 5) + synthetic circle/rectangle + real github logo (round 6 probe: Δ=(−0.505,−0.512)/(−0.5,−0.5)/(−0.516,−0.614), residual ≤0.11 px after +0.5) |
| N2 | Phase 3 | gt1 (round 5) + synthetic straight stroke (round 6 probe: engine ink x 91..308 round caps vs cairosvg x 100..299 flat) |
| N3 | Phase 3 | owl (round 5) + constructed U-frame (round 6 probe: naive normal area 26,400 vs per-edge tested 32,000) |
| N4 | Phase 0 | butterfly (round 5) + two-colour document (round 6 probe: receipt fc15357e… vs delivered 5ddec839…) |
| N6 | Phase 1 | gt1 (round 5) + github GT + constructed diagonal (round 6 probe: 54% of a 2,627 px AA band mislabelled; cap-end centreline bias +4.82 px vs tangent extrapolation −0.7 px) |
| N7 | Phase 3 | wordmark (round 5) + constructed rectangle document (round 6 probe: top-level `d` explicit invalid_argument; relative export lands in server cwd `exports/`, receipt hash matches) |
| N8a | Text Assembly | penguin (round 5) + constructed Fredoka text (round 6 probe: numeric-array ink (18.7,18.7,18.7) black vs hex (255,12.5,12.5) red) |
| N8b | Text Assembly | penguin (round 5) + constructed document (round 6 probe: `226.0` → `226` in exported `d`) |
| N8d | Phase 3 | penguin (round 5) + two-rectangle occlusion (round 6 probe: overlap colour flips with draw order) |
| layer_id | Phase 3 | wordmark + owl + gt1 (round 6, three images: missing `layer_id` → explicit invalid_argument; minimal probe does not warn) |

### Held in the appendix (1) — what verification is still missing

**N8c. Rendered glyph bottoms sit ~5 px higher than reference glyphs.**
Round-6 probe did NOT reproduce it: 72 px engine bottom row 84/203 vs
reference 83/203; 120 px engine top 62 matches the skill formula (62.5)
and sits 0–3 px BELOW the reference, not ~5 px above. The penguin 5 px
is likely a single-image case. Still missing: a second image class
showing rendered glyphs systematically higher than the reference.

### New one-image candidates (held — what validation each lacks)

**C1. Offset normal must be measured against the mask, not the centroid
direction (owl, round 6).** The centroid-direction normal flips into
the material on concave bays (beak bay shortened 2–4 px); measuring the
normal along the candidate direction (sample 2 px, the landing point
must be outside the mask) fixed it (white seam 2205 → 184 px).
Related to N3 (per-edge test) but a different remedy — mask-measured
vs per-edge-tested. Lacks: a second image class with a concave bay
where the mask-measured normal applies.

**C2. `list_atoms` hides non-stroke atoms by default (tiger, round 6).**
`count=0/total=0` while shapes are on disk — the default hides
non-stroke atoms (hidden_non_stroke); judge by dirty_bbox / the
exported file instead. Lacks: a second image class hitting the same
misread.

**C3. A thick stroke whose measured end width is <80% of the line width
should be drawn as a fill, not a stroke (butterfly, round 6).** A round
cap on a tapering end balloons into a ball (overreach 225 px). Lacks: a
second image class with a tapering stroke end.

**C4. Corner-split threshold by feature size (butterfly, round 6).** A
40° split threshold cuts small features' (<25 px) round corners into
flat tops; 60° kept them round. Lacks: a second image class with small
round-cornered features.

**C5. Least-squares cubic fitting needs an ill-conditioning guard; a
failed fit must split, never fall back to a long chord (gt3, round 6).**
A least-squares cubic with clustered sample t exploded (control points
40+ px outside the bbox, mean|d| 185.5) — assert control points inside
the run bbox expanded ~5 px, else split/re-fit; a failed short run
falling back to a whole-run chord drew a diagonal across the shape.
Lacks: a second image class with a clustered-t cubic or a failed-run
fallback.

**C6. Penguin round-6 gaps (penguin, round 6).** (a) Small-component
classification needs a "has hole" test — a dash was misread as a
bubble ring. (b) Alpha tiers group by name prefix — same-group
different names silently fail to group and harden into bands. (c) A
linear gradient does not fit a dome (3× shows a straight boundary) —
domes take radial gradients. (d) Tier steps need a quantitative rule
(experience: per-tier folded colour difference ≲4 units reads smooth).
(e) Outline fitting needs a default outset (this round: dark outline
+1.5 px, white body +1.2 px to close seams). Lacks: a second image
class for each of (a)–(e).

**C7. Outset at a sharp corner is amplified by the miter — cap it
(wordmark, round 6).** A +0.5 px outset ballooned to ~1.5 px at sharp
corners (miter); +0.2 px + miter cap 1.2 delivered. Extends B11 (which
says which direction to outset, not the miter cap). Lacks: a second
image class with a sharp-cornered outset.

**C8. S-curve fitter selection: chord-length parametrization and y(x)
polynomials both fail; iterative nearest-point re-parametrization
converges (gt1, round 6).** Both naive fitters left ~2–2.5 px
residual; iterative re-parametrization converged. Lacks: a second
image class with an S-curve needing the same fitter.

**C9. Grey AA halos need a thickness filter; outset must be conditioned
on thickness (tiger, round 6).** Grey AA halos thinner than 1.8 px
were dropped (462 px²); a 1 px outset on a thin rule doubles its
width. Lacks: a second image class with thin grey AA halos.

### Honest notes (v5.5)

- B13's promotion rewrites the rule: the deciding quantity is the
  rendered value step per tier, not the tier count — the v5.3/v5.4
  "≥8–10 tiers" wording only holds for shallow falloffs. A deep
  falloff needs proportionally more tiers; the nominal alpha step is
  amplified ~2× by layering, so the step must be measured on the
  render.
- N5 (timer jump detection / daemon-held T0) is a tool, not a skill
  rule: PARTIALLY VALIDATED in round 6 — 9/9 timers mono=wall,
  resets=0, basis=mono, credibility HIGH; daemon heartbeats every
  30 s, 0 gaps >45 s. But no jump fired this round (no VM restart),
  so cross-restart TOTAL credibility has no production sample — the
  only validation remains the coordinator's simulated-jump self-test.
  Do not claim restart-proof.
- Round-6 quality: 8/9 independent eye PASS (penguin NOT PASS, 3/10
  checklist items — P8/P9/P10 only). The penguin regressed vs round 5
  (5/10): P1 speckle density visibly thinner than the source at 3×,
  P7 blush concentric rings plainly visible at 3×, P4 armrest
  highlights still read as separate ellipses, P5 seat shadow still a
  flat ellipse with a hard edge, P6 chest still a flat icon, P3 feet
  still merged, P2 fish stripes drawn as straight bars (the checklist
  explicitly forbids single-row straight bars). Soft shading did not
  visibly improve despite radial gradients and 7-tier shadows — the
  highlight/shadow shapes themselves are still pasted on, not
  integrated into the form.
- Round-6 time: same-7 103.9' (target ≤91.3' missed by 12.6'; round 5
  91.3'); 9-image total 133.4' (round 5 121.2'). The slowdown is
  concentrated in three images: penguin +12.3' (two v1 bugs, one fix
  loop each), gt1 +5.1' (three measurement-troubleshooting rounds),
  butterfly +2.7' (antenna measurement rework); partly offset by
  github −5.8', owl −3.1', fox −1.5'. Loops over budget: owl 3, gt3 4
  (both misdiagnosis-driven).
- Two interface facts hit by three images each this round and now in
  the body: `draw_shape` needs `layer_id` (wordmark/owl/gt1) and the
  export-path rule (tiger + N7 probe).

---

## Appendix R7 — Cold-start round 7 additions (v5.6, 2026-10-28)

Source: cold-start round 7, 9 images, skill v5.5 draft (746 lines) +
engine 48f3190 release package (with raw). Independent coordinator eye
check: 6/9 pass (tiger, butterfly, penguin NOT PASS; round 6 was 8/9).
Same-7-images time 70.9' (target ≤91.3' met by 20.4'; round 6
103.9', round 5 91.3'); 9-image total 103.7' (round 6 133.4').
Penguin 7/10 checklist items (round 6: 3/10) — P5 seat shadow flipped
FAIL→PASS; P1 speckle, P4 armrest highlight, P6 chest still FAIL.
Full record: REPORT.md in the round-7 work directory.

Generalization gate (unchanged): a rule counts only after verification
on ≥2 image classes; rule text contains no image name, part name, or
colour value. Evidence classes below are provenance, not rule text.

### Revised in the body (1)

| 编号 | 改动 | 佐证图类 |
|---|---|---|
| N1 (revised) | Half-pixel offset is chain-dependent, not universal: verify with a rectangle probe before applying any uniform shift; outset is along the outward normal, sized by contour sampling (binary-mask ~0.5 px; 50%-coverage ~+0.5 px; +0.2 px insufficient for 50% contours); a uniform +0.5 translation is not an outset (top/left edges move inward) | github (round 7: binary-mask contour insets ~0.5 px, +0.5 shift insufficient and inward on top/left) + wordmark (round 7: 50% contour still needs ~+0.5 px outset) + gt3 (round 7: rectangle probe — fill path renders exactly on draw coordinates, no −0.5 shift; removing +0.5 moved every edge ~0.5 px closer, letter IoU 0.956/0.941→0.974/0.965) |

### New one-image candidates (held — what validation each lacks)

**G2. Corner-detection threshold vs run-split threshold (github,
round 7).** A 50° corner threshold rounds true tips — tension with
B14's ~50° run-split threshold; the skill should distinguish the
"smooth-segment run-split threshold" from the "corner-detection
threshold". Lacks: a second image class with true tips rounded by
the same threshold.

**G3. Dense anchors only remove black overshoot, not white seams
(github, round 7).** Adding anchors when a white seam appears is the
wrong direction — white seams need an outset, not anchors. Lacks: a
second image class with the same misdiagnosis.

**W2. "Overshoot" must be read stratified by source grey level
(wordmark, round 7).** Pure white vs anti-aliasing band — otherwise
threshold artifacts count as errors. Lacks: a second image class
with the same stratification need.

**W3. Ground-truth pairing scale needs a 1-D search, not a bbox
two-axis mean (wordmark, round 7).** Lacks: a second image class
with the same pairing-scale error.

**F1. Greedy 8-neighbour boundary walker breaks chains at
concavities and silently closes (fox, round 7).** Use
marching-squares (or an equivalent closure-guaranteed method) and
self-check "ring covers all boundary pixels". Lacks: a second image
class with a walker break at a concavity.

**F2. Polygon outset miter cap should be ≤1.3×d, not 2.5×d (fox,
round 7).** 2.5× creates ~1.2 px bumps on smooth-curve vertices,
reading as facets at 3×. Lacks: a second image class with the same
miter-bump facets.

**F3. Outset amount must state which contour sampling it pairs with
(fox, round 7).** Pixel edge vs pixel centre — 0.8 px suffices when
the marching-squares contour is already on pixel edges. Lacks: a
second image class with the same sampling/outset mismatch.

**T1. Grey-fill extra-side cause unlocated (tiger, round 7).** The
engine render has ~1,700 px more extra than local rasterization of
the same polygon, suggesting the engine rasterizer fattens fills
~0.5 px; 1 px erosion and 0.5 px eps are both the wrong scale for
that amount. Body stripes systematically too thin at 3× — the
round's tiger FAIL. Lacks: a second image class with the same
engine-vs-local extra, and a located root cause.

**T2. Catmull-Rom vs polygon makes no difference in engine rendering
(tiger, round 7).** CR's value is browser-side in the exported SVG,
not engine-PNG-side; judging CR by engine PNG alone misjudges it as
useless. Lacks: a second image class with the same misjudgment.

**T3. Fit diagnostics should rasterize parts locally first, then
drive the engine (tiger, round 7).** A local-raster probe located
the cause in 30 s; a full-drive probe could not distinguish. Lacks:
a second image class with the same diagnostic-order effect.

**GT1-obs. When converting arcs to cubics, normalize start/end
angles into the same monotone interval first (gt1, round 7).** Never
subtract 360° again from an end angle already in (−180°,180°] — the
crescent inner-arc end angle over-subtracted 360°, the arc wrapped
1.5 turns, gold overflowed. Lacks: a second image class with the
same angle-normalization error.

**GT3-2. Engine anti-aliasing is non-linear at 50% coverage (gt3,
round 7).** 50% coverage reads as alpha ~0.33 in the R channel, so
threshold-mask IoU systematically underestimates the engine render
(stem mask IoU 0.50 while the 3× eye check matches the source) —
mask comparison of engine renders should be coverage-aware. Lacks:
a second image class with the same AA non-linearity.

**GT3-3. `np.convolve(mode="same")` zero-padding tails (gt3, round
7).** End-segment fit residual 30–44 px, edge-padding brings it to
≤0.94 px. Lacks: a second image class with the same convolution
tail.

**GT3-4. Contour offset should be set per point by boundary-pixel
coverage, between index and index+0.5, not a uniform +0.5 (gt3,
round 7).** Same direction as the N1 revision; held separately
because the per-point form is not yet validated. Lacks: a second
image class with the same per-point offset need.

**GT3-5. N2 flat-cap stroke-as-fill-band reproduced on a second
image class (gt3, round 7).** The wave lines reproduce N2's method
— supporting evidence for the already-promoted N2, not a new rule.

**O1. Fitted-contour inset alert line: area smaller than the mask by
>3% means inset is suspected (owl, round 7).** Standard remedy is
lower eps + buffer∩ink (measured: body −5.39%, head-cap −8.25%,
beak-bottom −11.93%, overreach=0; white seam 3283→133 px). Lacks: a
second image class with the same >3% inset signature.

**O2. Nearest-class palette census must take all flat colours (owl,
round 7).** A top-7 exact-colour census missed one colour (2432 px)
and mislabelled a part; census must take the full flat-colour set.
Lacks: a second image class with the same census omission.

**O3. Ink polygons may be invalid — wash validity with `buffer(0)`
before buffer∩ink (owl, round 7).** Lacks: a second image class
with an invalid ink polygon.

**B1. Hairpin tips need a semicircular-tip fill band, not CR (butterfly,
round 7).** Bridge criterion (endpoint distance <30 px, both ends
near the same surface). The antenna tip was flattened by a CR
hairpin cut in cairo; the semicircular tip fixed it — but the head
V valley was still flattened ~2–3 px by the bridge chord, a
possible side effect of the same method. Lacks: a second image
class with a hairpin tip fixed the same way.

**B2. Stroke ends: engine round caps vs exported flat caps (butterfly,
round 7).** End lines should be drawn as fill bands (the dual-
renderer end trap cost a fix loop). Lacks: a second image class
with the same end-cap divergence.

**B3. Skeleton branch walking must include a junction-to-junction
stage (butterfly, round 7).** v1 dropped the stage and lost 2 main
veins. Lacks: a second image class with the same dropped stage.

**B4. Outline centreline should offset the simplified polygon, not
the raw contour (butterfly, round 7).** Offsetting the raw contour
made waves. Lacks: a second image class with the same offset-source
error.

**B5. Stroke width must be measured on the delivery renderer
(butterfly, round 7).** A cairosvg probe disproved "only an engine
rendering difference" — the delivered veins themselves were narrow
(20–21 vs source 24–25). Lacks: a second image class with the same
renderer-attribution error.

**B-FAIL-1. Vein-end shape: round-knob ends where the source has
diagonal flat cuts into the outline (butterfly, round 7).** Four
zones (z2/z3/z4/z6) show round-knob vein ends; fill-band end
closure must follow the source's diagonal cut angle, not a round
cap — B2's fill-band method does not cover end-closure shape.
Lacks: a second image class with the same end-closure mismatch.

**B-FAIL-2. Antenna mid-shaft kink (butterfly, round 7).** The left
antenna bends mid-shaft in the render (source is a straight rod);
centreline fitting/simplification must preserve straight-segment
straightness. Lacks: a second image class with the same kink.

**P-1..P-9. Penguin round-7 gaps (penguin, round 7).** (1) Mask
threshold upper bound must cover the brightest target colour in the
region (G<185 cut light-green highlight G 188–196 and made black
wedges; G<210 restored coverage 55.6%→84%). (2) Speckle positions
must be verified against the Chromium-rendered ink, not the
reference mask (92/220 points fell below the rendered glyph); density
must match the source grain coverage (~3.6%), taking only large
grains leaves half the density. (3) A centred dome highlight cannot
use a linear gradient — radial gradient only (but the radial
gradient still left a hard-edged triangular highlight this round —
the method is not yet complete). (4) Contour masks need a distance
gate (10,598/19,900 dark pixels were seat/armrest shadow pollution
>12 px from the body). (5) Per-row B-channel dips recover weak
stripes a fixed dark threshold misses (fish stripes vs background
differ by only ~15 in B). (6) Display type needs per-letter
geometric-mean calibration (whole-word calibration came out 21%
short). (7) A small 3D item (chest) needs arch curvature + side
light/dark gradient + band wrap thickness together — flat-colour
stacking alone still reads flat. (8) Cap highlight shapes must be
eroded (2 px) before tracing or the top corner spikes past the
armrest edge. (9) A shadow's "softness" needs an alpha gradient, not
a low-alpha solid (radial alpha 0.35→0 leaves no readable boundary)
— this is the mechanism behind the round's P5 flip. Lacks: a second
image class for each of (1)–(9); the penguin is the suite's only
soft-shading image, so (3) and (9) need soft-shading material
outside the suite for a second class.

### Honest notes (v5.6)

- Round-7 quality regressed vs round 6 (independent 6/9 vs 8/9):
  tiger and butterfly passed in round 6 and failed in round 7
  (tiger body stripes systematically too thin; butterfly vein ends
  systematically round-knobbed, antenna kinked, head V valley
  flattened). Speed improved decisively (same-7 70.9' vs 103.9'),
  but the speed gain carries a quality cost until T1 and B-FAIL-1
  have located root causes — do not read the time alone as progress.
- Penguin 7/10 (round 6: 3/10): P5 seat shadow flipped FAIL→PASS
  (radial alpha gradient, P-9); P1 speckle density still ~half the
  source; P4 armrest highlight is a hard-edged pale triangle, not an
  ellipse — the round-7 pass bar's ellipse test passes but the
  "highlight must blend into the gradient" requirement does not, so
  P4 stays FAIL and the next round's bar must read "no independent
  closed shape of any outline (ellipse/triangle/polygon), edge must
  be a gradient fade, not a hard line"; P6 chest still a flat icon.
- N5 (timer jump detection) still has no production restart sample
  (round 7: 9/9 timers mono=wall, resets=0, credibility HIGH) — do
  not claim restart-proof.
- Round-7 time: same-7 70.9' (target ≤91.3' met); 9-image total
  103.7'. Slowest image is the penguin (30.5', target ≤30' missed
  by 0.5' — final eye check runs past the render). Loops over
  budget: tiger 3, butterfly 3 (each 1 over).
- Two self-check overturns this round (butterfly self-PASS → independent
  NOT PASS; penguin self-8/10 → independent 7/10 on P4) — the same
  lesson as rounds 4–6: the executing agent's self-check is not the
  verdict.

## Appendix R8 — Cold-start round 8 additions (v5.7, 2026-10-28)

Source: cold-start round 8, 9 original images + 2 supplementary
(seal, logo), skill v5.6 draft (966 lines) + engine 48f3190 release
package (with raw). Independent coordinator eye check: 9/9 pass
(round 7 was 6/9; round 6 was 8/9) — best quality in 8 rounds.
Same-7-images time 96.4' (target ≤91.3' MISSED by 5.1'; round 7
70.9', round 6 103.9', round 5 91.3'); 9-image total 144.4'
(round 7 103.7'). Penguin 9/10 checklist items (round 7: 7/10;
round 6: 3/10) — P1 speckle and P4 armrest highlight flipped
FAIL→PASS; only P6 chest still FAIL. Full record: REPORT.md in the
round-8 work directory. v5.6 verdict: NOT shippable (speed gate
missed); this draft is the round-9 candidate.

Generalization gate (unchanged): a rule counts only after verification
on ≥2 image classes; rule text contains no image name, part name, or
colour value. Evidence classes below are provenance, not rule text.

### Promoted to the body (2)

| 编号 | 改动 | 佐证图类 |
|---|---|---|
| P-A (draw order) | Order by geometric containment, not by a contour-tree depth: a two-level hierarchy (RETR_CCOMP) does not express cross-part containment; recompute levels by geometric containment (or use RETR_TREE and verify) | tiger (round 8: 26 small black parts inside the giant gray hole drawn in the wrong order, −4,996 px; draw-order bug found after T1 falsification) + logo (round 8: a white hole covered a black island inside it when ordered by RETR_CCOMP depth; fixed by geometric-containment majority vote) |
| P-B (verdict view) | Dark-pixel counts on the engine PNG are not a geometry verdict: judge geometry on a reference-rasterizer (cairosvg) render; engine PNG only for visible defects; an engine dark-pixel deficit/excess alone never fails a part | tiger (round 8: GT's own smooth path loses 4.8% black in the engine; integer rect 0 error) + gt1 (round 8: 6 px thin band +5.1% exact dark px in engine vs +0.4% in cairosvg) + seal (round 8: same geometry missing 799 in cairosvg vs 2,489 in engine under <128) + wordmark (round 8: engine dark px ~9% fewer than cairosvg under <128; totals agree under coverage-aware thresholds) |

### Re-confirmed in the body (no change)

- **Outset normal must be tested per edge, inside/outside** (Phase 3,
  already in the body): fox (round 8 — centroid-direction normal
  flips on concave edges; mask-measured normals fix it) + owl
  (round 8 — U-frame inner edge shortened with centroid normal,
  area 26,400 vs 32,000 with per-edge tested normal). Second-class
  confirmation of an existing rule.
- **N1 revised (rectangle probe first)** (Phase 3, already in the
  body): seal (round 8 — revised N1 walked through) + github/gt3
  (round 8 — rectangle probe re-confirmed, no shift on this build).

### New one-image candidates (held — what validation each lacks)

**G8-1. Edge-touching silhouettes break contour tracing (github,
round 8).** A silhouette touching the canvas edge yields 2 rings
from marching-squares, one self-intersecting and discarded
(area −15%); pad one ring of background colour first, then trace.
Lacks: a second image class with an edge-touching silhouette.

**F8-2. Shared-edge white-seam diagnosis order (fox, round 8).**
Check outset vertex depth first (cheap), then adjust outset amount.
Lacks: a second image class with the same seam/diagnosis pair.

**W8-2. Logo background-rect removal step is missing from the
checklist (wordmark, round 8).** The engine's white-background
document exports a white background rect; Logo Variant requires a
transparent background, but the Deliverables Checklist does not say
"remove the background rect and re-render to verify". Lacks: a
second image class needing the same step.

**T8-2. The 0.5 px mask-contour rim is bidirectional (tiger,
round 8).** Black outer ring +0.5, gray hole −0.5 — give both as a
pair (an instance of revised N1). Lacks: a second image class with
the same paired rim.

**T8-5. Net-area accounting must use the actual draw order (tiger,
round 8).** Outer-union minus hole-union, not outer area minus hole
area — the latter overstates black when a hole covers small parts
(this round: 4,996 px). Lacks: a second image class with the same
accounting error.

**GT1-1. Chord-length-t cubic fitting is a fixed-point trap (gt1,
round 8).** On an S-curve, chord-length t and Bézier t differ by up
to 0.03; a single cubic fits with maxerr 1.2–2.2 px on exact GT data,
and iterative closest-point reparameterization cannot escape (the
alternating iteration's fixed point). Remedy: segment the fit, and
fit offsets with known sampling parameters. Tension with R6 C8
("iterative reparameterization converges") — C8's convergence
probably depends on a different seed/curve. Lacks: a second image
class with the same trap.

**GT1-2. Coverage fields need a residual gate as a general
precondition (gt1, round 8).** An ungated straight projection read
red pixels as gold coverage 1.0 and destroyed every fit (circle
centre off by 50 px); a residual ≤30 gate converged all parts to
0.05 px rms. N6 states residual gating only in the nearest-class
labelling context; this candidate generalizes it to coverage-field
projection. Lacks: a second image class with the same ungated
projection failure.

**GT1-3. Exact-colour pixel counts cannot judge ~1% differences
(gt1, round 8).** A 0.03 px edge shift moved exact green pixels by
−49 (−1.0%) on a 45° edge through whole-corner coverage loss.
Lacks: a second image class with the same threshold sensitivity.

**GT3-8-1. approx→ring index mapping must not be sorted (gt3,
round 8).** `sorted(set(idx))` scrambles the approx-vertex to ring
index correspondence, so a corner looks up the wrong ring index and
a span crosses the figure. Lacks: a second image class with the
same mapping error.

**GT3-8-2. Extrema detection must handle quantized flat tops
(gt3, round 8).** `dy1*dy2<0` never fires on a quantized centreline
+ smoothed flat top (derivative 0 on both sides of the peak), so a
whole wave keeps 1 span. Forward-fill zeros in the derivative-sign
sequence before taking sign changes. Lacks: a second image class
with the same flat-top loss.

**GT3-8-3. Edged cubics must not be refit through 3 points (gt3,
round 8).** Refitting through 3 offset points (endpoints fixed +
1 interior point) is underdetermined; the minimum-norm solution
straightens the curve. Offset the centreline cubic's control
points directly (per-parameter-point normals). Lacks: a second
image class with the same refit straightening.

**GT3-8-4. A reversed edge must swap its control points (gt3,
round 8).** Walking a band edge backwards, `C p1,p2,p3` draws a
self-loop (start = end); write `C p2,p1,p0`. Lacks: a second image
class with the same reversal.

**GT3-8-5. fit_span's chord fallback needs a length cap (gt3,
round 8).** Falling back to the whole-segment chord on a failed
fit draws a diagonal across the figure for long chords; chords
>30 px must keep splitting instead of falling back. Lacks: a
second image class with the same diagonal fallback.

**O8-1. Steep-edge nearest-class masks pull AA pixels into the
part (owl, round 8).** On near-vertical edges, AA pixels are near
equidistant from both colours, so noise decides ownership, and a
+0.5 outset then reaches ~1.5–2 px outward. Take steep-edge
contours from the 50%-coverage marching-squares field, not from a
nearest-class binary mask (or shrink the outset by edge direction).
Lacks: a second image class with the same steep-edge pull.

**O8-2. E1/E2 scan probes read row averages on steep edges (owl,
round 8).** Phase 1 already states this; this round's probes
tripped on it twice anyway — the "judge steep edges by path
geometry / row centres, not scan edges" rule belongs in the probe
procedure, not only in the Phase 1 body. Lacks: a second image
class with the same probe misread.

**O8-3. Uniform vs centripetal CR made no difference on this
image (owl, round 8).** Negative observation: with uneven anchor
spacing, bowing is not necessarily a parameterization problem;
densifying anchors was the effective fix here. Not a counter-example
to the centripetal rule, only a bound on its scope. Lacks: a
second image class with the same no-difference result.

**B8-1. Marching-squares quadrant contours are open paths (butterfly,
round 8).** Closing with a straight chord slices inner corners
(v2: 14.2k missing px). Close along the cut edges via the shared
corner. Lacks: a second image class with the same open-contour cut.

**B8-2. A stroked outline cannot cover merged-outline interiors
(butterfly, round 8).** A 22-wide stroke on an EDT-10.5 centreline
cannot cover the bar interior and rounds sharp concave gap tips;
a band fill (silhouette minus measured inner edge) covers the full
band exactly. Lacks: a second image class with the same
stroke-coverage failure.

**B8-3. The outline band is not uniform width (butterfly,
round 8).** Measured 21.1 px (top), 28.3 px (lower sides), 23.5 px
(bar). A fixed EDT-N ring cannot represent it; the fill's edge must
be the measured (light|vein) boundary. Lacks: a second image class
with the same non-uniform band.

**B8-4. Duplicate consecutive ring points produce NaN outset
normals (butterfly, round 8).** Dedupe + NaN-guard + segment-count /
area assertions. Lacks: a second image class with the same NaN
failure.

**B8-5. Catmull-Rom overshoots short straight edges (butterfly,
round 8).** With corner neighbours 20–40 px away, a vein-end edge
bowed 6–8 px into a round knob. Subdivide simplified edges to ≤8 px
before CR. Lacks: a second image class with the same CR overshoot.

**B8-6. Body profile needs clamp ranges at outline-join
inflations (butterfly, round 8).** Raw runs at the join zones are
outline bands, not body. Image-specific geometry; likely stays in
the appendix. Lacks: a second image class with the same clamp need.

**B8-7. Antenna tip deviation is cap-circle geometry, not a bend
(butterfly, round 8).** Mid-shaft residuals ≤0.24 px prove
straightness; the +4.5 px tip residual is the round cap. Lacks: a
second image class with the same tip/cap confusion.

**B8-8. The head-top "V valley" is a flat plateau (butterfly,
round 8).** y=175 flat between the antenna bases in the source, not
a dip; do not bridge it. Image-specific; likely stays in the
appendix.

**B8-9. The engine stroke renders ~2 px narrower than nominal
(butterfly, round 8).** Outer edge lands ~1–2 px inside the
silhouette edge at nominal 22. Lacks: a second image class with the
same stroke-narrowing measurement.

**B8-10. Binary-mask contours inset ~0.5 px (butterfly, round 8).**
N1 confirmation by measurement, not theory (underlay outset +0.5
fits the silhouette; outset 0 leaves 1,606 missing px). Not a new
rule — supporting evidence for revised N1.

**P8-1. Text-ink verification must use the Chromium render, not
the engine probe (penguin, round 8).** The engine probe's font
positions differ from Chromium's subset-font render by up to ~10 px
at letter bottoms (glyph-shape difference, not an offset). Speckle
positions must be re-sampled inside the rendered ink. Lacks: a
second image class with text speckles on a rendered font.

**P8-2. A speckle census must exclude non-letter parts in the ROI
(penguin, round 8).** The fish inside the text ROI polluted the
first census (mostly fish pixels). Exclude by colour class / region
first. Lacks: a second image class with the same ROI pollution.

**P8-3. CR bulges outward over closed contours (penguin,
round 8).** Sofa bottom edge bulged 16 px, domes bulged, button ends
spiked — replace rounded ends with hand-built cubic corners; match
assembly bboxes against exported measured bboxes. Lacks: a second
image class with the same closed-contour CR bulge.

**P8-4. The small-3D chest method is incomplete (penguin,
round 8).** Arch curvature + side gradient + corner posts still read
flat at 66×64 px — the skill has no operable spec for "gold-band
wrap-around thickness" (how many px of wrap, how many px of side
depth read as 3D). This is the root cause of P6 still failing.
Lacks: a second image class with a small 3D object.

**S8-1. CR cubics bow on long straight edges (seal, round 8).**
Centripetal CR inherits the corner arc's tangent direction, so a
300+ px straight edge bows (outer top edge control point y=−0.76,
row 0 all black; inner bottom bow cost 321 px at row 588). Emit a
straight segment when the edge is >40 px and the raw contour points
deviate from the chord by <1.0 px; judge by point-to-chord
deviation, not vertex turning angle (transition vertices turn up
to ~10°). Lacks: a second image class with the same long-edge bow.

**S8-3. Canvas-clipped frames have no ready-made procedure (seal,
round 8).** The outer contour breaks at the canvas boundary
(marching-squares emits no contour segment on the canvas edge);
close with an open chain + a straight canvas-edge segment. Lacks: a
second image class with a canvas-clipped frame.

**L8-1. A white-hole polygon eats thin lines inside the hole
(logo, round 8).** When a white hole encloses a 2 px line, the
simplified white-hole polygon drops the narrow slit around the line
and cuts across it, covering the line at draw time. Measure thin
lines from the source mask and draw them after the white hole.
Lacks: a second image class with a line through a hole.

**L8-3. The hole-direction rule can be stated by area (logo,
round 8).** Black/colour fills take the side where the area grows,
white holes the side where it shrinks — no winding dependence.
Lacks: a second image class needing the same area test.

### Honest notes (v5.7)

- **T1 is falsified, not fixed (tiger, round 8).** The round-7
  hypothesis "the engine fattens fills by ~0.5 px" does not hold:
  an integer-aligned rectangle and a donut render identically in
  the engine, locally, and geometrically (0 px error); true stripes
  read −3% thin in the engine. The round-7 ~1,700 px extra was a
  local-comparison underestimate or a construction issue, not
  engine fattening. What tiger actually fixed was draw order
  (P-A) — the 26 small black parts inside the giant gray hole.
- **The T1 second-class signal contradicts the falsification in
  direction, not in kind (gt1, round 8).** A 6 px thin band reads
  +5.1% dark in the engine vs +0.4% in cairosvg, while large fills
  read −4.8% in the engine. Both are engine AA non-linearity;
  the size dependence of the sign is unlocated. Recorded as an
  unresolved tension, not a rule.
- **Three CR observations, three different fixes (seal +
  butterfly + penguin, round 8).** Long straight edges bow
  (straight segment), short straight edges bulge (subdivide ≤8 px),
  closed contours bulge outward (hand-built cubic corners). They
  do not yet combine into one general CR rule; each stays in the
  appendix pending a second class.
- **Round-8 timing honesty:** same-7 96.4' missed the ≤91.3'
  hold by 5.1'; the miss is real diagnosis + loop work, not the
  ~2-minute VM restart window. P-A and P-B are exactly the
  diagnoses that cost the time this round — round 9 tests whether
  the skill now delivers the same quality faster.

## Appendix R9 — Cold-start round 9 additions (v5.8, 2026-10-28)

Round 9: same-7 86.5' (target ≤91.3' met), independent 8/9, penguin 7/10 (glance PASS; P1/P2/P6 FAIL). v5.7 packageable by the pre-set threshold (penguin via the 2-of-P4/P5/P6 branch). Two promotions to main text (see version header): W8-2 and the glance-gate sentence. Everything below is held — single-image-type evidence, needs a second image class.

**T9-1. Degenerate filament fallback (tiger).** 10–22 px, 1–2 px wide filament components vanish silently through approxPolyDP + Polygon + buffer (zero area). Candidate: assert/log any part whose simplified area is <1, or fall back to the mask bbox as a small rectangle. This round lost 234 px with no visible effect. Lacks: a second image class with the same silent loss.

**T9-2. Large-part eps lower bound (tiger).** Large-part eps 2.5→1.2 removed 3× faceting with no pixel steps (mean|d| improved in step). The skill's "large parts eps 2.0–3.5" band is too coarse for heraldic line art. Candidate: default large-part eps to ~1.2, or "when 3× shows faceting, lower eps before anything else". Lacks: a second image class.

**F9-1. Hole-ring outset sign (fox).** Phase 3 says "the normal must be tested against the mask" but does not cover hole rings: under a mask test, a hole ring's "outward" direction already points toward the hole center (the hole is not in the mask), so calling outward=True on both rings shrinks the hole and fattens the ink; flipping it to False (the intuitive "make the hole smaller") enlarges the hole ~1 px and leaves a white ring around features inside the hole (eyes). Candidate: add "under the mask test, hole rings do not flip sign" next to the Phase 3 hole rule. Lacks: a second image class with a hole-ring outset pair.

**F9-2. Near-class AA column mislabeled as a third class (fox).** A 1 px wide, 27 px tall AA column at an eye edge (face-orange + dark blend, (150,98,40)) was classified as inner-ear red and became a 1×27 ghost component. Drawing every component would paint a red line. This round avoided it with "large components only". Candidate: beyond the B1 area filter, add a width/solidity test for <2 px wide elongated components before trusting the census. Lacks: a second image class.

**G9-1. The 0.4 px straight-edge rim is a threshold-mask floor (github).** Right-tentacle vertical edge and top-left outer arc sit ~0.4 px outside the source 50% edge; the <128 threshold turns that into a "1 px whole-column overreach" when a pixel center lands on the edge. Do not chase it — it needs per-edge coverage calibration. Lacks: a second image class with the same floor error.

**GT1-9. Residual gating generalizes (gt1).** GT1-2 (residual gate before coverage projection) is not only for nearest-class labeling: this round, ungated dark sampling read the blue blob into the column sum and rejected a whole segment; gold did the same. Candidate: Phase 1 coverage rules state "any projection onto a color axis must pass the residual gate first, or off-color pixels mix in by their projected component". Lacks: a second image class.

**GT1-10. Band construction reverse-edge start (gt1).** After the top edge, the first segment of the reverse bottom edge C starts at the implicit current point (the top edge's end), not at the bottom edge's end — the end cap becomes a cube and the last bottom segment is replaced. Candidate: Phase 3 band construction states "before the reverse edge, explicitly M to the bottom edge's end, or use a straight end cap", plus a self-check (band outline first/last segment check). Lacks: a second image class.

**GT3-9. Corner/arc vertex criterion (gt3).** Turn angle alone misjudges: the B counter's right-end vertex (134°) was called a corner and drawn as an L joint, giving a pointed wedge — it is really a continuous arc's vertex. The first fix ("same-direction neighbors = arc vertex") overcorrected — every convex vertex of a closed convex ring turns the same way, so B's right angle (80,300) was also called an arc vertex (v2: 37 C / 0 L; CR control points landed on straight lines so the render was unchanged, v1↔v2 B-zone render difference 0 px). The usable combination is "turn angle + neighbor length/concavity sign": a corner needs turn >40° AND (opposite sign to the ring's dominant sign, i.e. a notch; or shortest neighbor >~20 px, i.e. two straight edges meeting). Validated both ways on gt3 (counter arc smoothed, waist notch and left right-angle stayed sharp). Lacks: a second image class.

**GT3-10. Tight-arc approxPolyDP starvation (gt3).** The B counter's tight arc at eps=1.0 kept 4 vertices; CR interpolation drew a chamfered shoulder. Using raw-contour anchors every ~4 px restored the round nose. Candidate: rings with <10 vertices do not use approxPolyDP vertices — use thinned raw-contour anchors (straight runs still handled by collinear dropping). Lacks: a second image class.

**GT3-11. Single CR through all midline samples (gt3).** Splitting a wavy midline at y-extrema and fitting an independent least-squares cubic per segment leaves tangent discontinuities — a kink at squiggle2's peak. One centripetal CR through all midline samples (no segmentation) removed the kink. Candidate: wavy midlines prefer one CR through all samples, not extremum-segmented fits. Lacks: a second image class.

**O9-1. Densify before the straight test (owl).** When a ring_to_d-style ring is sparse after simplify/buffer, the straight test defaults a dense segment with <3 points to straight and draws an arc as a polygon (this round: eye disc 37 L). Candidate: densify the ring to ≤2 px spacing before emitting the path, or require ≥3 points in a dense segment before calling it straight, else call it curved. Mechanism located; single image (owl). Lacks: a second image class.

**O9-2. Straight-test threshold dev<0.5 & len>40 (owl).** Tightening from dev<0.8 & len>25 removed belly/foot faceting, but the 0.5/40 pair is validated on owl only. Candidate: single-image empirical values. Lacks: a second image class.

**O9-3. Cusp pinning (owl).** Vertices with turn angle >35° pull the CR control point back to the vertex — saved the inter-eye V and the beak tip. Single-image candidate. Lacks: a second image class.

**B9-1. Shared-edge single-side boundary + solid underlay (butterfly).** For a "patch nested in a lower layer's bay" structure (light-blue field nested in the dark-blue wing's open bay), drawing the lower layer solid (union-contour) beats "each side outsets": v1's independent simplification left a 242 px white seam; uniform outset closed the seam but thinned the dark blue −9.9%; solid underlay + upper-side-only boundary gave seam 0 px and dark blue +2.7%. The skill only says "adjacent fills overlap 0.5–1 px" and does not distinguish "hole" from "open bay" (RETR_CCOMP holes cover fully enclosed regions only; an open bay's shared edge exists in both masks, and simplification mismatch is a white seam). Lesson: the first "remove the hole" implementation was a no-op — verify the candidate hole is really a closed hole before changing anything. Lacks: a second image class with the same patch-in-bay structure.

**S9-1. Corner threshold by smallest smooth curve (seal).** A fixed 28–30° threshold cuts small circles into polygons: a small-radius circle's per-vertex turn can reach 40–45° after simplification (eye hole 12 vertices all 28–41°, frame corner 40–50°). This round a 50° threshold fixed it while true sharp corners (≥50°) stayed. Candidate: the run-split turn threshold must exceed the largest per-vertex turn of the smallest smooth curve in the image; or size档 (features <~60 px use 50°). Lacks: a second image class.

**S9-2. CR end-tangent phantom mirror (seal).** If a CR run's end neighbor is the far endpoint of a 400 px+ straight edge, the tangent is pulled toward the straight edge and flattens the arc's end (half the cause of v1's flat-cut frame inner corner). A mirrored phantom (extend the endpoint along the run's last chord direction) restored the smooth joint. Candidate: a CR run's end neighbor is a mirror extension, not the adjacent ring vertex (unless that vertex is within ~1 chord length). Lacks: a second image class.

**L9-1. White-hole draw order by mask hierarchy tree (logo; P-A supplement).** A white hole and the black piece inside it have the same centroid-containment count (white hole contained by the black frame; black piece contained by black frame + white hole — and the white hole's centroid happens to fall inside the black piece's polygon), so naive counting puts them on the same layer and black-first lets the white hole cover the black piece. This round used the mask hierarchy tree (hole's parent +1, black piece inside a white hole +2). Candidate: add one line under P-A — "a white hole's covered pieces take their level from the mask hierarchy tree, not from centroid containment counting". Lacks: a second image class.

**L9-2. Small circles fit directly (logo).** Small circles (≤~12 px) must not use approxPolyDP polygons: eps 0.5 still cuts extremal points, renders polygonal at 3×, and reads 1 px short. Least-squares circle fit + 4-segment Bézier lands in one pass. Candidate: add to Logo Variant or Phase 3 — near-circular small parts (aspect ~1, small boundary-radius variance) fit a circle directly. Lacks: a second image class.

**P9-1. contour_to_d wraparound edge (penguin).** The old implementation handled wraparound only on the last edge. When a ring's index-0 point falls in the middle of the vertex list, that edge is skipped, the path cuts across the shape as a chord, and even-odd self-intersection cancels it to empty. v1's sofa base, white body, dark head, and button all failed to render. Candidate: take chains circularly + forward-search mapping, so any edge of a ring may cross index-0. Single-image (penguin) mechanism, located exactly. Lacks: a second image class.

**P9-2. Speckle census only inside letters (penguin).** A speckle verdict must look only at letter interiors (≥2 px from the edge). Dark pixels within 1–2 px of the edge are AA bands, not speckles. (Round 9's agent measured flat interiors correctly but inverted the verdict — flat is FAIL per §0; the census rule itself is sound.) Lacks: a second image class with letter texture.

**P9-3. Gradient ellipses clipped to the part outline (penguin).** Highlight/shadow ellipses that are not clipped read as balls on a black background. Candidate: every gradient ellipse is clipped to its part's outline (clipPath). Lacks: a second image class.

**P9-4. Small-chest gold-band wrap method (penguin).** Arch band as a stroke. Corner posts drawn after the bands to break them. Gold band gets a light/dark edge. Gem gets a gold seat. Side gets a wood dark strip. Single-image method. Lacks: a second image class — and the round-9 chest still failed P6, so this method is unproven even on its home image; do not promote on penguin alone.

**P9-5. Foot component classification by position (penguin).** A y-threshold alone misclassified the foot component (y=719) as a second "beak"; the watershed foot-split branch never ran. Candidate: classify components by position, not by a y threshold. Lacks: a second image class.

**P9-6. Fin notch on a body outline (penguin).** The body outline followed the fin's concavity and left a dark notch where the fin meets the belly. Candidate: the body outline takes the outer envelope; the fin is drawn separately on top of the body, or the fin shape is outset 1–2 px to cover the notch. Lacks: a second image class.

### Honest notes (v5.8)

- The penguin P6 chest has failed five rounds (5–9). The small-chest gold-band method (P9-4) did not flip it in round 9. Do not promote P9-4 on penguin alone.
- The P1 grain tension is unresolved: round 8's speckles read as too much noise to Wang's eye; round 9's flat letters fail §0. The target is fine grain at source density — neither round landed it.
- The S-curve −0.56 px systematic offset (gt1) is unresolved after 6 falsifications. Candidate: per-column centroid sub-pixel phase bias on slanted bands. No synthetic control run. Do not burn time on it.
- AA non-linearity changes sign with feature size (gt1 fine band +5.1% vs tiger large fill −4.8%). Root cause unlocated.
- Round 9 ran 11 agents in parallel; VM free memory fell to 329 MB and the batch reported resource_exhausted. Control parallel count next round, or put heavy work on the boxes.

## Appendix R10 — Cold-start round 10 additions (v5.9, 2026-10-28)

Round 10: same-7 108.6' (target ≤91.3' NOT met, +17.3'), independent 6/9 (target ≥8/9 NOT met), penguin 8/10 (main line ≥8/10 met for the first time; P1 flipped, P2 eye missing, P6 still FAIL). v5.8 NOT packageable. One promotion to main text (see version header): inspection-image interpolation consistency (Stop Rule). Everything below is held — single-image-type evidence, needs a second image class.

**F10-1. Small dark-part outset by grade (fox).** A uniform +0.7 px outset was right for the body but too much for the eyes — the eyes needed an analytic stadium rebuild instead. Candidate: outset amount by part size (small dark parts get less, or an analytic rebuild). Lacks: a second image class.

**F10-2. Blur-0.5 iso-contour inset is quantifiable (fox).** Precise-color mask + blur 0.5 iso-contour insets ~0.5–1 px; the default +0.5–0.7 px outset compensates it. Candidate: state the inset number and the compensating outset as a pair. Lacks: a second image class.

**T10-1. Asymmetric outset on a shared black/grey edge (tiger).** Toward a white edge +0.5 px; toward the opposite-color edge, less — a symmetric outset thins the neighbour. Candidate: shared-edge outset is asymmetric by neighbour. Lacks: a second image class, and a probe proving it does not open a white seam.

**T10-2. Grey <3 px component cutoff cannot tell real fine grey cells from AA speckles (tiger).** A size cutoff alone discards both. Candidate: a second discriminator (shape, or neighbourhood) for sub-3 px grey components. Lacks: a second image class.

**T10-3. Engine vs cairosvg black counts have opposite signs (tiger).** Same geometry reads −3.86% in the engine PNG and +1.82% in cairosvg. P-B re-confirmed (cairosvg adjudicates; engine PNG is visible-defect check only). Lacks: a second image class.

**GT1-11. A y(x) cubic polynomial is not a Bézier cubic's y(x) (gt1).** Fitting y(x) cubics leaves a ~±1.2 px systematic ripple floor (this round seg1 residual −1.22/+1.24 px). Candidate: S-curve centrelines need reparameterised Bézier least-squares, or the floor is stated as a limit. Lacks: a second image class; GT1-1 and C8 disagree on whether iterative reparameterisation converges.

**GT1-12. Two-segment fits must align the joint first, then share it (gt1).** The round's located root cause: two polynomials differed 3.2 px at the joint x=330 (seg1 498.4, seg2 501.6); constraining both to the measured centreline y(330)=500.0 and sharing the endpoint fixed the S-curve (per-column dc ≤1.0 px). Candidate: never let one segment's fitted value overwrite the other's at a joint — align, then share. Lacks: a second image class.

**GT1-13. Pre-drive numeric precheck must assert measurement-key hits before thresholds (gt1).** This round a v3 precheck passed empty because a dict key type mismatched. Candidate: assert the keys hit before asserting the gate. Lacks: a second image class.

**GT3-12. CR cuts the shoulder at a flat-top to tight-arc transition (gt3).** Anchor densification (4 px and 2 px) still insets ~0.5 px at the shoulder; a single cubic fit was also falsified. The B upper-counter nose kept a slight point. Candidate: a transition rule for flat-top to tight-arc joints. Lacks: a second image class.

**GT3-13. Least-squares cubic gate must deduct pixel quantisation noise (gt3).** maxdev 0.82 was mostly quantisation steps, not fit error. Candidate: the gate subtracts the quantisation floor first. Lacks: a second image class.

**GT3-14. Nose-fit segment start/end take transition points, not bbox thresholds (gt3).** Bbox thresholds cut the fit segment at the wrong place. Candidate: fit segments start and end at measured transition points. Lacks: a second image class.

**O10-1. Tip measurement by 50% coverage side edges, not threshold-mask extrema (owl).** A nearest-class threshold mask truncated the inter-eye V tip by ~12 px (1–2 px wide tip read as terracotta). The true tip came from 50% coverage side-edge measurement + GT 4x render, located at (312.5, 162.5). Candidate: tips are located by side-edge extension intersection, never by threshold-mask extrema. Lacks: a second image class.

**O10-2. Nearest-class misreads an AA band as a third colour (owl).** A dark-brown to white AA band became a 60 px false component. Area filtering avoided it. Supplementary evidence for B1. Lacks: a second image class.

**O10-3. Occluded parts' IoU/extrema deviations are meaningless (owl).** A dark part's extrema deviation of 213 px was occlusion, not error. Candidate: probes subtract occlusion first, or compare visible edges only. Lacks: a second image class.

**O10-4. Probe <30 colour threshold fails at sharp tips (owl).** The read bottom edge sat ~9 px above the true tip. Candidate: tips need coverage measurement, not a colour threshold. Lacks: a second image class (same lesson as O10-1, same image).

**O10-5. Tip repair by synchronous side-edge extension (owl).** Extend both side edges along measured curves to their intersection; parts sharing an edge must extend synchronously or a white seam opens. Candidate: joint tip repair rule. Lacks: a second image class.

**B10-1. CR vs polygon division has no executable criterion (butterfly).** Wing base is better with CR; dark net and veins are worse with CR (hairpin-tip breakage, concave-joint steps). Candidate: a "when to disable CR" criterion. Lacks: a second image class.

**B10-2. Polygon eps 1.0 3× readability has no quantified criterion (butterfly).** Chord error ≤1 px reads as slight flatness on long arcs at 3×. Candidate: a chord-error to 3×-visibility relation. Lacks: a second image class.

**P10-1. Fish parts must be filtered; hollow rings always excluded (penguin).** Bubble rings were misread as solid fin circles; dorsal and pectoral fins need separate dark-fin-colour mask contours. Candidate: filter parts by "within ≤20 px of the body bbox and top ≤ body bottom +10", drop hollow rings. Lacks: a second image class.

**P10-2. Chest lid top shares its top edge y with the band outer arch (penguin).** The lid top sat 2 px above the band top and read as a "W" notch. Candidate: after drawing, verify the top edge column by column for flushness. Lacks: a second image class.

**P10-3. Grain overflow by strict pixel count, not visual impression (penguin).** Grain overflow reads 0 by strict count; visual impression misreads it. Corroborates the promoted pixel-level-verification rule. Candidate: overflow is a count, not an impression. Lacks: a second image class.

**P10-4. Stripes by stripe-colour pixel count and per-row runs (penguin).** Stripes confirmed by counting stripe-colour pixels (1084 px); stripes follow per-row runs as slanted bands, not rectangles. Candidate: stripe landing is a pixel count, not a rectangle count. Lacks: a second image class.

**P10-5. Adjacent feet overlap 2 px, each outset 1 px (penguin).** Two feet overlapping 2 px with 1 px outset each closed the dark seam. Candidate: foot-overlap rule. Lacks: a second image class.

**P10-6. Fine-grain density criterion, validated on penguin (penguin).** Render grain visible, density 0.5–2× of source (this round 1.01×), typical size 1–3 px; flat (0 grains) is FAIL; grains coarser than 2× source at 3× is FAIL. P1 flipped this round. Candidate: the brief's executable density criterion as a skill rule. Lacks: a second image class with letter texture.

**W10-1. Libertinus Serif is not in the engine OFL allowlist (wordmark).** The receipt says font_license_unknown; Libertinus is an OFL font. This is an engine-side gap, not a skill rule — record for the fix program (list should add the Libertinus family, or the receipt should state its basis). The W middle glyph itself is a font ceiling: only path tracing would fix it, which breaks the true-`<text>` requirement.

**W10-2. Paint-order stroke weight correction (wordmark).** `paint-order="stroke"` plus a 0.6 px stroke adds ~12% area with bbox still within ±1 px. Candidate: measure the area gap first, then set the stroke width by probe, per letter (this round: A uncorrected). Lacks: a second image class.

**S10-1. Straightness test per sub-edge; cornerless rings go through it too (seal).** A whole segment containing an arc rejects its straight parts (frame inner edge inset 2–3 px, thin string 2–3 px thin). Cornerless rings (rounded rectangles) must also go through the straightness test, not whole-ring CR. Candidate: straightness is per sub-edge, for every ring. Lacks: a second image class.

**S10-2. CR end tangent out of control on long straight segments without midpoints (seal).** On a 318 px straight run the control point偏 3–4 px. Candidate: a straight run without midpoints takes L first, or the end tangent takes the straight segment's own direction. Lacks: a second image class.

**S10-3. Edge-touching frames: outer-ring truncation edge is a straight segment (seal).** The outer ring's CR outer bow is clipped by the canvas (no effect), but the same ring's inner edge is not clipped — its inset shows directly. Candidate: treat the truncation edge as straight. Lacks: a second image class.

**L10-1. Small-circle fit limit raised ~12 px to ~25 px (logo).** The "i" dot (19×18 px) needed the fit; the ~12 px limit from L9-2 was too low. Same rule family as L9-2, limit updated by new evidence — still logo-only. Lacks: a second image class.

**L10-2. Round-head pen-end arc fitting method missing (logo).** Y fork round heads are part of a ring; current methods cannot restore roundness (arc-capture probe failed; arc-length limit and turn jitter unlocated). Candidate: an arc-fitting method for round pen ends. Lacks: a second image class (and a working method — the probe failed).

**L10-3. Subpixel shift plus approx eps coordination missing (logo).** 50% subpixel shifts (buffer 0.5/0/0.25 + eps) all scored below v2, and the Y flat edge stayed. Candidate: a coordination rule for subpixel shift and approx eps. Lacks: a second image class.

**L10-4. Buffer joints default to round (logo).** Mitre sharpens small polygon vertices (measured radius std 1.32 px). Candidate: default buffer joint is round. Lacks: a second image class.

### Honest notes (v5.9)

- The penguin P6 chest has failed six rounds (5–10). The new round-10 method (facet split + side measurement) produced lid arches, a gem seat, and edge strips, but no readable side face — still a flat frontal icon at 3×. Do not promote any chest method on penguin alone.
- P1 flipped this round: the executable density criterion (0.5–2×, 1–3 px) worked — grain 3417 px vs source 3386 px, density ratio 1.01×. The grain distribution is bottom-weighted; the source spreads more evenly. Recorded as a residual, not a fail.
- P2 did not flip: the belly notch is fixed and the pectoral fin is back, but the eye is missing in the render while the execution agent claimed "eye present". Self-check claims about small parts must be verified at the pixel level (see the promoted rule).
- Speed regressed hard: same-7 108.6' vs round 9's 86.5' (+22.1'). Overruns: gt3 +10.5', gt1 +9.7', wordmark +9.3', penguin +7.7'. The round-9 gains (tiger P-A, butterfly) held; the losses are diagnosis time on gt1/gt3/wordmark.
- Independent quality fell: 6/9 vs round 9's 8/9. Fails: wordmark (W middle glyph, font ceiling), gt3 (B upper nose tip), penguin (P2 eye, P6 chest). The 3× bar is applied to shape errors; ±1 px edge-position noise (fox tip, owl toe, seal fringe) is the measurement floor and does not fail.
- The S-curve −0.56 px systematic offset (gt1) is still unlocated. This round located a different joint error (3.2 px at x=330) and fixed it; the −0.56 px offset is a separate, smaller effect. Do not burn time on it.
- AA non-linearity changes sign with feature size (gt1 fine band vs tiger large fill). Root cause unlocated.
- Parallel count ≤6 this round (two waves). No environment failure, unlike round 9's 11-parallel 329 MB incident.
- Timer honesty: `timer.py report` takes the current time, so re-running it later inflates TOTAL (penguin: 35.7' at T1 in NOTES vs 48.6' on a later re-run). The value is the one recorded at T1. Wordmark's honest wall time (~22.8') exceeds its timer TOTAL (15.3') — GT pairing search ran before `init` and a loop-2 probe ran after `report`. Record both numbers.
- Wordmark `<path>` exemption approved by Wang (2026-10-28 ~11:46): a pure-text image with 9 true `<text>` and 0 `<path>` is legitimate; the verifier criterion is now path ≥1 OR text ≥1. The W glyph itself is still an independent FAIL.

## Appendix R11 — Cold-start round 11 additions (v5.10, 2026-10-28)

Round 11: same-7 74.9' (threshold ≤91.3', pass; round 10 was 108.6'), independent 9/9 (threshold ≥8/9, pass; round 10 was 6/9), penguin 6/10 (threshold ≥8/10, FAIL; round 10 was 8/10). Conjunction fails — v5.10 draft is not shippable. Baseline: skill v5.9 draft + release 76154e1. Timing rule changed this round: stop point is NOTES written (round 10 stopped at eye-check pass + save); GT pairing search and post-loop probes count toward the time. Under the stricter rule the round is still 33.7' faster than round 10. Diagnosis absorption: all 7 round-10 diagnosis items were absorbed into the round-11 brief (11 changes) before any execution agent was dispatched.

### Promoted to the body (two image classes each)

**P-A. Call formats corrected (Phase 3 + Text Assembly + Phase 0).** `draw_shape` kind must live inside the `geometry` object; `draw_text` arguments go inside a `data` envelope; mincheck does not cover `draw_shape` — run one real `draw_shape` probe before timing starts. Evidence: gt1, butterfly, penguin (d-string-in-geometry, all calls ok); seal (subpaths form); github (mincheck gap). This promotion also corrects the v5.9 Phase 3 line that wrote the top-level form `{kind:"path", d:"…"}` as the most reliable authoring form — that form is rejected on the 76154e1 build. Note the seal misread, corrected here: seal's probe passed a bare d string as the whole `geometry` value (`geometry:'M 100 100 ...'`), which the engine rejects with `geometry 必须是对象`; seal never tested a d string inside a kind-bearing geometry object, and three other agents verified that form works on the same build. The correction stands on the three-agent verification, not on the seal claim.

**P-B. Shared-edge overlap neutral point by two-point interpolation (Phase 3).** The outset that zeroes net area error is not 0.5 — measured black 0.4 / grey 0.5 on one shared edge. Probe two outset values and interpolate to the zero point. Evidence: fox (outset 0.5 insufficient, 1.0 closes; white seam 1007→18 px), tiger (neutral point 0.4/0.5 by interpolation).

**P-C. CR anchors equi-arc-length resampled, spacing ≤ ~5 px; joints not by corner-peak method (Phase 3).** Evidence: owl (resampling the raw contour every 5 px through CR cut whole-image diff by >10%, 32% down), gt1 (smooth closed curve has no curvature-extremum joint; corner-peak method misfires; equi-arc-length CR passes in one go).

**P-D. Text-class images take the outline-tracing branch (Phase 3).** `<text>` only after the candidate font passes an internal-shape probe — a bbox pass is not enough. Evidence: wordmark (Libertinus inner strokes wrong despite a bbox pass; W failed on `<text>`, passed on outlines; 15.3' → 4.5'), logo (letters traced as outlines, 18 paths / 0 text, passed).

**P-E. eps by minimum curvature radius, not part size (Phase 3).** Evidence: seal (large parts at eps 2.0–3.5 polygonized small-radius curves; 0.8 on large parts and 0.6 on the head passed 3×), tiger (eps 1.2 on a 200 px-class long arc still leaves ~1 px flats).

### Appendix candidates (one image class each — not promoted)

**W11-1. Part table keys must be unique instance keys (wordmark).** Keying parts by letter/name lets repeated parts overwrite each other (three I's collapsed to one, driven 3 times — the round's loop-1 root cause). Use a unique instance key and number by order of appearance. Lacks: a second image class.

**W11-2. cairosvg transparent-SVG compositing (wordmark).** When cairosvg renders a transparent SVG, alpha_composite onto white first, then convert to RGB. Lacks: a second image class.

**T11-1. Grey-part count reconciliation against GT path count (tiger).** After dropping small grey parts, reconcile against the GT path count (GT 142 grey paths vs our 105; eye check found no loss). Lacks: a second image class.

**G11-1. ring_to_d M point must be the first run's start (github).** Otherwise the first C segment creates a through-chord slit. End segment's endpoint must equal M (self-check). Lacks: a second image class.

**G11-2. Outset angle-bisector direction from the two edge normals, not the edge directions (github).** 68/145 vertices were wrong on the edge-direction sum. Lacks: a second image class.

**G11-3. timer.py progress.log header hardcodes "coldstart6" (github).** It should take the current round. Display-only; not changed inside a round (one baseline per round). Lacks: a second image class.

**N11-1. Control-point offset for band-edge narrowing 0.43 px (gt1).** Requires true normal-sampled offset points, then refitting. Lacks: a second image class.

**N11-2. Near-white AA pixels pollute the coverage centroid through the residual gate (gt1).** Needs a window. Lacks: a second image class.

**N11-3. Linear extrapolation of endpoints off by 0.7 px (gt1).** Needs quadratic extrapolation. Lacks: a second image class.

**N11-4. Smooth closed curves have no curvature-extremum joint (gt1).** Corner-peak method misfires; equi-arc-length CR passes in one go. Folded into P-C as the joint half; kept here as the single-class record.

**B11-1. Mixed extrapolation for wave endpoint cap-face deviation (gt3).** Lacks: a second image class.

**B11-2. Star-tip straight-line fit + intersection (gt3).** Lacks: a second image class.

**B11-3. 3× "flat peak" impression denied by pixel-level verification (gt3).** Wave peak 1 read slightly flat at 3×; pixel check denied it (mean deviation +0.062 px, no same-sign long run — a scaling-display artifact). Third image-class evidence for the Stop Rule promotion (verify an apparent defect at the pixel level before believing or fixing it). Recorded here; the rule itself is already promoted.

**O11-1. Same-colour multi-connected-region cut (owl).** A light part spanning the full width can cut a same-colour dark region into two visible regions — draw each separately, do not force a bridge. Lacks: a second image class.

**F11-1. Miter outset amplifies too fast at sharp tips (butterfly).** Outset 0.25→0.5 px moved overflow from 41 px to 512 px. Candidate: add a tip clamp, or take 0.25 px as the fine-outline default. Lacks: a second image class.

**S11-1. CR end-tangent pollution by far neighbours (seal).** When the neighbouring edge is a 300 px+ straight edge, the far point pulls the CR end tangent off by 5–11 px. Fix: when a neighbour is more than 2× the chord away, use a mirror point in this segment's own direction as the end tangent. Lacks: a second image class.

**L11-1. Multi-cut ring becomes multiple chains (logo).** A ring cut at two or more places leaves multiple chains — splice in ring order, and count the segments before splicing. Round cause: the Y ring minus two round-head caps left two disconnected chains; v1 treated them as one chain and pulled a diagonal chord, filling the fork as a solid triangle. Lacks: a second image class.

**L11-2. Arc-capture probe fixed 45 px arc-length cap must go (logo).** Walk outward along the ring instead and stop when the residual exceeds 0.5 px. Lacks: a second image class.

**L11-3. Canvas crop-box straight-edge closure (S8-3) confirmed on a second class (logo).** Real measurement this round; second-class corroboration for the S8-3 rule. Not yet promoted (needs a body-rule decision, not more evidence alone).

**P11-1. draw_shape alpha is a 0–255 scale (penguin).** a=0.4 exports fill-opacity="0" (invisible); a=102 exports .4. Skill does not state it. Lacks: a second image class.

**P11-2. 6-layer alpha steps ring at small step sizes (penguin).** Armrest and blush moved to assembled linearGradient/radialGradient. Lacks: a second image class.

**P11-3. 1×1 px filled rectangles need integer coordinates (penguin).** A .5 coordinate lands on four pixel corners, 25% coverage each. Lacks: a second image class.

**P11-4. Fredoka Bold is 2%–32% wider than the source face at the same cap height (penguin).** Correct by per-letter horizontal scaling (round: assembly transforms sx 0.755–0.984, no re-drive). That scaling did not move the grain points — the P1 overflow root cause. Lacks: a second image class.

**P11-5. Joint padding between independently measured contours (penguin).** A ~15 px black gap band opened between the body bottom and the seat top edge; fill it with a padding rectangle. Lacks: a second image class.

**P11-6. Grain points must move with the letter transform (penguin).** Loop 6 scaled the letters horizontally without moving the grain points — 79 overflow points, P1 FAIL. The transform and the grain share one coordinate step. Lacks: a second image class (but a round-12 brief item regardless — see honest notes).

**G11-4. gt3 tangent constraint (single class, but fixed a round-10 FAIL).** At a flat-top→tight-arc transition, split at B9 and constrain the end tangent horizontal. Max error upper counter 0.318/0.115 px, lower counter 0.290/0.132 px; first-try pass. Kept in the appendix for the single-class rule; the method is recorded for the next round's brief.

**L11-4. logo eps back to 0.5 + arc-circle fit (single class, but fixed a round-10 FAIL).** Y round head passed self-check. Kept in the appendix for the single-class rule.

### Honest notes (v5.10)

- The penguin regressed: 8/10 → 6/10. P1 flipped back to FAIL (79 overflow grain points; round 10 had 0) and P4 flipped back to FAIL (armrests back to flat plates; round 10 had soft round tops). Both are true regressions, not re-measurement: loop 6 scaled letters without moving grain points, and the round never restored the round-top armrest model.
- P2 did not flip: the eye is back (round-10 missing eye fixed), but dorsal and pectoral fins are missing and the stripes are 4 vertical rectangular bars exceeding the body outline. The execution agent claimed "fins all in place" — untrue at 8×. Self-check claims about small parts must name each part at 4×; "all present" is not a verifiable claim. Same pattern as round-10 P2 "eye present".
- P6 has failed seven rounds. The round-11 scheme (different from the six prior facet routes, per the brief) still produced a flat frontal icon with no readable side; the claimed "wedge side ~10 px" is invisible in the render. Do not promote any chest method on penguin alone.
- Speed and independent quality both improved under a stricter timing rule: same-7 74.9' vs 108.6' (−33.7'), independent 9/9 vs 6/9. The three round-10 FAIL images (wordmark W, gt3 B nose tip, logo Y round head) all flipped to PASS. The round-10 diagnosis items paid off: wordmark −10.8' (outline branch), gt3 −10.2' (tangent constraint), gt1 −7.4' (joint rule).
- Timer honesty (new stop point): stop is NOTES written. timer.json last_rel_mono is the last `now` event; TOTAL is the T1 record after NOTES. The gap is NOTES-writing time (0–1.0' this round). github is the exception: last event 8.73' vs TOTAL 8.6' (+0.13', one extra `now` after report); TOTAL still follows the T1 record.
- timer.py progress.log header still hardcodes "coldstart6" (verified: first line reads "coldstart6 github progress.log"). Display-only; not changed inside a round.
- The S-curve −0.56 px systematic offset (gt1) is still unlocated. AA non-linearity changing sign with feature size is still unlocated. Both stay off the burn list per the brief.
- Parallel count ≤6 per wave (two waves). No environment failure.
- Wordmark `<path>` exemption still applies (Wang 2026-10-28 ~11:46): path ≥1 OR text ≥1. This round's wordmark is 9 paths / 0 text.

---
name: yanshi-font-identification
description: 找出位图插画或标志里一段文字的原始字体，或最接近的现成字体 —— 用多家在线字体识别服务交叉验证，再用本地逐字渲染评比定出最接近匹配。适用于把描摹里的文字做成真实 text 元素之前确认字体。原字可能是手绘/定制体，"找不到原字体"本身就是一个合法结论。不适用于 logo 字标（字母是轮廓，见 yanshi-vector-tracing 的 Logo variant），也不适用于照片修图。
---

# Yanshi Font Identification — Find the Original Font or the Closest Match

Identify the font of a piece of text in a raster artwork, or find the closest ready-made font, by cross-validating several online font-identification services and then ranking candidates with local per-letter rendering. Distilled from a production run; the measured evidence for that run lives in that run's own report, not in this document.

## Core Principles

- **Closest match is not the original font.** The verdict is one of two sentences, nothing in between: either a service result is adopted as the original font, or the conclusion is "no ready-made original font found; X is the closest match". Never upgrade a closest match into an original font because it looks close.
- **Cross-validate, never trust one service.** Every service has a different font library and different failure modes. One service's first candidate is a lead, not a verdict. A verdict needs at least two services to agree, or a top-list intersection plus a local ranking.
- **Record every service's result, not just the conclusion.** Per service, record the input image, the returned candidates with ranks, and what went wrong. A later reader must be able to re-derive the verdict from the log. A result recorded only in a browser page is lost if the page is lost — write it into the run's own notes file as you go; do not rely on exporting from a browser-side editor at the end.
- **Never fabricate a candidate.** If a service returns nothing, record "no candidates returned". A made-up candidate poisons the cross-validation and the verdict.
- **A wrong input produces a wrong result, not a result.** If the segmentation or the preview text is wrong, do not submit the search. Fix the input and submit again, and record both submissions.
- **The original may be hand-drawn.** Decorative bubble lettering and cartoon type are often custom. "Not found" is a legitimate outcome, and the local closest match is the deliverable in that case.

## Phase 1: Prepare the Inputs

- **Black text on a white background, tightly cropped around the text only.** Identification services expect dark text on a light background with clear letter spacing. Never send the whole artwork: a dark background, figures, or buttons in the frame get segmented as text.
- **Main input: the whole word (one line of text), black on white.** Services need a line of text to segment; this is the input for every service.
- **Backup input: the same crop in its original colors.** If a service does poorly on the thresholded black-and-white image, try the original-color version.
- **Backup input: single-letter images.** When a service mis-segments connected letters, send one letter at a time instead. Prepare at least one structurally distinct letter (a round letter and a letter with a descender or a distinctive tail).
- **CJK text: prepare a separate black-on-white image, but expect weak coverage.** Mainstream services barely support CJK. The reliable path for CJK is ink-box and ink-volume fitting plus manual glyph comparison (Phase 4), not the online services.
- **Measure the text first.** Before any upload, measure the reference text's geometry and style: baseline, cap height, x-height, per-letter ink boxes, letter spacing, stroke width as a fraction of x-height, and color. These measurements are what the local ranking (Phase 4) compares against.

## Phase 2: Run the Services

Run at least three services. For each one, record the input image, the returned candidates with ranks, and any defect. Do not record only the conclusion.

### WhatTheFont

- **Latin text only.** The service states it supports Latin text and does not support CJK. Do not send it CJK.
- **It needs a line of text.** Single-letter images fail with a "no text identified" message, and a manual crop dragged around the letter may not produce a usable crop area. If the whole-word image is the only working input, use it.
- **Ranking only, no percentage confidence.** Record the rank order; there is no confidence number to record.
- **Record per service run:** whether its automatic letter segmentation boxes are correct (connected letters are easily cut wrong), the top 5 candidates with ranks, whether registration or login was required, and whether the locally best candidate appears anywhere in the list.
- **Its library is a commercial catalogue.** A free or open font may simply not be in it, so its absence from the results is not evidence against that candidate.

### FontSquirrel Matcherator / Fontspring Matcherator

- **One engine.** The FontSquirrel page states it is powered by the Fontspring Matcherator; treat the two as the same service, not two independent votes.
- **The file picker may be a dead fallback input.** The "Choose File" control can show the file as attached while the page never processes it — no preview, no crop, no result. If nothing happens after upload, the control is not wired to the uploader: record that and stop retrying the same dead control.
- **The Fontspring site has bot verification.** Its behavior varies by tab and by session: sometimes verification passes on one click, sometimes it resets repeatedly. The environment-swap quota is limited (one swap), and a tab opened after a swap may fail file transfer entirely. Record each failure as it happens; do not keep hammering a verification that keeps resetting.
- **If it returns nothing, it returns nothing.** Record "no candidates returned" and let the other services carry the cross-validation. Never invent candidates to fill the gap.

### WhatFontIs

- **Flow: Upload, Crop, Optimize, Input, Results.** It claims a library of about 1.2 million fonts and 60 or more similar fonts per search. No registration or login required. Ranking only, no percentage confidence.
- **Segmentation is error-prone — check it before submitting.** Connected letters get cut into fragment boxes, empty boxes, and case errors (a lowercase letter read as uppercase). The allowed character set is lowercase a to z, uppercase A to Z, digits 0 to 9, and a few symbols; anything outside that set (for example an exclamation mark) produces an empty box.
- **Verify the preview text before submitting.** The preview text is built from the characters you confirm. If it does not read exactly the intended text, fix the characters and submit again. If a prompt truncates the input (for example, asking for only around 6 or 8 characters), the preview may come out wrong — check it.
- **Record both submissions when a correction is made.** Keep the flawed submission and the corrected one, with what was wrong in the first. A report that only shows the corrected run hides the trap the next run will hit.
- **CJK does not work here.** Segmentation may visually recognize the characters, but the input boxes only accept the Latin, digit, and symbol set — a CJK radical can be misread as a digit. Do not submit a search with wrong characters just to get a result; a search with garbage input produces a garbage result. Record "no valid return for CJK".

## Phase 3: Cross-Validate

- **Two or more services agree on the first candidate: adopt it.** Download or obtain the font, re-fit the text parameters with it (size, baseline, spacing), and re-render to confirm.
- **First candidates disagree: take the intersection.** Take the fonts that appear in the top 5 of more than one service, and rank that intersection by the local method in Phase 4. The highest local score wins.
- **No intersection, or no service recognizes the text: the conclusion is "no ready-made original font found; X is the closest match".** Record every service's returned candidates and ranks honestly, and do not claim any of them is the original font. The original may be hand-drawn or custom, or outside these services' libraries.
- **Confidence comes from agreement, not from a number.** Neither WhatTheFont nor WhatFontIs gives a percentage confidence, only ranks. State the confidence as the evidence itself: how many services agree, whether the top lists intersect, and the local IoU score.
- **CJK: if no service returns anything valid, the conclusion is "identification services do not cover CJK; Y is the closest match by ink-box and ink-volume fit".** The Y is the font fitted locally in Phase 4, not a service result.

## Phase 4: Rank Candidates Locally

Local ranking is what breaks ties and what produces the closest match when the services fail. It is also the check that a service-adopted font actually fits.

- **Per-letter rendering and IoU.** Render each candidate with a local renderer, one letter at a time, at the measured size with height aligned to the reference, and compute the intersection-over-union of the ink pixels per letter. Sample structurally distinct letters (a capital, a round letter, a letter with a descender, a letter with a distinctive tail, a straight-stemmed letter) — five letters is a good sample. Report the per-letter scores and the mean; the mean is the ranking number.
- **Stroke width as a fraction of x-height.** Measure the reference stroke width and each candidate's at the same size. A candidate whose shapes match but whose strokes are much thinner or thicker is not the closest match.
- **Manual glyph-feature comparison.** Compare structural features letter by letter: single-storey versus double-storey forms, tail shapes, opening sizes, terminal shapes. Record which features match and which do not. This is supporting evidence for the IoU ranking, not a verdict by itself.
- **Fit check after adoption.** After fitting the adopted or closest font (size, baseline, per-letter x positions, letter spacing), measure the rendered ink boxes and ink volume per letter against the reference. A font that ranks first on IoU but cannot be fitted to the measured ink boxes is not the answer.

## Phase 5: Write the Conclusion

Write the verdict in one of the two fixed sentences, followed by the evidence. Do not paraphrase the verdict into something stronger.

- **Latin text, no original found:** "No ready-made original font found; X is the closest match." Then the evidence: each service's first candidate and top-list ranks, whether the top lists intersect, and the local IoU mean for X.
- **Latin text, original adopted:** "The original font is X." Then the evidence: which services agreed on it, and the local fit check (ink boxes and ink volume per letter).
- **CJK text:** "Identification services do not cover CJK; Y is the closest match by ink-box and ink-volume fit." Then the evidence: the local fit (ink boxes matching position by position, ink volume within a few percent).
- **If a service returned nothing,** say so in the evidence: which service, what was tried, and what happened. An honest "no result" is part of the verdict.

## Pitfalls

- **Never send the whole image.** A whole artwork with a dark background, figures, and buttons makes the service segment the background as text. Tight-crop the text first.
- **Connected-letter segmentation errors.** A service can cut one word into too many boxes (fragment boxes, empty boxes, case errors). Check the preview text before submitting; if it is wrong, fix it and submit again — and record both submissions.
- **WhatTheFont needs a line of text.** Single-letter images fail, and a manual crop may not work. Use the whole-word image.
- **WhatFontIs cannot handle CJK.** Its input boxes accept only Latin letters, digits, and a few symbols, and it can misread a CJK radical as a digit. Do not submit a search with wrong characters; record "no valid return".
- **FontSquirrel's file picker may be a dead fallback input.** A file that shows as attached but is never processed means the control is not wired to the uploader. Record it and move on.
- **Fontspring's bot verification varies.** The same page can pass verification in one tab and reset repeatedly in another; the environment-swap quota is limited. Record each attempt; do not hammer a resetting verification.
- **A swapped-in browser tab may fail file transfer.** After an environment swap, uploads to the new tab can fail outright. Record the failure; do not retry the same broken transfer.
- **Record results as you go.** Browser tabs and pages can be closed or lost mid-run, and a browser-side editor's export can fail at the end. Write each service's result into the run's own notes file the moment it appears.
- **Font-name and rendering pitfalls (local side).** A variable font turned into static instances gets default subfamily names that do not match the weight — rename the name tables before refreshing the font cache, or the system will report the wrong names. Verification rendering must be cross-checked with a local renderer: one common SVG renderer mis-renders CJK weights (Bold comes out as Regular), so do not trust a single renderer for CJK weight. And do not instantiate the Black weight from a variable font: the instancer's default instance stays Thin, so the result renders thinner than Bold — use the shipped static Bold when the ink boxes already match.

## Deliverables Checklist

- [ ] Input images: whole-word black-on-white (main), original-color crop (backup), single-letter images (backup), CJK black-on-white (separate)
- [ ] Per-service log: service, input image, returned candidates with ranks, segmentation defects, what was recorded and what was not
- [ ] Cross-validation table: first candidates per service, top-5 intersection, local IoU ranking
- [ ] Local ranking table: per-letter IoU per candidate, stroke-width comparison, glyph-feature notes
- [ ] Conclusion in one of the fixed sentences, with the evidence listed after it
- [ ] Report file in the run directory: inputs, per-service results, cross-validation, local ranking, conclusion, pitfalls hit

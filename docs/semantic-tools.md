# Semantic tools: reserved design, not implemented

The design's `semantic` group lists six tools - `analyze_image`, `inpaint_region`,
`generate_mask_from_prompt`, `semantic_replace`, `vectorize_stroke` and `apply_style_transfer` - and states
that they call an **external model service**. This project has no external dependencies: the transport is
hand-written, the kernel is self-contained, and the only crate dependency is `wasm-bindgen`.

**Status: reserved, deliberately not developed.** Per the project owner's decision, the interface is
recorded here so the tool surface and the guarantees are settled, but **no code, no tool registration and no
network path exist**. Calling any of these names today returns "unknown tool", which is the honest state.

## Provider contract (what will be built)

* **One seam.** The kernel never talks to a model. A single provider interface sits beside the tool layer and
  owns every call: request in, structured result out. Nothing else in the codebase may reach the network.
* **Disabled by default.** The provider is chosen by configuration (an endpoint plus credentials in the
  environment). With no configuration the six tools stay unregistered, exactly as they are today, so the
  default build remains dependency-free and offline.
* **The `semantic` profile.** When a provider is configured the tools register under the `semantic` profile,
  so a session opts in explicitly, matching the design's profile mechanism.
* **Determinism split.** Model output is **D2** by nature: identical inputs may differ between runs and
  machines. That is acceptable only because of the next three rules, which keep it out of the deterministic
  core.

## The four guarantees the design requires

1. **Output is wrapped in an independent object** - a `RasterPatch`, or an object group with a clipping
   mask - never a mutation of existing objects, so the user can hide, move or delete the result to undo the
   AI's involvement entirely.
2. **The AI's original output stays pristine.** Later human edits apply as separate objects or as an
   explicit override; they never rewrite what the model produced.
3. **Standard atoms, blob first.** Results are committed through the ordinary tool path, so the blob is
   uploaded before the atom references it (the ordering protocol in 12.2), and history, revert, changesets,
   instances and export all work on them with no special cases.
4. **No new evaluation semantics.** A wrapped result is just an object; it renders, invalidates and
   round-trips through the log like everything else.

## Test strategy for when it is built

A **mock provider** in the test suite, with no network at all: the tests assert what the tool layer owes -
that the result is wrapped in an independent object, that the blob lands before the atom, that reverting the
atom removes the result, and that no other object was touched. A real endpoint is then a configuration
question, not a correctness one.

## What is deliberately not decided here

Which model, which endpoint protocol, prompt shaping, cost control, caching of identical requests, and
whether any of the six deserves a different wrapping (for example `vectorize_stroke`, whose natural output is
geometry rather than pixels) - all of that belongs to the round that implements it, with the owner's input.

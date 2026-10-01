# Sample artwork

These PNGs are the **rendered pictures of the bundled samples**, committed so that the samples exist on
**any** machine.

## Why they are baked

The samples are medium paintings: the oil, watercolour, marker and pencil plugins run **as WASM in the
browser**, and their output is uploaded as `import_image` atoms. The pixels therefore live in blobs inside one
developer's workspace, not in the repository - so on a fresh machine the sample ids did not exist at all,
`switchDocument` merely created an **empty** document, and every sample opened as a blank canvas.

Shipping the strokes instead (as a score for `window.yanshiApplyScore`) would be editable and much smaller,
but a fresh machine would have to **repaint** every sample on first open, which is minutes of waiting for the
big ones.

So the pictures ship as images, and the viewer imports one through the ordinary `import_image` path the first
time a sample document is opened **and is empty**. Nothing is ever overwritten: if the document already has
objects, seeding does nothing.

## Regenerating

The two generated pieces are reproducible from code:

```
node scripts/make-samples.mjs --port 8110 --only sample-lake
node scripts/make-samples.mjs --port 8110 --doc sample-yanshi --only sample-yanshi
```

Then re-export the render, e.g.

```
curl -s -X POST "http://127.0.0.1:8110/api/tools/render_region?doc=<id>&token=<token>" \
     -d '{"region":{"x":0,"y":0,"w":900,"h":640}}'
```

and fetch the returned blob hash from `/api/blob/<hash>`.

The other samples (`sample-oil`, `sample-watercolour`, `sample-brush`, `sample-reference`) were painted
through the UI before the generator existed, so only their pictures are reproducible for now.

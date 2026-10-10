# Grok Bot marks

`extract-marks.cjs` reads the Grok Bot avatar geometry (body plus eye poses)
from your own installed copy of the Grok Bot desktop app and writes
`~/.cache/t1-dash/marks.json`. That artwork belongs to the app and is not
included in this repository or compiled into t1-dash.

t1-dash runs it on start when the cache is missing or older than
`/opt/Grok Bot/resources/app.asar`, using the app's own Electron binary
(`ELECTRON_RUN_AS_NODE=1`) or `node`. You can also run it by hand:

    ELECTRON_RUN_AS_NODE=1 "/opt/Grok Bot/grok-bot" tools/extract-marks.cjs

The extractor finds the mark module by its color table, evaluates only that
module, and identifies the pieces it needs by behavior rather than by minified
names. If the app is not installed or extraction fails, the bots widget stays
hidden and the activity widget is used instead.

## Avatar formats

- Format 1 (older app builds): static outlines plus eye poses; t1-dash ports the idle and working animation itself.
- Format 2 (newer builds with 25 shapes and keyframed eyes): the extractor loads the app's own motion player headlessly under node, steps the idle and working states at 15 fps and stores the 2D outlines it produces (body and eyes per frame, deduplicated). t1-dash plays those frames back, so the motion and eye keyframes match the app. Extraction takes about 10 seconds and only reruns when the app updates.

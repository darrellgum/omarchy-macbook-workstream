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

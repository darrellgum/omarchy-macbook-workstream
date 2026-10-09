# Optional Grok Bot marks

`gen-marks.cjs` bakes the Grok Bot avatar geometry (body plus eye poses) into
`assets/marks.json`. Its input is the mark geometry module from your own
installed copy of the Grok Bot desktop app. That artwork belongs to the app and
is not redistributed here, so `assets/marks.json` is not checked in.

    node tools/gen-marks.cjs /path/to/marks-geometry-module.js assets/marks.json
    ./build.sh            # enables the grok-bot feature when assets/marks.json exists

Without it the renderer builds in generic mode: the bots widget is compiled
out and the activity widget shows status-dir entries instead.

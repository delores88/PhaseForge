# Installed simulation video exports

On September 11, the second installed local checkpoint (built 14:52:29 UTC,
installer SHA a5803c66523b8a2556b905a0433eec919332c50349131d72726ef6c2db21866f)
exported the actual recovered OpenMM trajectory
`cdff7bda-8a88-480a-9d40-0afe2f2cd67d` through the ordinary viewer controls.

| Export | Saved dimensions | Playback | Frames | Render elapsed |
| --- | --- | --- | --- | --- |
| a138b47b-d1dd-4e11-b5a3-1280afe18030 | 1280 × 720 | 3 seconds, 30 fps | 90 | 27.422 s |
| dc80dd4d-cb96-41dd-88ff-c0d2d66eb420 | 1920 × 1080 | 6 seconds, 30 fps | 180 | 70.391 s |

Both use the complete 0–100 ps interval, including retained states 0 and 5,000.
The second movie plays the same physical interval at half the first movie's speed.
The numerical result still has SHA
`2c2d3e495d39a4c8753a34a6b57f36bef3acad222bfc9d1343463f86ff0c242e`.
No new solver job was created by either export. These are two display durations
of a calculation, not two new scientific trajectories.

Both exports were saved using the app's Save MP4 action and the actual Windows
Save dialog into the user's Downloads directory. SHA256 checks match the retained
render result. Independent FFprobe 9.0.1 decoded all frames and confirmed H.264,
dimensions, frame rate and duration. FFplay 9.0.1 played both files to completion
with exit code 0; its 720p log recorded zero dropped display frames and its 1080p
log recorded two. All encoded frames are present. Separate first/final encoded
frame PNGs are retained for inspection, alongside the renderer endpoint previews.

During rendering, navigation to Settings and then a different project worked.
The source project/job showed a running indicator; completion left the selected
page/project unchanged and showed a blue unread dot. Opening the completed export
cleared that job's unread dot while other unseen results remained marked.

Reproduce the read-only saved-file verification with
`python tools/check_installed_exports.py`. It writes
`.local/science/installed-exports/report.json` and decoded endpoint PNGs. That
report passes all checks. Actual player logs are in the same directory. Cancellation
also passed in installed job `fae4e5a4-a369-4e98-94f3-5984b85773d5`: the user-interface
Cancel export action stopped an active 900-frame export after frame 163. Its
authoritative state is cancelled, the worker exited, no download is published and
the original completed numerical source is retained. Final release bytes still
require their own installation check.

## Why

<!-- What this changes and why. Link the PLAN.md phase or the upstream module it ports. -->

## Changes

-

## Screenshots / video

<!--
Required for every feature or user-visible change: at least one screenshot or a
short video of it working in the app. Show the new state, and the old one too when
the change alters existing UI. Headless: `cargo run -p wraex-app -- --snap /tmp/x.png`.

gh cannot upload images, so agents push them to the `pr-media` branch and link them
(see AGENTS.md, "Screenshots and video"):
![what it shows](https://github.com/Dumspy/wraex/blob/pr-media/<branch>/<file>.png?raw=true)

Only when nothing is visible (CI, tooling, refactors, parser or data work with no UI
change), write "None: <why>" instead. A feature PR without media is not done.
-->

## Test plan

- [ ] `devenv tasks run check:all`
- [ ] `devenv tasks run check:fixtures` (if parsers, data or fixtures changed)
- [ ] Screenshots or video above show the feature working

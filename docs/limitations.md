# Known limitations

This document is deliberately honest. Every item here is a real software
boundary in the current build — not a bug, and not hidden. Items marked
**planned** have an implementation path sketched in ARCHITECTURE.md.

## Show format & workflow

1. **No OS file association.** Double-clicking a `.stagecue` file does not
   open StageCue yet — use File → Open inside the app. (Planned; simple
   installer/desktop metadata.)
2. **Media is referenced, not embedded.** The project stores paths. Moving
   folders changes nothing until you **Relink…**; a clean "collect assets
   into project folder" command is not implemented yet.
3. **Cues are a flat list.** Grouping exists as an ID field (`groupId`), but
   there is no scene/group hierarchy in the cue list UI today. For long
   shows with scenes, use cue numbering conventions (e.g. `1-04`).

## Audio

4. **Per-cue multichannel output.** All audio plays to the selected default
   (stereo pair) output. Separate buses to different hardware outputs (e.g.
   front-of-house vs monitor send) are not implemented; the "bank"
   (`Music/Effects/Ambience/Direct`) is a mix-session organization tool, not
   a routing matrix.
5. **Metering is envelope-based.** Mixer meters display computed envelope
   levels rather than true sample-accurate peak/RMS windows. It reads the
   pre-computed peak (`peakDb`) from import for static display, and uses a
   30 Hz push from the engine for live level; do not use it as a
   broadcast-accurate peak meter.
6. **No native MP3/FLAC duration probe.** WAV files get an instant duration
   from the header probe at import; other formats show duration only after
   the cue first plays. Import doesn't block, so use WAV for pre-show timing
   tables.
7. **No DAW-grade time-stretch or pitch shifting.** Playback rate is native.

## Visual

8. **Video preview in the console is a placeholder.** The console preview
   pane shows a frame hint (poster/label) for video cues; real decoded video
   plays only in the stage output window. For editing confidence, plan video
   cues in rehearsal with the stage window visible.
9. ~~**StageVisual lacks fit mode.**~~ **Fixed.** The stage window now
   receives the cue's `fitMode` and maps it to object-fit classes
   (`contain` → letterbox, `cover`, `stretch` → fill, `center` → native
   size centered). The config note: this needed the asset protocol to be
   enabled, which is now set in `tauri.conf.json`.
10. **No PDF rendering engine.** Slide assets (`pdf`) are listed and
    importable but are not rasterized — pair slides as images (PNG/JPG) for
    show use today.
11. **No built-in image filters/keystone.** Stage output does no geometry
    correction; if the projector isn't straight-on, correct at the projector.

## Networking / integration

12. **No OSC / MIDI / SMPTE / DMX.** StageCue is intentionally standalone.
    These protocols need a real threat model for a live-control surface and
    are not in the current build; the deterministic machine means they are
    easy to add later as pure event sources into the machine.
13. **No external device triggering.** Footswitches and GPI trigger boxes can
    only be used via OS-level keyboard emulation toward the app's shortcuts.

## Metering, safety, validation

14. **Master ceiling & limiter.** `maxMasterDb` and the safety limiter are
    wired into the audio path, but the limiter is a soft ceiling on the
    master gain — do not treat it as a broadcast limiter for calibration.
15. **Validation is static.** The `validate_show` command reports missing
    media and structurally empty cues, but can't audition whether transitions
    sound right — rehearse with **[device]** for the last word.

## Platform

16. **Windows rendering size.** Windows WebView2 versions have been tested
    at the 1440×900 default; very large multi-monitor layouts (for the
    console) are untested. Stage output is borderless fullscreen and is
    unaffected.
17. **Not a git repo at authoring time.** This source tree has been versioned
    only by its test suite; before distribution, put it under actual version
    control.

## Testing coverage honesty

- State machine and persistence are thoroughly unit-tested (`cargo test
  --lib`, 36 tests, including MockClock timing).
- The frontend has limited unit coverage (5 Vitest tests).
- There is no automated E2E test runner yet; the smoke-test checklist in
  [quick-start.md](quick-start.md#9-smoke-test-checklist-full-run) is manual
  and requires ears and a projector: audio and visuals cannot be verified
  meaningfully in CI.

# Troubleshooting

Symptoms are ordered by how often they come up in rehearsal settings.
Where a fix is device-dependent, it's marked **[device]**.

## Audio

### No sound at all

1. Check the **output device** selector (top-right of the console) — after a
   device change (unplugging your interface, sleep/wake), StageCue keeps the
   name; if the device is gone, it falls back to the system default. Pick
   again explicitly.
2. Check master mute (`m`) and the master level — the mute state is shown in
   the bottom console, never color-only.
3. Check if the cue itself is muted or a solo is latching other cues down.
4. Check that the audio file still exists (the cue list shows a missing-media
   marker; use **Relink…** if the path changed).

### Blocky/fading timing feels wrong

- Fades run on Rust threads at ~85 Hz — behavior is bit-stable across
  machines. Wrong *feel* usually means a curve mismatch: e.g. an EqualPower
  curve on a 30 s ambient fade feels very different from Linear. Inspect
  `fadeCurveIn/out` in the Inspector.
- Audio glitches at the end of a cue that also has a visual? Video decode
  uses the same CPU; close heavy preview panels during the show or reduce
  visual size (see [limitations](limitations.md)).

### Pops / clicks on stop

- StageCue already overlays an 8 ms ramp on any hard stop; if you still hear
  clicks **[device]**, the stop is likely landing on a DC offset — check the
  source file in your DAW. StageCue cannot fix a DC-offset source.

### Harmless log noise you can ignore

- `ALSA lib pcm_route ... Found no matching channel map` — printed by ALSA on
  many desktop setups when enumerating devices; playback is unaffected.
- `gtk_window_set_titlebar() called on a realized window` — GTK cosmetic
  warning when the console window is created; harmless.

## Stage output

### Fullscreen window appears on the operator display, not the projector

OS-level placement decides which monitor a new window opens on. Put the
stage window on the projector from the OS display settings; the text mode is
borderless fullscreen, so it will re-fullscreen to whatever monitor it was on
last. **[device]**

### Blackout didn't clear

Blackout latches. Press `b` again (or hit the button) — if the stage window
itself was closed, re-open **Output window**; blackout state persists so the
stage stays dark until you consciously un-blackout.

### Visual doesn't show but audio plays

- The cue has no visual asset attached, or
- `holdVisualAfterAudio` is off on a previous cue and the stage cleared on
  audio-finish (this is *by design* per cue settings), or
- The file moved — see the missing-media marker and **Relink…**.

## Project files

### "project was written by a newer StageCue"

The `.stagecue` file has a `formatVersion` bigger than this build supports.
Update the app. We never load a file that might round-trip-lossy — your data
was never at risk.

### "This file is not a StageCue project"

The kind marker in the envelope (`kind: "stagecue.project"`) tells us this
file was renamed from another program, or is truncated. Check you picked the
right file; if it's one you suspect was truncated when e.g. copying off a USB
stick, try the matching `.bak`.

### App crashed / machine rebooted — did I lose work?

On start, StageCue offers the recovery snapshot (written while dirty). If you
never got the offer, check autosave/backup copies next to your project
(`name.stagecue.bak*`). Recovery offers Restore or Discard; **Discard** keeps
the `.bak` files so nothing is deleted automatically.

### Media on a USB stick / removable drive

StageCue stores absolute or project-relative paths; it doesn't copy your
media into the project. On show machines, keep media in a stable folder (a
local copy of the drive), and if the drive letter changed, use **Relink…** or
resolve using the validation report.

## Recent projects & show log

Recent projects are shown in the shell menu; if the list goes out of sync
(e.g. you moved files), open the project once and the entry refreshes.

The last session's transport events are available through the app's show
log. If a report is needed, export both the project file and the log — the
log records every transport transition with its timestamp, so we can
reconstruct exactly what the machine did.

## Still stuck?

Run with logs visible:

```bash
npm run tauri dev
```

and include: OS + version, audio interface, display count, the exact error
toast text, and the state shown in the top bar (e.g. `Running · Q7 · blackout`)
when it happens. The show log is available via the app for the last session's
transport events.

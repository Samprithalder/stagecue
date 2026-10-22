# Operator quick-start

This guide walks from a fresh install to running a small show. It doubles as
the project's manual smoke test: steps marked **[device]** exercise hardware
(speakers, display, microphone of the room) that cannot be verified by an
automated test runner — always do them on the actual show machine.

## 1. Start

```bash
npm run tauri dev      # developer mode, or launch the installed app
```

The **Scheduler / Operator Console** opens (1440×900 minimum, dark UI).

## 2. Create or open a show

- Top bar → **New Project** (or `Ctrl/Cmd+S` to save after editing; the app
  asks for a save location when the project is untitled).
- To resume work, open a `.stagecue` file. If the last session ended dirty,
  StageCue offers to **Restore** a recovery snapshot first — this is
  intentional; the snapshot is more recent than any save.

## 3. Import media

Use **Import assets** and select files. Supported kinds:

| Kind | Extensions |
|---|---|
| Audio | wav, mp3, flac, ogg/oga, m4a, aac, opus, aiff/aif, wma |
| Image | png, jpg/jpeg, webp, gif, bmp, tif/tiff |
| Video | mp4, mov, mkv, webm, avi, m4v |
| Slide | pdf |
| Text | txt, md |

- WAV files get a duration probe at import time so cue lengths are displayed
  immediately. Other audio formats display duration once played.
- If a file is missing later (e.g. a renamed folder), the cue list flags it
  and **Relink…** points it to its new location. Nothing is embedded or
  duplicated — StageCue stores paths, so keep media in a stable folder for
  show machines (see limitations).

## 4. Build cues

- **Add cue** gives `Q1, Q2, …` automatically; rename it to match your script
  (e.g. *“Act 1 — storm builds”*).
- Select a cue and use the **Inspector**: attach audio assets (multiple are
  mixed), a visual asset, volume/pan, fade in/out times and curves, trigger
  mode (`Manual`, `Follow`, delay-based), hold-visual-after-audio, and the
  stop behavior.
- Right-click / context actions: **duplicate** (great for “same backdrop,
  next sound”), **delete** (confirmation required), **reorder** by drag.

## 5. Set up audio **[device]**

- Pick the **output device** (e.g. the interface feeding the PA, not the
  laptop speakers) from the audio settings.
- Set the **master ceiling** (Max Master dB) so a nervous default cannot
  hurt the room.
- Use the mixer strip (bottom console) for per-cue gain, mute, solo; the
  meter shows envelope level — peaks trust pre-computed values, see
  [limitations](limitations.md#metering).

## 6. Set up the stage output **[device]**

- **Output window** opens a fullscreen audience view — put it on your
  projector/second display (OS screen settings decide placement).
- **Test pattern** verifies routing: operators see the pattern only in the
  stage window, never in the console preview pane.
- **Blackout** (`b`) is immediate and latching; the stage preview shows the
  same state so you never guess.
- Note: the stage window is re-created by the Output command; if you like,
  set your OS to always place it on the projector display.

## 7. Rehearse

1. Click the first cue to **arm** it (green = armed; labels, never color
   alone, carry the state).
2. **GO** (Enter / Right arrow / button) runs the cue.
3. **STOP** (`s`) stops the *current* cue and re-arms it — press GO again to
   retry it.
4. **FADE OUT** (`f`) eases the current audio per its fade curve.
5. **Previous** (Left arrow) arms the cue before the current one without
   touching audio — for a “back up one” moment.
6. **PANIC** — double-guarded button; keyboard **Space** is play/pause, not
   panic. Panic stops everything and requires re-arming before GO works
   again (deliberate scratch line).
7. **Performance mode** (F10) collapses the console to a minimal view for
   dark rooms.

## 8. Run the show

- The top bar shows running state, current cue, next cue and blackout.
- Save (`Ctrl/Cmd+S`) before house opens. Autosave and backups are on by
  default; a **.bak** copy is kept per save.
- If the machine must reboot mid-show: reopen the project and **Restore**
  recovery — you lose at most the autosave interval.

## 9. Smoke-test checklist (full run)

- [ ] App starts; both windows come up; no console errors.
- [ ] Import 3 files: a WAV, a PNG, and one unsupported kind (expect a
      clean rejection message, no crash).
- [ ] Build 3 cues: audio-only, visual-only, audio+visual with hold.
- [ ] GO through all three; STOP mid-cue; verify re-arm behavior. **[device]**
- [ ] FADE OUT; verify curve feels right on the actual speakers. **[device]**
- [ ] Blackout toggles stage output instantly; console preview mirrors it. **[device]**
- [ ] Test pattern appears in the stage window only. **[device]**
- [ ] PANIC: everything stops, GO refused, arm a cue, GO works.
- [ ] Save; kill the app mid-dirty; reopen and verify recovery snapshot offer.
- [ ] Open the example project from `examples/` — it loads without errors.

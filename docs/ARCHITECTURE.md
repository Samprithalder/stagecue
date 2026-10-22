# Architecture overview

This document explains how StageCue is structured, *why* it is structured that
way, and which parts you can trust to be deterministic. It is written for
contributors and for anyone auditing the app before running it in a live show.

## Guiding rules

1. **The show must not stop.** Every transport path (GO, STOP, FADE, PANIC)
   is guarded and idempotent. Two commands arriving within the debounce
   window cannot interleave and race — they collapse to one.
2. **Timing belongs to Rust.** Audio fades, cue-completion detection,
   auto-advance scheduling and the show tick loop run on native threads.
   The React frontend only renders state that Rust pushes to it.
3. **The core is pure.** `src-tauri/src/core/` is a plain Rust library with no
   Tauri, rodio or UI dependency. The cue state machine is fully synchronous
   and testable against a mock clock. This is where correctness lives.
4. **One source of truth.** The Rust `Project` model is authoritative. The
   frontend store is a rendering cache fed by events and command results.
5. **Fully offline.** There is no network access, no telemetry, no analytics,
   and no update check anywhere in the app.

## Layer map

```
┌──────────────────────────── Frontend (TypeScript) ───────────────────────────┐
│  console window             stage output window                             │
│  ├─ TopBar                  └─ output.html (blackout, test pattern,          │
│  ├─ CueList                    full-bleed image/video/slide preview)         │
│  ├─ StagePreviewPanel                                                      │
│  ├─ Inspector               state = read-only view of showStore             │
│  ├─ BottomConsole (mixer)                                                   │
│  └─ PerformanceMode        showStore.ts (Zustand) — single store shared     │
│                            by both windows via the Rust event channel       │
└───────────────┬─────────────────────────────────────────────────────────────┘
                │ typed invoke() calls            stagecue://* events (push)
┌───────────────▼─────────────────────────────────────────────────────────────┐
│  commands.rs — typed IPC surface                                            │
│    project_new/open/save/save_as, show_go/stop/fade/pause/blackout/…,       │
│    cue_add/remove/update/reorder, asset_import/relink, audio_* mixer        │
│                                                                             │
│  show.rs — ShowRunner                                                       │
│    transports CueEvents into the audio engine, owns the 30 Hz tick loop,    │
│    emits stagecue://show|visual|summary|voices|master|device                │
│                                                                             │
│  audio/engine.rs — rodio 0.23                                               │
│    one Player voice per cue id · fade ramps on Rust threads (~85 Hz) ·      │
│    equal-power PanSource · master → per-voice → mute/solo gain graph        │
│                                                                             │
└───────────────┬─────────────────────────────────────────────────────────────┘
┌───────────────▼─────────────────────────────────────────────────────────────┐
│  core/ — pure, dependency-free logic (the trustworthy part)                 │
│    model.rs       Project / Cue / MediaAsset + format migration             │
│    machine.rs     deterministic cue state machine (80 ms debounce,          │
│                   MockClock-tested)                                         │
│    persistence.rs atomic save (temp + rename), rotating backups, recovery   │
│                   snapshot, versioned migration                             │
│    media.rs       import validation, WAV header duration probe, relink      │
│    ops.rs         cue move/duplicate                                        │
└─────────────────────────────────────────────────────────────────────────────┘
```

## The cue state machine

`machine.rs` implements a synchronous state machine. It receives semantic
events (`CueEvent`) such as `Go`, `StopCurrent`, `MarkAudioFinished`,
`FadeComplete`, `ArmCue`, `Tick` and returns a `ShowSnapshot` plus the list of
`CueEvent`s the runtime must execute (start audio, fade, show visual…).

The flow states are:

| State | Meaning |
|---|---|
| `Idle` | No cue armed; GO is refused. |
| `Ready` | A cue is armed; the next GO fires it. |
| `Running` | The armed cue's media is playing/transitioning. |
| `HoldingVisual` | Audio finished; the visual is held on screen (e.g. backdrop stays up) while waiting for the next GO or an auto-advance delay. |
| `Stopped` | After PANIC. GO is refused until a cue is explicitly re-armed — a safety scratch line. |

Semantics deliberately locked in by the test suite (see `machine.rs` tests):

- **GO debounce** — a second GO within 80 ms is ignored. A double GO can
  never fire two cues.
- **STOP re-arms** — stopping the current cue re-arms the *same* cue, so an
  operator mistake ("stopped too early") is recoverable by pressing GO again.
- **HOLD stop behavior** — a stop on a keep-visual cue emits
  `KeepAudioChangeVisual` + `held_audio`: the backdrop stays, the audio
  changes.
- **Audio finish with hold** — `MarkAudioFinished` with `holdVisualAfterAudio`
  enters `HoldingVisual`; if `autoAdvanceMs` is set, a pending auto-follow
  with delay is scheduled; a later GO or the delay fires the next cue.
- **Fade-out completion** lands in either `HoldingVisual` (hold cues) or
  `ClearVisual + Idle` (cut cues).

Everything in the machine can be tested without audio hardware: the tests use
a `MockClock`, so timing behavior is deterministic and fast.

## Audio engine

`audio/engine.rs` wraps **rodio 0.23**:

- One rodio `Player` voice lives in a shared map keyed by **cue id** (a
  `Player` is not `Clone`, voices are looked up and driven from the runner).
- Fades are computed on a Rust thread stepping at ~85 Hz (≈12 ms granularity)
  and applied as gain ramps — the UI thread never touches audio.
- An 8 ms ramp is overlaid on any hard stop so there is never a click.
- Panning uses an equal-power law (cos/sin) so center stays at full width.
- The gain graph is `master level → per-voice level → mute/solo`, with a
  configurable master ceiling (`maxMasterDb`) and an optional safety limiter.
- Output-device selection re-builds the sink; failures fall back to the
  default device with an error surfaced to the operator, not a crash.

## Persistence

`persistence.rs` uses a "nothing is ever lost" strategy:

- **Envelope format.** A `.stagecue` file contains `kind: "stagecue.project"`,
  `formatVersion` and the flattened project. The kind marker rejects
  accidentally renamed foreign files; `formatVersion` drives migration
  (unknown *newer* versions refuse to load with a clear message).
- **Atomic saves.** Data is written to a unique temp file in the same
  directory and renamed over the target — a crash midway can never leave a
  truncated show file.
- **Rotating backups.** Point-in-time `.bak` copies are kept so an operator
  can roll back a bad save.
- **Recovery.** If the app was closed while dirty, a recovery snapshot is
  written and offered at next start (`recovery_candidate` command; the UI
  asks Restore / Discard).

## Frontend

- `store/showStore.ts` (Zustand) holds snapshot, cue list, voices, master
  level, notifications. It is initialized by `ensureStoreInit()` which
  attaches the `stagecue://*` event listeners exactly once.
- The **console window** (`index.html`, label `console`) is the operator UI.
- The **stage output window** (`output.html`, label `stage`) is created on
  demand, is borderless/fullscreen-able, and shows only visuals in response
  to `stagecue://visual` events. It never shows operator controls.
- Both windows read the *same* store shape; the stage window simply renders
  the visual slice of it.
- **Shortcuts** (`src/app/shortcuts.ts`) map keys to the same guarded IPC
  calls as the on-screen buttons — the keyboard path has no separate,
  unguarded logic. Typing targets and Ctrl/Cmd/Alt combos (e.g. Ctrl+S =
  save) are exempt from transport keys.

## Event channel

| Event | Payload |
|---|---|
| `stagecue://show` | Full `ShowSnapshot` (state, current/next cue, blackout, panic) |
| `stagecue://visual` | What the stage window should display |
| `stagecue://summary` | Project summary / dirty flag / recent projects |
| `stagecue://voices` | Per-cue gain/mute/solo/position in the mixer strip |
| `stagecue://master` | Master metering, mute, level |
| `stagecue://device` | Audio output device list / selection changes |

The tick loop runs at 30 Hz, which is enough for smooth metering and
auto-advance accuracy well within a frame of human perception, while keeping
CPU cost negligible.

## Windows and capabilities

Declared in `src-tauri/capabilities/default.json`: the console and stage
windows get only the scoped permissions they need (dialog, fs, log, core
window APIs). The stage window is intentionally not granted any mutating
capability — an operator cannot accidentally hack it into a control surface.

## Testing strategy

- **Rust (`cargo test --lib`, 36 tests):** the state machine (flows, debounce,
  hold/auto-advance, panic re-arm, MockClock), persistence (atomicity, backup
  rotation, recovery, migration), and media (validation, relink).
- **Frontend (`npm test`, Vitest):** store reducers and pure UI logic.
- **Manual smoke test:** documented step-by-step in
  [quick-start.md](quick-start.md), with device-dependent steps clearly
  marked because an automated runner cannot hear audio or see a projector.

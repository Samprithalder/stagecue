# StageCue

**Cue-based theatre playback and backdrop control for live performance.**

StageCue is a desktop show-control application for theatre, rehearsals,
worship, presentations and cue-based events. It combines:

- **Audio playback and mixing** (WAV/MP3/FLAC/OGG/M4A with fades, channels,
  master gain, output-device selection)
- **Backdrop, image, video and slide output** on a separate projector display
- **Cue sequencing** with a deterministic, race-free state machine
- **A dedicated operator console** optimized for dark rooms and pressure
- **A clean, chromeless stage output window** that never shows operator UI

Built with **Tauri 2 · Rust · React + TypeScript · Vite**.
Fully offline after install. No accounts, no cloud, no telemetry.

---

## Repository layout

```
stagecue/
├── src-tauri/               Rust core (Tauri 2)
│   ├── src/core/            Pure logic — project model, cue state machine,
│   │                        persistence (atomic saves + backups), media
│   │                        validation. All heavy logic + tests live here.
│   ├── src/audio/           rodio-based audio engine (fades on Rust threads)
│   ├── src/show.rs          Runtime: drives the machine, translates CueEvents
│   ├── src/commands.rs      Typed IPC surface
│   └── tauri.conf.json
├── src/                     Frontend (React + TypeScript)
│   ├── app/                 Console shell, performance mode, shortcuts
│   ├── panels/              TopBar, CueList, StagePreview, Inspector, Mixer bar
│   ├── output/              The Stage Output window (audience-facing)
│   ├── store/               Zustand store; listens to Rust event channel
│   └── design/theme.css     Design tokens (charcoal/amber console language)
└── docs/                    Guides: quick-start, shortcuts, packaging,
                             troubleshooting, limitations, architecture
```

## Development (Linux/macOS/Windows)

Prerequisites: Node 20+, Rust 1.77+, the Tauri v2 [prerequisites]
(webkit2gtk-4.1 on Linux; on Windows WebView2 and MSVC; on macOS Xcode).

```bash
npm install
npm run tauri dev      # runs the console + stage output windows
```

## Build & package

```bash
npm run tauri build
```

Platform-specific instructions, including **Arch Linux / AppImage / .deb**
and macOS signing/notarization notes, are in
[`docs/packaging.md`](docs/packaging.md).

## Tests

```bash
cd src-tauri && cargo test --lib    # 36 tests: state machine, persistence, media
npm test                            # frontend logic (Vitest)
```

An end-to-end smoke test walkthrough (manual, with device-dependent steps
clearly marked) is in [`docs/quick-start.md`](docs/quick-start.md).

## Documentation index

- [Operator quick-start](docs/quick-start.md)
- [Keyboard shortcuts](docs/shortcuts.md)
- [Packaging (Windows / macOS / Arch)](docs/packaging.md)
- [Architecture overview](docs/ARCHITECTURE.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Known limitations](docs/limitations.md)

## Example show

[`examples/twilight-rehearsal.stagecue`](examples/twilight-rehearsal.stagecue)
is a small 4-cue rehearsal project (ambience → storm with auto-follow →
intermission → emergency cut). Open it via File → Open. It references media
under `media/` relative to the project file that is **not included** — put
any files with those names there (or use it as-is to see how missing media is
reported, then relink). `src-tauri/examples/load_example.rs` is a tiny
sanity binary: `cargo run --example load_example` confirms the file parses.

## Design values

1. **The show must not stop.** Every transport command is idempotent;
   repeated GO/STOP/FADE presses are debounce-guarded no-ops, never races.
2. **Timing belongs to Rust.** Fades, auto-follow scheduling and cue
   completion run on native threads; React only renders state.
3. **Deliberate danger.** Deleting cues and exiting an active show require
   confirmation; Panic requires a double press (3 s window).
4. **Operator clarity.** Amber = action, blue = preview/information,
   green = armed/ready, red = stop/blackout/panic only. State is never
   communicated by color alone — labels always accompany it.

# Keyboard shortcuts

These are the shipped defaults. The keyboard path uses the same guarded
handlers as the on-screen buttons — destructive actions (Stop All / Panic)
keep their double-press confirmation going through either path.

## Transport

| Key | Action | Notes |
|---|---|---|
| `Enter` or `→` | **GO** | Fires the armed cue. Second press within 80 ms is ignored (debounce). |
| `←` | **Previous cue** | Arms the cue *before* the current one; never touches audio. No-op at the first cue. |
| `Space` | **Play / Pause** | Toggles pause on the running cue. |
| `s` | **Stop current** | Stops the current cue and re-arms it — GO retries the same cue. |
| `f` | **Fade out** | Fades the current cue's audio per its curve; visual behaves per stop setting. |

## Output & mixer

| Key | Action | Notes |
|---|---|---|
| `b` | **Blackout toggle** | Latches the stage output to black; pressing again restores. |
| `m` | **Master mute toggle** | Mutes/unmutes the master bus. |
| `F10` | **Performance mode** | Collapses the console to the minimal performance view. |
| `Escape` | **Cancel pending** | Reserved for closing dialogs (handled locally); not a global panic. |

## Editing & file

| Key | Action | Notes |
|---|---|---|
| `Ctrl/Cmd + s` | **Save project** | Atomic save; rotates a `.bak` backup. |

## Safety rules built in

- Typing in an input, textarea, select or contenteditable is never captured
  by transport keys.
- Modifier combos (Ctrl/Cmd/Alt) never trigger transport actions except the
  explicit Ctrl/Cmd+S save.
- GO never advances from `Stopped` state after **PANIC** until a cue is
  explicitly re-armed — a deliberate scratch line against accidental
  restarts after an emergency stop.
- A second GO within 80 ms cannot fire a second cue.
- Repeat keydowns from a held key are not debounced at the front-end beyond
  the 80 ms machine guard; avoid jamming keys — the machine is race-free but
  your timing should still be deliberate.

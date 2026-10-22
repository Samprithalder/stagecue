# Packaging & distribution

StageCue is built with Tauri 2 and targets **Windows, macOS and Linux
(AppImage / .deb / Arch package)**. One command builds the release bundle:

```bash
npm run tauri build
```

The frontend is compiled once (`tsc + vite`, two entry points: the console
`index.html` and the stage `output.html`) and embedded into the binary — no
runtime webview download dependency beyond the platform's own.

## Prerequisites per platform

| Platform | Toolchain |
|---|---|
| Windows 10/11 | Rust + MSVC, WebView2 (pre-installed on most Win11), WIX/NSIS auto-setup by Tauri CLI |
| macOS 11+ | Xcode command-line tools; signing needs an Apple Developer account |
| Linux | webkit2gtk-4.1, GTK dev headers, AppImage tooling (Tauri auto-fetches linuxdeploy) |

## Linux (primary dev target: Arch Linux)

### Arch / AUR

StageCue's default bundle already targets **appimage** and **deb**
(see `src-tauri/tauri.conf.json → bundle.targets`). An AUR package would:

1. Build the AppImage in CI on a clean-ish Arch container, or package the
   deb contents directly.
2. Install `stagecue` (the binary), `icons/icon.png` into
   `/usr/share/icons/hicolor/…`, and a `.desktop` entry pointing at the
   binary with a descriptive name.
3. Declare dependencies: `gtk3`, `webkit2gtk-4.1`, `alsa-lib` (rodio/cpal
   uses ALSA directly; no pulse forwarder needed but works under PipeWire's
   ALSA shim).

Sketch for `PKGBUILD`:

```bash
depends=('gtk3' 'webkit2gtk-4.1' 'alsa-lib')
# package() {
#   install -Dm755 "target/release/stagecue" "$pkgdir/usr/bin/stagecue"
#   install -Dm644 "icons/icon.png" "$pkgdir/usr/share/icons/hicolor/512x512/apps/stagecue.png"
#   install -Dm644 "stagecue.desktop" "$pkgdir/usr/share/applications/stagecue.desktop"
# }
```

### .deb

Enable in `tauri.conf.json` (already enabled): produces a deb via Tauri's
built-in packaging; requires `linuxdeploy` downloaded at build time, so run
this on a networked machine and copy the artifact.

## Windows

- `npm run tauri build` produces an **MSI (WiX)** and/or **NSIS** installer.
- WebView2 is needed at runtime; bundle the evergreen installer or require
  the pre-installed runtime (default on Win11).
- **Code signing:** provide a certificate (`signtool` / Tauri's
  `windows.certificateThumbprint` settings); unsigned installers will
  SmartScreen-warn. Sign for any distribution beyond your own machine.

## macOS

- Build on Apple silicon or Intel matching your target (or do both with
  `--target universal-apple-darwin` for a single universal build).
- **Signing & notarization:** need an Apple Developer ID; set
  `APPLE_CERTIFICATE`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific), and
  enable the Tauri notarization flow. Unsigned apps are blocked by Gatekeeper
  for other users — sign for any distribution beyond your own machine.

## What the bundle includes

- `stagecue` binary + webview assets (console + stage output pages).
- Icon: RGBA PNG at `src-tauri/icons/icon.png` (used by all platforms).
- Machine-readable metadata: category *Multimedia*, short and long
  descriptions are already present.

## Release checklist

- [ ] `npm run tauri build` completes for the target platform(s).
- [ ] The bundled app runs with **no dev server**; stage output opens.
- [ ] Open a `.stagecue` file that lives outside the app directory.
- [ ] Audio device selection persists across launches. **[device]**
- [ ] Panic works in the packaged app (it's wired through the same command).
- [ ] Rehearsal at the venue with the actual output device & display. **[device]**

## File association (planned)

Currently open-by-double-click is not registered in the OS. Users launch
StageCue, then **File → Open**. OS file association registration is planned
for a follow-up release; on Linux this is just a `.desktop` `MimeType`
entry, on Windows/macOS it's installer metadata.

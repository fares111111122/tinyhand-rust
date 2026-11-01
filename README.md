# TinyHand — Rust edition

A tiny hand on a stick that points, taps, and draws over your desktop.
A Rust port of [nilbuild/tinyhand](https://github.com/nilbuild/tinyhand), using the original MIT-licensed artwork and sounds. The application runs without Swift or a webview.

## Platforms

- **Windows:** native desktop application, built with MSVC.
- **macOS:** Apple Silicon and Intel builds; the packaging script creates a `.app` for macOS 13+.
- **Linux:** **X11**, with a compositor for transparency and an AppIndicator-compatible tray. Native Wayland is not supported. XWayland does not give a reliable overlay above native Wayland applications; use an X11 desktop session.

The repository contains a native-build CI matrix for all four CPU/OS combinations. Adding a workflow does not establish that its jobs have passed; check your own Actions run before distributing Windows/Linux binaries.

For Windows only, manually run **Rust build** in GitHub Actions with `platform: windows`. This schedules one Windows job and uploads `TinyHand-windows-x86_64` with the portable executable and license; selecting `all` builds the full matrix.

## Build and run

Install a current stable [Rust toolchain](https://rustup.rs/). Build on the target OS:

```sh
cargo run --release --locked
```

On Debian/Ubuntu, install native dependencies first:

```sh
sudo apt-get install build-essential pkg-config libgtk-3-dev libayatana-appindicator3-dev libasound2-dev libxdo-dev libxrandr-dev libxcb-randr0-dev libxkbcommon-dev libgl1-mesa-dev
```

Windows builds require the Visual Studio C++ build tools. macOS builds require Xcode Command Line Tools. Audio output is optional: an unavailable device produces a visible status message and the pointer remains usable.

Build a macOS app bundle:

```sh
./scripts/package-rust-macos.sh
open dist/TinyHand.app
```

The bundle is ad-hoc signed for local use, without Developer ID signing or notarization. Windows/Linux CI produces portable archives. Windows CI links the Microsoft C runtime statically to avoid a separate Visual C++ Redistributable requirement; Linux still needs the runtime libraries corresponding to the packages above. No release is published automatically.

## Use

| Action | Result |
| --- | --- |
| Start pointer / tray menu / `Ctrl+Alt+H` | Toggle the overlay |
| Click | Tap with the selected sound and leave an ink dot |
| Hold and drag | Draw; clicks are captured and do not reach underlying applications |
| `Ctrl+Alt+X` / Clear | Clear all drawings |
| `Esc` / Stop | Clear and disable the overlay |
| Settings / Quit in the tray menu | Open settings / exit the process |

On Mac, Alt is Option. The toolbar offers all 36 original hands, a laser, mute, ink color, clear, stop and quit. Settings control size, ink width/lifetime, shadow and six tap sounds. Zero ink lifetime retains ink until cleared. Closing Settings hides it to the tray; Quit exits. Use the toggle shortcut to interact with other applications.

Preferences persist in eframe's platform-standard application data directory. Upstream Swift preferences are not imported or modified. Desktop layouts are rediscovered when activating the pointer. Stop and restart after connecting a monitor or changing its resolution. Ink is limited to the latest 256 strokes, with 50,000 sampled points per stroke, to bound memory use.

## Port boundaries

This edition implements the pointing/drawing workflow. It does **not yet** reproduce these optional Swift features:

- Hold a modifier to temporarily pass clicks through while keeping the pointer visible.
- Custom-hand image creation/import, pinned toolbar ordering, configurable hotkeys, or pointer-matched ink colors.
- Launch at login, Sparkle updates, and the macOS-specific welcome/permission flow.
- The original spring/smoothing animation and full blurred hand shadows; Rust uses the same artwork with direct aiming, a pressed pose and a rod shadow.

The overlay uses normal always-on-top desktop windows. Protected/secure desktops, exclusive fullscreen applications, macOS system UI and other Spaces may cover it. Mixed-DPI, multiple-monitor positioning and screen-recording visibility need real-hardware acceptance on each OS. Shortcut registration conflicts are reported as startup errors rather than silently disabling the shortcuts.

## Development

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

The tests cover ink clearing/fading, preference validation, rendering geometry, and decoding/measuring every bundled hand and sound. They do not assert native window transparency, tray visibility, global keyboard delivery or speaker playback.

Local verification on **2026-11-01**: all six release tests passed on macOS ARM64; release Clippy passed with warnings denied; formatting, bundle plist and ad-hoc signature checks passed. The native UI was opened and hand tracking, drawing a stroke, stopping with Escape, saved preferences and the Settings accessibility tree were observed. Windows cross-compilation was checked with `cargo check --locked --target x86_64-pc-windows-msvc`; a check is not a linked executable or a runtime test. Linux CI and Windows/Linux runtime acceptance have not run here.

The user confirmed that **Ctrl+Option+H works in both directions on a physical Mac keyboard**. Automated input did not produce Carbon callbacks even in an independent AppKit control, so those synthetic events were not used as the keyboard acceptance oracle. No local-key fallback was added. Multi-monitor/mixed-DPI behavior, composited transparency, screen capture and audible output remain hardware acceptance checks. The temporary diagnostic applications were removed.

- `rust/src/main.rs`: application, tray, shortcuts, settings, per-screen windows.
- `rust/src/model.rs`: bounded drawing state, fading and artwork measurement.
- `rust/src/render.rs`: textured hand/rod rendering.
- `rust/build.rs`: resizes the original artwork to 1024 pixels and embeds all hands and tap sounds; no runtime asset downloads.
- `.github/workflows/build.yml`: native lint/test/build/package jobs.

`Sources/`, `Package.swift`, `README.swift.md` and the original scripts are retained as upstream references. `scripts/legacy-swift-release.yml` is an archived workflow, outside `.github/workflows`, and never runs. `docs/` is the original upstream website, not a Rust download page. The Rust baseline was ported from upstream commit `f243657fcbbd0613e004908d188042c14dbb53a5`.

## License

MIT. See [LICENSE](LICENSE). Original code, artwork and sounds retain the upstream copyright notice.

# No More Dee Pee Eye

A lightweight native Windows desktop interface for [Zapret2](https://github.com/bol-van/zapret2), written in Rust and Slint. It helps manage domain-scoped TLS desynchronization profiles through a small, dark UI.

## What it does

- Starts and stops a pinned Zapret2 runtime through an elevated helper.
- Applies one of three TLS profiles to a user-managed domain list.
- Verifies the bundled runtime before use and validates a profile before interception starts.
- Keeps the UI unprivileged; the helper uses authenticated local named pipes and a Windows Job Object to manage the engine lifetime.
- Provides HTTPS checks, bounded in-memory logs, a system tray menu, and an optional UI-only startup entry.

No More Dee Pee Eye does not change DNS, proxy, firewall, or IP settings. The current profiles target TLS over TCP port 443; QUIC/UDP and HTTP port 80 are outside the scope of this release.

## Download and run

Download and run `NoMoreDeePeeEye-0.1.4-windows-x64.exe` from the GitHub release. It contains the pinned Zapret2 runtime and prepares it locally on its first launch. A ZIP is also available for people who prefer a portable folder.

1. New installations start with Steam domains. Open **Configuration** to add or change domains, one per line.
2. Select a TLS profile and save it.
3. Click **Connect** and accept the UAC prompt when Windows asks to start the engine helper.
4. Use **Disconnect** or the tray menu to stop Zapret2.

Closing the window sends the application to the tray. The application keeps existing settings in `%LOCALAPPDATA%\Umbra` so upgrades retain the prior configuration.

## Build from source

Requirements: Rust 1.90 or later, MSVC Build Tools, and the Windows SDK. Node.js and WebView2 are not required.

```powershell
.\scripts\fetch-engine.ps1
cargo test --locked
.\scripts\package.ps1
```

The portable output is written to `dist\NoMoreDeePeeEye`. For a development run:

```powershell
cargo build --locked
New-Item -ItemType Directory -Force .\target\debug\runtime
Copy-Item .\runtime\* .\target\debug\runtime -Recurse -Force
.\target\debug\no-more-dee-pee-eye.exe
```

## Verification commands

```powershell
# Render all four pages without starting the engine
.\target\debug\no-more-dee-pee-eye.exe --screenshots .\artifacts\screenshots

# Verify the local IPC protocol without elevation or interception
.\target\debug\no-more-dee-pee-eye.exe --ipc-self-test

# Validate all three profiles with Zapret2 (requires UAC; no interception)
.\target\debug\no-more-dee-pee-eye.exe --check-engine .\artifacts\engine-check.txt
```

## Repository layout

- `ui/app.slint` — interface theme and screens.
- `src/app.rs` — UI callbacks, controller, HTTPS probe, and logs.
- `src/config.rs` — settings schema and domain normalization.
- `src/engine.rs` — runtime verification and Zapret2 process management.
- `src/platform.rs` — UAC, local IPC, Windows Job Object, and single-instance support.
- `scripts/package.ps1` — portable release packaging.
- `engine-manifest.json` — checksums for the pinned runtime.

## Runtime and licensing

The official Windows runtime bundle is pinned at commit `6eb463a6758fb48cd101bc55dfd057e6e9d98af1` of [zapret-win-bundle](https://github.com/bol-van/zapret-win-bundle). It is fetched only during a build; the application never downloads binaries at runtime.

No More Dee Pee Eye is licensed under the [GNU General Public License v3.0](LICENSE). See [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for notices covering Zapret2, WinDivert, Cygwin, Slint, and Inter.

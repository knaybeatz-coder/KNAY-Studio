# Michael Lane customized edition - 31 October 2026

Lemon Studio is an existing GPL-licensed project by ItsAlphaNeon and its contributors: https://github.com/ItsAlphaNeon/Lemon-Studio . This package contains Michael Lane's customized edition, not a claim to original authorship or an official upstream release. See LICENSE and THIRD-PARTY-NOTICES.txt.

## Source and build
Linux release 2026103102: x86-64, glibc 2.36 baseline, cross-compiled on Windows with cargo-zigbuild 0.24.0 / Zig 0.13.0 and Debian 12 ALSA development files. Compiled and packaged only; not run or audio-tested on Linux, at the owner's request. For a native build install Rust, a C/C++ toolchain, pkg-config, and ALSA development headers; set OPENSTUDIO_NO_PLUGIN_BUILD=1 and run `cargo build --locked --release -p os-app`. The local mp3lame-sys patch preserves native builds and fixes Linux ABI/header selection when its build script runs on Windows.

Cross-platform download page: https://michaellanemusic.de/lemon-studio/ . Windows supports automatic installation; Linux and Mac Update buttons open the download page. Mac release instructions and packaging scripts are in packaging/mac. A Mac binary is available only after a native Mac build and publication.

This archive includes the corresponding modified source and required embedded assets. No external plugins, personal projects, recordings or device configuration are required. Cargo.lock pins dependencies; network access is required to download them. Rust 1.99.1 was used for the Windows build.

Windows: install Visual Studio C++ Build Tools/Windows SDK and LLVM. Set LIBCLANG_PATH to LLVM/bin, OPENSTUDIO_NO_PLUGIN_BUILD=1, then run `cargo build --locked --release -p os-app`. The bundled ASIO host files are selected by .cargo/config.toml. This builds the standalone application with linked native effects; it does not build separate VST/CLAP plugin binaries.

macOS: use native Rust and Apple Command Line Tools, set OPENSTUDIO_NO_PLUGIN_BUILD=1, run the same cargo command, then `cargo run --locked --release -p bundle-app -- --help` for bundling options. A Mac binary must be built/tested on the Mac; this Windows package includes no verified Mac binary.

Installer: packaging/windows/LemonStudio.nsi uses NSIS 3.11. Stage lemonstudio.exe, LICENSE, THIRD-PARTY-NOTICES.txt, READ ME.txt, Lemon Studio Source.zip and Lemon Starter.flp in PAYLOAD; stage the licensed x64 Microsoft.VC143.CRT redistributable DLLs in PAYLOAD/runtime (from Visual Studio VC/Redist, not System32); pass /DPAYLOAD and /DOUTPUT to makensis. TEST_INSTALL uses a separate uninstall key and a shortcut inside the isolated installation folder. Install/uninstall leave user projects and preferences intact.

## Changes in this edition
Customized channel strips, faster controls, mixer section navigation and drag reordering; linked Playlist audio/MIDI/take groups; take recording/comping; live clip edits; monitor queue and start/stop smoothing; browser search; joined-window layout; startup presentation; tap tempo; 16-point live/512-point offline sampler quality. New users start with Lemon Starter, an empty native-only copy of the personal mixer layout. Personal template migration is not included in the public binary.

## Focused verification
Synthetic/offscreen checks cover drag/drop, linked Playlist rows, undo/redo, plugin state save, native sends and sync timing, vocal sidechain ducking, factory defaults and preservation of explicit preferences. Hardware recording, subjective sound and visual acceptance are user checks. The Windows installer is unsigned. No payment, activation licensing or analytics was added. This build checks the Michael Lane Music update feed automatically; see AUTOMATIC_UPDATES.md. The bundled SnapTune integration is AGPL-3.0; full corresponding source and licence notices are included.

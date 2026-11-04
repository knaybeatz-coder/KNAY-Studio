# Lemon Wave standalone (Windows)

Connect and switch on the MIDI keyboard before opening **Lemon Wave.exe**. This runs Wave directly, without Lemon Studio, a plugin scan, or an on-screen keyboard. The synth, modulation and Table Edit pages are the same shared implementation as the plugin. Wavetables and included recordings remain inside the instrument window. Close Wave normally when finished.

The Desktop copy is configured for **LPK25**, using the Windows default audio output, 48 kHz and a 256-sample processing period. Live latency also depends on Windows/device buffering. Notes, velocity, sustain pedal and pitch bend use the existing synth MIDI path. Connect devices before launch; restart Wave after changing devices. Close another program if it has exclusive use of the MIDI keyboard or output.

Optional **Settings.json** beside the app selects `midi_input` and `output_device` by exact device name. A null output uses Windows' default. A null MIDI input selects the first available MIDI input (which may be virtual). Other optional settings: `sample_rate`, `period_size`, `dpi_scale`. The installed file explicitly selects LPK25 to avoid the VoicePilot virtual input. No microphone/audio input is configured. The standalone does not save a DAW project; use Wave's User Banks for edited tables.

Headless checks: `--build-info <file>`, `--list-midi <file>` (enumeration only), `--self-test <file>` (synthetic engine rendering only). These do not open the editor or audio stream. Advanced NIH standalone command-line options remain available; passing any such arguments bypasses Settings.json device choices.

Build from the repository root:

```powershell
cargo build --locked --release --manifest-path tools/lemon-wave-standalone/Cargo.toml
```

This tool has an isolated Cargo workspace/lock because NIH's standalone audio dependencies differ from the DAW's CPAL generation. It depends on the existing Lemon Wave source; there is no separate synth copy. The recorded library build input follows the plugin's existing LEMON_WAVE_LIBRARY/LOCALAPPDATA build rules and is not stored in Git. Windows personal Desktop delivery only; no public publication or Mac/Linux standalone verification.

# Current local update: Wavetable Workshop

Same-window drawing, 64 harmonic bars, 1-128 editable/reorderable frames, undo/redo, recording-section extraction, stacked morph preview and named user banks. See [WAVETABLE-WORKSHOP.md](WAVETABLE-WORKSHOP.md). All edits remain drafts until Apply; existing installed sounds and 109 host controls are preserved.

# Current local build: included recordings and inline dropdowns

The installed Windows build embeds 34 Mini-Moog and 226 800DV recordings. Same-editor dropdowns replace the browser panel. Raised controls, recessed displays, frame morph previews and unison Blend visualization cover both pages. See [SOUND-LIBRARY.md](SOUND-LIBRARY.md) for use, build inputs and compatibility. Earlier notes below are historical.

# Current local update: Sound Browser

Six named standard waves, visible wavetable selection, and a grouped local recorded-synth library. See [SOUND-LIBRARY.md](SOUND-LIBRARY.md). Still 109 stable host controls; custom sounds and root pitch are stored as patch assets.

# Current local development: four drawable LFOs, eight routes, four macros

109 parameters and fourteen factory choices. See [MODULATION.md](MODULATION.md) for the new controls, workflow and verification limits.

# Previous envelope/filter development update — 3 November 2026

Lemon Wave now has 72 parameters, eleven factory choices in its dedicated editor, selectable curved AHDSR envelopes and six reconstructed nonlinear SVF paths. Choose **Curved AHDSR** at the top (takes effect on the next note), then drag Hold / Attack curve / Decay curve / Release curve in either envelope panel. Curve 50% is linear; sustain is squared in this mode. Attack/decay/release retain Lemon's existing 1 ms–10 s time controls; Hold spans 0–32 s. Curves/durations latch at stage entry; continuously retiming an active stage is not implemented. New presets: Velvet Pluck, Held Bloom, Resonant Band.

Filter / Mix combines Low/Band/High pass with Classic, SVF 12 or SVF 24. New SVF paths include nonlinear drive, resonance compensation and 2x internal processing. Lemon's voice routing, input gain, interpolation and output processing remain its own; these are not full Serum filter/audio equivalence. See research/envelope-filter.md for the evidence and limits.

Existing 44-, 50- and 62-parameter native patches migrate with Classic envelope/filter defaults. Three captured old patches render bit-identically. The original oscillator controls and IDs remain intact. The host app itself is not rebuilt by this plugin update.

The historical stage notes below describe earlier checkpoints. Current runtime evidence supersedes their initial static-only investigation status.

# Lemon Wave

An original wavetable instrument for Lemon Studio, developed in stages at Michael's request. The first stage is a Windows CLAP/VST3 instrument, not a complete Serum clone. No Serum code, factory tables, presets or artwork are included.

## Stage 1

Two oscillators, each with four analytic wavetable families (five interpolated frames), table position, gain, semitone tuning, 1–7 unison voices, detune, pan and phase bend. Twenty-four polyphonic voices, sub and noise sources, stereo spread, a resonant state-variable filter, drive, amplitude/filter ADSRs, four LFO shapes with four destinations and host-tempo rate divisions, stereo chorus and ping-pong delay. Eight original factory patches. Forty-four stable host parameters; CLAP and VST3 state recall and MIDI sample offsets.

The engine precomputes harmonic-limited table levels outside audio processing. It uses interpolation and 10 ms continuous-control smoothing. Phase bend and nonlinear drive/output can still introduce aliasing; oversampling and audio-reference comparisons are future work. LFO tempo sync means tempo-relative rate with note retrigger, not host-song phase locking. Fixed voice stealing can produce a transient at the polyphony limit. Delay-time changes can pitch the tail. These are explicit first-stage limitations.

The editor uses Lemon Studio's existing parameter gestures, automation hooks and preset wrapper. Factory `.ospreset` files select the CLAP identity. The VST3 build can save its own host presets, but does not load CLAP preset headers directly.

## Stage 2a — WAV import and embedded tables

Each oscillator now has **Import WAV**, **Cycle**, and **Use built-in** controls. Choose the number of samples in each cycle before importing (64, 128, 256, 512, 1024, 2048 or 4096; default 2048). Position scans the imported frames. The Bank knob takes effect again after Use built-in. The two original example files in `wavetables` use 2048 samples per cycle and eight frames each.

Supported input: mono/stereo RIFF WAV, PCM 8/16/24/32-bit or IEEE float32, up to 16 MB and 128 whole frames. Stereo channels are averaged. Files with incomplete frames, non-finite samples, unsupported formats or silence after DC removal are rejected without replacing the current sound. This imports periodic waves, not arbitrary recordings with automatic pitch/cycle detection. Frame size is explicit; vendor metadata does not override it.

Imported frames are Fourier-resampled to 1024 samples, DC/Nyquist bins removed, and harmonic-limited levels prepared before playback. One peak-reduction gain preserves relative frame levels; quiet frames are not individually boosted. The engine retains up to 256 harmonics at its richest level. Files are embedded in saved plugin state, so projects and new user presets reopen without their source WAV. Forty-four parameter IDs and ranges remain unchanged. The updated eight starter presets carry full CLAP state, restoring built-in tables as well as controls. Older parameter-only presets cannot clear assets: use both Use built-in buttons before loading those older copies.

Import/state recall is a discrete sound change: it may pause briefly while the host restores and reinitializes the instrument, and resets held notes/effect tails. Import between notes. Normal rendering does no file reads or FFT construction; table snapshots use a nonblocking read, and retired tables remain owned outside rendering. Bad embedded data falls back to built-in tables and reports an error in the editor. Oversampling/warp aliasing, hand drawing and spectral editing remain future work.

Focused tests cover supported WAV formats/cycle sizes, truncated/invalid input, Fourier downsampling, frame interpolation, harmonic rejection, exact audio recall after deleting the source WAV, legacy state clearing, retired asset ownership, and offscreen import controls. `verify_host` additionally verifies embedded table audio recall in both actual Lemon Studio host formats. Live file-dialog/editor interaction and listening remain the user's check.

## Build and verify

`cargo test --locked -p lemon-wave --lib`

`cargo build --locked --release -p lemon-wave`

Package `lemon_wave.dll` as `Lemon Wave.clap` and as `Lemon Wave.vst3/Contents/x86_64-win/Lemon Wave.vst3` on Windows. `tools/bundle-plugins` also knows the module.

`cargo run --locked -p lemon-wave --example verify_host -- BUNDLE_DIRECTORY` tests both actual Lemon Studio hosting layers without opening a GUI or audio device. `render_presets` takes bundle, preset and output directories and validates/renders all patches offline. `LEMON_WAVE_PREVIEW` selects an offscreen test image directory.

## Next stages (not implemented)

1. Continue wavetable editing: frame editing/drawing, spectral editing, table export, higher-quality warp/oversampling, phase/randomization controls. WAV import and embedded tables are implemented above.
2. Additional expression sources, editable envelope points, mono/legato/glide. Drawable LFOs, macros, drag/drop routing and modulation displays are now implemented; see MODULATION.md.
3. Expanded filter/routing and reorderable effects rack: EQ, distortion modes, compression, reverb, phaser/flanger, delay synchronization and additional chorus modes.
4. Serum 2-style broader synthesis options: third oscillator, sample/multisample, granular and spectral engines; arpeggiation/sequencing; larger preset browser and sound library.
5. Listening, spectral/aliasing tests, CPU profiling, automation stress and user-guided interaction refinement. Mac/Linux builds require their own verification.

## Reference investigation

Installed target: Xfer Serum 2.1.5, native Windows x64 VST3, 19,236,864 bytes. SHA-256 `33369e196ae66d05d20485f3c5e800707dc34a5ad89c27959250d60387b2ea55`. Version and classes are observed in `Contents/Resources/moduleinfo.json`. REA 6.8.0/Ghidra 12.1.4 performs static inspection only; it has not run Serum or established audio equivalence. The initial cold import exceeded 330 seconds. Evidence/results from the extended attempt are recorded in the project handoff and Desktop research folder.

Xfer's official feature description establishes that Serum 2 includes wavetable, multisample, sample, granular and spectral modes: https://xferrecords.com/web-manual/serum-2/exploring-sound-design-in-serum . Its routing documentation describes three oscillator sources plus sub/noise and two filters: https://xferrecords.com/web-manual/serum-2/routing-an-oscillator-or-filter . These are published behavior descriptions, not proof of recovered internal algorithms. Lemon Wave's DSP is an independent implementation; remaining parity claims are unknown.

## November modulation upgrade

109 controls, four drawable LFOs, eight routes, four macros and live voice displays. Fourteen embedded factory choices. See MODULATION.md. Earlier stage descriptions are historical.

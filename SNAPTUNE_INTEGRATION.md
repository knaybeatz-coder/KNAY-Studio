# Native vocal tuning

SnapTune DSP upstream: https://github.com/NicoFerni/snaptune
Pinned revision: 40c7f5aa23387b7c3567bd11c971bed0e339a198.
License: AGPL-3.0, retained in crates/os-dsp/vendor/snaptune/LICENSE and licenses/SnapTune-AGPL-3.0.txt. This is a new copyleft dependency; future distribution packaging must include its notice and corresponding source and account for the combined-work AGPL terms. No public installer or Mac archive was updated here.

DSP adaptation and source changes are documented in vendor/snaptune/README.lemon.txt. Only DSP is incorporated: no JUCE code, plugin telemetry or network update checks. Local Support.h supplies ordinary FFT, smoothing, constants and clamp operations. Build uses the existing cc crate/toolchain. C ABI instances are exclusively owned by Rust and transferred across engine rebuilds; no shared DSP mutation or processing-thread allocation.

Vox layout alone enables access (legacy channel defaults32-41). Tune precedes Low Cut, Gate, Drive, EQ, TAME, compressor and DS, then effect slots/fader/routes. Live mode only; the native reported nominal delay (130 samples at48kHz) participates in mixer delay compensation. Pitch-synchronous live processing has signal-dependent timing; this is not zero-latency tuning.

Switch defaults OFF and strength defaults0. OFF bypasses the original audio exactly, reports zero tuning delay, and preserves all settings. Explicit Learn while OFF analyses a scratch copy without delaying or changing output. Strength controls correction amount0..1, retune200*(1-strength)^2 milliseconds, and humanize0.5*(1-strength). Voice menu maps Male to lowMale65-600Hz and Female to soprano110-1400Hz; these are pitch ranges, not voice transformations.

Learning is a Lemon addition, not an upstream feature. Duration-weighted pitch classes from SnapTune input detection are matched to major/minor tonal profiles after at least6 seconds of voiced content, at least4 significant pitch classes and correlation>=0.45. Silence, insufficient notes and weak evidence leave Listening active; click again cancels. Result is a suggestion, not proof of a unique key. Manual menu offers12 roots and13 scales. It learns this channel's melody, not the whole song; use isolated vocals. Learning requests are transient and not serialized. Settings and switch persist via LSCSv6; earlier chunk versions remain readable. Strength automation uses mixer strip control8. Undo/redo covers amount, settings and switch.

Tests are synthetic/offscreen only; no microphone, GUI automation or device listening. User must audition actual vocals and validate operating-system presentation.

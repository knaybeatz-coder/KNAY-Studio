# Oscillator reconstruction — 3 November 2026

`src/reference_oscillator.rs` is an evidence-backed, compiled reconstruction core.
The shipping plugin still uses the earlier independent `dsp::Engine`. This module
is NOT yet connected to its voice engine and does not establish whole-Serum parity.

Analyzed target: installed Serum2.1.5 x64 VST3, SHA256
`33369e196ae66d05d20485f3c5e800707dc34a5ad89c27959250d60387b2ea55`.
REA/Ghidra12.1.4 observations and synthetic offline renders were used. No GUI,
audio device, microphone, personal preset or running DAW session was accessed.

## Implemented and measured

- Analytic2048-sample Init saw (half-cycle rotation relative to an ascending ramp).
  Its identity is supported by output comparison, not a claim that a particular
  vendor WAV is loaded. No vendor WAV, skin or preset is included.
- Spectral truncation, shrinking mip tables, float32 reciprocal rate promoted to
  double phase,16-tap512-phase interpolation with observed SIMD summation order.
- Bend+/−/both, Asym+/−/both, PWM, Flip, Mirror and Quantize. PWM selects full-table
  linear interpolation; the other tested modes select the mip/sinc path.
- Recovered2x output decimator (10 nonzero coefficients in three SIMD sections).
- Upstream double level ramp with float32 coefficient
  `1/(sample_rate/44100*1500+1)`, snapping below0.001; squared level followed by
  per-oversampled-sample float32 ramp coefficient0.05.

34 complete waveform comparisons passed:31 steady A4 oscillator cases at48kHz
(Off plus10 modes at25/50/75%), and level1→0.5 changes at44.1/48/96kHz.
Relative RMS errors below3e-7, maximum absolute error below4.2e-8 in these cases.
Acceptance limits were1e-5 relative RMS and1e-6 peak absolute error. Steady checks
compare seconds2–3; step checks compare seconds1–3. Note-on envelopes are excluded.
The offline reference executable is `examples/render_reference_oscillator.rs`.

`oscillator-fixtures.bin` contains eight256-sample mono excerpts from the actual
offline Serum recordings. Metadata records source hashes and sample positions.
The regression test checks five warp cases and all three level-change rates.
Two focused library tests passed; no whole-application test claim.

## Evidence addresses

- Lookup0x1803d0980, builder0x1804409e0, interpolation initialization0x1804408d0.
  `reference_interpolation.bin` stores the observed512x16base then512x16delta
  coefficients as float32LE, generated from retained coefficient evidence.
- Phase rate0x1803c87d0; amount/warp0x18028c980/0x18028ca50; PWM dispatch0x1803d1b10.
- Output0x180063840, REA ev_eeb32eb4a970e9cbb57aa540b7184611cdc137ce0d57e2de50c388c394938896.
  Constructor coefficients0x18003fe42–0x18003fec7, within0x18003dcf0.
- Rate law0x1800490c0, ev_f6f14f4be845df17641bbd182e88b4f847671b9891bd2698f6aad3aab947e64f.
  Upstream ramp0x18007b000, ev_c04f254f83b72f9c68d0099ecd7727b7997e925f38611d745815720762a0f3c6.
  Local gain0x1803d17f0, ev_4af8edc169932ec2de7cf93314fc19ab56c25b34217c283f5d0c6e1d9ab10e92.

## Remaining integration gates

Runtime comparisons do not yet cover arbitrary imported tables, changing mip
levels, changing warp modes/amounts, note starts/releases, full stereo unison,
other warps, filters or effects.64-sample mip crossfade is implemented from static
evidence but has not passed a dynamic reference comparison. Shared random state
is seeded with5489 in the observed constructor, but complete draw ordering and
reset behavior remain unverified; no RNG sequence is installed in this module.
Avoid silently changing saved patches to this new engine. Establish voice/state
migration and full-path comparisons before switching the playable plugin.

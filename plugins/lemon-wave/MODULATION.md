# Modulation upgrade - 3 November 2026

Open MODULATION beside SYNTH. Draw directly in any LFO graph. Each curve has 65 interpolated points. Sine, Triangle, Ramp and Square replace the shape; Undo restores its previous edit. LFO1 switches to Drawn automatically; select Classic for the original shape selector.

Free sync uses the Hz knob. Tempo divisions support straight, dotted and triplet timing. Note loop restarts per note; Free run shares a continuously advancing clock; One shot runs once and holds its last value. Tempo-relative timing is not host-song phase locking.

Drag an LFO, envelope or macro label onto an oscillator, filter or output destination knob. Hover SYNTH during a drag to reveal those knobs. New routes start at +25%; adjust signed depth in the matrix. Clear removes a route. Identical routes are retained; unsupported drops cancel; full matrices report no available slot.

Eight routes, thirteen sources and twenty-one destinations. The first two rows retain original restricted selectors for automation compatibility; six new rows offer all choices. Four macros are host-automatable. Rings and playheads follow the newest active voice.

Custom curves are embedded in state. Finishing a curve edit restores host state and can restart held notes/effect tails; draw between notes. Older 44/50/62/72-control native states restore original defaults and clear custom curves. Parameter-only presets cannot carry custom assets.

New header presets: Motion - Glass Tide, Motion - Clockwork, Motion - Four Corners.

Verification: 21 Wave and 29 shared-editor tests passed (one existing ignored). Software-rendered bounds, drawing/undo and cross-page routing checked. Actual CLAP/VST3 hosts checked 109 controls, state/audio recall, MIDI offsets, release/reset and embedded curves/tables. Synthetic tests verify all four LFOs, macros, one-shot/free clocks and block-size invariance. User live interaction/listening remains; no microphone or audio device was used.

Reference scope: retained Serum evidence informed investigation. 0x1803cabb0 is registration and 0x1803fe170 dependency activation, not recovered waveform algorithms. A fresh offline probe recorded 36 actual Serum LFO-rate displays across four LFOs. This is not modulated-audio parity evidence. Curve storage/interpolation, triggering, macro mapping and routing here are original Lemon implementations. No complete Serum reconstruction or LFO audio equivalence is claimed.

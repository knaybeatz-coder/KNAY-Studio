# Lemon Wave live development workflow

## For Michael

Open **Lemon Wave Live Preview** on the Desktop and leave it open while we work. The preview draws the actual Wave interface. It is silent; use **Lemon Wave Development** with your LPK25 for playing and hands-on interaction. The normal **Lemon Wave** shortcut remains the installed stable version.

The preview shows the current real editor design at full width, with Synth, Modulation and Table Edit navigation. There is no palette, finish or parameter-editing sidebar. Design changes are made collaboratively in the shared design file and refresh automatically. The picture itself is for visual review, not native control interaction.

We can also edit the design file directly: the preview watches it. The Development app reads that same file while open, so these appearance changes do not need compilation there either. Nothing automatically replaces or closes the app you are playing.

**When you like the result:** ask to finish/install the update. That is when we compile final versions, run focused checks, update the normal installed app/plugins and synchronize the finished source. This setup does not automatically publish anything.

## What updates immediately

`plugins/lemon-wave/design/live.json` is the shared design definition. Supported fields:

- `colors`: maps original six-digit RGB hex values to replacement RGB values. Defaults use the existing palette. Examples: background `101319`, panels `25303a`, text `e8edf3`, oscillator A `e3df8b`, oscillator B `73dacc`, modulation `bda4fb`.
- `radius_scale`: 0–2.5 for raised panel/button corners.
- `bevel`: 0–0.4 for raised panel/button shading.
- `shadow_depth`: 0–12 logical pixels.
- `font_offset`: -3–4 for supported editor text sizes.
- `knobs`: parameter ID to `[horizontal_offset, vertical_offset]`. Changes painting and hit area together. Check overlap visually.
- `selectors`: parameter ID to `[x,y,width,height]`. Changes painting and hit area together for parameter dropdowns. Source/library menus retain their existing layout.

Bad/incomplete JSON keeps the last good view. Regular finished builds embed this JSON; they do not read a live file. Only the separately built development app enables external-file refresh. This is not a general no-code layout engine: new controls, arbitrary section rearrangements, new interactions and audio behavior still require Rust changes.

## Agent/developer workflow

1. Read the project handoff and preserve all customization. For supported design edits, change `design/live.json`; do not rebuild or reinstall on each adjustment. The preview polls every400ms; the native development editor reloads on UI paints, at most every300ms. Actual browser/device timing varies.
2. For Rust drawing/interaction changes, run **Build-Preview.ps1** with Source, Runtime and Target. It builds just the preview renderer, stages it under a new versioned workspace path and updates a pointer. The open preview swaps its own rendering worker automatically. The browser stays open. No full DAW build or plugin packaging is involved.
3. For audio/native interaction changes, run **Build-Development.ps1** with Source, Destination and Target. It builds only the standalone with incremental development settings, stages a new executable and updates `development.json`. The currently running app is not replaced or killed. User closes/reopens Development when ready to test the new build.
4. Once approved, use normal focused verification, final compilation and installed delivery. Commit intended changes and run source synchronization. Publication needs separate authorization.

PowerShell helpers require PowerShell7 (the delivered shortcuts select it). Source and build caches remain in the existing checkout/workspace; finished development app copies and user instructions go on Desktop. The standalone Cargo workspace is deliberately isolated from the DAW audio dependency versions.

## Implementation and boundaries

The persistent `wave-preview` process calls the real TableEditor/Logic/Canvas renderer. Python serves the resulting PNG and page navigation on loopback `127.0.0.1:18765`; it has no public listener or audio/MIDI stream. Requests that write designs require the local page token and matching host/origin. There is no screenshot capture or GUI automation. The browser page is not a separately reimplemented synth UI.

The development app uses LPK25 input and follows the Windows default output, as requested. Desktop Workshop Settings.json has output_device set to null. The real native app remains the authority for audio, MIDI, menus, text entry, dragging and saving banks. Visual preview refresh times do not imply audio latency. Initial compilation is required once; subsequent design edits require no compilation. Incremental Rust build times depend on which code/dependencies changed.

## Modulation handles
In Development, hold and drag the small grip-and-arrow icon in an envelope or LFO header onto a supported oscillator, filter or output control. A pulsing marker and short fading trail follow the pointer; valid destinations glow. A successful new connection emits a brief teal ripple. No text follows the pointer. LFO2-4 and macro handles are on MOD; hover SYNTH while dragging to cross pages. Edit assigned depth and remove routes in the MOD matrix. The old Drag Source strip is removed. The Live Preview remains visual only.

## Tabbed envelope and LFO editors
The Synth page now has ENV 1 / AMP and ENV 2 tabs, plus LFO 1–4 tabs. Tabs select the visible editor without changing its sound settings; selected-tab drag handles route the corresponding source. Tab selection is temporary UI state.

Envelope controls: attack, hold, decay, sustain, release and three curve controls. Drag graph points horizontally to adjust stage time; the decay/sustain point also moves vertically to adjust sustain. The release point sits halfway along the release segment. Hold has a graph handle when its segment is wide enough. Hold and curve shaping require Curved AHDSR; the nearby engine menu and top engine menu control the same mode for both envelopes. ENV 2 also retains its direct To filter amount.

Each LFO can be drawn in the large graph. Sine, Triangle, Ramp and Square replace the current curve; Undo restores its last edit. Copy/Paste transfers the displayed shape between tabs. LFO1 switches to Drawn shape when drawn or pasted; its classic shape and direct-depth/target controls remain available. All four LFOs expose free rate, beat sync, Note loop, Free run, One shot, and Straight/Dotted/Triplet timing. Drawn shapes are stored with the patch. Detailed routes/depth remain on MOD. Existing two-envelope/four-LFO engine and parameter identities are preserved.

## MIDI ownership and closing Wave
Use the Desktop shortcuts for Development and the stable Lemon Wave standalone. They now share a single-instance launcher: opening again focuses the existing Wave window instead of opening another MIDI client. After the last Wave window closes, the launcher allows two seconds for normal shutdown and then ends its own leftover child process if necessary, releasing MIDI/audio handles. It never closes Lemon Studio or unrelated apps. Launching the executable directly bypasses this shortcut protection.

Development always loads the Workshop Settings.json (currently LPK25). Use the keyboard in one application at a time: release LPK25 in Lemon Studio, or close Studio, before playing the standalone. Likewise close standalone Wave before selecting LPK25 in Studio. Live Preview does not open MIDI. The current design iteration is in Development; the built-in Studio instrument is not automatically replaced.


## Compact oscillator effects rack (3 November 2026)
Development now includes Distortion, OTT Comp, Chorus, Delay and Reverb inside the existing Synth effects panel. Click each row's A/B buttons to assign that effect to either oscillator, both, or neither. Drag each value vertically; Ctrl gives fine control. Each row has its own Mix; zero is dry. Routing and amounts are normal saved/automatable parameters.

The fixed chain is Distortion > OTT Comp > Chorus > Delay > Reverb. OTT Comp is an original three-band upward/downward compressor, with Depth, envelope Time and Mix. Effects share their row settings but keep separate stereo processing histories for A and B. In independent routing, Sub/Noise remain outside the effects rack. Existing patches with both oscillators assigned, only the original chorus/delay and no new effects retain the original summed processing path; enabling a new effect or changing routing crossfades into isolated oscillator/filter paths. Consequently shared nonlinear filter interaction changes in that mode. No third-party OTT emulation claim.

The current Development pointer is updated without stopping an open app. Close and reopen Development to load this code update. Stable Wave and the DAW are separate installations. Audio/MIDI listening and native interaction acceptance remain for Michael.


## Addable effects rack (3 November 2026, supersedes the compact fixed rack)
Use + ADD FX inside Synth's EFFECTS panel. The rack holds up to 16 modules, including repeated types. Each card has ON/OFF, oscillator A/B assignments, up/down reorder buttons and remove. Scroll over the rack with the mouse wheel; wheel movement scrolls rather than changing a knob. The scrollbar can also be clicked. Drag knobs vertically; Ctrl gives fine adjustment. The chain follows card order and each slot retains its automation identity when reordered.

13 core module types: Distortion, Compressor, Chorus, Delay, Reverb, Equalizer, Filter, Flanger, Phaser, Hyper/Dimension, Frequency Shifter, Convolve and Utility. These are original implementations informed by the module categories in https://xferrecords.com/manual/serum-2, not exact Serum DSP emulations. Parallel/nested splitter buses are not implemented in this oscillator rack. Convolve currently uses four original generated sparse FIR responses; external IR loading is not included.

Delay modes: Stereo, Ping pong, Mono. Timing: free milliseconds, quarter/eighth/sixteenth/thirty-second notes, dotted eighth, eighth triplet, sixteenth triplet. Plugin mode follows host tempo; standalone uses its transport tempo (120 BPM unless supplied by its host). The Free time knob is used only in Free ms mode. Feedback, tone, width and mix are per module. Reverb has Hall, Room, Plate and Spring flavors plus size/decay/damping/pre-delay/width/mix. Distortion offers soft clip, tube, hard clip, wave fold, rectify, bit crush, downsample and sine fold. Compressor offers multiband upward/downward OTT, downward-only and upward-only modes, with threshold/ratio/attack/release/makeup/mix.

New empty patches display an empty rack. Older patches keep their existing audio path until the rack is first edited; their active fixed effects are then converted into modules. The conversion keeps routing, wet amounts and analogous controls but is not a bit-identical reproduction after editing. Sub/noise stay outside independently routed oscillator effects. Modules share controls between A/B, while their processing buffers remain separate. Added slot parameters are appended after existing identities. No per-sample heap allocation; slot buffers are preallocated and reset without clearing large memory blocks. A zero-mix or fully bypassed oscillator module skips DSP and clears its tail.

Verified: module/mode audio, exact dry bypass, reset, delay beat timing and ping-pong direction, slot/oscillator isolation, repeated modules, ordering, capacity, add/remove/recall, actual editor knob gestures and wheel behavior, scroll clipping and clearances at 100%/150%, and real-renderer images. The installed offline check includes an eight-note compressor/delay/reverb chain; live listening remains Michael's review.

# ARA 2 source editing

Implemented 2 November 2026. Windows development integration; the user confirmed Melodyne ARA editing works. Broader playback/export verification remains outstanding. This is source-based ARA editing, not full arrangement/tempo-map exchange or a claim of compatibility with every ARA editor.

## User workflow

Right-click an audio clip > Edit with ARA > choose an advertised compatible VST3 editor. If it is absent, use Scan for ARA editors and reopen the menu after scanning. Opening an attached source uses its existing editor. The normal VST3 mixer insert remains available separately.

Copies and split clips of one audio channel share edits. Make Unique creates an independent document from the latest stored edits. The source file is unchanged. Remove ARA restores original playback and can be undone. Existing sampler preprocessing, envelopes, pitch and time stretch must be rendered/reset before attaching ARA; new timing/pitch edits belong in the ARA editor. Playlist trims, positioning, fades, clip gain and mixer routing remain host operations.

## Ownership and rendering

The pinned Apache-2.0 Celemony ARA API header (vendor/ara) produces platform-correct packed bindings with bindgen. The actual VST3 factory must advertise an unambiguous name-paired ARA class. IMainFactory and IPlugInEntryPoint2 negotiate ARA 2.0 final, generation 4. A shared weak registry initializes each binary/factory once while its documents exist. Factory and module stay alive through all documents and plugin objects.

Each audio-source channel owns one document, musical context, region sequence, immutable stereo source, modification and full-source playback region at time zero. Source/modification persistent IDs are stable within that document. Construction/restoration uses editing transactions, then enables sample access. Playback/editor roles bind before activation or editor creation. Both playback and editor renderers register the region; the editor view receives selection. Teardown removes registrations, destroys the VST3 instance, disables sample access, destroys model children then controller, and finally uninitializes the shared factory. Existing UI-thread retirement and crash containment protect hosted instance lifetimes.

Audio readers retain immutable Arc samples independently of the host reader factory, serve planar float/double data and zero-pad bounds. Archive callbacks use checked offsets, bounded storage (512 MiB) and error propagation. Model notifications are polled on the UI thread and captured into project/history state, outside the audio callback.

Source-relative voices render the ARA processor at trimmed source positions and combine up to 64 overlaps, with velocity, pan, individually smoothed clip gain and timed stops. Event and scratch storage is preallocated. The ARA path uses synchronous VST3 processing mode: realtime asynchronous seeks were observed to return transient silence with Melodyne. This prevents incomplete seek/export results; CPU cost and realtime responsiveness still need hands-on testing. Ordinary VST3 processing modes are unchanged. Song export and playback share the same hosted instrument path, with no second original-sample route.

## Persistence and failure behavior

AudioClip channels retain their kind and original sample_path, plus the Lemon ARA marker and a versioned binary envelope in plugin_state (FLP channel events 201/213). The header carries plugin path/class, factory/archive IDs, source frame count/rate/content fingerprint and archive checksum; opaque plugin archive bytes follow it. Existing/new FLP files write the archive; removing ARA removes it. Standard component presets are not ARA archives.

Restore validates identity, compatible archive IDs, checksum and decoded source identity before binding. Missing/incompatible plugins or changed source audio leave the saved archive intact and report fallback to original playback. Archive failure aborts project saving and retains the previous project file. An unarchived editor change keeps the project dirty. Undo/reconciliation recreates only changed document instances; copies preserve archive bytes. Source relocation is accepted when decoded audio is identical.

## Current boundaries

- The editor sees a full source at time zero, with a 120 BPM source-relative render context; host arrangement positions, tempo/signature maps, MIDI/chord content and transport commands are not exchanged. Optional host content-access and playback-controller interfaces are absent.
- ARA timing/pitch edits are heard within the original source span; automatic extension beyond source duration is not implemented. Sampler pitch/stretch controls do not override ARA playback.
- Editor preview roles are registered and stopped-transport processing is available where the existing engine services the instrument. Editor-only audition and native window interactions have not been visually/listening verified.
- Original waveform display is retained; modified-note waveforms are not regenerated.
- VST3 only; no AU/CLAP ARA binding, Linux/Mac runtime verification or publication in this update.
- Windows Melodyne 5.4.2 is the actual tested editor. Its non-modal capability/license query still returned false after the user confirmed activation, although document binding, source rendering and archives succeeded. No license bypass; final editor/license operation needs the user's review.

## Verification

Five focused tests cover factory pairing, stereo float/double random reads, bounds, independent reader lifetime, archive random IO/overflow, envelope roundtrip/corruption/truncation, voice offsets/overlap/gain/stops/rates.

The opt-in ara_melodyne_project_playback_restore_undo test uses LEMON_ARA_TEST_PLUGIN, synthetic two-second audio and no editor/audio device/microphone. It exercises actual Melodyne document binding, source trims and gain, overlapping voices, song export compared with raw-source peak, project save/reopen, different playback sample rate, independent Make Unique instances, missing-plugin preservation, remove/undo/redo and incompatible archive/source rejection. The ara_document example separately probes raw document save/restore and renderer lifecycle.

User review remains: open Melodyne from a disposable clip, verify analysis/editing without Transfer, change an audible note, play/seek/loop/fade/overlap, save/reopen and export, compare the edited sound, then test window closure and project shutdown. These checks have not been replaced with GUI automation.

## Official references

- https://github.com/Celemony/ARA_SDK
- https://github.com/Celemony/ARA_API/blob/main/ARAVST3.h
- https://github.com/Celemony/ARA_API/blob/main/ARAInterface.h
- https://github.com/Celemony/ARA_Examples

## 2 November 2026: editor frame resizing

VST3 editors that report canResize now receive host frame-edge and maximize/restore requests through checkSizeConstraint/onSize. Sizes cross the app frame in logical pixels and the plugin API in device pixels (view points on macOS). The frame follows the accepted size, including plugin minimum/maximum constraints. Requests survive audio-thread lock contention and are discarded on window close. Existing built-in host-link sizing is retained; fixed-size editors stay fixed. Windows local build only; native Melodyne dragging is for the user's verification.

Focused headless regression covers VST3 resizing, constraints, scale conversion, refused changes and fixed-size editors; the existing frame edge/corner regression also passes. No native plugin UI or audio device is opened by these tests. Spacebar focus/transport integration and Auto-Tune Pro's empty editor/muted playback remain separate reported issues, not addressed by this change.


## 2 November 2026: Auto-Tune Pro 11.0.0 investigation (UNRESOLVED)

User confirms normal Auto-Tune mixer-effect playback works, but ARA opens an empty graph and mutes the source. User explicitly authorized opening Auto-Tune in an isolated generated-tone editor test; no user project, audio device or microphone was used. They chose to keep testing 11.0.0 instead of updating when asked. No working ARA fix was found and no runtime change was installed.

The opt-in Windows ara_editor_audio_from_env test uses OS_VST3_PLUGIN, OS_VST3_NAME, and optional OS_SHOT_DIR. It attaches a real visible editor to its own temporary window, pumps native messages, renders synthetic stereo audio offline, optionally captures only that window, and closes it before asserting output. It is ignored by default and needs explicit GUI permission. Auto-Tune 11.0.0 shows Graph Mode / ARA Mode, an Unnamed track 0, disabled graph controls and no waveform; output peak is exactly zero. License-capability query returned true. Diagnostic callback logging showed a reader created, but no sample reads, including a roughly 14-second editor/pumped-processing run. Do not misdiagnose this as merely a hidden window or infer licensing failure.

Independent comparison: built Celemony ARA_Examples abd7c8aa5854591995e1fbf16f854c65b0998e8d with ARA_API 65ec5c43b943a48cb5446f448a0492db6af8534b and ARA_Library d18a6a5e489816316be84a9de0eaf7307bc1abe4 in the workspace sibling ara-reference folder. Ran ARATestHost -vst3 <binary> -test PlaybackRendering, with a diagnostic full-buffer peak added to TestCases.cpp. Auto-Tune peak 0; SDK test plugin and Melodyne each 0.99999976. Its advertised ARA generation range is 3..5; all three tested generations returned silence. Adding a 120 BPM tempo map and 4/4 signature to the reference host also did not fix playback. These results narrow the issue to Auto-Tune integration/compatibility, but do NOT prove it cannot work in a supported commercial DAW or prove a vendor defect.

Other unsuccessful experiments (reverted): controller/binding/model creation order, model creation or property refresh after opening the editor, selected region/sequence notifications, renderer-role registration, sample-access re-enable, processing modes, and avoiding temporary view creation. An experimental test accidentally omitted the region sequence musicalContextRef and crashed; restored the valid baseline before later checks. Do not reuse that invalid experiment. Temporary host-name compatibility probing also had no effect and was reverted. No production host identity/settings changes remain. Bound Auto-Tune document archiving succeeded (about 16 KB), but archive restoration crashed in early diagnostics; no Auto-Tune persistence-success claim.

The headless ara_document diagnostic now binds the VST3 instance before archiving, checks audible output before continuing, and checks the restored renderer as well. Melodyne passed with original/restored peaks 0.09999999 and a 32019-byte archive. The final Auto-Tune headless diagnostic fails explicitly at the zero-output check before attempting archive restoration. This is diagnostic coverage only, not a product fix. Existing Melodyne rendering and resizing remain unchanged. Next useful evidence is a successful Auto-Tune 11.0.0 ARA setup in another host or Antares guidance on its initialization requirements. The newer version was offered and the user declined; do not repeat that question.

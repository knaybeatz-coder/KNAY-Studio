# Lemon Master Assistant

Based on Wamphyre/oXygen v1.2.1, commit e261ea518c70316d0aab2cdef2c995f114dac694 (8 September 2026).
Upstream: https://github.com/Wamphyre/oXygen
The original BSD-3-Clause licence is preserved in LICENSE. JUCE 8.0.9 is fetched at build time and used under its AGPLv3 option; see its LICENSE.md. Retain both notices with distribution.

Lemon changes: distinct VST3 identity, Lemon green/cream interface and tactile controls, compact resizable editor, master-bus Open and on/off integration. Original DSP, reference matching and measurement-based assistant are retained. No trained model or cloud service is claimed.

This is a bundled engine, not a separately installed user plugin. Normal playback and song export use the same master effect slot. Opening it initially leaves the host slot disabled; enable on the master strip before listening/analysing. Existing settings are retained on bypass. Project state stores the VST3 class identity and processor state.

Build with CMake 3.22+ and a C++20 compiler. Configure -DOXYGEN_BUILD_TESTS=ON, build oXygen_VST3 and oXygenDSPTests. The resulting Lemon Master Assistant.vst3 bundle belongs in Builtin Plugins beside the app executable. This is currently a local Windows working integration; public distribution is forbidden until Michael explicitly approves publishing the Master Assistant.

The child rack parameters are forwarded to the outer VST3 processor so host edits, dirty tracking and automation see their actual controls. The master assistant uses a normal master effect slot and respects the existing global FX bypass and export Master FX switches. Enabling it restores the master FX group, like the existing master effects button. OPEN reuses the existing instance; occupied effects and the reserved EQ slot are never replaced.

Windows reproducibility: use an ASCII source/build path (a temporary subst drive may point to the work folder) to avoid JUCE resource compiler trouble with accented profile paths. Configure with Visual Studio 17 2022, x64; cmake --build BUILD --config Release --target oXygen_VST3 oXygenDSPTests. Run oXygenDSPTests without --workflow for headless synthetic checks. The optional Rust test master_assistant_bundled_host_render_and_state requires LEMON_MASTER_TEST_BUNDLE pointing to the complete .vst3 bundle and --include-ignored. Other focused tests: master_assistant_slot_identity_and_safe_placement and headphone_master_selector_layout_and_search. Cargo alone does not build this external engine; include it only with explicitly private local app builds.

Private build: cargo build --locked --release -p os-app --features private-master-assistant. Default builds exclude the assistant. Public source export uses --public and omits this engine and the private integration module. Private home-server recovery snapshots retain the source. A generic publish request does not lift the hold.

The default editor now presents one actual EQ response curve, three broad tone controls (Bass, Body, Air), Width and Loudness. Tone controls adjust groups of existing bands while preserving their relative shape; Width adjusts the three upper bands. Detailed controls opens the original rack. Controls write existing processor parameters, so state and host automation retain their normal behavior.


2 November 2026 private polish: Master Assistant opens its own editor and is excluded from user-visible mixer inserts, their numbering and slot moves. New instances use the final internal master effect position; existing project keys are retained so saved state and automation do not move. The internal hosted effect still supplies playback, PDC, bypass and ordinary export processing; this is not a separate standalone process. The host preserves existing user effects and will refuse creation if it cannot append safely.

Detailed controls now uses stage tabs and a true interactive parametric peak EQ: 15 movable frequency/gain points, Q via the wheel or selected-band controls, fine Ctrl-drag, typed values and band reset. Existing Graphic EQ module name and Gain parameter identities remain for compatibility. New frequency/Q controls are appended after all legacy host parameters. Old states reset missing frequency/Q values to the original graphic-EQ centres/bandwidths; assistant/reference proposals restore those centres before applying their fitted gains. Overview response uses the real edited frequencies/Q; broad tone macros follow current frequency regions. Audio frequency/Q changes are smoothed over35ms.

The charcoal, lemon-yellow and cream surfaces follow Lemon's plugin family. Live spectrum/meter behavior is retained. Private only: no new public packaging or publication authorization.

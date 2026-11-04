# Local stem separation

Right-click a Playlist audio clip and choose **Stem separation...**. Choose any combination of Vocals, Drums, Bass and Instruments, or All stems, then click Separate. At least one must be selected. First use asks Yes/No before downloading a private Python runtime, CPU PyTorch/Demucs engine and the HTDemucs four-stem model. No administrator rights or existing Python/FFmpeg installation are needed. Allow several GB of free disk space. Download and setup errors can be retried from the same menu.

After setup, the chosen sample is separated locally into Vocals, Drums, Bass and Instruments. Nothing uploads audio. Only selected stems are written to disk and imported; one empty Playlist row per selected stem receives a result with the original clip's position, trim, stretch and channel settings. The original clip is muted, not deleted. One Undo restores the original; Redo restores the selected stems. Completed WAV files remain on disk for project reopening and redo. Use the existing zipped project export to transfer the project and its audio to another computer.

The progress dialog shows the current download/setup/separation stage and elapsed time, with Cancel. Editing is temporarily blocked while that dialog is open; processing runs off the UI and audio threads. Cancellation never imports partial results. Other existing clips and tracks are not overwritten. Subsequent uses run locally without a setup download.

Runtime and outputs are in the active user profile's `Stem separation/demucs-4.0.1-torch-2.5.1-v1` folder. The personal and development profiles remain separate. `setup.log` records installation and model execution errors. A failed engine execution removes the readiness marker so the next attempt offers repair. An OS file lock prevents concurrent writes to the same runtime.

## Dependencies and distribution

The DAW embeds only its own worker/bootstrap code and constraints, not Python, model weights or personal audio. Downloads come from official HTTPS sources after consent:

- uv 0.9.5: https://github.com/astral-sh/uv (MIT OR Apache-2.0). Platform archive SHA-256 values are pinned before extraction/execution.
- Managed CPython 3.11.14 via uv: https://github.com/astral-sh/python-build-standalone (Python PSF and bundled component licences).
- PyTorch/torchaudio 2.5.1: https://pytorch.org/ (BSD-style licences). Windows/Linux select CPU wheels; macOS uses its platform wheels.
- Demucs 4.0.1 and official HTDemucs weights: https://github.com/facebookresearch/demucs (MIT). The model `955717e8-8726e21a.th` is downloaded from Meta's official host and checked against full SHA-256 `8726e21a993978c7ba086d3872e7608d7d5bfca646ca4aca459ffda844faa8b4` before loading.
- NumPy 1.26.4 and SoundFile 0.13.1; other runtime dependencies are constrained to the tested versions in `stem_constraints.txt`. Downloaded distributions retain their licence metadata.

CPU processing is intentional for this initial version; no CUDA/AMD GPU acceleration is promised. The source includes platform bootstrap choices for Windows x64, Linux x64 and macOS ARM64. Windows installation and inference are tested; macOS/Linux runtime verification is still required before describing those platforms as tested. Intel Mac and other unsupported architectures show an availability message before offering any download.

## Focused checks

`cargo test --locked --release -p os-app stem_separation::tests`

The separately ignored `fresh_download_and_real_cpu_separation` test requires explicit `LEMON_STEM_TEST_ROOT` pointing to a scratch folder. It downloads the actual engine/model, separates synthetic audio (never microphone input), verifies four finite stereo WAVs with exact source frame count/sample rate, repeats separation without reinstalling, and exercises cancellation. No live GUI/audio-device test is performed.

The model still estimates all four sources internally even when only one output is requested; choosing fewer stems reduces saved files and Playlist tracks, not necessarily processing time. Existing installations reuse their engine and model without another download.

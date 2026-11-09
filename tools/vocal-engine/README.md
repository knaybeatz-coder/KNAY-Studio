# Lemon vocal engine v1

Optional native WORLD-based monophonic vocal analysis and resynthesis. No neural models or user audio uploads.

Build with a C++17 compiler and CMake 3.22 or newer:

    cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
    cmake --build build --config Release

On Windows use Visual Studio 2022 Build Tools. On macOS, -DCMAKE_OSX_ARCHITECTURES="arm64;x86_64" builds a universal library. On Linux use GCC or Clang with position-independent shared-library support.

The C ABI version is 1. Inputs are planar float32 stereo. F0 arrays are float64 at 5 ms steps. Analyze outputs frequency in Hz (zero means unvoiced). Render uses the original spectral envelope and a corrected F0 contour, preserving source duration and the stereo side signal. Original audio is returned exactly when no correction is requested. Formant shift is in semitones. Inputs are limited to 10 minutes, 16–192 kHz and 33,554,432 frames. Large jobs are processed in overlapping synthesis blocks. Calls must not run on an audio callback.

bridge.cpp is GPL-3.0-or-later. See LEMON-LICENSE.txt in the downloadable source archive or the repository LICENSE. WORLD is BSD-3-Clause; its complete original source, copyright, licence and pinned revision are in vendor/world. This is an independent native engine, not OpenTune or PitchNet code.

Packages are installed only after user consent and Ed25519/SHA-256 verification. Public binaries and matching source are distributed under https://michaellanemusic.de/lemon-studio/components/vocal-world-v1/ . Engine changes require a new versioned package directory; do not overwrite published v1 files.

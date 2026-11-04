/*
 * Copyright (C) 2026 KNAY
 * SPDX-License-Identifier: GPL-3.0-or-later
 * This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
 */
// Offline pitch / formant rendering with Signalsmith Stretch for VariAudio, set up as Giannitune's
// SignalsmithShifter does (21 ms blocks, 5 ms interval), with the transpose and formant factors changing every
// `hop` samples. The engine's latency is compensated: the output lines up with the input sample for sample.
#include "signalsmith-stretch.h"
#include <vector>
#include <random>
#include <cmath>
#include <algorithm>

extern "C" int knay_signalsmith_render(const float* const* input, float* const* output, int channels, int frames,
                                       float sampleRate, const float* transpose, const float* formant, int steps,
                                       int hop) {
    if (channels < 1 || channels > 2 || frames <= 0 || hop <= 0 || sampleRate <= 0) return 0;
    signalsmith::stretch::SignalsmithStretch<float, std::default_random_engine> engine;
    const int block = std::max(256, (int)std::lround(sampleRate * 0.021));
    const int interval = std::max(64, (int)std::lround(sampleRate * 0.005));
    engine.configure(channels, block, interval, false);
    engine.setFormantBase(0.0f);
    const int latency = engine.inputLatency() + engine.outputLatency();
    const int total = frames + latency;
    std::vector<std::vector<float>> in(channels, std::vector<float>(total + hop, 0.0f));
    std::vector<std::vector<float>> out(channels, std::vector<float>(total + hop, 0.0f));
    for (int c = 0; c < channels; ++c) std::copy(input[c], input[c] + frames, in[c].begin());
    std::vector<const float*> ip(channels);
    std::vector<float*> op(channels);
    for (int pos = 0, step = 0; pos < total; pos += hop, ++step) {
        const int s = std::min(step, steps - 1);
        const float t = (steps > 0 && transpose) ? transpose[std::max(s, 0)] : 1.0f;
        const float f = (steps > 0 && formant) ? formant[std::max(s, 0)] : 1.0f;
        engine.setTransposeFactor(std::clamp(t, 0.25f, 4.0f));
        engine.setFormantFactor(std::clamp(f, 0.25f, 4.0f), true);
        const int n = std::min(hop, total - pos);
        for (int c = 0; c < channels; ++c) { ip[c] = in[c].data() + pos; op[c] = out[c].data() + pos; }
        engine.process(ip.data(), n, op.data(), n);
    }
    for (int c = 0; c < channels; ++c) std::copy(out[c].begin() + latency, out[c].begin() + latency + frames, output[c]);
    return 1;
}

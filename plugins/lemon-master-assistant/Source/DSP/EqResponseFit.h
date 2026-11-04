#pragma once
#include "../Modules/EqualizerModule.h"

namespace oxygen::dsp
{
    // Fit a requested frequency response to the overlapping graphic EQ filters.
    // A 1 dB response matrix plus regularization avoids stacking adjacent boosts
    // and unstable opposing gains. The solve runs only in the analysis worker.
    inline std::array<float, EqualizerModule::NumBands> fitEqResponse(
        const std::array<float, EqualizerModule::NumBands>& target,
        const std::array<float, EqualizerModule::NumBands>& weights,
        double sampleRate, float gainLimit = 4.2f)
    {
        constexpr int count = EqualizerModule::NumBands;
        std::array<std::array<double, count>, count> response {};
        std::array<float, count> gains {};
        std::array<double, count> estimate {};
        for (int band = 0; band < count; ++band)
        {
            if (EqualizerModule::Frequencies[band] > sampleRate * 0.45)
                continue;
            const auto filter = juce::dsp::IIR::Coefficients<double>::makePeakFilter(
                sampleRate, (double) EqualizerModule::Frequencies[band],
                (double) EqualizerModule::BandQValues[(size_t) band], std::pow(10.0, 1.0 / 20.0));
            for (int point = 0; point < count; ++point)
                response[(size_t) point][(size_t) band] = 20.0 * std::log10(filter->getMagnitudeForFrequency(
                    juce::jmin((double) EqualizerModule::Frequencies[point], sampleRate * 0.45), sampleRate));
        }
        for (int pass = 0; pass < 80; ++pass)
        {
            for (int band = 0; band < count; ++band)
            {
                double numerator = 0.0, denominator = 0.10;
                for (int point = 0; point < count; ++point)
                {
                    const double r = response[(size_t) point][(size_t) band];
                    const double w = weights[(size_t) point];
                    numerator += w * r * (target[(size_t) point] - estimate[(size_t) point] + r * gains[(size_t) band]);
                    denominator += w * r * r;
                }
                const float next = juce::jlimit(-gainLimit, gainLimit, (float) (numerator / denominator));
                for (int point = 0; point < count; ++point)
                    estimate[(size_t) point] += response[(size_t) point][(size_t) band] * (next - gains[(size_t) band]);
                gains[(size_t) band] = next;
            }
        }
        return gains;
    }
}

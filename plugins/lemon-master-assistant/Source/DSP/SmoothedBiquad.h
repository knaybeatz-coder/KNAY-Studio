#pragma once

#include <JuceHeader.h>
#include <array>
#include <cmath>

namespace oxygen::dsp
{
    template <typename SampleType>
    struct BiquadCoefficients
    {
        SampleType b0 = 1;
        SampleType b1 = 0;
        SampleType b2 = 0;
        SampleType a1 = 0;
        SampleType a2 = 0;
    };

    template <typename SampleType>
    BiquadCoefficients<SampleType> makePeakCoefficients(double sampleRate,
                                                        SampleType frequency,
                                                        SampleType q,
                                                        SampleType gainDb)
    {
        const auto pi = static_cast<SampleType>(juce::MathConstants<double>::pi);
        const auto omega = static_cast<SampleType>(2) * pi * frequency
                         / static_cast<SampleType>(sampleRate);
        const auto sinOmega = std::sin(omega);
        const auto cosOmega = std::cos(omega);
        const auto alpha = sinOmega / (static_cast<SampleType>(2) * q);
        const auto a = std::pow(static_cast<SampleType>(10), gainDb / static_cast<SampleType>(40));
        const auto a0 = static_cast<SampleType>(1) + alpha / a;

        return {
            (static_cast<SampleType>(1) + alpha * a) / a0,
            (-static_cast<SampleType>(2) * cosOmega) / a0,
            (static_cast<SampleType>(1) - alpha * a) / a0,
            (-static_cast<SampleType>(2) * cosOmega) / a0,
            (static_cast<SampleType>(1) - alpha / a) / a0
        };
    }

    template <typename SampleType>
    BiquadCoefficients<SampleType> makeBandPassCoefficients(double sampleRate,
                                                            SampleType frequency,
                                                            SampleType q)
    {
        const auto pi = static_cast<SampleType>(juce::MathConstants<double>::pi);
        const auto omega = static_cast<SampleType>(2) * pi * frequency
                         / static_cast<SampleType>(sampleRate);
        const auto sinOmega = std::sin(omega);
        const auto cosOmega = std::cos(omega);
        const auto alpha = sinOmega / (static_cast<SampleType>(2) * q);
        const auto a0 = static_cast<SampleType>(1) + alpha;

        return {
            alpha / a0,
            static_cast<SampleType>(0),
            -alpha / a0,
            (-static_cast<SampleType>(2) * cosOmega) / a0,
            (static_cast<SampleType>(1) - alpha) / a0
        };
    }

    /** Allocation-free transposed-direct-form-II biquad with coefficient ramps. */
    template <typename SampleType, size_t MaxChannels = 2>
    class SmoothedBiquad
    {
    public:
        void reset()
        {
            z1.fill(static_cast<SampleType>(0));
            z2.fill(static_cast<SampleType>(0));
        }

        void setCoefficients(const BiquadCoefficients<SampleType>& coefficients,
                             int rampSamples = 32)
        {
            if (!initialised || rampSamples <= 0)
            {
                current = target = coefficients;
                step = { static_cast<SampleType>(0), static_cast<SampleType>(0),
                         static_cast<SampleType>(0), static_cast<SampleType>(0),
                         static_cast<SampleType>(0) };
                remaining = 0;
                initialised = true;
                return;
            }

            target = coefficients;
            remaining = rampSamples;
            const auto divisor = static_cast<SampleType>(rampSamples);
            step.b0 = (target.b0 - current.b0) / divisor;
            step.b1 = (target.b1 - current.b1) / divisor;
            step.b2 = (target.b2 - current.b2) / divisor;
            step.a1 = (target.a1 - current.a1) / divisor;
            step.a2 = (target.a2 - current.a2) / divisor;
        }

        void advance()
        {
            if (remaining <= 0)
                return;

            current.b0 += step.b0;
            current.b1 += step.b1;
            current.b2 += step.b2;
            current.a1 += step.a1;
            current.a2 += step.a2;

            if (--remaining == 0)
                current = target;
        }

        SampleType processSample(int channel, SampleType input)
        {
            jassert(channel >= 0 && channel < static_cast<int>(MaxChannels));
            const auto index = static_cast<size_t>(channel);
            const auto output = current.b0 * input + z1[index];
            z1[index] = current.b1 * input - current.a1 * output + z2[index];
            z2[index] = current.b2 * input - current.a2 * output;
            return output;
        }

    private:
        BiquadCoefficients<SampleType> current;
        BiquadCoefficients<SampleType> target;
        BiquadCoefficients<SampleType> step;
        std::array<SampleType, MaxChannels> z1 {};
        std::array<SampleType, MaxChannels> z2 {};
        int remaining = 0;
        bool initialised = false;
    };
}

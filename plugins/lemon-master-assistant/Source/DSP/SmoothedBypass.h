#pragma once

#include <JuceHeader.h>

namespace oxygen::dsp
{
    template <typename SampleType>
    class SmoothedBypass
    {
    public:
        void prepare(double sampleRate, double fadeSeconds = 0.01)
        {
            wet.reset(sampleRate, fadeSeconds);
            wet.setCurrentAndTargetValue(static_cast<SampleType>(1));
        }

        void setBypassed(bool bypassed)
        {
            wet.setTargetValue(bypassed ? static_cast<SampleType>(0)
                                        : static_cast<SampleType>(1));
        }

        SampleType getNextWetMix() { return wet.getNextValue(); }
        SampleType getCurrentWetMix() const { return wet.getCurrentValue(); }
        bool isSmoothing() const { return wet.isSmoothing(); }

    private:
        juce::LinearSmoothedValue<SampleType> wet;
    };
}

#pragma once

#include <JuceHeader.h>
#include <array>
#include <cmath>

namespace oxygen::dsp
{
    /**
        Four-band Linkwitz-Riley crossover with phase-matched branches.

        The centre split is followed by an all-pass section on each branch before
        the lower and upper splits. This gives both sides of the centre crossover
        the same accumulated phase response and avoids the mismatched branch
        topology that otherwise colours the signal around the crossover points.

        Users of this class should still recombine processing as a delta against
        the dry signal when an exact unity null is required.
    */
    template <typename SampleType>
    class FourBandCrossover
    {
    public:
        void prepare(const juce::dsp::ProcessSpec& spec)
        {
            rootSplit.prepare(spec);
            lowSplit.prepare(spec);
            highSplit.prepare(spec);
            lowBranchAllPass.prepare(spec);
            highBranchAllPass.prepare(spec);
            reset();
        }

        void reset()
        {
            rootSplit.reset();
            lowSplit.reset();
            highSplit.reset();
            lowBranchAllPass.reset();
            highBranchAllPass.reset();
            currentLowMid = static_cast<SampleType>(-1);
            currentMidHigh = static_cast<SampleType>(-1);
            currentHigh = static_cast<SampleType>(-1);
        }

        void setCrossovers(SampleType lowMid, SampleType midHigh, SampleType high)
        {
            if (approximatelyEqual(lowMid, currentLowMid)
                && approximatelyEqual(midHigh, currentMidHigh)
                && approximatelyEqual(high, currentHigh))
                return;

            currentLowMid = lowMid;
            currentMidHigh = midHigh;
            currentHigh = high;

            rootSplit.setCutoffFrequency(midHigh);
            lowSplit.setCutoffFrequency(lowMid);
            highSplit.setCutoffFrequency(high);

            // Sum of the low/high outputs of a Linkwitz-Riley split is an
            // all-pass response. Matching these sections on opposite branches
            // keeps the four-way split phase coherent.
            lowBranchAllPass.setCutoffFrequency(high);
            highBranchAllPass.setCutoffFrequency(lowMid);
        }

        void processSample(int channel, SampleType input, std::array<SampleType, 4>& bands)
        {
            SampleType lowPath = 0;
            SampleType highPath = 0;
            rootSplit.processSample(channel, input, lowPath, highPath);

            SampleType phaseLowA = 0;
            SampleType phaseLowB = 0;
            lowBranchAllPass.processSample(channel, lowPath, phaseLowA, phaseLowB);
            const auto phaseMatchedLowPath = phaseLowA + phaseLowB;

            SampleType phaseHighA = 0;
            SampleType phaseHighB = 0;
            highBranchAllPass.processSample(channel, highPath, phaseHighA, phaseHighB);
            const auto phaseMatchedHighPath = phaseHighA + phaseHighB;

            lowSplit.processSample(channel, phaseMatchedLowPath, bands[0], bands[1]);
            highSplit.processSample(channel, phaseMatchedHighPath, bands[2], bands[3]);
        }

    private:
        using Filter = juce::dsp::LinkwitzRileyFilter<SampleType>;

        static bool approximatelyEqual(SampleType a, SampleType b)
        {
            return std::abs(a - b) <= static_cast<SampleType>(1.0e-4);
        }

        Filter rootSplit;
        Filter lowSplit;
        Filter highSplit;
        Filter lowBranchAllPass;
        Filter highBranchAllPass;
        SampleType currentLowMid = static_cast<SampleType>(-1);
        SampleType currentMidHigh = static_cast<SampleType>(-1);
        SampleType currentHigh = static_cast<SampleType>(-1);
    };
}

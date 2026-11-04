#pragma once

#include <JuceHeader.h>
#include "../MasteringModule.h"
#include "../DSP/FourBandCrossover.h"

namespace oxygen
{

    class MultibandCompressorModule : public MasteringModule
    {
    public:
        MultibandCompressorModule();
        ~MultibandCompressorModule() override;

        void prepareToPlay(double sampleRate, int samplesPerBlock) override;
        void processBlock(juce::AudioBuffer<float>& buffer, juce::MidiBuffer& midiMessages) override;

        bool hasEditor() const override { return true; }
        juce::AudioProcessorEditor* createEditor() override;

        // Parameter layout
        static juce::AudioProcessorValueTreeState::ParameterLayout createParameterLayout();
        juce::AudioProcessorValueTreeState apvts;
        juce::RangedAudioParameter* getBypassParameter() const override { return apvts.getParameter("Bypass"); }

    private:

        struct BandParams
        {
            float thresh = -10.0f;
            float ratio = 2.0f;
            float attack = 10.0f;
            float release = 100.0f;
        };

        struct BandRuntimeState
        {
            float thresholdDb = -10.0f;
            float ratioInverseMinusOne = -0.5f;
            float attackCoeff = 0.0f;
            float releaseCoeff = 0.0f;
            float envelope = 0.0f;
        };

        dsp::FourBandCrossover<float> crossover;

        // Temporary buffers for bands
        juce::AudioBuffer<float> bufferLow, bufferLowMid, bufferHighMid, bufferHigh;

        // Parameter caching for optimization
        std::array<BandParams, 4> lastBandParams;
        std::array<BandRuntimeState, 4> bandRuntimeStates;
        double lastSampleRate = 0.0;

        // Helper to update parameters
        void updateParameters();
        void processStereoLinkedBand(juce::AudioBuffer<float>& bandBuffer, BandRuntimeState& state, int numSamples);

        JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(MultibandCompressorModule)
    };

}

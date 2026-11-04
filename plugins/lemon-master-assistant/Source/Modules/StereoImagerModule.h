#pragma once

#include <JuceHeader.h>
#include "../MasteringModule.h"
#include "../DSP/FourBandCrossover.h"

namespace oxygen
{

    class StereoImagerModule : public MasteringModule
    {
    public:
        StereoImagerModule();
        ~StereoImagerModule() override;

        void prepareToPlay(double sampleRate, int samplesPerBlock) override;
        void processBlock(juce::AudioBuffer<float>& buffer, juce::MidiBuffer& midiMessages) override;

        bool hasEditor() const override { return true; }
        juce::AudioProcessorEditor* createEditor() override;

        // Parameter layout
        static juce::AudioProcessorValueTreeState::ParameterLayout createParameterLayout();
        juce::AudioProcessorValueTreeState apvts;
        juce::RangedAudioParameter* getBypassParameter() const override { return apvts.getParameter("Bypass"); }

    private:

        // Crossovers (same structure as Multiband Compressor)
        dsp::FourBandCrossover<float> crossover;

        // Temporary buffers for bands
        juce::AudioBuffer<float> bufferLow;
        juce::AudioBuffer<float> bufferLowMid;
        juce::AudioBuffer<float> bufferHighMid;
        juce::AudioBuffer<float> bufferHigh;

        void updateParameters();
        struct WidthState
        {
            double l2 = 0.0, r2 = 0.0, lr = 0.0, mid2 = 0.0, side2 = 0.0;
            float sideGain = 1.0f, midGain = 1.0f;
        };
        std::array<WidthState, 4> widthStates;
        double detectorCoefficient = 0.0;
        float gainCoefficient = 0.0f;
        void processBand(juce::AudioBuffer<float>& bandBuffer, float width,
                         WidthState& state, int numSamples);

        JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(StereoImagerModule)
    };

}

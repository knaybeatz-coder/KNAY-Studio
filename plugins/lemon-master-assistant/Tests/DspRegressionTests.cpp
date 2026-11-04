#include "../Source/GUI/MainComponent.h"
#include "../Source/GUI/EqualizerEditor.h"
#include "../Source/GUI/MasterOverview.h"
#include "../Source/GUI/LemonLookAndFeel.h"
#include <JuceHeader.h>
#include "../Source/AI/MasteringParameterApplier.h"
#include "../Source/DSP/SlidingMinimum.h"
#include "../Source/DSP/EqResponseFit.h"
#include <chrono>
#include <iostream>
#include <random>
#include <stdexcept>

void runAssistantWorkflowTests();
void runAssistantCancellationTests();

namespace
{
void require(bool condition, const std::string& message)
{
    if (!condition) throw std::runtime_error(message);
}
void set(juce::AudioProcessorValueTreeState& state, const juce::String& id, float value)
{
    auto* parameter = state.getParameter(id);
    parameter->setValueNotifyingHost(parameter->convertTo0to1(value));
}
juce::AudioBuffer<float> signal(double rate, int channels, double seconds = 3.0)
{
    juce::AudioBuffer<float> audio(channels, (int) (rate * seconds));
    for (int ch = 0; ch < channels; ++ch)
        for (int n = 0; n < audio.getNumSamples(); ++n)
        {
            const double t = n / rate;
            double value = 0.0;
            for (int band = 0; band < 15; ++band)
            {
                const double frequency = oxygen::EqualizerModule::Frequencies[band];
                if (frequency > rate * 0.43) continue;
                value += std::sin(juce::MathConstants<double>::twoPi * frequency * t + band * 0.47 + ch * band * 0.09)
                       * std::pow(100.0 / frequency, 0.30);
            }
            const double envelope = 0.65 + 0.35 * std::exp(-18.0 * std::fmod(t, 0.5));
            audio.setSample(ch, n, (float) (value * envelope * 0.018));
        }
    return audio;
}
void prepare(juce::AudioProcessor& module, double rate, int channels)
{
    module.setPlayConfigDetails(channels, channels, rate, 512);
    module.prepareToPlay(rate, 512);
}
juce::AudioBuffer<float> render(juce::AudioProcessor& module, const juce::AudioBuffer<float>& input,
                                 double rate, bool variable, bool doublePath = false)
{
    prepare(module, rate, input.getNumChannels());
    juce::AudioBuffer<float> output(input);
    juce::MidiBuffer midi;
    constexpr int sizes[] { 1, 13, 127, 512, 71, 256, 3 };
    int step = 0;
    for (int offset = 0; offset < output.getNumSamples();)
    {
        const int count = juce::jmin(variable ? sizes[step++ % 7] : 512, output.getNumSamples() - offset);
        if (doublePath)
        {
            juce::AudioBuffer<double> block(output.getNumChannels(), count);
            for (int ch = 0; ch < block.getNumChannels(); ++ch)
                for (int n = 0; n < count; ++n) block.setSample(ch, n, output.getSample(ch, offset + n));
            module.processBlock(block, midi);
            for (int ch = 0; ch < block.getNumChannels(); ++ch)
                for (int n = 0; n < count; ++n) output.setSample(ch, offset + n, (float) block.getSample(ch, n));
        }
        else
        {
            juce::AudioBuffer<float> block(output.getArrayOfWritePointers(), output.getNumChannels(), offset, count);
            module.processBlock(block, midi);
        }
        offset += count;
    }
    return output;
}
float difference(const juce::AudioBuffer<float>& a, const juce::AudioBuffer<float>& b)
{
    float error = 0.0f;
    for (int ch = 0; ch < a.getNumChannels(); ++ch)
        for (int n = 0; n < a.getNumSamples(); ++n)
            error = juce::jmax(error, std::abs(a.getSample(ch, n) - b.getSample(ch, n)));
    return error;
}
float rmsDb(const juce::AudioBuffer<float>& audio)
{
    double energy = 0.0;
    for (int ch = 0; ch < audio.getNumChannels(); ++ch)
        for (int n = 0; n < audio.getNumSamples(); ++n)
        {
            const auto x = audio.getSample(ch, n);
            require(std::isfinite(x), "Non-finite render");
            energy += (double) x * x;
        }
    return (float) (10.0 * std::log10(energy / (audio.getNumSamples() * audio.getNumChannels()) + 1.0e-20));
}
float truePeakDb(const juce::AudioBuffer<float>& audio)
{
    float peak = audio.getMagnitude(0, audio.getNumSamples());
    std::vector<float> up((size_t) audio.getNumSamples() * 8);
    for (int ch = 0; ch < audio.getNumChannels(); ++ch)
    {
        juce::WindowedSincInterpolator interpolator;
        interpolator.process(0.125, audio.getReadPointer(ch), up.data(), (int) up.size(), audio.getNumSamples(), 0);
        for (float value : up) peak = juce::jmax(peak, std::abs(value));
    }
    return juce::Decibels::gainToDecibels(peak);
}
void testMinimum()
{
    std::mt19937 random(42);
    for (const size_t window : { 1u, 5u, 961u, 3841u })
    {
        oxygen::dsp::SlidingMinimum minimum;
        minimum.prepare(window);
        std::vector<float> naive(window, 1.0f);
        for (size_t n = 0; n < 20000; ++n)
        {
            float value = n % 2000 < 1000 ? (float) (random() % 100) / 100.0f : (float) (n % 1000) / 1000.0f;
            naive[n % window] = value;
            require(minimum.push(value) == *std::min_element(naive.begin(), naive.end()), "Lookahead minimum differs");
        }
        minimum.reset();
        require(minimum.push(1.0f) == 1.0f, "Minimum reset");
    }
    std::cout << "PASS exact lookahead minimum (random, ramps, ties, wrap and reset)\n";
}
void testCrossovers()
{
    for (const double rate : { 44100.0, 48000.0, 96000.0 })
    {
        for (const double frequency : { 40.0, 120.0, 200.0, 1400.0, 2000.0, 6000.0, 8000.0, 14000.0 })
        {
            juce::AudioBuffer<float> tone(2, (int) rate);
            for (int ch = 0; ch < 2; ++ch)
                for (int n = 0; n < tone.getNumSamples(); ++n)
                    tone.setSample(ch, n, (float) (0.1 * std::sin(juce::MathConstants<double>::twoPi * frequency * n / rate)));
            oxygen::StereoImagerModule imager;
            oxygen::MultibandCompressorModule compressor;
            for (const auto* band : { "Low", "LowMid", "HighMid", "High" })
                set(compressor.apvts, juce::String(band) + "Ratio", 1.0f);
            for (auto* module : { (juce::AudioProcessor*) &imager, (juce::AudioProcessor*) &compressor })
            {
                auto result = render(*module, tone, rate, false);
                const int start = (int) (rate * 0.5);
                const float ratio = result.getRMSLevel(0, start, start) / tone.getRMSLevel(0, start, start);
                require(std::abs(juce::Decibels::gainToDecibels(ratio)) < 0.025f, "Crossover colours unity response");
            }
        }
    }
    std::cout << "PASS flat crossover magnitude at 44.1/48/96 kHz\n";
}
void testBlockSizes()
{
    const auto audio = signal(48000.0, 2, 1.0);
    oxygen::MasteringParameters params;
    params.dynamicEqRanges.fill(3.0f);
    params.dynamicEqThresholds.fill(-40.0f);
    params.maximizerThreshold = -10.0f;
    params.highWidth = 1.2f;
    for (int kind = 0; kind < 4; ++kind)
    {
        auto make = [kind]() -> std::unique_ptr<juce::AudioProcessor>
        {
            if (kind == 0) return std::make_unique<oxygen::DynamicEqModule>();
            if (kind == 1) return std::make_unique<oxygen::MultibandCompressorModule>();
            if (kind == 2) return std::make_unique<oxygen::StereoImagerModule>();
            return std::make_unique<oxygen::MaximizerModule>();
        };
        auto a = make(), b = make(), c = make();
        for (auto* module : { a.get(), b.get(), c.get() }) oxygen::applyParametersToModule(module, params);
        auto fixed = render(*a, audio, 48000.0, false);
        auto variable = render(*b, audio, 48000.0, true);
        auto doubleOutput = render(*c, audio, 48000.0, true, true);
        require(difference(fixed, variable) < 2.0e-5f, "Host block size changes module " + std::to_string(kind));
        require(difference(variable, doubleOutput) < 2.0e-5f, "Double fallback changes module " + std::to_string(kind));
    }
    std::cout << "PASS variable blocks and float/double paths\n";
}
void testLimiterSafety()
{
    for (double rate : { 44100.0, 48000.0, 96000.0 })
        for (int mode = 0; mode < 3; ++mode)
        {
            auto input = signal(rate, 2, 0.25);
            input.applyGain(0.9f / input.getMagnitude(0, input.getNumSamples()));
            oxygen::MaximizerModule limiter;
            set(limiter.apvts, "Threshold", -6.0f);
            set(limiter.apvts, "Ceiling", -1.0f);
            set(limiter.apvts, "Release", 120.0f);
            set(limiter.apvts, "Mode", (float) mode);
            const auto output = render(limiter, input, rate, true);
            const float peak = truePeakDb(output);
            std::cout << "Limiter " << rate << " Hz, mode " << mode << ": " << peak << " dBTP\n";
            require(peak <= -0.70f, "Hot limiter render exceeds true-peak tolerance: " + std::to_string(peak));
            oxygen::MaximizerModule bypass;
            set(bypass.apvts, "Bypass", 1.0f);
            juce::AudioBuffer<float> impulse(2, 2048);
            impulse.clear();
            impulse.setSample(0, 0, 0.5f);
            const auto delayed = render(bypass, impulse, rate, true);
            require(delayed.getSample(0, bypass.getLatencySamples()) == 0.5f, "Bypass latency mismatch");
        }
    std::cout << "PASS hot limiter renders in all modes and latency-compensated bypass at 44.1/48/96 kHz\n";
}

std::array<double, 15> toneProfile(const juce::AudioBuffer<float>& audio, double rate)
{
    std::array<double, 15> profile {};
    for (int band = 0; band < 15; ++band)
    {
        double real = 0.0, imaginary = 0.0;
        for (int n = (int) rate; n < audio.getNumSamples(); ++n)
        {
            const double phase = juce::MathConstants<double>::twoPi * oxygen::EqualizerModule::Frequencies[band] * n / rate;
            real += audio.getSample(0, n) * std::cos(phase);
            imaginary += audio.getSample(0, n) * std::sin(phase);
        }
        profile[(size_t) band] = 10.0 * std::log10(real * real + imaginary * imaginary + 1.0e-20);
    }
    const double mean = std::accumulate(profile.begin(), profile.end(), 0.0) / 15.0;
    for (auto& value : profile) value -= mean;
    return profile;
}
double tonalError(const juce::AudioBuffer<float>& source, const juce::AudioBuffer<float>& reference)
{
    const auto a = toneProfile(source, 48000.0), b = toneProfile(reference, 48000.0);
    double sum = 0.0;
    for (size_t i = 0; i < a.size(); ++i) sum += (a[i] - b[i]) * (a[i] - b[i]);
    return std::sqrt(sum / 15.0);
}
void testAssistant()
{
    oxygen::InferenceEngine engine;
    auto reference = signal(48000.0, 2);
    auto source = reference;
    oxygen::EqualizerModule colour;
    for (int band = 0; band < 15; ++band)
        set(colour.apvts, "Gain" + juce::String(band), band < 6 ? 1.5f : -1.5f);
    source = render(colour, source, 48000.0, false);
    const auto params = engine.matchReference(source, 48000.0, reference, 48000.0);
    require(params.usedAnalysis, "Reference analysis failed");
    oxygen::EqualizerModule correction;
    oxygen::applyParametersToModule(&correction, params);
    auto corrected = render(correction, source, 48000.0, false);
    const double before = tonalError(source, reference), after = tonalError(corrected, reference);
    std::cout << "Reference tonal RMS error: " << before << " -> " << after << " dB\n";
    require(after < before * 0.65, "Reference matching does not improve measured tone");

    auto quieterReference = reference;
    quieterReference.applyGain(0.25f);
    const auto identical = engine.matchReference(reference, 48000.0, quieterReference, 48000.0);
    for (float gain : identical.eqBandGains) require(std::abs(gain) < 0.05f, "Reference EQ depends on level");

    source = reference;
    source.applyGain(0.2f);
    const auto suggestion = engine.predict(source, 48000.0, { oxygen::AssistantGenre::Pop, oxygen::ArtisticDirection::Balanced });
    require(suggestion.usedAnalysis && suggestion.maximizerThreshold < -8.0f, "Quiet source retains fixed gain cap");
    auto output = source;
    std::array<std::unique_ptr<juce::AudioProcessor>, 6> rack {
        std::make_unique<oxygen::EqualizerModule>(), std::make_unique<oxygen::DynamicEqModule>(),
        std::make_unique<oxygen::MultibandCompressorModule>(), std::make_unique<oxygen::StereoImagerModule>(),
        std::make_unique<oxygen::GainModule>(), std::make_unique<oxygen::MaximizerModule>()
    };
    for (auto& module : rack)
    {
        oxygen::applyParametersToModule(module.get(), suggestion);
        output = render(*module, output, 48000.0, true);
    }
    const float lift = rmsDb(output) - rmsDb(source);
    const float peak = truePeakDb(output);
    const float crest = peak - rmsDb(output);
    std::cout << "Assistant quiet-source lift: " << lift << " dB; true peak: " << peak << " dBTP; crest: " << crest << " dB\n";
    require(lift > 8.0f, "Master remains too quiet");
    require(peak <= -0.65f, "Master exceeded true-peak headroom tolerance");
    require(crest > 6.0f, "Master flattened transients");
    require(rmsDb(output) > -20.0f, "Calibrated master misses a useful output level");

    juce::AudioBuffer<float> silence(2, 48000);
    silence.clear();
    require(!engine.predict(silence, 48000.0).usedAnalysis, "Silence accepted");
    require(!engine.matchReference(source, 48000.0, silence, 48000.0).usedAnalysis, "Silent reference accepted");
    silence.setSample(0, 0, std::numeric_limits<float>::quiet_NaN());
    require(!engine.predict(silence, 48000.0).usedAnalysis, "NaN accepted");
    auto mono = signal(48000.0, 1, 1.0);
    require(engine.predict(mono, 48000.0).usedAnalysis, "Mono analysis failed");
    std::cout << "PASS assistant/reference renders, level independence, invalid input and mono\n";
}
void benchmarkMinimum()
{
    constexpr size_t window = 961, count = 300000;
    std::vector<float> input(count), ring(window, 1.0f);
    std::mt19937 random(42);
    for (auto& value : input) value = (float) (random() % 10000) / 10000.0f;
    const auto start = std::chrono::steady_clock::now();
    double naiveChecksum = 0.0;
    for (size_t n = 0; n < count; ++n)
    {
        ring[n % window] = input[n];
        naiveChecksum += *std::min_element(ring.begin(), ring.end());
    }
    const auto middle = std::chrono::steady_clock::now();
    oxygen::dsp::SlidingMinimum minimum;
    minimum.prepare(window);
    double fastChecksum = 0.0;
    for (float value : input) fastChecksum += minimum.push(value);
    const auto end = std::chrono::steady_clock::now();
    require(naiveChecksum == fastChecksum, "Benchmark checksums differ");
    std::cout << "Lookahead detector only, 300000 samples, 961-sample window: scan "
              << std::chrono::duration<double, std::milli>(middle - start).count() << " ms; queue "
              << std::chrono::duration<double, std::milli>(end - middle).count() << " ms\n";
}

}
int main(int argc, char** argv)
{
    const bool workflow = argc > 1 && std::string(argv[1]) == "--workflow";
   #if JUCE_LINUX
    if (workflow && juce::SystemStats::getEnvironmentVariable("DISPLAY", {}).isEmpty())
    {
        std::cout << "GUI workflow tests require an X11 display (e.g. xvfb-run).\n";
        return 77;
    }
   #endif
    juce::ScopedJuceInitialiser_GUI init;
    juce::ScopedNoDenormals noDenormals;
    try {
        if(argc>2 && std::string(argv[1])=="--controls-check") {
            OxygenAudioProcessor p;LemonLookAndFeel look;int rendered=0;
            for(auto& node:p.getModuleNodes()) {
                auto* module=node->getProcessor();if(module->getName()=="Graphic EQ"||!module->hasEditor())continue;
                std::unique_ptr<juce::AudioProcessorEditor> editor(module->createEditor());editor->setLookAndFeel(&look);editor->setSize(820,editor->getHeight());editor->setVisible(true);
                int controls=0;for(auto* child:editor->getChildren())if(auto* slider=dynamic_cast<juce::Slider*>(child)){
                    require(slider->isRotary(),"Linear slider survived redesign");
                    require(slider->getTextBoxPosition()!=juce::Slider::NoTextBox,"Missing value readout");
                    require(editor->getLocalBounds().contains(slider->getBounds()),"Control is clipped");++controls;
                }
                require(controls>=3,"Missing module controls");
                juce::Image image(juce::Image::ARGB,820,editor->getHeight(),true,juce::SoftwareImageType());{juce::Graphics g(image);editor->paintEntireComponent(g,true);}
                auto path=juce::File(juce::String::fromUTF8(argv[2])).getChildFile(module->getName()+".png");juce::FileOutputStream output(path);require(output.openedOk(),"Cannot write control preview");output.setPosition(0);output.truncate();juce::PNGImageFormat png;require(png.writeImageToStream(image,output),"Preview write failed");
                editor->setLookAndFeel(nullptr);++rendered;
            }
            require(rendered==4,"Not all four stages were checked");
            std::cout<<"PASS four rotary-control surfaces, value readouts and layout bounds\n";return 0;
        }
        if(argc>2 && std::string(argv[1])=="--eq-check") {
            OxygenAudioProcessor p;oxygen::EqualizerModule* eq=nullptr;
            for(auto& n:p.getModuleNodes())if(auto* e=dynamic_cast<oxygen::EqualizerModule*>(n->getProcessor()))eq=e;
            require(eq!=nullptr,"Missing EQ");
            set(eq->apvts,"Freq7",1500.f);set(eq->apvts,"Gain7",6.f);set(eq->apvts,"Q7",2.f);
            juce::MemoryBlock state;p.getStateInformation(state);
            OxygenAudioProcessor restored;restored.setStateInformation(state.getData(),(int)state.getSize());
            oxygen::EqualizerModule* restoredEq=nullptr;
            for(auto& n:restored.getModuleNodes())if(auto* e=dynamic_cast<oxygen::EqualizerModule*>(n->getProcessor()))restoredEq=e;
            require(restoredEq && std::abs(restoredEq->apvts.getRawParameterValue("Freq7")->load()-1500.f)<.2f,"Frequency state lost");
            require(std::abs(restoredEq->apvts.getRawParameterValue("Q7")->load()-2.f)<.02f,"Q state lost");
            auto xml=juce::AudioProcessor::getXmlFromBinary(state.getData(),(int)state.getSize());
            auto oldTree=juce::ValueTree::fromXml(*xml);
            for(auto module:oldTree.getChildWithName("MODULE_STATES")) if(module["name"].toString()=="Graphic EQ")
                for(int b=0;b<15;++b){module.removeProperty("Freq"+juce::String(b),nullptr);module.removeProperty("Q"+juce::String(b),nullptr);}
            juce::MemoryBlock oldState;juce::AudioProcessor::copyXmlToBinary(*oldTree.createXml(),oldState);
            restored.setStateInformation(oldState.getData(),(int)oldState.getSize());
            require(std::abs(restoredEq->apvts.getRawParameterValue("Freq7")->load()-oxygen::EqualizerModule::Frequencies[7])<.1f,"Old state kept edited frequency");
            require(std::abs(restoredEq->apvts.getRawParameterValue("Gain7")->load()-6.f)<.01f,"Old gain was changed");
            int legacyParameters=0;
            for(auto& node:p.getModuleNodes())for(auto* parameter:node->getProcessor()->getParameters()) {
                auto* id=dynamic_cast<juce::AudioProcessorParameterWithID*>(parameter);
                if(dynamic_cast<oxygen::EqualizerModule*>(node->getProcessor())&&(id->paramID.startsWith("Freq")||id->paramID.startsWith("Q")))continue;
                auto* actual=dynamic_cast<juce::AudioProcessorParameterWithID*>(p.getParameters()[legacyParameters++]);
                require(actual->paramID==node->getProcessor()->getName()+"_"+id->paramID,"Legacy host parameter index shifted");
            }
            // Check the actual audio response at the edited centre frequency.
            eq->setRateAndBufferSizeDetails(48000,512);eq->prepareToPlay(48000,512);
            juce::MidiBuffer midi;double inEnergy=0,outEnergy=0;
            for(int block=0;block<24;++block){juce::AudioBuffer<float> audio(2,512);
                for(int n=0;n<512;++n){float sample=.05f*std::sin(juce::MathConstants<double>::twoPi*1500.*(block*512+n)/48000.);for(int ch=0;ch<2;++ch)audio.setSample(ch,n,sample);if(block>8)inEnergy+=sample*sample;}
                eq->processBlock(audio,midi);for(int n=0;n<512;++n){float v=audio.getSample(0,n);require(std::isfinite(v),"Nonfinite EQ output");if(block>8)outEnergy+=v*v;}}
            require(std::abs(10.*std::log10(outEnergy/inEnergy)-6.)<.15,"Graph controls do not reach real peak filter");
            LemonLookAndFeel look;MainComponent view(p);view.setLookAndFeel(&look);view.setSize(960,740);
            for(auto* c:view.getChildren())if(auto* b=dynamic_cast<juce::TextButton*>(c))if(b->getButtonText()=="Detailed controls")b->onClick();
            view.setVisible(true);juce::Image shot(juce::Image::ARGB,960,740,true,juce::SoftwareImageType());{juce::Graphics g(shot);view.paintEntireComponent(g,true);}
            juce::FileOutputStream output{juce::File(juce::String::fromUTF8(argv[2]))};require(output.openedOk(),"Cannot write EQ preview");output.setPosition(0);output.truncate();juce::PNGImageFormat png;require(png.writeImageToStream(shot,output),"EQ PNG write failed");view.setLookAndFeel(nullptr);
            std::cout<<"PASS parametric frequency/Q state, real 6 dB response, detailed offscreen render\n";return 0;
        }
        if(argc>2 && std::string(argv[1])=="--overview-check") {
            OxygenAudioProcessor p;
            MasterOverview panel(p);panel.setSize(820,440);
            auto* bass=dynamic_cast<juce::Slider*>(panel.getChildComponent(0));
            require(bass!=nullptr,"Bass control missing");
            bass->setValue(2.,juce::sendNotificationSync);
            oxygen::EqualizerModule* eq=nullptr;
            for(auto& node:p.getModuleNodes())if(auto* m=dynamic_cast<oxygen::EqualizerModule*>(node->getProcessor()))eq=m;
            require(eq!=nullptr && std::abs(eq->apvts.getRawParameterValue("Gain0")->load()-2.f)<.001f,"Tone control did not reach the real EQ");
            require(std::abs(eq->apvts.getRawParameterValue("Gain10")->load())<.001f,"Bass changed air");
            juce::MemoryBlock saved;p.getStateInformation(saved);
            OxygenAudioProcessor restored;restored.setStateInformation(saved.getData(),(int)saved.getSize());
            MasterOverview reopened(restored);
            require(std::abs(dynamic_cast<juce::Slider*>(reopened.getChildComponent(0))->getValue()-2.)<.001,"Macro did not restore from engine state");
            LemonLookAndFeel look;MainComponent view(p);view.setLookAndFeel(&look);view.setSize(960,740);view.setVisible(true);
            juce::Image shot(juce::Image::ARGB,960,740,true,juce::SoftwareImageType());{juce::Graphics g(shot);view.paintEntireComponent(g,true);}
            require(shot.getPixelAt(10,10).getAlpha()>0,"Offscreen view was not painted");
            juce::FileOutputStream output{juce::File(juce::String::fromUTF8(argv[2]))};
            require(output.openedOk(),"Cannot write overview preview");output.setPosition(0);output.truncate();juce::PNGImageFormat png;require(png.writeImageToStream(shot,output),"PNG write failed");
            view.setLookAndFeel(nullptr);
            std::cout<<"PASS overview tone mapping, state restoration and offscreen rendering\n";return 0;
        }
        if (workflow) { runAssistantWorkflowTests(); return 0; } if (argc > 1 && std::string(argv[1]) == "--benchmark") { benchmarkMinimum(); return 0; } testMinimum(); testCrossovers(); testBlockSizes(); testLimiterSafety(); testAssistant(); runAssistantCancellationTests(); }
    catch (const std::exception& error) { std::cerr << "FAIL: " << error.what() << '\n'; return 1; }
    return 0;
}

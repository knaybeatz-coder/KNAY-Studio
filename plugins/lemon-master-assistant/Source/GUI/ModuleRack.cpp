#include "ModuleRack.h"
#include "../PluginProcessor.h"
#include "../MasteringModule.h"
#include "ModuleWrapper.h"

using namespace oxygen;

ModuleRack::ModuleRack(OxygenAudioProcessor& p) : audioProcessor(p)
{
    contentComponent = std::make_unique<juce::Component>();
    setViewedComponent(contentComponent.get(), false);

    // Show the vertical scrollbar only when the rack is taller than the viewport.
    setScrollBarsShown(true, false);
}

ModuleRack::~ModuleRack()
{
}

void ModuleRack::updateModuleList()
{
    tabs.clear();
    moduleWrappers.clear();
    contentComponent->removeAllChildren();

    const auto& nodes = audioProcessor.getModuleNodes();

    for (int i = 0; i < (int)nodes.size(); ++i)
    {
        auto* node = nodes[i].get();
        if (auto* processor = node->getProcessor())
        {
            auto name = processor->getName();
            juce::RangedAudioParameter* bypassParam = nullptr;

            if (auto* mm = dynamic_cast<oxygen::MasteringModule*>(processor))
                bypassParam = mm->getBypassParameter();

            if (processor->hasEditor())
            {
                auto* wrapper = new oxygen::ModuleWrapper(name,
                                                          std::unique_ptr<juce::AudioProcessorEditor>(processor->createEditor()),
                                                          bypassParam,
                                                          processor);
                moduleWrappers.add(wrapper);
                contentComponent->addAndMakeVisible(wrapper);
                const int index=moduleWrappers.size()-1;
                auto* tab=new juce::TextButton(name=="Graphic EQ"?"Equalizer":name);
                tabs.add(tab);contentComponent->addAndMakeVisible(tab);
                tab->onClick=[this,index]{selected=index;resized();};

                wrapper->onHeightChanged = [this] { resized(); };

                // Use a copy of i to avoid scope issues in lambda
                wrapper->onMoveUp   = [this, i] { audioProcessor.moveModuleUp(i);   updateModuleList(); };
                wrapper->onMoveDown = [this, i] { audioProcessor.moveModuleDown(i); updateModuleList(); };
            }
        }
    }

    resized();
}

void ModuleRack::resized()
{
    juce::Viewport::resized();

    selected=juce::jlimit(0,juce::jmax(0,moduleWrappers.size()-1),selected);
    const int width=juce::jmax(1,getWidth()-getScrollBarThickness());
    const int tabWidth=width/juce::jmax(1,tabs.size());
    int height=44;
    for(int i=0;i<moduleWrappers.size();++i){
        auto* wrapper=moduleWrappers[i];const bool active=i==selected;
        wrapper->setVisible(active);tabs[i]->setToggleState(active,juce::dontSendNotification);
        tabs[i]->setBounds(i*tabWidth,0,tabWidth-5,32);
        if(active){wrapper->setBounds(0,44,width,wrapper->getHeight());height+=wrapper->getHeight();}
    }
    contentComponent->setSize(width,juce::jmax(height,getHeight()));
}

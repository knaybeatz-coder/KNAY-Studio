# Quick Sidechain and expandable mixer effects

Select the trigger mixer channel (for example Kick), right-click the destination's routing arrow and choose **Quick Sidechain**. Lemon Limiter loads with its editor closed, in slot 10 or after the last occupied user slot. Existing effects keep their order. The trigger keeps its other outputs and gains a sidechain-only send to the destination.

The limiter starts on COMP with the correct external input, threshold -30 dB, ratio 4:1 and release 200 ms. Destination effect processing is enabled. Open the limiter normally to adjust threshold, ratio, release and other parameters. Global All FX bypass remains an audition control: the status message explains if it must be switched off to hear ducking. Routing loops and a full chain are refused before changing the project. The setup is undoable.

Filling the final visible user slot reveals another empty slot; **+ Add slot** explicitly reveals one too. A channel supports 63 user effects, separate from its dedicated mini EQ. The panel shows ten rows at a time and its visible range. Wheel over the list scrolls once it extends beyond ten rows; Alt+wheel and the slot menu still move an effect. The small slot mix knobs retain their normal wheel behavior. Slot moves skip the reserved EQ. The EQ's pre/post position stays relative to the entire expanded user chain.

Additional filled slots, plugin states, bypasses and mix levels are saved in the Lemon-native LSFX project chunk. Host keys and parameter targets support expanded slots; playback, sidechain input selection and delay compensation include them. Older builds that only support ten slots cannot play these extra effects: open such projects in this build or newer.

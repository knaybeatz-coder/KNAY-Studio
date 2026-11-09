# Playlist track freeze and bounce

Right-click a Playlist track header.

- **Freeze track (save CPU)** renders unmuted audio and patterns through their hosted instruments into floating-point audio, one stem per used mixer input. Original clips remain saved in the project for **Unfreeze track (resume editing)**. The header says FROZEN. Mixer processing, routing, buses and sidechains remain live: freeze saves instrument CPU, not mixer-effect CPU. Instruments shared with other rows stay active. Pattern audition remains available.
- **Unfreeze track (resume editing)** restores original clips at the row's current location. Unfreeze before recording into the row or choosing another comp take. Edits to rendered freeze clips are replaced by the originals on unfreeze.
- **Bounce audio > Bounce audio in new track** inserts a rendered audio track below the source or its complete take-lane group and mutes the source track after a successful bounce. The original clips remain editable and can be heard again by unmuting the source.
- **Bounce audio > Bounce and replace track** replaces the source track's audio and pattern clips with the render on the same row. It keeps automation clips, take lanes and the track's existing mute state. Undo restores the original clips.

Both bounce options render insert processing and outgoing buses. The bounced audio plays through the live master. The Bounce audio menu can be hidden in Workspace Visibility.

Renders preserve song positions and automation, include tails, and use unique filenames in the project's Rendered folder. The existing offline progress/cancellation path is used. The Playlist changes only after successful rendering and file validation. Undo/redo is supported. Frozen source records follow track moves, project save/load, channel and pattern renumbering, and PPQ changes.

Comp renders use the chosen comp on the leader, leaving source takes intact. Insertion preserves contiguous grouping and moves subsequent row assignments. A full Playlist blocks insertion without dropping the last row. In-place replacement needs no free row. Wet bounce with an external sidechain is refused rather than silently rendering the wrong pumping; freeze keeps mixer sidechains live. Freeze buffers only the mixer stems used by the source track.

Focused checks cover playback levels with non-unity mixer/master faders, direct-to-master and multiple insert stems, synthetic hosted instrument rendering, exclusive generator suspension and shared-generator retention, undo/redo, save/reopen then move/unfreeze, comp-family bounce and existing comping regressions. No microphone or production project is used by these checks.

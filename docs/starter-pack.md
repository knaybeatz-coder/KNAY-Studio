# Lemon Studio Starter Pack

The empty Samples browser offers **Add my sample folder**, **Drag samples or folders here**, and **Download Lemon Studio Starter Pack**. There is one curated pack, no pack selector. The browser arrow menu also offers the download for existing libraries, or Cancel while downloading. Search with no matches and empty Favorites keep their usual messages.

Version 1 contains 263 unchanged CC0 WAV files (296,600,789 bytes, approximately 283 MiB):

- 149 drum and percussion hits (electronic and acoustic kicks, snares, claps, hats, cymbals, toms and percussion)
- 32 loops (drums, bass, guitar, melodic/chord and texture loops)
- 26 keyboard hits (grand piano, FM piano and clavisynth)
- 16 marimba/vibraphone hits and 8 harp hits
- 12 vocal hits and 20 effects/textures

This is a practical starter selection, not a complete multisampled instrument library. Original filenames retain supplied tempo/key/pitch; nothing is guessed. The contributor credits and CC0 license are installed with the sounds. Sources are Stargate's CC0 pack at commit dbfd6ec52d4ed53b60bdbea5fc6adf295127c027 and VCSL at c1ea7bcc3c7309650ab0da9d15c9cd1fbc4a4c7e. Lemon names the selection, not the recording authorship. See assets/starter-pack for pinned URLs, byte counts, SHA-256 hashes and upstream license/credit texts.

No network access or download thread starts until the user clicks Download. Individual pinned WAVs download over HTTPS without redirects, with bounded lengths and SHA-256 validation. A temporary, hidden staging directory is renamed into the final library only after every sample has been verified and copied. There are no installers or executable downloads. Cancellation/network errors stay inline, successful cached files can be reused on retry, and existing destination contents are never overwritten. Connection/read timeouts bound offline waits; cancellation is observed between network reads (up to 15 seconds if a read is stalled).

The pack lives in the active profile's Samples/Lemon Studio Starter Pack folder. Download cache lives in Sample Downloads/Starter Pack v1 (allow about 570 MiB total storage). If the user's library points elsewhere, the finished pack is added as an extra browser root, without changing that library preference. Personal and Development profiles remain separate. No samples are downloaded at startup or included in the application binary.

Dropping a folder uses the existing folder flow. Audio files dropped onto the Samples browser become persistent references in the browser, without copying/moving originals or adding project channels; keep those source files in place. Dropping onto the playlist/rack keeps existing behavior. References can be managed in the browser extra-search-folder settings.

Focused verification covers manifest paths/hashes, corrupt/truncated/cancelled transfers, an existing destination's preservation, empty/search/Favorites UI, folder-picker action and (with LEMON_STARTER_AUDIT set to the curated WAV directory) actual engine decoding of every WAV, one real HTTPS production download, cached retry, complete installation, tree refresh and file drops. LEMON_STARTER_SHOT optionally saves an offscreen browser image. No GUI automation or microphone use. Musical audition and native drag/drop are the user's hands-on checks. This feature is local until separately published.

Vendored from nih-plug de421011f41a6d10fc8c7a6084e4f4dee0143683, ISC license.
Only core and derive sources retained; workspace membership moved to the isolated standalone workspace.
Lemon Wave standalone patch: process oversized CPAL output callbacks in bounded
chunks, advance transport by actual frames, and consume only the actual input
frame count. Device-independent regression coverage exercises the output callback.
The normal DAW/plugin workspace does not use this patch.

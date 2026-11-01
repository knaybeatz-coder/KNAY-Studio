# SPDX-License-Identifier: GPL-3.0-or-later
"""Lemon's private local worker. No audio uploads; no FFmpeg or system Python needed."""
import os
import sys
from pathlib import Path
import torch
import numpy as np
import soundfile as sf
from demucs.pretrained import get_model
from demucs.apply import apply_model
from demucs.audio import convert_audio

root = Path(__file__).resolve().parent
torch.set_num_threads(min(4, os.cpu_count() or 1))
torch.manual_seed(0)
# LocalRepo verifies the official model's SHA-256 filename suffix before loading.
model = get_model("955717e8", repo=root / "models").cpu().eval()
if sys.argv[1] == "--check":
    assert set(model.sources) == {"vocals", "drums", "bass", "other"}
    print("Lemon stem separation engine ready", flush=True)
    sys.exit(0)
source, out = Path(sys.argv[1]), Path(sys.argv[2])
selected = set(sys.argv[3].split(",")) if len(sys.argv) > 3 else set(model.sources)
if not selected or not selected.issubset(model.sources):
    raise ValueError("Choose at least one valid stem")
audio, rate = sf.read(source, dtype="float32", always_2d=True)
frames = len(audio)
wav = convert_audio(torch.from_numpy(audio.T.copy()), rate, model.samplerate, model.audio_channels)
ref = wav.mean(0)
mean, std = ref.mean(), ref.std()
if not torch.isfinite(wav).all():
    raise ValueError("The input contains invalid audio values")
with torch.inference_mode():
    if float(std) < 1e-8:
        # Silence is a valid input; do not divide by zero.
        sources = torch.zeros((len(model.sources), model.audio_channels, wav.shape[-1]))
        sources[model.sources.index("other")] = wav
    else:
        sources = apply_model(model, ((wav - mean) / std)[None], device="cpu", shifts=1,
                              split=True, overlap=0.25, progress=True, num_workers=0)[0]
        sources = sources * std + mean
    for name, track in zip(model.sources, sources):
        if name not in selected:
            continue
        track = convert_audio(track, model.samplerate, rate, 2).T.numpy()
        track = track[:frames]
        if len(track) < frames:
            track = np.pad(track, ((0, frames-len(track)), (0, 0)))
        if not np.isfinite(track).all():
            raise ValueError("Invalid separated audio")
        sf.write(out / (name + ".wav"), track, rate, subtype="FLOAT")
print("Selected stems saved", flush=True)

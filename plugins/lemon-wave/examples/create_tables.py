"""Generate the original tutorial wavetables (8 frames, 2048 samples per frame)."""
import math
import pathlib
import struct
import wave

out = pathlib.Path(__file__).resolve().parent.parent / "wavetables"
out.mkdir(exist_ok=True)
for name, hollow in [("Citrus Motion", False), ("Hollow Drift", True)]:
    samples = []
    for frame in range(8):
        t = frame / 7
        for i in range(2048):
            phase = math.tau * i / 2048
            value = math.sin(phase) * 0.55
            for h in range(2, 33):
                amplitude = (math.exp(-((h - (3 + t * 16)) / 3) ** 2) * 0.3 / math.sqrt(h)
                             if hollow else t * 0.3 / h)
                if not hollow or h % 2:
                    value += math.sin(phase * h) * amplitude
            samples.append(value)
    gain = 0.85 / max(abs(x) for x in samples)
    with wave.open(str(out / f"{name}.wav"), "wb") as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(48000)
        f.writeframes(b"".join(struct.pack("<h", round(x * gain * 32767)) for x in samples))
print("Created two original 8-frame tables; import at Cycle: 2048.")

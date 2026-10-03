"""Generate docs/fixtures/assets/ritmo.ogg, the audio-reactive test track.

Eight seconds at 120 BPM (a beat every 0.5 s): the first bar (0-2 s) is a
quiet pad; from 2 s a kick on every beat, a hi-hat on every off-beat and a
bass line. The samples are deterministic (the noise uses a fixed seed); the
Vorbis encoder may still change a few bytes between runs. Needs FFmpeg.

    python tools/generate_audio_fixture.py
"""

import math
import random
import subprocess
import tempfile
import wave
from pathlib import Path

RATE = 22050
SECONDS = 8.0
BEAT = 0.5
OUT = Path(__file__).resolve().parent.parent / "docs/fixtures/assets/ritmo.ogg"


def main() -> None:
    rng = random.Random(7)
    count = int(RATE * SECONDS)
    samples = [0.0] * count
    for n in range(count):
        t = n / RATE
        # A soft pad all along, so the quiet bar is not silence.
        samples[n] += 0.06 * math.sin(2 * math.pi * 220 * t) * (0.6 + 0.4 * math.sin(2 * math.pi * 0.25 * t))
    beats = [i * BEAT for i in range(int(SECONDS / BEAT)) if i * BEAT >= 2.0]
    for start in beats:
        # Kick: a sine sweeping 120 -> 45 Hz with a fast decay.
        for k in range(int(0.25 * RATE)):
            t = k / RATE
            phase = 2 * math.pi * (45 * t + 75 * (1 - math.exp(-t * 30)) / 30)
            index = int(start * RATE) + k
            if index < count:
                samples[index] += 0.9 * math.sin(phase) * math.exp(-t * 12)
        # Hi-hat on the off-beat: short bright noise.
        off = start + BEAT / 2
        for k in range(int(0.05 * RATE)):
            index = int(off * RATE) + k
            if index < count:
                samples[index] += 0.25 * rng.uniform(-1, 1) * math.exp(-k / RATE * 80)
    # Bass line: a note per beat, from 2 s.
    notes = [55.0, 55.0, 65.4, 49.0]
    for i, start in enumerate(beats):
        frequency = notes[i % len(notes)]
        for k in range(int(0.45 * RATE)):
            t = k / RATE
            index = int(start * RATE) + k
            if index < count:
                samples[index] += 0.25 * math.sin(2 * math.pi * frequency * t) * min(1.0, t * 50) * math.exp(-t * 3)
    peak = max(abs(value) for value in samples)
    frames = b"".join(int(value / peak * 0.9 * 32767).to_bytes(2, "little", signed=True) for value in samples)
    with tempfile.TemporaryDirectory() as directory:
        wav = Path(directory) / "ritmo.wav"
        with wave.open(str(wav), "wb") as file:
            file.setnchannels(1)
            file.setsampwidth(2)
            file.setframerate(RATE)
            file.writeframes(frames)
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-i", str(wav), "-c:a", "libvorbis", "-q:a", "4", "-map_metadata", "-1", str(OUT)],
            check=True,
        )
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()

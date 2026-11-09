"""Private Community Samples service for the native Lemon Studio Browser."""
import json
import os
import re
import sys
import struct
import subprocess
import tempfile
from pathlib import Path

from exchange import CATS, TERMS, credentials, request, psutil, tk, filedialog, messagebox
import base64


def check_wave_container(audio):
    # The service validates the encoding and frames while generating its waveform.
    if len(audio) < 44 or audio[:4] != b"RIFF" or audio[8:12] != b"WAVE":
        raise ValueError("Choose a valid WAV file.")


AUDIO_TYPES = ("*.wav", "*.wave", "*.flac", "*.mp3", "*.ogg", "*.oga", "*.aif", "*.aiff", "*.aifc", "*.wv")
MAX_BYTES = 5 * 1024 * 1024


def wave_chunks(audio):
    if audio[:4] != b"RIFF" or audio[8:12] != b"WAVE":
        return
    pos = 12
    while pos + 8 <= len(audio):
        length = int.from_bytes(audio[pos + 4:pos + 8], "little")
        end = pos + 8 + length
        if end > len(audio):
            raise ValueError("The WAV file is incomplete.")
        yield audio[pos:pos + 4], memoryview(audio)[pos + 8:end]
        pos = end + length % 2


def upload_audio(path):
    if path.stat().st_size > MAX_BYTES:
        raise ValueError("Community samples must be 5 MB or smaller.")
    audio = path.read_bytes()
    chunks = dict(wave_chunks(audio))
    fmt = chunks.get(b"fmt ", b"")
    if len(fmt) >= 16 and chunks.get(b"data"):
        tag, channels, _, _, stride, bits = struct.unpack_from("<HHIIHH", fmt)
        if tag == 65534 and len(fmt) >= 40:
            tag = int.from_bytes(fmt[24:26], "little")
        if channels and stride == channels * bits // 8 and (
            (tag == 1 and bits in (8, 16, 24, 32)) or (tag == 3 and bits in (32, 64))
        ):
            return audio
    converter = Path(__file__).resolve().with_name("ffmpeg.exe")
    with tempfile.TemporaryDirectory() as scratch:
        source = path
        # Lemon Studio also reads complete Ogg streams inside legacy Vorbis-ACM WAVs.
        if len(fmt) >= 2 and int.from_bytes(fmt[:2], "little") in (0x674F, 0x6750, 0x6751, 0x676F, 0x6770, 0x6771):
            source = Path(scratch) / "stream.ogg"
            source.write_bytes(chunks[b"data"])
        trim = []
        if source != path and len(chunks.get(b"fact", b"")) >= 4:
            frames = int.from_bytes(chunks[b"fact"][:4], "little")
            if frames:
                trim = ["-af", f"atrim=end_sample={frames}"]
        out = Path(scratch) / "community.wav"
        proc = subprocess.run([str(converter), "-nostdin", "-v", "error", "-protocol_whitelist", "file,pipe",
                               "-format_whitelist", "wav,w64,aiff,mp3,flac,ogg,wv", "-i", str(source.resolve()), "-map", "0:a:0", *trim, "-c:a", "pcm_f32le",
                               "-fs", str(MAX_BYTES + 65536), str(out)], capture_output=True,
                              creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        if proc.returncode:
            raise ValueError("Could not decode this audio file. Check that it plays in Lemon Studio.")
        if out.stat().st_size > MAX_BYTES:
            raise ValueError("The decoded WAV exceeds the 5 MB community limit. Choose a shorter excerpt.")
        audio = out.read_bytes()
        check_wave_container(audio)
        return audio


def sample_path(root, item):
    ident = item["id"]
    if not re.fullmatch(r"[a-zA-Z0-9-]{1,64}", ident):
        raise ValueError("Invalid sample identifier")
    title = re.sub(r"[^a-zA-Z0-9 _-]", "_", item["title"])[:60]
    return root / (title + " - " + ident + ".wav")


def run(job, root):
    cred = credentials()
    action = job["action"]
    token = job.get("token", "")
    if action == "catalog":
        items = request(cred, "/catalog")
        for item in items:
            item["path"] = str(sample_path(root, item))
        return {"items": items}
    if action == "fetch":
        item = job["item"]
        dest = sample_path(root, item)
        root.mkdir(parents=True, exist_ok=True)
        if not dest.is_file():
            audio = request(cred, "/wav/" + item["id"])
            check_wave_container(audio)
            with tempfile.NamedTemporaryFile(dir=root, delete=False) as out:
                tmp = Path(out.name)
            try:
                tmp.write_bytes(audio)
                tmp.replace(dest)
            finally:
                tmp.unlink(missing_ok=True)
        return {"path": str(dest)}
    if action == "vote":
        request(cred, "/vote", {"id": job["id"]}, token)
        return {"message": "Vote saved."}
    if action in ("login", "register"):
        data = dict(email=job["email"], password=job["password"])
        if action == "register":
            data["invite"] = cred["invite"]
        return {"token": request(cred, "/" + action, data)["token"], "message": "Signed in."}
    if action == "upload":
        path = Path(job["path"])
        if not job.get("agree") or job["category"] not in CATS:
            raise ValueError("Choose a category and accept the royalty-free upload terms.")
        audio = upload_audio(path)
        request(cred, "/upload", dict(title=job["title"], creator=job["creator"], category=job["category"],
                subtype=job["subtype"], genre=job["genre"], agree=True, wav=base64.b64encode(audio).decode()), token)
        return {"message": "Sample uploaded."}
    window = tk.Tk()
    window.withdraw()
    window.attributes("-topmost", True)
    try:
        if action == "terms":
            messagebox.showinfo("Royalty-free upload terms", TERMS, parent=window)
        elif action == "choose":
            path = filedialog.askopenfilename(parent=window, filetypes=[("Lemon Studio audio", AUDIO_TYPES), ("All files", "*.*")])
            if path:
                return {"path": path}
        elif action == "newsletter":
            old = request(cred, "/me", token=token)["newsletter"]
            if messagebox.askyesno("Email preferences", ("Unsubscribe from" if old else "Subscribe to") +
                                    " Lemon Studio news by email?", parent=window):
                request(cred, "/newsletter", {"subscribe": not old}, token)
                return {"message": "Email preference saved."}
        else:
            raise ValueError("Unknown Browser action.")
        return {}
    finally:
        window.destroy()


if __name__ == "__main__":
    try:
        host = psutil.Process(int(sys.argv[1]))
        if host.pid != os.getppid() or Path(host.exe()).resolve() != Path(__file__).resolve().parents[1] / "lemonstudio-development.exe":
            raise ValueError("Open Community Samples in Lemon Studio Development.")
        result = run(json.loads(sys.stdin.buffer.read()), Path(sys.argv[2]) / "Sample Exchange")
    except Exception as error:
        result = {"error": str(error)}
    sys.stdout.buffer.write(json.dumps(result).encode("utf-8"))

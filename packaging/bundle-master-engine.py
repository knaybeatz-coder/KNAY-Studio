"""Package a built native VST3 for embedding; never consumes credentials or personal data."""
import argparse, hashlib, json, zipfile
from pathlib import Path
p=argparse.ArgumentParser()
p.add_argument('--bundle', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--os', choices=['windows','linux','macos'], required=True)
p.add_argument('--arch', choices=['x86_64','aarch64'], required=True)
a=p.parse_args()
if a.bundle.name != 'Lemon Master Assistant.vst3' or not a.bundle.is_dir(): p.error('Expected the built VST3 bundle directory')
files=sorted(x for x in a.bundle.rglob('*') if x.is_file())
expected={'windows':'Contents/x86_64-win/Lemon Master Assistant.vst3','linux':'Contents/x86_64-linux/Lemon Master Assistant.so','macos':'Contents/MacOS/Lemon Master Assistant'}[a.os]
if not (a.bundle/expected).is_file(): p.error('Platform engine binary is missing')
manifest=dict(os=a.os, arch=a.arch, files=[])
a.output.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(a.output, 'w', zipfile.ZIP_DEFLATED) as z:
    for f in files:
        if f.is_symlink(): p.error('Symlinks are not supported')
        name=f.relative_to(a.bundle.parent).as_posix()
        b=f.read_bytes()
        manifest['files'].append(dict(path=name, size=len(b), sha256=hashlib.sha256(b).hexdigest()))
        z.write(f,name)
    z.writestr('engine.json',json.dumps(manifest))
print(f'Packaged {len(files)} engine files for {a.os}/{a.arch}: {a.output}')

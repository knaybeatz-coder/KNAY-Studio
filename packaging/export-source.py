"""Export corresponding source, including uncommitted customizations and embedded assets."""
from pathlib import Path
import subprocess, sys, zipfile, os

from private_build import private_source, public_source_bytes
public = "--public" in sys.argv[2:]

root = Path(__file__).resolve().parents[1]
out = Path(sys.argv[1]).resolve()
if root == out or root in out.parents:
    raise SystemExit('Export outside the source checkout.')
if (root / '.git').exists():
    names = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root).decode().split('\0')
else:
    names = [p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file()]
names = set(names) | {'crates/os-app/assets/startup.wav', 'crates/os-app/assets/templates/Lemon Starter.flp'}
out.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for name in sorted(names):
        if public and private_source(name): continue
        p = root / name
        if not name or not p.is_file(): continue
        if any(part.startswith('target') or part == '.git' for part in Path(name).parts): continue
        if '.before-' in name or p.suffix in {'.pem', '.key', '.enc'} or p.name == '.env': continue
        if public:
            z.writestr('Source/' + name, public_source_bytes(name, p.read_bytes()))
        else:
            z.write(p, 'Source/' + name)
    # Corresponding source includes the exact JUCE tree used for the bundled engine.
    if public:
        juce = Path(os.environ.get('LEMON_JUCE_SOURCE', root/'plugins/lemon-master-assistant/JUCE'))
        if not (juce/'modules/juce_core/juce_core.h').is_file():
            raise SystemExit('Set LEMON_JUCE_SOURCE to the JUCE 8.0.9 source used to build Master Assistant.')
        for p in sorted(juce.rglob('*')):
            rel = p.relative_to(juce)
            if p.is_file() and '.git' not in rel.parts:
                name = 'plugins/lemon-master-assistant/JUCE/' + rel.as_posix()
                if name not in names:
                    z.writestr('Source/' + name, public_source_bytes(name, p.read_bytes()))
print(out)

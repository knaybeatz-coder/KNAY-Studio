"""Export corresponding source, including uncommitted customizations and embedded assets."""
from pathlib import Path
import subprocess, sys, zipfile

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
        p = root / name
        if not name or not p.is_file(): continue
        if any(part.startswith('target') or part == '.git' for part in Path(name).parts): continue
        if '.before-' in name or p.suffix in {'.pem', '.key', '.enc'} or p.name == '.env': continue
        z.write(p, 'Source/' + name)
print(out)

"""Run after the native Mac build. Creates a DMG and a Mac-only website update ZIP."""
from pathlib import Path
import hashlib, json, platform, re, shutil, subprocess, sys, tempfile, zipfile
if platform.system() != 'Darwin': raise SystemExit('macOS is required.')
root = Path(__file__).resolve().parents[2]
app = Path(sys.argv[1]).resolve()
version = re.search(r'pub const VERSION: u64 = (\d+);', (root/'crates/os-app/src/updates.rs').read_text()).group(1)
arch = platform.machine()
if arch not in ('arm64','x86_64'): raise SystemExit('Unsupported architecture')
out = Path.home()/'Desktop/Lemon Studio Mac Public Download'/version
out.mkdir(parents=True, exist_ok=True)
source = out/f'Lemon-Studio-Source-Mac-{arch}-{version}.zip'
subprocess.run([sys.executable, str(root/'packaging/export-source.py'), str(source)], check=True)
stage = Path(tempfile.mkdtemp(prefix='mac-package-', dir=app.parent))
subprocess.run(['ditto', str(app), str(stage/'Lemon Studio.app')], check=True)
(stage/'Applications').symlink_to('/Applications')
for name in ('LICENSE','THIRD-PARTY-NOTICES.txt'): shutil.copy2(root/name, stage/name)
shutil.copy2(source, stage/'Lemon Studio Source.zip')
(stage/'READ ME.txt').write_text(f'Lemon Studio - Michael Lane edition\nMac architecture: {arch}\nDrag the app to Applications. This community build is ad-hoc signed, not notarized.\nMatching editable source and notices are included. Existing projects and settings are preserved.\n')
dmg = out/f'Lemon-Studio-Mac-{arch}-{version}.dmg'
if dmg.exists(): raise SystemExit('Versioned DMG already exists; preserve published artifacts and increment the build number for changes.')
subprocess.run(['hdiutil','create','-volname','Lemon Studio','-srcfolder',str(stage),'-format','UDZO',str(dmg)], check=True)
subprocess.run(['hdiutil','verify',str(dmg)], check=True)
manifest = {'version':int(version),'label':f'Mac {arch}', 'url':f'mac/{version}/{dmg.name}', 'source':f'mac/{version}/{source.name}', 'sha256':hashlib.sha256(dmg.read_bytes()).hexdigest()}
(out/f'mac-{arch}.json').write_text(json.dumps(manifest, indent=2))
with zipfile.ZipFile(out/f'Mac-{arch}-{version}-files.zip','w',zipfile.ZIP_DEFLATED) as z:
    for p in (dmg, source): z.write(p, f'lemon-studio/mac/{version}/{p.name}')
with zipfile.ZipFile(out/f'Mac-{arch}-{version}-publish-last.zip','w',zipfile.ZIP_DEFLATED) as z:
    z.write(out/f'mac-{arch}.json', f'lemon-studio/platforms/mac-{arch}.json')
print(f'Ready for publication: {out}\nUpload/extract files first, verify HTTPS downloads, then upload/extract publish-last. See MAC_RELEASE.md.')

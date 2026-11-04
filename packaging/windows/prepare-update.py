"""Prepare a signed update; never uploads or publishes it.
Requires Python cryptography. Keep the private key outside this repository.
Usage: python packaging/windows/prepare-update.py --output ABSOLUTE_STAGING_FOLDER
Build and test the app first. Review output before publishing a new release.
"""
from pathlib import Path
import argparse, hashlib, json, re, shutil, subprocess, zipfile, sys
from cryptography.hazmat.primitives.serialization import load_pem_private_key, Encoding, PublicFormat

root = Path(__file__).resolve().parents[2]
sys.path.insert(0,str(root/"packaging"))
from private_build import require_public_binary
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path)
parser.add_argument('--output', required=True, type=Path)
parser.add_argument('--key', type=Path, default=Path.home()/'.lemon-studio-release/update-signing-key.pem')
parser.add_argument('--notes')
args = parser.parse_args()
out = args.output.resolve()
if out == root or root in out.parents:
    raise SystemExit('Stage releases outside the source checkout.')
version = int(re.search(r'pub const VERSION: u64 = (\d+);', (root/'crates/os-app/src/updates.rs').read_text()).group(1))
if args.notes is None:
    history=json.loads((root/'packaging/changelog.json').read_text(encoding='utf-8'))
    section=next((s for s in history['sections'] if str(version) in s['title']),None)
    if section is None:raise SystemExit('Missing release changelog entry')
    args.notes=(section['title']+' — '+' '.join(section['changes'][:2]))[:3000]
exe = args.binary or root/'target/release/lemonstudio.exe'
require_public_binary(exe)
actual = subprocess.check_output([str(exe),'--update-version'],text=True).strip()
if actual != str(version): raise SystemExit('Rebuild the executable after changing its version.')
key = load_pem_private_key(args.key.read_bytes(),None)
public = key.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw).hex()
if public != (root/'crates/os-app/src/update-public-key.hex').read_text().strip():
    raise SystemExit('Signing key does not match the compiled public key.')
site = out/'lemon-studio'
release = site/'updates'/str(version)
release.mkdir(parents=True,exist_ok=True)
subprocess.run([sys.executable,str(root/'packaging/export-source.py'),str(release/'Lemon Studio Source.zip'),'--public'],check=True)
for name in ['LICENSE','THIRD-PARTY-NOTICES.txt']:shutil.copy2(root/name,release/name)
shutil.copy2(exe,release/'lemonstudio.exe')
files=[]
for name in ['lemonstudio.exe','Lemon Studio Source.zip','THIRD-PARTY-NOTICES.txt','LICENSE']:
    p=release/name
    files.append(dict(name=name,sha256=hashlib.sha256(p.read_bytes()).hexdigest(),size=p.stat().st_size))
label=f'{str(version)[:4]}.{str(version)[4:6]}.{str(version)[6:8]}.{int(str(version)[8:])}'
manifest=dict(product='lemon-studio-michael-lane-windows-x64',version=version,label=label,notes=args.notes,files=files)
payload=json.dumps(manifest,separators=(',',':'),ensure_ascii=True)
signed=json.dumps(dict(payload=payload,signature=key.sign(payload.encode()).hex()),indent=2)
(release/'signed.json').write_text(signed,encoding='utf-8')
(site/'updates/stable.json').write_text(signed,encoding='utf-8')
(site/'updates/.htaccess').write_text('Options -Indexes\n<IfModule mod_headers.c>\nHeader set X-Robots-Tag "noindex, nofollow"\n<Files "stable.json">\nHeader set Cache-Control "no-store"\n</Files>\n</IfModule>\n',encoding='utf-8')
subprocess.run([str(exe),'--verify-update',str(release/'signed.json')],check=True)
print(f'Prepared {label}: {release}')
print('Publish versioned files first; replace stable.json last. Keep the signing key private.')

"""Prepare signed updates. Private keys stay on the build machine."""
from pathlib import Path
from private_build import require_public_binary
import argparse, hashlib, json, re, shutil
from cryptography.hazmat.primitives.serialization import load_pem_private_key, Encoding, PublicFormat
root=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser()
p.add_argument('--platform',choices=['linux-x64','mac-arm64','mac-x86_64'],required=True)
p.add_argument('--site',type=Path,required=True)
p.add_argument('--app',type=Path,required=True)
a=p.parse_args()
v=int(re.search(r'pub const VERSION: u64 = (\d+);',(root/'crates/os-app/src/updates.rs').read_text()).group(1))
mac=a.platform.startswith('mac-')
require_public_binary(a.app/'Contents/MacOS/lemonstudio' if mac else a.app)
keyfile='mac-update-signing-key.pem' if mac else 'update-signing-key.pem'
key=load_pem_private_key((Path.home()/'.lemon-studio-release'/keyfile).read_bytes(),None)
public=key.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw).hex()
expected=root/'crates/os-app/src'/('update-mac-public-key.hex' if mac else 'update-public-key.hex')
if public!=expected.read_text().strip():raise SystemExit('Update signing key mismatch')
feedroot=a.site/('mac/updates/'+a.platform[4:] if mac else 'updates/linux-x64')
release=feedroot/str(v)
release.mkdir(parents=True,exist_ok=True)
if mac:
    names=['Contents/MacOS/lemonstudio','Contents/Info.plist','Contents/PkgInfo','Contents/Resources/Assets.car','Contents/Resources/lemonstudio.icns','Contents/_CodeSignature/CodeResources']
    actual={p.relative_to(a.app).as_posix() for p in a.app.rglob('*') if p.is_file()}
    if actual!=set(names) or any(p.is_symlink() for p in a.app.rglob('*')):raise SystemExit('Unexpected app bundle layout')
    sources=[(name,a.app/name) for name in names]
else:
    sources=[('lemonstudio',a.app),('Lemon Studio Source.zip',a.site/'updates'/str(v)/'Lemon Studio Source.zip')]+[(name,root/name) for name in ['LICENSE','THIRD-PARTY-NOTICES.txt']]
files=[]
for name,source in sources:
    dest=release/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,dest)
    files.append(dict(name=name,sha256=hashlib.sha256(dest.read_bytes()).hexdigest(),size=dest.stat().st_size))
label=f'{str(v)[:4]}.{str(v)[4:6]}.{str(v)[6:8]}.{int(str(v)[8:])}'
history=json.loads((root/'packaging/changelog.json').read_text(encoding='utf-8'))
section=next((s for s in history['sections'] if str(v) in s['title']),None)
if section is None:raise SystemExit('Missing release changelog entry')
notes=(section['title']+' — '+' '.join(section['changes'][:2]))[:3000]
m=dict(product='lemon-studio-michael-lane-'+a.platform,version=v,label=label,notes=notes,files=files)

payload=json.dumps(m,separators=(',',':'),ensure_ascii=True)
signed=json.dumps(dict(payload=payload,signature=key.sign(payload.encode()).hex()),indent=2)
(release/'signed.json').write_text(signed)
(feedroot/'stable.json').write_text(signed)
print('Prepared signed update:',feedroot)

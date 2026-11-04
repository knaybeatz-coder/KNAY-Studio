"""Publish only a prepared, checksum-verified complete DAW release, using Actions' token."""
import hashlib,json,os,re,subprocess,tempfile,urllib.parse,urllib.request
from pathlib import Path
root=Path(__file__).resolve().parents[1]
plan=json.loads((root/'packaging/github-release.json').read_text())
if os.environ.get('GH_REPO')!='Dorfhaus/Lemon-Studio-Michael-Lane-Edition':raise SystemExit('Wrong public repository')
tag=plan['tag']
if not re.fullmatch(r'v[0-9]{4}\.[0-9]{2}\.[0-9]{2}\.[0-9]+',tag):raise SystemExit('Invalid release tag')
def gh(*args):subprocess.run(['gh',*args],check=True)
with tempfile.TemporaryDirectory(prefix='lemon-publication-') as folder:
    folder=Path(folder);assets=[]
    for entry in plan['assets']:
        parsed=urllib.parse.urlsplit(entry['url']);name=entry['name']
        if parsed.scheme!='https' or parsed.hostname!='michaellanemusic.de' or not parsed.path.startswith('/lemon-studio/') or Path(name).name!=name:raise SystemExit('Invalid artifact location')
        dest=folder/name;digest=hashlib.sha256()
        with urllib.request.urlopen(entry['url'],timeout=180) as response,dest.open('wb') as out:
            while chunk:=response.read(1024*1024):out.write(chunk);digest.update(chunk)
        if digest.hexdigest()!=entry['sha256']:raise SystemExit('Checksum mismatch: '+name)
        assets.append(dest)
    sums=folder/'SHA256SUMS.txt';sums.write_text(''.join(e['sha256']+'  '+e['name']+'\n' for e in plan['assets']));assets.append(sums)
    notes=folder/'notes.md';notes.write_text(plan['notes'])
    existing=subprocess.run(['gh','release','view',tag,'--json','isDraft'],capture_output=True,text=True)
    if existing.returncode==0:
        if not json.loads(existing.stdout)['isDraft']:raise SystemExit('Published release is immutable; verify it instead of overwriting')
    else:gh('release','create',tag,'--target',os.environ['GITHUB_SHA'],'--title',plan['title'],'--notes-file',str(notes),'--draft')
    gh('release','upload',tag,*map(str,assets),'--clobber')
    releases=json.loads(subprocess.check_output(['gh','api','repos/'+os.environ['GH_REPO']+'/releases?per_page=100'],text=True))
    data=next((release for release in releases if release['tag_name']==tag),None)
    if data is None:raise SystemExit('Prepared draft release not found')
    remote={a['name']:a for a in data['assets']}
    for path in assets:
        expected='sha256:'+hashlib.sha256(path.read_bytes()).hexdigest()
        if remote.get(path.name,{}).get('digest')!=expected:raise SystemExit('GitHub asset digest mismatch: '+path.name)
    gh('release','edit',tag,'--draft=false','--latest')
    print('Complete DAW release published and asset hashes verified:',tag)

"""Shared local-build workflow. Credentials live in the OS keychain, never Git/SMB."""
from pathlib import Path, PurePosixPath
import argparse, datetime, ftplib, getpass, hashlib, io, json, os, platform, re
import shutil, ssl, subprocess, sys, tarfile, urllib.request, urllib.parse, uuid, shlex, stat

ROOT = Path(__file__).resolve().parents[2]
PRIVATE = Path.home()/'.lemon-studio-release'
CONFIG = PRIVATE/'workflow.json'
BASE = 'https://michaellanemusic.de/lemon-studio/'
SERVICE = 'Lemon Studio publishing'

def run(*args, cwd=ROOT, env=None):
    subprocess.run([str(x) for x in args], cwd=cwd, env=env, check=True)

def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()

def read_config():
    return json.loads(CONFIG.read_text(encoding='utf-8-sig')) if CONFIG.exists() else {}

def digest(p):
    with Path(p).open('rb') as f: return hashlib.file_digest(f, 'sha256').hexdigest()

def version():
    return int(re.search(r'pub const VERSION: u64 = (\d+);', (ROOT/'crates/os-app/src/updates.rs').read_text()).group(1))

def shared_folder():
    cfg=read_config()
    if cfg.get('share'): p=Path(cfg['share'])
    elif os.name=='nt': p=Path(r'\\192.168.178.132\Home-Shared\Storage\External-HD-Large-Files\Lemon Studio')
    else: p=Path('/Volumes/Home-Shared/Storage/External-HD-Large-Files/Lemon Studio')
    if not (p/'Lemon Studio.git').is_dir(): raise RuntimeError('Mount the home-server share first; shared Git repository is unavailable.')
    return p

def sync():
    if git('status','--porcelain'): raise RuntimeError('Commit/reconcile local source changes before syncing. Nothing was stashed, reset or overwritten.')
    run('git','fetch','home','main')
    run('git','merge','--ff-only','home/main')
    run('git','push','home','HEAD:main')
    if git('rev-parse','HEAD') != git('rev-parse','home/main'): raise RuntimeError('Shared revision mismatch')

def connect(cfg):
    if cfg.get('ftp_credentials_file'):
        p=Path(cfg['ftp_credentials_file']).expanduser()
        if os.name!='nt' and (p.stat().st_uid!=os.getuid() or stat.S_IMODE(p.stat().st_mode)&0o077):
            raise RuntimeError('Existing hosting credential file must be owned by this user and private (0600).')
        values={}
        for line in p.read_text().splitlines():
            line=line.strip().removeprefix('export ')
            if not line or line.startswith('#') or '=' not in line:continue
            key,value=line.split('=',1)
            if key in ('FTP_HOST','FTP_USER','FTP_PASS'):
                parts=shlex.split(value,comments=True)
                if len(parts)!=1:raise RuntimeError('Unsupported hosting credential syntax; no shell evaluation is performed.')
                values[key]=parts[0]
        if values.get('FTP_HOST')!=cfg['ftp_host']:raise RuntimeError('Existing credentials belong to a different host.')
        user=values.get('FTP_USER');password=values.get('FTP_PASS')
    else:
        import keyring
        user=cfg.get('ftp_user')
        if not user: raise RuntimeError('Publishing login is not configured. Run release.py configure once on this computer.')
        password=keyring.get_password(SERVICE,user)
    if not password: raise RuntimeError('Publishing password missing from OS keychain. Run release.py configure.')
    ftp=open_secure_connection(cfg['ftp_host'])
    return login_connection(ftp,user,password,cfg['ftp_root'])

def open_secure_connection(host):
    """Check connectivity before asking for credentials; never downgrade TLS."""
    ftp=ftplib.FTP_TLS(context=ssl.create_default_context(), timeout=8)
    print('Checking secure upload server...',flush=True)
    try:
        ftp.connect(host,21)
    except OSError as ex:
        ftp.close()
        raise RuntimeError(f'Cannot reach the upload server {host} on port 21. No username or password was sent. The FTP service or network connection must be restored; changing your password will not fix this connection failure.') from ex
    try:
        ftp.auth()
    except Exception as ex:
        ftp.close()
        raise RuntimeError('The server answered, but the encrypted connection failed. No login details were sent. '+str(ex)) from ex
    ftp.timeout=30
    ftp.sock.settimeout(30)
    print('Secure connection established.',flush=True)
    return ftp

def login_connection(ftp,user,password,root):
    step='signing in'
    try:
        print('Signing in...',flush=True)
        ftp.login(user,password);ftp.prot_p()
        step='opening the Lemon Studio folder'
        ftp.cwd(root)
        return ftp
    except ftplib.error_perm as ex:
        ftp.close()
        if str(ex).startswith('530'):
            raise RuntimeError('The upload server rejected the login. The password has not been saved or changed.') from ex
        raise RuntimeError('Upload server refused access while '+step+': '+str(ex)) from ex
    except OSError as ex:
        ftp.close()
        raise RuntimeError('Connection lost while '+step+'. No successful connection has been saved.') from ex

def configure():
    import keyring
    cfg=read_config()
    if cfg.get('publish_via_ssh'):
        check_connection()
        print('Connected through the Mac. No Windows hosting password is needed.')
        return
    if cfg.get('ftp_credentials_file'):
        check_connection()
        print('Connected using this Mac\'s existing private hosting configuration.')
        return
    user=cfg.get('ftp_user','michaellane-music')
    print('Connect Publishing - Michael Lane Music')
    print('Hosting username: '+user+' (already filled in)')
    ftp=open_secure_connection('web12.webprojaggt.de')
    try: password=getpass.getpass('Enter ONLY your KeyHelp password, then press Enter (typing is hidden): ')
    except BaseException:
        ftp.close();raise
    candidate=dict(cfg,ftp_host='web12.webprojaggt.de',ftp_user=user,ftp_root='/www/michaellanemusic.de/lemon-studio')
    ftp=login_connection(ftp,user,password,candidate['ftp_root'])
    # Read-only proof: the configured root must be this product's existing website.
    print('Checking access to the download page...',flush=True)
    data=bytearray()
    try:ftp.retrbinary('RETR index.html',data.extend)
    except OSError as ex:
        raise RuntimeError('Login succeeded, but the separate FTP file-transfer connection timed out. The server\'s passive FTP data connection needs attention. Password was not saved.') from ex
    finally:ftp.close()
    if b'Lemon Studio' not in data: raise RuntimeError('Wrong publishing folder; no credentials saved.')
    keyring.set_password(SERVICE,user,password)
    PRIVATE.mkdir(exist_ok=True);CONFIG.write_text(json.dumps(candidate,indent=2))
    if os.name!='nt': CONFIG.chmod(0o600)
    print('Connected. Future publishing uses the saved credential without a browser login.')

def permitted(rel,scope):
    p=PurePosixPath(rel)
    if p.is_absolute() or '..' in p.parts or '\\' in rel: return False
    if scope=='windows-linux':
        return bool(re.fullmatch(r'(?:downloads|linux|updates)/\d+/[^/]+',rel) or rel in ('platforms/windows.json','platforms/linux.json','updates/stable.json'))
    if scope in ('mac-arm64','mac-x86_64'):
        return bool(re.fullmatch(r'mac/\d+/[^/]+',rel) or rel==f'platforms/{scope}.json')
    return False

def verify_remote(rel,p):
    with urllib.request.urlopen(BASE+urllib.parse.quote(rel)+'?verify='+uuid.uuid4().hex,timeout=120) as r:
        h=hashlib.sha256()
        while block:=r.read(1024*1024): h.update(block)
    if h.hexdigest()!=digest(p): raise RuntimeError('Public download checksum mismatch: '+rel)

def remote_call(cfg,action,*args):
    repo=cfg['mac_repo'];python=cfg['mac_python'];q=shlex.quote
    g='git -C '+q(repo)
    command=(f'set -e; if [ -n "$({g} status --porcelain)" ]; then echo "Mac checkout has local changes; reconcile first." >&2; exit 1; fi; '
             f'{g} fetch home main; {g} merge --ff-only home/main; '
             f'test "$({g} rev-parse HEAD)" = {q(git("rev-parse","HEAD"))} || {{ echo "Mac and PC source revisions differ." >&2; exit 1; }}; '
             'exec '+shlex.join([python,repo+'/packaging/workflow/release.py',action,*map(str,args)]))
    run('ssh','-T',cfg['publish_via_ssh'],command)

def check_connection(upload=False):
    cfg=read_config()
    if cfg.get('publish_via_ssh'):
        remote_call(cfg,'verify-upload' if upload else 'check-connection');return
    ftp=connect(cfg)
    try:
        data=bytearray();ftp.retrbinary('RETR index.html',data.extend)
        if b'Lemon Studio' not in data:raise RuntimeError('Wrong publishing directory')
        if upload:
            # Only this unique disposable probe is created/replaced/deleted; no release files.
            name='connection-check-'+uuid.uuid4().hex+'.txt';temp=name+'.upload'
            try:
                for content in (b'Lemon Studio encrypted upload check\n',b'Lemon Studio encrypted replacement check\n'):
                    ftp.storbinary('STOR '+temp,io.BytesIO(content))
                    ftp.rename(temp,name)
                    with urllib.request.urlopen(BASE+name+'?v='+uuid.uuid4().hex,timeout=30) as response:
                        if response.read()!=content:raise RuntimeError('HTTPS upload verification failed')
                print('Upload, atomic replacement and public HTTPS download verified.',flush=True)
            finally:
                for path in (temp,name):
                    try:ftp.delete(path)
                    except ftplib.error_perm as ex:
                        if not str(ex).startswith('550'):raise
        print('Secure publishing connection verified.',flush=True)
    finally:ftp.quit()

def relay_publish(cfg,plan_path,plan):
    destination=shared_folder()/'relay'/uuid.uuid4().hex
    destination.mkdir(parents=True)
    for e in plan['files']:
        target=destination/e['local'];target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2(plan_path.parent/e['local'],target)
        if digest(target)!=e['sha256']:raise RuntimeError('Relay copy checksum mismatch')
    shutil.copy2(plan_path,destination/'publish-plan.json')
    remote_path=PurePosixPath(cfg['mac_share'])/'relay'/destination.name/'publish-plan.json'
    remote_call(cfg,'publish',remote_path)
    receipt=destination/'PUBLISHED.json'
    if not receipt.exists():raise RuntimeError('Mac did not produce a publication receipt')
    shutil.copy2(receipt,plan_path.parent/'PUBLISHED.json')
    print('Published through the Mac; receipt copied back to this PC.')

def publish(plan_path):
    plan_path=Path(plan_path).resolve(); plan=json.loads(plan_path.read_text());scope=plan['scope']
    entries=plan['files']; paths=[e['path'] for e in entries]
    if len(paths)!=len(set(paths)) or not all(permitted(p,scope) for p in paths): raise RuntimeError('Publication plan exceeds platform scope')
    for e in entries:
        p=(plan_path.parent/e['local']).resolve()
        if plan_path.parent not in p.parents or digest(p)!=e['sha256']: raise RuntimeError('Staged artifact changed or is outside package')
    cfg=read_config()
    if cfg.get('publish_via_ssh'):
        relay_publish(cfg,plan_path,plan);return
    ftp=connect(cfg)
    # Per-platform lock excludes simultaneous publications of the same architecture.
    lock='.publish-'+scope+'.lock'
    ftp.mkd(lock)
    try:
        for e in sorted(entries,key=lambda x: x['path'].startswith('platforms/') or x['path']=='updates/stable.json'):
            rel=e['path'];p=plan_path.parent/e['local'];mutable=rel.startswith('platforms/') or rel=='updates/stable.json'
            if mutable and rel.startswith('platforms/'):
                old=bytearray()
                try: ftp.retrbinary('RETR '+rel,old.extend)
                except ftplib.error_perm as ex:
                    if not str(ex).startswith('550'): raise
                if old and json.loads(old).get('version',0)>plan['version']: raise RuntimeError('Refusing to replace a newer platform release')
            if not mutable:
                old=hashlib.sha256();exists=True
                try: ftp.retrbinary('RETR '+rel,old.update)
                except ftplib.error_perm as ex:
                    if not str(ex).startswith('550'): raise
                    exists=False
                if exists:
                    if old.hexdigest()!=e['sha256']: raise RuntimeError('Versioned artifact already exists with different contents: '+rel)
                    verify_remote(rel,p);continue
            parent=PurePosixPath(rel).parent
            current=''
            for part in parent.parts:
                current=(current+'/' if current else '')+part
                try: ftp.mkd(current)
                except ftplib.error_perm:
                    # Do not mask permission failures: confirm that directory exists.
                    saved=ftp.pwd();ftp.cwd(current);ftp.cwd(saved)
            temporary=rel+'.upload-'+uuid.uuid4().hex
            with p.open('rb') as f: ftp.storbinary('STOR '+temporary,f,1024*1024)
            h=hashlib.sha256();ftp.retrbinary('RETR '+temporary,h.update)
            if h.hexdigest()!=e['sha256']: raise RuntimeError('Upload checksum mismatch')
            ftp.rename(temporary,rel)
            verify_remote(rel,p)
        receipt=dict(scope=scope,version=plan['version'],commit=plan['commit'],published_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),files=entries)
        (plan_path.parent/'PUBLISHED.json').write_text(json.dumps(receipt,indent=2))
        destination=shared_folder()/'published'/str(plan['version'])/scope
        destination.mkdir(parents=True,exist_ok=True)
        shutil.copy2(plan_path.parent/'PUBLISHED.json',destination/'PUBLISHED.json')
        print('Published and verified:',BASE)
    finally:
        ftp.rmd(lock);ftp.quit()

def write_plan(site,out,scope):
    entries=[]
    for p in sorted(site.rglob('*')):
        if p.is_file():
            rel=p.relative_to(site).as_posix()
            if permitted(rel,scope): entries.append(dict(path=rel,local=p.relative_to(out).as_posix(),sha256=digest(p)))
    plan=out/'publish-plan.json'
    plan.write_text(json.dumps(dict(scope=scope,version=version(),commit=git('rev-parse','HEAD'),files=entries),indent=2))
    return plan

def build_windows(out,cfg):
    env=dict(os.environ,OPENSTUDIO_NO_PLUGIN_BUILD='1',LIBCLANG_PATH=cfg['llvm'])
    run('cargo','build','--locked','--release','-p','os-app',env=env)
    run(sys.executable,ROOT/'packaging/windows/prepare-update.py','--output',out)
    site=out/'lemon-studio';v=str(version());payload=out/'payload';payload.mkdir()
    for name in ('lemonstudio.exe','Lemon Studio Source.zip','LICENSE','THIRD-PARTY-NOTICES.txt'):shutil.copy2(site/'updates'/v/name,payload/name)
    shutil.copytree(cfg['runtime'],payload/'runtime')
    shutil.copy2(ROOT/'crates/os-app/assets/templates/Lemon Starter.flp',payload/'Lemon Starter.flp')
    (payload/'READ ME.txt').write_text('Lemon Studio - Michael Lane edition\nDownloads and matching editable source: '+BASE+'\n')
    label=f'{v[:4]}.{v[4:6]}.{v[6:8]}.{int(v[8:])}'
    installer=site/'downloads'/v/f'Lemon-Studio-Windows-Setup-{label}.exe';installer.parent.mkdir(parents=True)
    run(cfg['nsis'],f'/DPAYLOAD={payload}',f'/DOUTPUT={installer}',ROOT/'packaging/windows/LemonStudio.nsi')
    # The isolated cross-compiler was installed by the initial Linux build.
    drive='Q:'
    if Path(drive+'/').exists(): raise RuntimeError('Q: is occupied; do not remap another drive. Configure a free build drive.')
    run('subst',drive,ROOT.parent)
    try:
        tools=Path('Q:/linux-tools')
        cross=dict(env,CARGO_ZIGBUILD_ZIG_PATH=str(tools/'python/ziglang/zig.exe'),CARGO_ZIGBUILD_CACHE_DIR=str(tools/'zig-cache'),PKG_CONFIG=str(tools/'pkg-config.cmd'),PKG_CONFIG_ALLOW_CROSS='1',PKG_CONFIG_LIBDIR=str(tools/'sysroot/usr/lib/x86_64-linux-gnu/pkgconfig'))
        run(tools/'python/bin/cargo-zigbuild.exe','zigbuild','--locked','--release','-p','os-app','--target','x86_64-unknown-linux-gnu.2.36',env=cross)
    finally:run('subst',drive,'/D')
    linux=site/'linux'/v/f'Lemon-Studio-Linux-x86_64-{v}.tar.gz';linux.parent.mkdir(parents=True)
    with tarfile.open(linux,'w:gz') as t:
        for src,name,mode in [(ROOT/'target/x86_64-unknown-linux-gnu/release/lemonstudio','lemonstudio',0o755),(ROOT/'packaging/linux/README.txt','README.txt',0o644),(ROOT/'LICENSE','LICENSE',0o644),(ROOT/'THIRD-PARTY-NOTICES.txt','THIRD-PARTY-NOTICES.txt',0o644)]:
            info=t.gettarinfo(str(src),'Lemon Studio/'+name);info.mode=mode;info.uid=info.gid=0;info.uname=info.gname=''
            with src.open('rb') as f:t.addfile(info,f)
    (site/'platforms').mkdir(exist_ok=True)
    for scope,label,p in [('windows','Windows',installer),('linux','Linux',linux)]:
        (site/'platforms'/f'{scope}.json').write_text(json.dumps(dict(version=version(),label=label,url=p.relative_to(site).as_posix(),source=f'updates/{v}/Lemon Studio Source.zip',sha256=digest(p))))
    return write_plan(site,out,'windows-linux')

def build(publish_now):
    cfg=read_config();shared_folder();sync()
    if publish_now:
        check_connection() # Fail before a long build if publishing is not configured.
    if platform.system()=='Windows':
        for key in ('llvm','nsis','runtime'):
            if key not in cfg or not Path(cfg[key]).exists():raise RuntimeError('Missing Windows build configuration: '+key)
        with urllib.request.urlopen(BASE+'platforms/windows.json',timeout=30) as r: live=json.load(r)['version']
        if version()<=live:
            new=max(live+1,int(datetime.datetime.now().strftime('%Y%m%d')+'01'))
            p=ROOT/'crates/os-app/src/updates.rs';p.write_text(re.sub(r'pub const VERSION: u64 = \d+;',f'pub const VERSION: u64 = {new};',p.read_text()))
            label=f'{str(new)[:4]}.{str(new)[4:6]}.{str(new)[6:8]}.{int(str(new)[8:])}'
            p=ROOT/'packaging/windows/LemonStudio.nsi';p.write_text(re.sub(r'("DisplayVersion" ")[^"]+',lambda m:m[1]+label,p.read_text()))
            run('git','add','crates/os-app/src/updates.rs','packaging/windows/LemonStudio.nsi');run('git','commit','-m',f'Prepare release {new}');run('git','push','home','HEAD:main')
        out=Path.home()/'Desktop/Lemon Studio/Lemon Studio Public Download'/str(version())
        if out.exists():raise RuntimeError('Local release folder already exists. Preserve it; resume its publish plan or increment version.')
        out.mkdir(parents=True);plan=build_windows(out,cfg)
    elif platform.system()=='Darwin':
        run('bash',ROOT/'packaging/mac/build-release.command')
        out=Path.home()/'Desktop/Lemon Studio Mac Public Download'/str(version());arch=platform.machine()
        m=json.loads((out/f'mac-{arch}.json').read_text());site=out/'lemon-studio';(site/'platforms').mkdir(parents=True,exist_ok=True)
        for rel in (m['url'],m['source']):
            p=site/rel;p.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(out/Path(rel).name,p)
        shutil.copy2(out/f'mac-{arch}.json',site/'platforms'/f'mac-{arch}.json')
        plan=write_plan(site,out,f'mac-{arch}')
    else:raise RuntimeError('Use the Windows desktop or MacBook for this release workflow.')
    # Archive the concrete publication plan and artifacts on the home server too.
    dest=shared_folder()/'ready'/str(version())/json.loads(plan.read_text())['scope']
    if dest.exists(): raise RuntimeError('Shared ready package already exists; preserve it and reconcile before publishing.')
    shutil.copytree(out,dest)
    for e in json.loads(plan.read_text())['files']:
        if digest(dest/e['local'])!=e['sha256']:raise RuntimeError('Shared artifact checksum mismatch')
    print('Built and copied to home server:',dest)
    if publish_now:publish(plan)
    else:print('Prepared only; not published. Run release.py publish "'+str(plan)+'"')

def main():
    ap=argparse.ArgumentParser();ap.add_argument('action',choices=['sync','build','build-publish','publish','configure','status','check-connection','verify-upload']);ap.add_argument('plan',nargs='?');a=ap.parse_args()
    if a.action=='sync':sync()
    elif a.action=='configure':configure()
    elif a.action in ('check-connection','verify-upload'):check_connection(a.action=='verify-upload')
    elif a.action=='publish':
        if not a.plan:ap.error('publish requires a publish-plan.json path')
        publish(a.plan)
    elif a.action in ('build','build-publish'):build(a.action=='build-publish')
    else:
        print('Local revision:',git('rev-parse','--short','HEAD'));print('Shared repository:',shared_folder()/'Lemon Studio.git')
        cfg=read_config()
        print('Publishing route:', 'Mac SSH relay' if cfg.get('publish_via_ssh') else 'Direct FTPS')
        print('Publishing configured:',bool(cfg.get('publish_via_ssh') or cfg.get('ftp_credentials_file') or cfg.get('ftp_user')))

if __name__=='__main__':
    try:main()
    except Exception as ex:print('STOP:',ex,file=sys.stderr);sys.exit(1)

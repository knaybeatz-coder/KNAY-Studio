"""One resumable, four-platform release. `check` checks setup without building or publishing; `run` publishes."""
from pathlib import Path, PurePosixPath
from concurrent.futures import ThreadPoolExecutor
import argparse, datetime, hashlib, json, mmap, os, platform, re, shutil, subprocess, sys, time, urllib.request, urllib.error, urllib.parse, zipfile
import release as r
import builders as b
from checkpoints import Journal, atomic_json, digest, exclusive, plan_files, stage_plan

CONFIG=r.PRIVATE/'pipeline.json'
PLATFORMS=('windows','linux','mac-arm64','mac-x86_64')
REPOSITORY='Dorfhaus/Lemon-Studio-Michael-Lane-Edition'


def config():
    if not CONFIG.exists():raise RuntimeError('Publishing pipeline is not configured on this computer: '+str(CONFIG))
    return json.loads(CONFIG.read_text(encoding='utf-8-sig'))


def read_url(url):
    request=urllib.request.Request(url,headers={'User-Agent':'Lemon-Studio-release','Cache-Control':'no-cache'})
    with urllib.request.urlopen(request,timeout=120) as response:return response.read()


def live_versions():
    return {p:json.loads(read_url(r.BASE+'platforms/'+p+'.json?check='+str(time.time_ns())))['version'] for p in PLATFORMS}


def engine_id():return r.git('rev-parse','HEAD:plugins/lemon-master-assistant')


def probe(mac=False):
    c=config();required=PLATFORMS[2:] if mac else PLATFORMS[:2]
    if not Path(c['juce_source'],'modules/juce_core/juce_core.h').is_file():raise RuntimeError('Matching JUCE source is missing')
    for p in required:
        e=c['engines'][p];archive=Path(e['bundle'])
        if e['source']!=engine_id():raise RuntimeError('Master Assistant source changed. Rebuild and validate the '+p+' engine before publishing; cached engines were not reused.')
        if not archive.is_file() or digest(archive)!=e['sha256']:raise RuntimeError('Engine archive changed: '+p)
        with zipfile.ZipFile(archive) as z:
            m=json.loads(z.read('engine.json'));expected=('macos','aarch64' if p=='mac-arm64' else 'x86_64') if mac else (p,'x86_64')
            if (m['os'],m['arch'])!=expected:raise RuntimeError('Wrong engine architecture: '+p)
            for f in m['files']:
                if hashlib.sha256(z.read(f['path'])).hexdigest()!=f['sha256']:raise RuntimeError('Damaged native engine archive')
    from cryptography.hazmat.primitives.serialization import load_pem_private_key,Encoding,PublicFormat
    key=r.PRIVATE/('mac-update-signing-key.pem' if mac else 'update-signing-key.pem')
    expected=r.ROOT/'crates/os-app/src'/('update-mac-public-key.hex' if mac else 'update-public-key.hex')
    public=load_pem_private_key(key.read_bytes(),None).public_key().public_bytes(Encoding.Raw,PublicFormat.Raw).hex()
    if public!=expected.read_text().strip():raise RuntimeError('Signing key does not match application')
    subprocess.run(['cargo','--version'],check=True,stdout=subprocess.DEVNULL)
    if mac:
        subprocess.run(['xcode-select','-p'],check=True,stdout=subprocess.DEVNULL)
        targets=subprocess.check_output(['rustup','target','list','--installed'],text=True)
        if not all(t in targets for t in ['aarch64-apple-darwin','x86_64-apple-darwin']):raise RuntimeError('Both Mac Rust targets must be installed')
        r.check_connection()
    else:
        cfg=r.read_config()
        for k in ('llvm','nsis','runtime'):
            if not Path(cfg[k]).exists():raise RuntimeError('Missing build tool: '+k)
        if not (Path(c['linux_tools'])/'python/bin/cargo-zigbuild.exe').is_file():raise RuntimeError('Linux cross-compiler missing')
        if Path('Q:/').exists():raise RuntimeError('Q: is occupied; the Linux worker will not replace it')
        public=Path(c['public_checkout'])
        remote=subprocess.check_output(['git','remote','get-url','github'],cwd=public,text=True).strip()
        if remote!='git@github.com:'+REPOSITORY+'.git':raise RuntimeError('Wrong GitHub publication repository')
        subprocess.run(['git','ls-remote','github','refs/heads/michael-lane'],cwd=public,check=True,stdout=subprocess.DEVNULL)
    return {'platforms':list(required),'engine_source':engine_id(),'signing_key_matches':True}


def remote(args,log,sync=False):
    import shlex
    cfg=r.read_config();repo=cfg['mac_repo'];q=shlex.quote
    prefix='export PATH="$HOME/.cargo/bin:$HOME/.lemon-studio-release/tools/bin:$PATH"; set -e; '
    if sync:
        prefix+=f'test -z "$(git -C {q(repo)} status --porcelain)"; git -C {q(repo)} fetch home main; git -C {q(repo)} merge --ff-only home/main; '
    prefix+=f'test -z "$(git -C {q(repo)} status --porcelain)"; test "$(git -C {q(repo)} rev-parse HEAD)" = {q(r.git("rev-parse","HEAD"))}; '
    command=prefix+'exec '+shlex.join([cfg['mac_python'],repo+'/packaging/workflow/pipeline.py',*map(str,args)])
    for attempt in range(3 if args[0]=='publish-worker' else 1):
        try:
            b.command(log,'ssh','-T',cfg['publish_via_ssh'],command);return
        except subprocess.CalledProcessError:
            if attempt==2 or args[0]!='publish-worker':raise
            time.sleep(5*(attempt+1))


def release_notes():
    history=json.loads((r.ROOT/'packaging/changelog.json').read_text(encoding='utf-8'))
    sections=[s for s in history['sections'] if str(r.version()) in s['title']]
    if not sections or not sections[0]['changes']:raise RuntimeError('Add accurate changelog notes for this build before publishing')
    return sections[0]


def prepare(notes):
    live=live_versions();v=r.version()
    if notes:
        n=json.loads(Path(notes).read_text(encoding='utf-8-sig'))
        if not isinstance(n.get('title'),str) or not n['title'].strip() or not n.get('changes') or not all(isinstance(x,str) and x.strip() for x in n['changes']):raise RuntimeError('Notes need a title and a list of accurate user-facing changes')
        v=max(v,max(live.values())+1,int(datetime.date.today().strftime('%Y%m%d')+'01'))
        history_path=r.ROOT/'packaging/changelog.json';history=json.loads(history_path.read_text(encoding='utf-8'))
        if any(str(v) in section['title'] for section in history['sections']):raise RuntimeError('This build already has release notes; run without --notes')
        p=r.ROOT/'crates/os-app/src/updates.rs';p.write_text(re.sub(r'pub const VERSION: u64 = \d+;',f'pub const VERSION: u64 = {v};',p.read_text(encoding='utf-8')),encoding='utf-8')
        label=f'{str(v)[:4]}.{str(v)[4:6]}.{str(v)[6:8]}.{int(str(v)[8:])}'
        p=r.ROOT/'packaging/windows/LemonStudio.nsi';p.write_text(re.sub(r'("DisplayVersion" ")[^"]+',lambda m:m[1]+label,p.read_text(encoding='utf-8')),encoding='utf-8')
        p=r.ROOT/'packaging/changelog.json';history=json.loads(p.read_text(encoding='utf-8'))
        if any(str(v) in s['title'] for s in history['sections']):raise RuntimeError('This build already has release notes; preserve them and run without --notes')
        history['sections'].insert(0,dict(date=datetime.date.today().isoformat(),title=n['title']+' · public build '+str(v),changes=n['changes']))
        atomic_json(p,history)
        r.run(sys.executable,r.ROOT/'packaging/render-changelog.py')
        r.run('git','add','crates/os-app/src/updates.rs','packaging/windows/LemonStudio.nsi','packaging/changelog.json','packaging/download-page.html')
        r.run('git','commit','-m',f'Prepare complete release {v}')
    elif v<=max(live.values()):raise RuntimeError('This build is already published. Supply --notes with the new changes to prepare a new release.')
    release_notes()
    r.run('git','push','--receive-pack=git -c receive.autogc=false receive-pack','home','HEAD:main')


def identity():
    c=config()
    return {'version':r.version(),'commit':r.git('rev-parse','HEAD'),'engine_source':engine_id(),'configuration':hashlib.sha256(json.dumps(c,sort_keys=True).encode()).hexdigest()}


def output_root():return Path.home()/'Desktop/Lemon Studio/Lemon Studio Public Download'/str(r.version())/'Pipeline'


def privacy(plan):
    pack=Path(os.environ.get('LOCALAPPDATA',''))/'Lemon Studio/Lemon Wave/Included Library.lwpack'
    if pack.is_file():
        data=pack.read_bytes()
        for p in plan_files(plan)[1:]:
            if p.name in ('lemonstudio','lemonstudio.exe'):
                with p.open('rb') as f,mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as m:
                    if m.find(data)>=0:raise RuntimeError('Personal Wave library found in public binary; publication stopped')


def shared_run():return r.shared_folder()/'pipeline'/str(r.version())/r.git('rev-parse','HEAD')[:12]


def stage(path,name):
    privacy(path);dest=stage_plan(path,shared_run()/name)
    return str(dest),plan_files(dest)


def worker(arch):
    if platform.system()!='Darwin':raise RuntimeError('Mac worker requires macOS')
    root=output_root()/('mac-'+arch);root.mkdir(parents=True,exist_ok=True)
    with exclusive(r.PRIVATE/('pipeline-mac-'+arch+'.lock')):
        j=Journal(root/'state.json',identity())
        def build(attempt):
            plan=b.mac(config(),arch,root/f'build-{attempt}',root/'build.log');return str(plan),plan_files(plan)
        plan=j.step('Build mac-'+arch,build)
        if r.git('status','--porcelain') or r.git('rev-parse','HEAD')!=j.data['identity']['commit']:raise RuntimeError('Mac source changed during build; publication stopped')
        j.step('Stage mac-'+arch,lambda _:stage(plan,'mac-'+arch))


def publish_shared(name):
    path=shared_run()/name/'publish-plan.json';r.publish(path)


def artifacts(plans):
    index={}
    for path in plans:
        p=json.loads(Path(path).read_text())
        for e in p['files']:index[e['path']]=(Path(path).parent/e['local'],e['sha256'])
    assets=[]
    for name in PLATFORMS:
        manifest=json.loads(index['platforms/'+name+'.json'][0].read_text())
        for rel in (manifest['url'],manifest['source']):
            f,h=index[rel]
            if any(a['path']==rel for a in assets):continue
            dest=f.name if f.name!='Lemon Studio Source.zip' else 'Lemon-Studio-Source-'+str(r.version())+'.zip'
            assets.append(dict(name=dest,url=r.BASE+urllib.parse.quote(rel),sha256=h,path=rel,local=str(f)))
    return index,assets


def verify_live(plans,out):
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
    index,assets=artifacts(plans)
    for rel in ['platforms/'+p+'.json' for p in PLATFORMS]+['updates/stable.json','updates/linux-x64/stable.json','mac/updates/arm64/stable.json','mac/updates/x86_64/stable.json']:
        data=read_url(r.BASE+rel+'?verify='+str(time.time_ns()))
        if hashlib.sha256(data).hexdigest()!=index[rel][1]:raise RuntimeError('Live feed or manifest differs: '+rel)
        value=json.loads(data)
        if 'payload' in value:
            key=r.ROOT/'crates/os-app/src'/('update-mac-public-key.hex' if rel.startswith('mac/') else 'update-public-key.hex')
            Ed25519PublicKey.from_public_bytes(bytes.fromhex(key.read_text().strip())).verify(bytes.fromhex(value['signature']),value['payload'].encode())
            value=json.loads(value['payload'])
        if value['version']!=r.version():raise RuntimeError('Wrong live release version')
    if read_url(r.BASE+'?verify='+str(time.time_ns()))!=(r.ROOT/'packaging/download-page.html').read_bytes():raise RuntimeError('Live changelog page differs')
    # Artifact bytes were verified over HTTPS by each publisher before its manifest became live.
    for path in plans:
        p=json.loads(Path(path).read_text());receipt=json.loads(Path(path).with_name('PUBLISHED.json').read_text())
        if any(receipt[k]!=p[k] for k in ('version','commit','scope','files')):raise RuntimeError('Publication receipt does not match prepared artifacts')
    for a in assets:
        dest=out/'Downloads'/a['name'];dest.parent.mkdir(exist_ok=True)
        if not dest.exists():shutil.copyfile(a['local'],dest)
        if digest(dest)!=a['sha256']:raise RuntimeError('Desktop artifact hash differs')
    notes=release_notes();v=str(r.version());label=f'{v[:4]}.{v[4:6]}.{v[6:8]}.{int(v[8:])}'
    plan=dict(tag='v'+label,title='Lemon Studio Michael Lane Edition '+label,notes='\n'.join('- '+x for x in notes['changes'])+'\n\n'+r.BASE,assets=[{k:a[k] for k in ('name','url','sha256')} for a in assets])
    atomic_json(out/'github-release.json',plan);atomic_json(out/'VERIFIED.json',{'version':r.version(),'platforms':list(PLATFORMS),'signatures':4,'download_hashes_verified_by_publishers':True,'website_exact':True})
    return str(out/'github-release.json'),[out/'github-release.json',out/'VERIFIED.json']+[out/'Downloads'/a['name'] for a in assets]


def github_stage(plan_path,plans,out,attempt,force=False):
    plan=json.loads(Path(plan_path).read_text());c=config();public=Path(c['public_checkout'])
    b.command(out/'github.log','git','fetch','github','michael-lane',cwd=public)
    previous=subprocess.check_output(['git','show','FETCH_HEAD:packaging/github-release.json'],cwd=public,text=True)
    previous=json.loads(previous);previous.pop('publication_attempt',None)
    head=subprocess.check_output(['git','rev-parse','FETCH_HEAD'],cwd=public,text=True).strip()
    if previous==plan and not force:return {'tag':plan['tag'],'commit':head},[Path(plan_path)]
    if force and previous!=plan:raise RuntimeError('Another release changed the GitHub branch; refusing to replace it')
    work=Path(c['cache'])/('github-'+str(r.version())+'-'+str(attempt))
    if work.exists():raise RuntimeError('Preserving existing GitHub worktree; retry creates a fresh attempt')
    b.command(out/'github.log','git','worktree','add','--detach',work,'FETCH_HEAD',cwd=public)
    index,_=artifacts(plans);source=index['updates/'+str(r.version())+'/Lemon Studio Source.zip'][0]
    expected=set()
    with zipfile.ZipFile(source) as z:
        for entry in z.infolist():
            if entry.is_dir():continue
            if not entry.filename.startswith('Source/'):raise RuntimeError('Unexpected source archive layout')
            name=entry.filename[7:];p=(work/name).resolve()
            if not p.is_relative_to(work.resolve()) or '.git' in Path(name).parts:raise RuntimeError('Unsafe source archive path')
            expected.add(name);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(z.read(entry))
    tracked=subprocess.check_output(['git','ls-files','-z'],cwd=work).decode().split('\0')
    for name in tracked:
        if name and name not in expected:(work/name).unlink(missing_ok=True)
    atomic_json(work/'packaging/github-release.json',dict(plan,publication_attempt=attempt) if force else plan)
    b.command(out/'github.log','git','add','-A',cwd=work)
    b.command(out/'github.log',sys.executable,'packaging/private_build.py','--check-source',cwd=work)
    b.command(out/'github.log','git','commit','-m','Publish complete DAW '+plan['tag'],cwd=work)
    b.command(out/'github.log','git','push','github','HEAD:michael-lane',cwd=work)
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=work,text=True).strip()
    return {'tag':plan['tag'],'commit':head},[Path(plan_path)]


class GitHubJobFailed(RuntimeError):pass

def github_verify(plan_path,out,commit=None):
    plan=json.loads(Path(plan_path).read_text());url='https://api.github.com/repos/'+REPOSITORY+'/releases/tags/'+plan['tag']
    for attempt in range(30):
        try:
            release=json.loads(read_url(url))
            if not release['draft']:break
        except urllib.error.HTTPError as e:
            if e.code not in (404,429,502,503):raise
        if commit and attempt%4==0:
            runs=json.loads(read_url('https://api.github.com/repos/'+REPOSITORY+'/actions/workflows/publish-edition.yml/runs?branch=michael-lane&per_page=10'))
            matching=[x for x in runs.get('workflow_runs',[]) if x['head_sha']==commit]
            if matching and matching[0]['conclusion'] in ('failure','timed_out'):
                raise GitHubJobFailed('GitHub publication workflow failed; prepared downloads are preserved')
            if matching and matching[0]['conclusion']=='cancelled':raise RuntimeError('GitHub publication was cancelled; inspect it before resuming')
        time.sleep(min(10+attempt*5,45))
    else:raise RuntimeError('GitHub release is not complete. Check Actions, then resume this run.')
    expected={a['name']:'sha256:'+a['sha256'] for a in plan['assets']}
    expected['SHA256SUMS.txt']='sha256:'+hashlib.sha256(''.join(a['sha256']+'  '+a['name']+'\n' for a in plan['assets']).encode()).hexdigest()
    actual={a['name']:a.get('digest') for a in release['assets']}
    if expected!=actual:raise RuntimeError('GitHub assets differ from verified website downloads')
    dest=out/'GITHUB-VERIFIED.json';atomic_json(dest,dict(url=release['html_url'],assets=actual,tag=plan['tag']))
    return release['html_url'],[dest]


def coordinator(notes=None,check=False):
    if platform.system()!='Windows':raise RuntimeError('Run the complete release from the Windows coordinator; this Mac provides native workers.')
    with exclusive(r.PRIVATE/'pipeline.lock'):
        if r.git('status','--porcelain'):raise RuntimeError('Commit intended source changes first; nothing was stashed or reset')
        r.run('git','fetch','home','main');r.run('git','merge','--ff-only','home/main')
        r.run('git','push','--receive-pack=git -c receive.autogc=false receive-pack','home','HEAD:main')
        c=config();prelog=r.PRIVATE/'pipeline-preflight.log'
        probe();remote(['probe'],prelog,sync=True)
        if check:
            print('Preflight passed: four architectures, engine fingerprints, build tools, signing keys, GitHub and hosting access. Nothing published.');return
        out=output_root();state=out/'state.json'
        previous=json.loads(state.read_text()) if state.exists() else None
        if previous and previous['identity']['commit']==r.git('rev-parse','HEAD'):
            if notes:raise RuntimeError('Resume this reserved run without --notes')
        else:
            if previous and previous['steps'].get('Back up source',{}).get('status')!='complete':raise RuntimeError('An unfinished release belongs to another revision; resume that revision first')
            prepare(notes);out=output_root();state=out/'state.json'
        release_notes();remote(['probe'],prelog,sync=True)
        out.mkdir(parents=True,exist_ok=True);j=Journal(state,identity())
        def win(attempt):
            p=b.windows(c,out/f'windows-{attempt}',out/'windows.log');return str(p),plan_files(p)
        def lin(_):
            p=b.linux_compile(c,out/'linux.log');return str(p),[p]
        def mac(arch):
            def action(_):
                remote(['worker',arch],out/('mac-'+arch+'.log'));p=shared_run()/('mac-'+arch)/'publish-plan.json';return str(p),plan_files(p)
            return j.step('Build mac-'+arch,action)
        with ThreadPoolExecutor(max_workers=4) as pool:
            winjob=pool.submit(j.step,'Build Windows',win);linjob=pool.submit(j.step,'Build Linux',lin)
            macjobs=[pool.submit(mac,a) for a in ('arm64','x86_64')]
            wp=winjob.result();binary=linjob.result();macplans=[job.result() for job in macjobs]
        def package(attempt):
            p=b.linux_package(c,binary,wp,out/f'linux-{attempt}',out/'linux.log');return str(p),plan_files(p)
        lp=j.step('Package Linux',package)
        def merged(_):
            p=b.merged_plan([wp,lp],out);return str(p),plan_files(p)
        combined=j.step('Combine Windows and Linux',merged)
        win_shared=j.step('Stage Windows and Linux',lambda _:stage(combined,'windows-linux'))
        def website(_):
            folder=out/'website';site=folder/'lemon-studio';site.mkdir(parents=True,exist_ok=True);shutil.copy2(r.ROOT/'packaging/download-page.html',site/'index.html')
            p=r.write_plan(site,folder,'website');return stage(p,'website')
        webplan=j.step('Stage website',website)
        plans=[win_shared,*macplans]
        if r.git('status','--porcelain') or r.git('rev-parse','HEAD')!=j.data['identity']['commit']:raise RuntimeError('Source changed during the release; no packages will be published')
        def publish(p):
            name=json.loads(Path(p).read_text())['scope']
            def action(_):
                remote(['publish-worker',name],out/('publish-'+name+'.log'));receipt=Path(p).with_name('PUBLISHED.json');return str(receipt),[receipt]
            return j.step('Publish '+name,action)
        with ThreadPoolExecutor(max_workers=3) as pool:list(pool.map(publish,plans))
        publish(webplan)
        gp=j.step('Verify live downloads and feeds',lambda _:verify_live(plans,out))
        publication=j.step('Publish GitHub',lambda attempt:github_stage(gp,plans,out,attempt))
        def finish_github(attempt):
            commit=publication['commit']
            for retry in range(3):
                try:return github_verify(gp,out,commit)
                except GitHubJobFailed:
                    if retry==2:raise
                    result,_=github_stage(gp,plans,out,1000+attempt*10+retry,force=True)
                    commit=result['commit']
        j.step('Verify GitHub',finish_github)
        def backup(_):
            env=dict(os.environ,LEMON_JUCE_SOURCE=c['juce_source'],PYTHONUTF8='1')
            b.command(out/'backup.log','powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',c['source_sync'],'-Source',r.ROOT,env=env)
            latest=json.loads((r.shared_folder()/'LATEST.json').read_text(encoding='utf-8-sig'));p=r.shared_folder()/latest['source']
            if digest(p)!=latest['sha256']:raise RuntimeError('Source backup hash differs')
            latest.update(mac_status='Both Mac architectures published and verified',commit=j.data['identity']['commit'],github=json.loads((out/'GITHUB-VERIFIED.json').read_text())['url'])
            atomic_json(r.shared_folder()/'LATEST.json',latest)
            receipt=out/'BACKUP-VERIFIED.json';atomic_json(receipt,latest);return str(p),[p,receipt]
        j.step('Back up source',backup)
        print('COMPLETE: website, Windows, Linux, both Macs, GitHub, signed feeds and backup verified. '+str(out),flush=True)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('action',choices=['run','check','status','probe','worker','publish-worker']);parser.add_argument('worker',nargs='?');parser.add_argument('--notes',type=Path);a=parser.parse_args()
    if a.action=='probe':print(json.dumps(probe(mac=platform.system()=='Darwin')))
    elif a.action=='worker':
        if a.worker not in ('arm64','x86_64'):parser.error('Choose arm64 or x86_64')
        worker(a.worker)
    elif a.action=='publish-worker':
        if a.worker not in ('windows-linux','mac-arm64','mac-x86_64','website'):parser.error('Invalid scope')
        publish_shared(a.worker)
    elif a.action=='status':
        p=output_root()/'Progress.txt';print(p.read_text(encoding='utf-8') if p.exists() else 'No pipeline run for the current source version.')
    else:coordinator(a.notes,a.action=='check')

if __name__=='__main__':
    try:main()
    except Exception as error:print('STOP: '+str(error),file=sys.stderr);sys.exit(1)

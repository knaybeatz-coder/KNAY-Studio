"""Isolated public build workers. Credentials never enter these environments."""
from pathlib import Path
import json, os, shutil, subprocess, tarfile
import release as r
from checkpoints import atomic_json, digest


def environment(config,platform):
    env=dict(os.environ,OPENSTUDIO_NO_PLUGIN_BUILD='1',LEMON_RELEASE='1',LEMON_WAVE_LIBRARY='',CARGO_BUILD_JOBS=str(config.get('jobs_per_build',4)),PYTHONUTF8='1')
    for key in ('CARGO_BUILD_TARGET','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_BUILD_RUSTFLAGS'):env.pop(key,None)
    env['CARGO_TARGET_DIR']=str(Path(config['cache'])/platform)
    env['LEMON_MASTER_BUNDLE']=config['engines'][platform]['bundle']
    env['LEMON_JUCE_SOURCE']=config['juce_source']
    if platform.startswith('mac-'):
        env.update(LEMON_MAC_ARCH=platform[4:],LEMON_PIPELINE='1')
        env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+str(Path.home()/'.lemon-studio-release/tools/bin')+os.pathsep+env.get('PATH','')
    else:env['LIBCLANG_PATH']=r.read_config()['llvm']
    return env


def command(log,*args,env=None,cwd=r.ROOT):
    Path(log).parent.mkdir(parents=True,exist_ok=True)
    with Path(log).open('a',encoding='utf-8') as f:
        f.write('\n> '+str(args[0])+' '+str(args[1] if len(args)>1 else '')+'\n');f.flush()
        subprocess.run(list(map(str,args)),cwd=cwd,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)


def windows(config,out,log):
    out=Path(out);out.mkdir(parents=True);env=environment(config,'windows');cfg=r.read_config()
    command(log,'cargo','build','--locked','--release','-p','os-app',env=env)
    binary=Path(env['CARGO_TARGET_DIR'])/'release/lemonstudio.exe'
    command(log,r.sys.executable,r.ROOT/'packaging/windows/prepare-update.py','--output',out,'--binary',binary,env=env)
    site=out/'lemon-studio';v=str(r.version());payload=out/'payload';payload.mkdir()
    for name in ('lemonstudio.exe','Lemon Studio Source.zip','LICENSE','THIRD-PARTY-NOTICES.txt'):shutil.copy2(site/'updates'/v/name,payload/name)
    shutil.copytree(cfg['runtime'],payload/'runtime');shutil.copy2(r.ROOT/'crates/os-app/assets/templates/Lemon Starter.flp',payload/'Lemon Starter.lmn')
    (payload/'READ ME.txt').write_text('Lemon Studio — Michael Lane Edition\n'+r.BASE,encoding='utf-8')
    label=f'{v[:4]}.{v[4:6]}.{v[6:8]}.{int(v[8:])}'
    installer=site/'downloads'/v/f'Lemon-Studio-Windows-Setup-{label}.exe';installer.parent.mkdir(parents=True)
    command(log,cfg['nsis'],f'/DPAYLOAD={payload}',f'/DOUTPUT={installer}',r.ROOT/'packaging/windows/LemonStudio.nsi',env=env)
    (site/'platforms').mkdir(exist_ok=True)
    atomic_json(site/'platforms/windows.json',dict(version=r.version(),label='Windows',url=installer.relative_to(site).as_posix(),source=f'updates/{v}/Lemon Studio Source.zip',sha256=digest(installer)))
    return r.write_plan(site,out,'windows-linux')


def linux_compile(config,log):
    env=environment(config,'linux');tools=Path(config['linux_tools'])
    # Existing cross-tool wrappers require Q:. Hold the coordinator lock, never replace another mapping.
    if Path('Q:/').exists():raise RuntimeError('Q: is occupied; preserve the existing drive mapping.')
    command(log,'subst','Q:',r.ROOT.parent)
    try:
        qtools=Path('Q:/')/tools.relative_to(r.ROOT.parent)
        env.update(CARGO_ZIGBUILD_ZIG_PATH=str(qtools/'python/ziglang/zig.exe'),CARGO_ZIGBUILD_CACHE_DIR=str(qtools/'zig-cache'),PKG_CONFIG=str(qtools/'pkg-config.cmd'),PKG_CONFIG_ALLOW_CROSS='1',PKG_CONFIG_LIBDIR=str(qtools/'sysroot/usr/lib/x86_64-linux-gnu/pkgconfig'))
        command(log,qtools/'python/bin/cargo-zigbuild.exe','zigbuild','--locked','--release','-p','os-app','--target','x86_64-unknown-linux-gnu.2.36',env=env)
    finally:command(log,'subst','Q:','/D')
    return Path(env['CARGO_TARGET_DIR'])/'x86_64-unknown-linux-gnu/release/lemonstudio'


def linux_package(config,binary,win_plan,out,log):
    out=Path(out);out.mkdir(parents=True);site=out/'lemon-studio';v=str(r.version());env=environment(config,'linux')
    win_site=Path(win_plan).parent/'lemon-studio';source=site/'updates'/v/'Lemon Studio Source.zip';source.parent.mkdir(parents=True);shutil.copy2(win_site/'updates'/v/source.name,source)
    archive=site/'linux'/v/f'Lemon-Studio-Linux-x86_64-{v}.tar.gz';archive.parent.mkdir(parents=True)
    with tarfile.open(archive,'w:gz') as t:
        for src,name,mode in [(Path(binary),'lemonstudio',0o755),(r.ROOT/'packaging/linux/README.txt','README.txt',0o644),(r.ROOT/'LICENSE','LICENSE',0o644),(r.ROOT/'THIRD-PARTY-NOTICES.txt','THIRD-PARTY-NOTICES.txt',0o644)]:
            info=t.gettarinfo(str(src),'Lemon Studio/'+name);info.mode=mode;info.uid=info.gid=0;info.uname=info.gname=''
            with src.open('rb') as f:t.addfile(info,f)
    (site/'platforms').mkdir()
    atomic_json(site/'platforms/linux.json',dict(version=r.version(),label='Linux',url=archive.relative_to(site).as_posix(),source=f'updates/{v}/Lemon Studio Source.zip',sha256=digest(archive)))
    command(log,r.sys.executable,r.ROOT/'packaging/prepare-unix-update.py','--platform','linux-x64','--site',site,'--app',binary,env=env)
    return r.write_plan(site,out,'windows-linux')


def mac(config,arch,out,log):
    env=environment(config,'mac-'+arch);out=Path(out);out.mkdir(parents=True);env['LEMON_PACKAGE_OUT']=str(out)
    command(log,'bash',r.ROOT/'packaging/mac/build-release.command',env=env)
    manifest=json.loads((out/f'mac-{arch}.json').read_text());site=out/'lemon-studio';(site/'platforms').mkdir(parents=True)
    for rel in (manifest['url'],manifest['source']):
        dest=site/rel;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(out/Path(rel).name,dest)
    shutil.copy2(out/f'mac-{arch}.json',site/'platforms'/f'mac-{arch}.json')
    command(log,r.sys.executable,r.ROOT/'packaging/prepare-unix-update.py','--platform','mac-'+arch,'--site',site,'--app',Path(env['CARGO_TARGET_DIR'])/'bundled/Lemon Studio.app',env=env)
    return r.write_plan(site,out,'mac-'+arch)


def merged_plan(plans,out):
    out=Path(out);entries={}
    for path in plans:
        path=Path(path);p=json.loads(path.read_text())
        for e in p['files']:
            if e['path'] in entries:
                if entries[e['path']]['sha256']!=e['sha256']:raise RuntimeError('Conflicting platform files')
                continue
            local=(path.parent/e['local']).resolve().relative_to(out.resolve()).as_posix()
            entries[e['path']]=dict(e,local=local)
    p=dict(scope='windows-linux',version=r.version(),commit=r.git('rev-parse','HEAD'),files=list(entries.values()))
    dest=out/'publish-plan.json';atomic_json(dest,p);return dest

"""Durable, revision-bound release checkpoints. No publishing side effects on import."""
from pathlib import Path
import contextlib, datetime, hashlib, json, os, threading, time


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def atomic_json(path, value):
    path=Path(path);path.parent.mkdir(parents=True,exist_ok=True)
    tmp=path.with_name(path.name+'.pending-'+str(os.getpid())+'-'+str(threading.get_ident()))
    with tmp.open('w',encoding='utf-8') as f:
        json.dump(value,f,indent=2);f.flush();os.fsync(f.fileno())
    os.replace(tmp,path)


@contextlib.contextmanager
def exclusive(path):
    """OS-owned lock is released after crashes; never delete a possibly live lock."""
    path=Path(path);path.parent.mkdir(parents=True,exist_ok=True)
    f=path.open('a+b');f.seek(0);f.write(b'0');f.flush();f.seek(0)
    try:
        if os.name=='nt':
            import msvcrt
            msvcrt.locking(f.fileno(),msvcrt.LK_NBLCK,1)
        else:
            import fcntl
            fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
    except (OSError,BlockingIOError):
        f.close();raise RuntimeError('Another publisher is running. Use status; do not start a second release.')
    try:yield
    finally:
        if os.name=='nt':
            f.seek(0);msvcrt.locking(f.fileno(),msvcrt.LK_UNLCK,1)
        else:fcntl.flock(f,fcntl.LOCK_UN)
        f.close()


def proof(paths):
    return {str(Path(p).resolve()):digest(p) for p in paths}


def verify_proof(files):
    for p,h in files.items():
        if not Path(p).is_file() or digest(p)!=h:
            raise RuntimeError('Saved release artifact changed or is missing: '+p+'. Preserve this run and investigate; do not rebuild published versioned files.')


class Journal:
    def __init__(self,path,identity):
        self.path=Path(path);self.lock=threading.RLock()
        self.data=json.loads(self.path.read_text(encoding='utf-8')) if self.path.exists() else {'identity':identity,'steps':{}}
        if self.data['identity']!=identity:raise RuntimeError('Release identity changed; resume its original revision or start a new version.')
        self.save()

    def save(self):
        atomic_json(self.path,self.data)
        lines=['Lemon Studio publishing',f"Build {self.data['identity'].get('version')} · source {self.data['identity'].get('commit','')[:10]}",'']
        for name,step in self.data['steps'].items():
            lines.append(f"{name}: {step['status']}"+(f" — {step['error']}" if step.get('error') else ''))
        self.path.with_name('Progress.txt').write_text('\n'.join(lines)+'\n',encoding='utf-8')

    def step(self,name,action):
        with self.lock:
            old=self.data['steps'].get(name,{})
            if old.get('status')=='complete':
                verify_proof(old['files']);print(name+': already verified',flush=True);return old['result']
            attempt=old.get('attempt',0)+1
            self.data['steps'][name]={'status':'running','attempt':attempt,'started':time.time()};self.save()
        print(name+': starting',flush=True)
        try:
            result,paths=action(attempt)
            files=proof(paths)
            with self.lock:
                self.data['steps'][name].update(status='complete',result=result,files=files,finished=time.time());self.save()
            print(name+': complete',flush=True);return result
        except BaseException as e:
            with self.lock:
                self.data['steps'][name].update(status='failed',error=str(e),finished=time.time());self.save()
            raise


def plan_files(path):
    path=Path(path);p=json.loads(path.read_text(encoding='utf-8'))
    files=[path]
    for e in p['files']:
        f=(path.parent/e['local']).resolve()
        if not f.is_relative_to(path.parent.resolve()) or not f.is_file() or digest(f)!=e['sha256']:
            raise RuntimeError('Invalid or changed publication artifact: '+str(f))
        files.append(f)
    return files


def stage_plan(path,destination):
    """Only publication files cross the network; duplicate content uses one object."""
    import shutil
    path=Path(path);destination=Path(destination)
    plan_files(path);p=json.loads(path.read_text(encoding='utf-8'));entries=[]
    existing=destination/'publish-plan.json'
    if existing.exists():
        previous=json.loads(existing.read_text(encoding='utf-8'))
        if any(previous[k]!=p[k] for k in ('version','commit','scope')):
            raise RuntimeError('Shared release destination belongs to another revision')
    destination.mkdir(parents=True,exist_ok=True)
    for e in p['files']:
        src=path.parent/e['local'];local='objects/'+e['sha256']+'/'+src.name;dst=destination/local
        if dst.exists():
            if digest(dst)!=e['sha256']:raise RuntimeError('Shared release object changed: '+str(dst))
        else:
            dst.parent.mkdir(parents=True,exist_ok=True);tmp=dst.with_name(dst.name+'.pending')
            shutil.copyfile(src,tmp)
            if digest(tmp)!=e['sha256']:raise RuntimeError('Shared copy verification failed')
            os.replace(tmp,dst)
        entries.append(dict(e,local=local))
    staged=dict(p,files=entries)
    if existing.exists() and json.loads(existing.read_text())!=staged:
        raise RuntimeError('Release plan changed after staging; keep the original artifacts')
    atomic_json(existing,staged)
    return existing

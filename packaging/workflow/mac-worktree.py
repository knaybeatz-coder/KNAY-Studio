"""Prepare one persistent native build worktree; serialize Git housekeeping only."""
from pathlib import Path
import subprocess,sys,os
import fcntl
root=Path(sys.argv[1]).resolve();arch=sys.argv[2]
if arch not in ('arm64','x86_64'):raise SystemExit('Unsupported architecture')
cache=Path(os.environ.get('LEMON_BUILD_SOURCE_CACHE',Path.home()/'.cache/lemon-studio-build-sources'));cache.mkdir(parents=True,exist_ok=True)
dest=cache/('public-'+arch)
def git(*args,cwd=root):return subprocess.check_output(['git',*map(str,args)],cwd=cwd,text=True).strip()
with (cache/'worktree.lock').open('a+b') as lock:
    # Both native workers may prepare their own worktree at the same time.
    # Serialize only Git setup; the two actual builds remain parallel.
    fcntl.flock(lock,fcntl.LOCK_EX)
    commit=git('rev-parse','HEAD')
    if not dest.exists():subprocess.run(['git','worktree','add','--detach',str(dest),commit],cwd=root,check=True,stdout=sys.stderr)
    else:
        if not (dest/'.git').is_file() or Path(git('rev-parse','--path-format=absolute','--git-common-dir',cwd=dest)).resolve()!=(root/'.git').resolve():raise SystemExit('Build cache belongs to another checkout; preserving it')
        if git('status','--porcelain',cwd=dest):raise SystemExit('Build worktree contains changes; preserving them')
        subprocess.run(['git','checkout','--detach',commit],cwd=dest,check=True,stdout=sys.stderr)
print(dest)

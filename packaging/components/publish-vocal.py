"""Publish immutable optional vocal-engine v1 files. Does not publish a DAW release.
Build the native library on each platform first. Credentials stay on their existing machines.
"""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,sys,zipfile,shlex
ROOT=Path(__file__).resolve().parents[2]
PRIVATE=Path.home()/'.lemon-studio-release'
BASE='https://michaellanemusic.de/lemon-studio/components/vocal-world-v1/'
def digest(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--windows',required=True);parser.add_argument('--linux',required=True);parser.add_argument('--mac',required=True);parser.add_argument('--output',required=True);parser.add_argument('--publish',action='store_true');a=parser.parse_args()
 out=Path(a.output);out.mkdir(parents=True,exist_ok=True)
 source=out/'LemonVocalEngine-Source-v1.zip'
 with zipfile.ZipFile(source,'w',zipfile.ZIP_DEFLATED)as z:
  for p in (ROOT/'tools/vocal-engine').rglob('*'):
   if p.is_file():z.write(p,p.relative_to(ROOT/'tools/vocal-engine').as_posix())
  z.write(ROOT/'LICENSE','LEMON-LICENSE.txt')
 from cryptography.hazmat.primitives.serialization import load_pem_private_key
 key=load_pem_private_key((PRIVATE/'update-signing-key.pem').read_bytes(),None)
 for platform,src,ext in [('windows-x86_64',a.windows,'.dll'),('linux-x86_64',a.linux,'.so'),('macos-aarch64',a.mac,'.dylib'),('macos-x86_64',a.mac,'.dylib')]:
  name='LemonVocalEngine-'+platform+ext;target=out/name;shutil.copy2(src,target)
  payload=json.dumps(dict(revision=1,platform=platform,file=name,sha256=digest(target),size=target.stat().st_size,source=source.name),separators=(',',':'))
  (out/(platform+'.json')).write_text(json.dumps(dict(payload=payload,signature=key.sign(payload.encode()).hex()),indent=2),encoding='utf-8')
 files=[p for p in out.iterdir()if p.is_file() and (p.suffix in ['.dll','.so','.dylib','.json','.zip'])]
 cfg=json.loads((PRIVATE/'workflow.json').read_text(encoding='utf-8-sig'))
 share=Path(cfg.get('share',r'\\192.168.178.132\Home-Shared\Storage\External-HD-Large-Files\Lemon Studio'))/'components-work/vocal-world-v1/publish'
 share.mkdir(parents=True,exist_ok=True)
 for p in files:
  shutil.copy2(p,share/p.name)
  if digest(share/p.name)!=digest(p):raise RuntimeError('Staged component hash mismatch')
 print('Prepared and verified',len(files),'component files.',flush=True)
 if not a.publish:return
 code=r"""from pathlib import Path
import sys,json,urllib.request,urllib.parse,hashlib,time,ftplib,io
repo=Path(REPO);sys.path.insert(0,str(repo/'packaging/workflow'));import release as r
share=r.shared_folder()/'components-work/vocal-world-v1/publish'
allowed={'LemonVocalEngine-Source-v1.zip','LemonVocalEngine-windows-x86_64.dll','LemonVocalEngine-linux-x86_64.so','LemonVocalEngine-macos-aarch64.dylib','LemonVocalEngine-macos-x86_64.dylib','windows-x86_64.json','linux-x86_64.json','macos-aarch64.json','macos-x86_64.json'}
files=[p for p in share.iterdir()if p.name in allowed]
if {p.name for p in files}!=allowed:raise RuntimeError('Incomplete vocal component package')
ftp=r.connect(r.read_config())
try:
 for part in ['components','components/vocal-world-v1']:
  try:ftp.mkd(part)
  except ftplib.error_perm as e:
   if not str(e).startswith('550'):raise
 receipts=[]
 for p in sorted(files,key=lambda p:p.suffix=='.json'):
  data=p.read_bytes();digest=hashlib.sha256(data).hexdigest();rel='components/vocal-world-v1/'+p.name;url=r.BASE+urllib.parse.quote(rel)
  try:
   with urllib.request.urlopen(url+'?check='+str(time.time_ns()),timeout=45)as response:existing=response.read()
  except urllib.error.HTTPError as e:
   if e.code!=404:raise
   existing=None
  if existing is not None:
   if hashlib.sha256(existing).hexdigest()!=digest:raise RuntimeError('Immutable component already exists with different content: '+p.name)
  else:
   ftp.storbinary('STOR '+rel+'.uploading',io.BytesIO(data));ftp.rename(rel+'.uploading',rel)
  with urllib.request.urlopen(url+'?verify='+str(time.time_ns()),timeout=45)as response:download=response.read()
  if hashlib.sha256(download).hexdigest()!=digest:raise RuntimeError('HTTPS verification failed: '+p.name)
  receipts.append(dict(file=p.name,sha256=digest,size=len(data)));print('Published and HTTPS verified:',p.name,flush=True)
 (share/'PUBLISHED.json').write_text(json.dumps(dict(base=r.BASE+'components/vocal-world-v1/',files=receipts),indent=2))
finally:ftp.quit()
""".replace('REPO',repr(cfg['mac_repo']))
 subprocess.run(['ssh','-o','ConnectTimeout=15','-T',cfg['publish_via_ssh'],shlex.join([cfg['mac_python'],'-c',code])],check=True)
 shutil.copy2(share/'PUBLISHED.json',out/'PUBLISHED.json')
 print('Optional vocal engine packages published; application release was not changed.')
if __name__=='__main__':main()

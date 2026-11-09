"""Lemon Studio's Windows Development sample exchange."""

import sys, os, traceback, ctypes
from pathlib import Path

startup_log = Path(os.environ['LOCALAPPDATA']) / 'Lemon Sample Exchange' / 'startup.log'
def report_startup_error(kind, error, trace):
 try:
  startup_log.parent.mkdir(parents=True, exist_ok=True)
  startup_log.write_text(''.join(traceback.format_exception(kind, error, trace)), encoding='utf-8')
 finally:
  ctypes.windll.user32.MessageBoxW(None, str(error), 'Community Samples could not open', 0x10)
sys.excepthook = report_startup_error
import os, sys, json, time, hashlib, hmac, base64, re, winsound, wave, threading, queue
from pathlib import Path
from ctypes import *
from ctypes import wintypes as wt
from urllib.request import Request,urlopen
from urllib.error import HTTPError
import tkinter as tk
from tkinter import ttk, filedialog, simpledialog, messagebox
sys.path.insert(0,str(Path(__file__).parent/'vendor'))
from tkinterdnd2 import TkinterDnD,DND_FILES
import psutil
BASE='https://lemon-studio-exchange-api.lemon-community-8808aa.workers.dev'
CATS=['Vocals','Drums','Bass','Melody','Loops','FX']
TERMS='By uploading, you grant everyone a worldwide, perpetual, irrevocable, non-exclusive, royalty-free license to use, edit, remix, distribute, sublicense and sell this sample, alone or in other works, for any purpose including commercial releases. No attribution, further permission or payment is required. You retain ownership. Removing a sample does not cancel licenses already granted. You must have authority from all rights holders and performers, including vocalists, to grant these rights.'
def credentials():
 class Blob(Structure):_fields_=[('cb',wt.DWORD),('pb',c_void_p)]
 enrollment=Path(__file__).resolve().parent/'device.bin'
 if not enrollment.is_file():
  enrollment=Path(os.environ['LOCALAPPDATA'])/'Lemon Sample Exchange/device.bin'
 b=create_string_buffer(enrollment.read_bytes());a=Blob(len(b.raw)-1,cast(b,c_void_p));out=Blob()
 if not windll.crypt32.CryptUnprotectData(byref(a),None,None,None,None,1,byref(out)):raise RuntimeError('This installation is not enrolled.')
 try:return json.loads(string_at(out.pb,out.cb))
 finally:windll.kernel32.LocalFree(c_void_p(out.pb))
def request(cred,path,data=None,token=''):
 body=json.dumps(data).encode() if data is not None else b'';t=str(int(time.time()));method='POST' if data is not None else 'GET'
 sig=hmac.new(cred['device'].encode(),'\n'.join([method,path,t,hashlib.sha256(body).hexdigest()]).encode(),hashlib.sha256).hexdigest()
 req=Request(BASE+path,method=method,data=body if data is not None else None,headers={'User-Agent':'LemonStudio-SampleExchange/1.0','X-Studio-Time':t,'X-Studio-Signature':sig,'X-Member':token,'Content-Type':'application/json'})
 try:
  with urlopen(req,timeout=25) as r:return r.read() if path.startswith('/wav/') else json.load(r)
 except HTTPError as e:raise ValueError(f'{e.code}: '+e.read(250).decode(errors='replace'))
def main():
 host=psutil.Process(int(sys.argv[1]));expected=Path(__file__).resolve().parents[1]/'lemonstudio-development.exe'
 if Path(host.exe()).resolve()!=expected or host.pid!=os.getppid():raise RuntimeError('Open Sample Exchange in Lemon Studio Development.')
 cred=credentials();cache=Path(sys.argv[2])/'Sample Exchange';cache.mkdir(parents=True,exist_ok=True)
 root=TkinterDnD.Tk();root.title('Lemon Studio  /  Sample Exchange');root.geometry('850x570');root.configure(bg='#10191e')
 root.update_idletasks()
 # Own the companion window from Studio so it follows the DAW in the window stack.
 owners=[]
 @WINFUNCTYPE(wt.BOOL,wt.HWND,wt.LPARAM)
 def find_owner(hwnd,_):
  pid=wt.DWORD();windll.user32.GetWindowThreadProcessId(hwnd,byref(pid))
  if pid.value==host.pid and windll.user32.IsWindowVisible(hwnd):owners.append(hwnd)
  return True
 windll.user32.EnumWindows(find_owner,0)
 if owners:
  getparent=windll.user32.GetParent;getparent.argtypes=[wt.HWND];getparent.restype=wt.HWND
  setowner=windll.user32.SetWindowLongPtrW;setowner.argtypes=[wt.HWND,c_int,c_ssize_t];setowner.restype=c_ssize_t
  setowner(getparent(root.winfo_id()) or root.winfo_id(),-8,owners[0])
 style=ttk.Style();style.theme_use('clam');style.configure('Treeview',rowheight=30);pad={'padx':12,'pady':5};token='';rows=[];files={};jobs=queue.Queue()
 def poll():
  if not host.is_running():root.destroy();return
  while not jobs.empty():jobs.get()()
  root.after(200,poll)
 def task(fn,done):
  def run():
   try:v=fn();jobs.put(lambda:done(v))
   except Exception as exc:msg=str(exc);jobs.put(lambda:status.set(msg))
  threading.Thread(target=run,daemon=True).start()
 title=ttk.Label(root,text='SAMPLE EXCHANGE',font=('Segoe UI',22,'bold'));title.pack(anchor='w',**pad)
 filters=ttk.Frame(root);filters.pack(fill='x',**pad)
 search=tk.StringVar();cat=tk.StringVar(value='All categories');sort=tk.StringVar(value='Latest');status=tk.StringVar(value='Connecting…')
 ttk.Entry(filters,textvariable=search,width=30).pack(side='left');ttk.Combobox(filters,textvariable=cat,values=['All categories']+CATS,state='readonly',width=16).pack(side='left');ttk.Combobox(filters,textvariable=sort,values=['Latest','Top samples'],state='readonly',width=14).pack(side='left')
 tree=ttk.Treeview(root,columns=('name','artist','category','votes'),show='headings',selectmode='browse')
 for k,h,w in [('name','Sample',240),('artist','Creator',220),('category','Category',100),('votes','Votes',65)]:tree.heading(k,text=h);tree.column(k,width=w)
 tree.pack(fill='both',expand=True,**pad)
 def draw(*_):
  tree.delete(*tree.get_children())
  for s in sorted(rows,key=lambda s:(s['votes'] if sort.get()=='Top samples' else 0,s['time']),reverse=True):
   if (cat.get()=='All categories' or cat.get()==s['category']) and search.get().lower() in (s['title']+' '+s['creator']).lower():tree.insert('', 'end',iid=s['id'],values=[s['title'],s['creator'],s['category'],s['votes']])
 for v in (search,cat,sort):v.trace_add('write',draw)
 def refresh():
  def got(v):rows[:]=v;draw();status.set('Select a sample to cache it. Double-click to hear it, then drag its row to the playlist.')
  task(lambda:request(cred,'/catalog'),got)
 def selected():return next((s for s in rows if s['id'] in tree.selection()),None)
 def cache_sample(play=False):
  s=selected()
  if not s:return
  ident=s['id']
  if not re.fullmatch('[a-zA-Z0-9-]{1,64}',ident):return
  name=re.sub(r'[^a-zA-Z0-9 _-]','_',s['title'])[:60];file=cache/(name+' - '+ident+'.wav')
  def fetch():
   if not file.exists():
    audio=request(cred,'/wav/'+ident);tmp=file.with_suffix('.'+os.urandom(4).hex()+'.tmp');tmp.write_bytes(audio)
    try:
     with wave.open(str(tmp)) as w:assert w.getnframes()>0
     tmp.replace(file)
    finally:tmp.unlink(missing_ok=True)
   return file
  def ready(f):
   files[ident]=f;status.set(s['title']+' ready · Drag this row to the playlist.')
   if play and selected()==s:winsound.PlaySound(str(f),winsound.SND_FILENAME|winsound.SND_ASYNC)
  task(fetch,ready)
 tree.bind('<<TreeviewSelect>>',lambda e:cache_sample());tree.bind('<Double-1>',lambda e:cache_sample(True));tree.drag_source_register(1,DND_FILES)
 def drag(e):
  s=selected();f=files.get(s['id']) if s else None
  if not f:return ('refuse_drop',DND_FILES,'')
  return ('copy',DND_FILES,root.tk.call('list',str(f)))
 tree.dnd_bind('<<DragInitCmd>>',drag)
 def auth(register=False):
  nonlocal token
  email=simpledialog.askstring('Member account','Email address',initialvalue=cred['email'],parent=root)
  if not email:return
  pw=simpledialog.askstring('Member account','Choose a password (12+ characters)' if register else 'Password',show='*',parent=root)
  if not pw:return
  def signed(v):
   nonlocal token
   token=v['token'];status.set('Signed in as '+email)
  task(lambda:request(cred,'/register' if register else '/login',dict(email=email,password=pw,invite=cred['invite']) if register else dict(email=email,password=pw)),signed)
 def vote():
  s=selected()
  if s:task(lambda:request(cred,'/vote',{'id':s['id']},token),lambda _:refresh())
 def upload():
  if not token:return status.set('Sign in first.')
  path=filedialog.askopenfilename(parent=root,filetypes=[('WAV audio','*.wav')])
  if not path:return
  if os.path.getsize(path)>5242880:return status.set('Choose a WAV under 5 MB.')
  name=simpledialog.askstring('Upload','Sample title',initialvalue=Path(path).stem,parent=root);artist=simpledialog.askstring('Upload','Public creator or vocalist credit',parent=root);category=simpledialog.askstring('Upload','Category: '+', '.join(CATS),initialvalue=cat.get() if cat.get() in CATS else 'Vocals',parent=root)
  if not name or not artist or category not in CATS:return
  if not messagebox.askyesno('Accept terms and publish?',TERMS,parent=root):return
  task(lambda:request(cred,'/upload',dict(title=name,creator=artist,category=category,agree=True,wav=base64.b64encode(Path(path).read_bytes()).decode()),token),lambda _:refresh())
 def news():
  if not token:return status.set('Sign in first.')
  def choose(v):
   old=v['newsletter'];label='Unsubscribe from' if old else 'Subscribe to'
   if messagebox.askyesno('Optional email preference',label+' Lemon Studio news and product updates by email?',parent=root):task(lambda:request(cred,'/newsletter',{'subscribe':not old},token),status.set)
  task(lambda:request(cred,'/me',token=token),choose)
 bar=ttk.Frame(root);bar.pack(fill='x',**pad)
 for label,fn in [('Preview',lambda:cache_sample(True)),('Stop',lambda:winsound.PlaySound(None,0)),('Vote',vote),('Upload WAV',upload),('Refresh',refresh),('Sign in',auth),('Create preview account',lambda:auth(True))]:ttk.Button(bar,text=label,command=fn).pack(side='left')
 ttk.Label(root,textvariable=status,wraplength=790).pack(fill='x',**pad)
 foot=ttk.Frame(root);foot.pack(fill='x',**pad)
 ttk.Button(foot,text='Royalty-free upload terms',command=lambda:messagebox.showinfo('Upload license',TERMS,parent=root)).pack(side='left');ttk.Button(foot,text='Newsletter preferences',command=news).pack(side='left')
 refresh();poll();root.mainloop()
if __name__=='__main__':main()


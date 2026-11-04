"""Loopback-only Wave visual preview. Renders the real editor, never opens audio/MIDI."""
from pathlib import Path
import argparse, hashlib, http.server, json, os, secrets, subprocess, threading, time, urllib.request, webbrowser

APP = 'lemon-wave-live-v1'
class Studio:
    def __init__(self, source, runtime, renderer):
        self.source, self.runtime = source, runtime
        self.design = source/'plugins/lemon-wave/design/live.json'
        self.runtime.mkdir(parents=True, exist_ok=True)
        self.lock=threading.RLock(); self.token=secrets.token_urlsafe(24)
        self.page=0;self.values={};self.revision=0;self.error='';self.signature='';self.result={};self.previous=None
        env=os.environ.copy();env.pop('LEMON_WAVE_DESIGN',None)
        self.log=(runtime/'renderer.log').open('a',encoding='utf-8')
        self.renderer=str(renderer)
        self.worker=subprocess.Popen([str(renderer)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log,text=True,encoding='utf-8',env=env,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
        self.render()
    def render(self, candidate=None):
        with self.lock:
            started=time.perf_counter()
            try:
                pointer=self.runtime/'renderer-path.txt'
                selected=pointer.read_text(encoding='utf-8-sig').strip() if pointer.exists() else self.renderer
                if selected!=self.renderer:
                    self.worker.stdin.close();self.worker.wait(timeout=10)
                    self.renderer=selected
                    env=os.environ.copy();env.pop('LEMON_WAVE_DESIGN',None)
                    self.worker=subprocess.Popen([selected],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log,text=True,encoding='utf-8',env=env,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
                text=self.design.read_text(encoding='utf-8-sig') if candidate is None else json.dumps(candidate,indent=2)+'\n'
                if len(text)>65536:raise ValueError('Design file is too large')
                design=json.loads(text)
                temporary=self.runtime/'frame-next.png'
                request={'design':design,'page':self.page,'values':self.values,'output':str(temporary)}
                self.worker.stdin.write(json.dumps(request)+'\n');self.worker.stdin.flush()
                response=self.worker.stdout.readline()
                if not response:raise RuntimeError('Renderer stopped. Restart Live Preview.')
                result=json.loads(response)
                if not result.get('ok'):raise ValueError(result.get('error','Render failed'))
                if candidate is not None:
                    original=self.design.read_text(encoding='utf-8-sig')
                    history=self.runtime/'design-history';history.mkdir(exist_ok=True)
                    (history/(str(time.time_ns())+'.json')).write_text(original,encoding='utf-8')
                    temporary_design=self.design.with_suffix('.json.tmp')
                    temporary_design.write_text(text,encoding='utf-8');temporary_design.replace(self.design)
                    self.previous=json.loads(original)
                temporary.replace(self.runtime/'frame.png')
                self.current=result['design'];self.signature=hashlib.sha256(text.encode()).hexdigest()
                self.result=result;self.revision+=1;self.error='';self.ms=round((time.perf_counter()-started)*1000)
            except Exception as e:
                self.error=str(e)
                if candidate is not None:raise
    def state(self):
        with self.lock:return {'app':APP,'revision':self.revision,'error':self.error,'ms':getattr(self,'ms',0),'page':self.page,'design':getattr(self,'current',{}),'parameters':self.result.get('parameters',[]),'hits':self.result.get('hits',[]),'source':str(self.design)}
    def watch(self):
        while True:
            time.sleep(.4)
            try:
                text=self.design.read_text(encoding='utf-8-sig')
                signature=hashlib.sha256(text.encode()).hexdigest()
                pointer=self.runtime/'renderer-path.txt'
                changed_renderer=pointer.exists() and pointer.read_text(encoding='utf-8-sig').strip()!=self.renderer
                if signature!=self.signature or changed_renderer:self.render()
            except Exception as e:self.error=str(e)

def run():
    parser=argparse.ArgumentParser();parser.add_argument('--source',type=Path,required=True);parser.add_argument('--runtime',type=Path,required=True);parser.add_argument('--renderer',type=Path,required=True);parser.add_argument('--port',type=int,default=18765);parser.add_argument('--open',action='store_true');args=parser.parse_args()
    url=f'http://127.0.0.1:{args.port}'
    try:
        data=json.load(urllib.request.urlopen(url+'/state',timeout=1))
        if data.get('app')==APP:
            if args.open:webbrowser.open(url)
            return
    except Exception:pass
    studio=Studio(args.source.resolve(),args.runtime.resolve(),args.renderer.resolve())
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def send(self,data,status=200,kind='application/json'):
            if not isinstance(data,bytes):data=json.dumps(data).encode()
            self.send_response(status);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(data)));self.send_header('Cache-Control','no-store');self.send_header('X-Content-Type-Options','nosniff');self.end_headers();self.wfile.write(data)
        def local(self):return self.headers.get('Host')==f'127.0.0.1:{args.port}'
        def do_GET(self):
            if not self.local():return self.send({'error':'Local access only'},403)
            path=self.path.split('?')[0]
            if path=='/':return self.send((Path(__file__).with_name('preview.html').read_text(encoding='utf-8').replace('__TOKEN__',studio.token)).encode(),kind='text/html; charset=utf-8')
            if path=='/state':return self.send(studio.state())
            if path=='/frame.png':
                with studio.lock:
                    try:return self.send((studio.runtime/'frame.png').read_bytes(),kind='image/png')
                    except FileNotFoundError:return self.send({'error':studio.error},503)
            return self.send({'error':'Not found'},404)
        def do_POST(self):
            if not self.local() or self.headers.get('X-Wave-Token')!=studio.token or self.headers.get('Origin',url)!=url:return self.send({'error':'Local preview token required'},403)
            try:
                size=int(self.headers.get('Content-Length','0'))
                if size<1 or size>65536:raise ValueError('Invalid request size')
                data=json.loads(self.rfile.read(size))
                with studio.lock:
                    if self.path=='/design':studio.render(data)
                    elif self.path=='/undo':
                        if studio.previous is not None:studio.render(studio.previous)
                    elif self.path=='/view':
                        studio.page=max(0,min(2,int(data.get('page',studio.page))))
                        if 'values' in data:studio.values={str(k):int(v) for k,v in data['values'].items()}
                        studio.render()
                    else:return self.send({'error':'Not found'},404)
                return self.send(studio.state())
            except Exception as e:return self.send({'error':str(e)},400)
    try:server=http.server.ThreadingHTTPServer(('127.0.0.1',args.port),Handler)
    except OSError:
        studio.worker.terminate();raise
    threading.Thread(target=studio.watch,daemon=True).start()
    if args.open:webbrowser.open(url)
    try:server.serve_forever()
    finally:studio.worker.terminate();studio.log.close()
if __name__=='__main__':run()

"""Launch one Wave standalone and release its devices when its window closes."""
from pathlib import Path
import ctypes, json, os, subprocess, sys, time
from ctypes import wintypes

user32=ctypes.WinDLL('user32',use_last_error=True)
kernel32=ctypes.WinDLL('kernel32',use_last_error=True)
kernel32.CreateMutexW.argtypes=[ctypes.c_void_p,wintypes.BOOL,wintypes.LPCWSTR]
kernel32.CreateMutexW.restype=wintypes.HANDLE
kernel32.CloseHandle.argtypes=[wintypes.HANDLE]
user32.GetWindowThreadProcessId.argtypes=[wintypes.HWND,ctypes.POINTER(wintypes.DWORD)]
user32.IsWindowVisible.argtypes=[wintypes.HWND]
user32.GetWindowTextLengthW.argtypes=[wintypes.HWND]
user32.ShowWindow.argtypes=[wintypes.HWND,ctypes.c_int]
user32.SetForegroundWindow.argtypes=[wintypes.HWND]
callback_type=ctypes.WINFUNCTYPE(wintypes.BOOL,wintypes.HWND,wintypes.LPARAM)
user32.EnumWindows.argtypes=[callback_type,wintypes.LPARAM]

def windows(pid):
    found=[]
    @callback_type
    def visit(hwnd,_):
        owner=wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd,ctypes.byref(owner))
        if owner.value==pid and user32.IsWindowVisible(hwnd) and user32.GetWindowTextLengthW(hwnd)>0:
            found.append(hwnd)
        return True
    user32.EnumWindows(visit,0)
    return found

def message(text):
    user32.MessageBoxW(None,text,'Lemon Wave',0x40)

def running_wave():
    command="Get-CimInstance Win32_Process | Where-Object { $_.Name -in @('Lemon Wave.exe','Lemon Wave Development.exe') } | Select-Object ProcessId,ExecutablePath | ConvertTo-Json -Compress"
    result=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',command],capture_output=True,creationflags=subprocess.CREATE_NO_WINDOW,timeout=15)
    if result.returncode: raise RuntimeError('Could not check existing Wave instances. Please try again.')
    raw=result.stdout.decode('utf-8',errors='replace').strip()
    if not raw:return []
    data=json.loads(raw)
    return data if isinstance(data,list) else [data]

def main():
    guard=kernel32.CreateMutexW(None,False,'Local\\LemonWaveStandaloneLauncher')
    if not guard:raise ctypes.WinError(ctypes.get_last_error())
    duplicate=ctypes.get_last_error()==183
    child=None
    try:
        existing=running_wave()
        if duplicate or existing:
            for process in existing:
                for hwnd in windows(process['ProcessId']):
                    user32.ShowWindow(hwnd,9)
                    user32.SetForegroundWindow(hwnd)
                    return
            message('A Lemon Wave standalone is already running or still closing. Wait a moment before reopening it. Only one standalone can use the MIDI keyboard at a time.')
            return
        env=os.environ.copy()
        if sys.argv[1]=='--standalone':
            executable=Path(sys.argv[2]).resolve()
        else:
            root=Path(sys.argv[1]);data=json.loads((root/'development.json').read_text(encoding='utf-8-sig'))
            executable=Path(data['executable']).resolve()
            env['LEMON_WAVE_DESIGN']=data['design']
            # The Workshop settings are the authoritative device choice for every build.
            settings=root/'Settings.json'
            if settings.exists():
                (executable.parent/'Settings.json').write_bytes(settings.read_bytes())
        env['NIH_LOG']=str(executable.parent/'audio-startup.log')
        child=subprocess.Popen([str(executable)],cwd=str(executable.parent),env=env)
        seen=False;missing_since=None;started=time.monotonic()
        while child.poll() is None:
            if windows(child.pid):
                seen=True;missing_since=None
            elif seen:
                if missing_since is None:missing_since=time.monotonic()
                # Give the host its normal shutdown first. Some old builds leave
                # their MIDI/audio thread alive after the last window is closed.
                if time.monotonic()-missing_since>2.:
                    child.terminate();child.wait(timeout=10);break
            elif time.monotonic()-started>90.:
                child.terminate();child.wait(timeout=10)
                message('Wave could not finish opening. Its background process has been closed so it will not keep your MIDI keyboard occupied. If Lemon Studio is using LPK25, release that input before opening standalone Wave.')
                break
            time.sleep(.2)
    finally:
        kernel32.CloseHandle(guard)

if __name__=='__main__':
    try:main()
    except Exception as error:message(str(error))

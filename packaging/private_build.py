"""Reject stale private preview builds; Master Assistant publication authorized 2 November 2026."""
from pathlib import Path
import io, zipfile, json, re
MARKER = b"LEMON_PRIVATE_MASTER_ASSISTANT_BUILD"
def require_public_binary(path):
    if private_binary(Path(path).read_bytes()):
        raise SystemExit("Private Master Assistant build: publication is blocked until Michael explicitly approves it.")
def private_source(name):
    name=name.replace("\\","/")
    return name == "AGENTS.md" or name.endswith("/WORKFLOW.md") or name.endswith("/MAC_RELEASE.md")  # Local operator notes are not corresponding application source.

def public_source_bytes(name, data):
    """Do not redistribute upstream Android demo API keys with the JUCE source."""
    name = name.replace("\\", "/").removeprefix("Source/")
    if name.startswith("plugins/lemon-master-assistant/JUCE/") and name.endswith("/google-services.json"):
        config = json.loads(data)
        for client in config.get("client", []):
            client["api_key"] = []
        data = (json.dumps(config, indent=2) + "\n").encode("utf-8")
    if re.search(rb"AIza[0-9A-Za-z_-]{35}", data):
        raise ValueError("Google API key pattern in public source: " + name)
    return data

def private_binary(data):
    return MARKER in data or b"LEMON_UNBUNDLED_MASTER_ASSISTANT_BUILD" in data

def require_public_artifact(path):
    path=Path(path)
    def inspect(name,data,depth=0):
        normalized=name.replace("\\","/")
        relative=normalized.removeprefix("Source/")
        if normalized.endswith("/google-services.json") and re.search(rb"AIza[0-9A-Za-z_-]{35}", data):
            raise SystemExit("Unsanitized Google demo API key in publication package: " + normalized)
        if private_source(relative):
            raise SystemExit("Private Master Assistant files found in publication package.")
        if normalized.endswith('.exe') or normalized.rsplit('/',1)[-1]=='lemonstudio':
            if private_binary(data): raise SystemExit("Private Master Assistant binary cannot be published.")
        if normalized.lower().endswith('.zip'):
            if depth>4: raise SystemExit("Publication archive nesting is too deep to verify.")
            with zipfile.ZipFile(io.BytesIO(data)) as z:
                for item in z.infolist():
                    if not item.is_dir(): inspect(item.filename,z.read(item),depth+1)
    inspect(path.name,path.read_bytes())

if __name__ == "__main__":
    import subprocess, sys
    if sys.argv[1:] != ["--check-source"]:
        raise SystemExit("Usage: private_build.py --check-source")
    root = Path(__file__).resolve().parents[1]
    names = subprocess.check_output(["git", "ls-files", "-z"], cwd=root).decode().split("\0")
    matches = [name for name in names if name and (root/name).is_file()
               and re.search(rb"AIza[0-9A-Za-z_-]{35}", (root/name).read_bytes())]
    if matches:
        raise SystemExit("Google API key pattern found in: " + ", ".join(matches))
    print("Tracked source check passed: no Google API key patterns.")

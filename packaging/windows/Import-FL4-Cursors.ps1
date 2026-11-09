# SPDX-License-Identifier: GPL-3.0-or-later
# Open your own FL Studio 4 copy before running this script. Restart Lemon after import.
param([string]$SkinFolder = (Join-Path $env:LOCALAPPDATA 'Lemon Studio/Skins/FL4'))
$ErrorActionPreference = 'Stop'

$fl = Get-Process -Name FL -ErrorAction SilentlyContinue | Where-Object {
    $_.Modules | Where-Object ModuleName -eq 'FLEngine.dll'
} | Select-Object -First 1
if (!$fl) { throw 'Open FL Studio 4 before importing its cursors.' }
$engine = $fl.Modules | Where-Object ModuleName -eq 'FLEngine.dll' | Select-Object -First 1
if (!$engine -or (Get-FileHash -LiteralPath $engine.FileName).Hash -ne '717E4CB1AF4891BBE8846E7673333539B6C7C96AE28ED42D9814A200AA4C56E3') {
    throw 'This importer supports the verified FL Studio 4 engine build only.'
}

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class VintageCursorReader {
    [DllImport("kernel32", SetLastError = true)]
    public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32", SetLastError = true)]
    public static extern bool CloseHandle(IntPtr process);
    [DllImport("kernel32", SetLastError = true)]
    static extern bool ReadProcessMemory(IntPtr process, IntPtr address, byte[] bytes, UIntPtr length, out UIntPtr read);

    public static byte[] Read(IntPtr process, long address, int length) {
        byte[] bytes = new byte[length];
        UIntPtr read;
        if (!ReadProcessMemory(process, new IntPtr(address), bytes, new UIntPtr((uint)length), out read)
            || read.ToUInt64() != (ulong)length) {
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        }
        return bytes;
    }
}
'@
$handle = [VintageCursorReader]::OpenProcess(0x10, $false, $fl.Id)
if ($handle -eq [IntPtr]::Zero) { throw 'Could not read the running FL Studio 4 process.' }
$base = $engine.BaseAddress.ToInt64()
$names = 'EEBRUSH','EECUT','EEDEL','EEIP','EEPEN','EESEL','EESLIDE','EEZOOM','MOVE','MOVEH','MOVEV','SELDOWN','SELUP','SELUPDOWN','SIZEH','32761','32762','32763','32764','32765','32766','32767'
$images = @{}
try {
    for ($i = 0; $i -lt $names.Count; $i++) {
        $group = [VintageCursorReader]::Read($handle, $base + 0x38221c + 20 * $i, 20)
        $id = [BitConverter]::ToUInt16($group, 18)
        if ([BitConverter]::ToUInt16($group, 2) -ne 2 -or [BitConverter]::ToUInt16($group, 4) -ne 1 -or $id -lt 1 -or $id -gt 22) {
            throw "Unexpected cursor group: $($names[$i])"
        }
        $image = [VintageCursorReader]::Read($handle, $base + 0x26f08c + 308 * ($id - 1), 308)
        if ([BitConverter]::ToUInt32($image, 4) -ne 40 -or [BitConverter]::ToUInt32($image, 8) -ne 32 -or
            [BitConverter]::ToUInt32($image, 12) -ne 64 -or [BitConverter]::ToUInt16($image, 18) -ne 1 -or
            [BitConverter]::ToUInt32($image, 20) -ne 0) { throw "Unexpected cursor image: $($names[$i])" }
        $images[$names[$i]] = $image
    }
} finally {
    [void][VintageCursorReader]::CloseHandle($handle)
}
$folder = Join-Path $SkinFolder 'Cursors'
New-Item -ItemType Directory -Path $folder -Force | Out-Null
foreach ($name in $names) {
    [IO.File]::WriteAllBytes((Join-Path $folder "$name.bin"), $images[$name])
}
Write-Output "Imported $($names.Count) original cursor resources. Restart Lemon Studio to use them."

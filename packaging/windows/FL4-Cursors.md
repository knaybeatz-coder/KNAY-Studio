# FL Studio 2005 cursors in Lemon Studio

The FL Studio 2005 theme selects the FL Studio 2005 cursor pack automatically. You can also select it in the Theme Designer's cursor packs or choose it for individual cursor roles in another theme.

Lemon includes matching monochrome cursors for roles without original artwork. To use original cursor artwork from a compatible local FL Studio 4 installation on Windows, open FL Studio 4 and run `Import-FL4-Cursors.ps1` from this directory in 64-bit PowerShell. The importer checks the engine version by checksum and reads only the cursor resources from the running copy. Restart Lemon Studio after importing.

The original artwork stays in your local Lemon Studio skin folder. If you use Lemon on another machine, you can set `LEMON_VINTAGE_SKIN` to a local skin directory containing the imported `Cursors` folder.

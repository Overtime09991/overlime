# Baut den Installer. Aufruf:  .\build.ps1 -Version 0.13 -ExeDir ..\..\release-0.13
param(
    [string]$Version = "0.13",
    [string]$ExeDir = ""
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

# Bilder und Symbol erzeugen (Pillow nötig: pip install pillow)
python make_assets.py

$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "C:\Program Files (x86)\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 nicht gefunden (winget install JRSoftware.InnoSetup)" }

$args = @("/DMyAppVersion=$Version")
if ($ExeDir) { $args += "/DExeDir=$((Resolve-Path $ExeDir).Path)" }
& $iscc @args overlime.iss
if ($LASTEXITCODE -ne 0) { throw "ISCC fehlgeschlagen" }
Get-ChildItem Output

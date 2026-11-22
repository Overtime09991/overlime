# Baut den Installer (ein Setup für alle, Basisversion 0.12, der Rest kommt per Auto-Update).
#   .\build.ps1                       lädt die Basisversion von GitHub und baut Output\OVERLIME-Setup.exe
#   .\build.ps1 -ExeDir <Ordner>      nutzt eine lokale overlime.exe (+ .sha256) statt des Downloads
param(
    [string]$ExeDir = "",
    [string]$BaseTag = "v0.12"
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (-not $ExeDir) {
    $ExeDir = Join-Path $PSScriptRoot "base"
    New-Item -ItemType Directory -Force $ExeDir | Out-Null
    $url = "https://github.com/Overtime09991/overlime/releases/download/$BaseTag"
    Invoke-WebRequest "$url/overlime.exe" -OutFile (Join-Path $ExeDir "overlime.exe")
    Invoke-WebRequest "$url/overlime.exe.sha256" -OutFile (Join-Path $ExeDir "overlime.exe.sha256")
}
# Prüfsumme kontrollieren
$expected = ((Get-Content (Join-Path $ExeDir "overlime.exe.sha256") -Raw).Trim() -split "\s+")[0].ToLower()
$actual = (Get-FileHash (Join-Path $ExeDir "overlime.exe") -Algorithm SHA256).Hash.ToLower()
if ($expected -ne $actual) { throw "Prüfsumme der Basisversion stimmt nicht" }

# Bilder und Symbol erzeugen (Pillow nötig: pip install pillow)
python make_assets.py

$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "C:\Program Files (x86)\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 nicht gefunden (winget install JRSoftware.InnoSetup)" }

& $iscc "/DExeDir=$((Resolve-Path $ExeDir).Path)" overlime.iss
if ($LASTEXITCODE -ne 0) { throw "ISCC fehlgeschlagen" }
Get-ChildItem Output

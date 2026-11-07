[CmdletBinding()]
param([string]$Iscc)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (!$Iscc) {
    $Iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
    if (!$Iscc) {
        $Iscc = @(
            "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
            "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe"
            "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
            "${env:ProgramFiles(x86)}\Inno Setup 7\ISCC.exe"
        ) | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    }
}
if (!$Iscc) { throw 'Inno Setup compiler not found. Supply its path with -Iscc.' }
$cli = Join-Path $root 'dist\EasyMediaCompress\compress.exe'
if (!(Test-Path -LiteralPath $cli)) { throw 'Run build.ps1 first.' }
$version = & $cli --version
if ($LASTEXITCODE -ne 0 -or $version -notmatch '^\d+\.\d+\.\d+$') { throw 'Could not read the packaged version.' }
& $Iscc "/DAppVersion=$version" (Join-Path $root 'packaging\windows.iss')
if ($LASTEXITCODE -ne 0) { throw "Installer build failed ($LASTEXITCODE)." }

$installer = Join-Path $root 'dist\EasyMediaCompress-Setup-x64.exe'
$stub = Join-Path $root 'target\release\legacy_bridge.exe'
$legacy = New-Item -ItemType Directory -Force (Join-Path $root 'dist\legacy')
$bridge = Join-Path $legacy.FullName 'EasyMediaCompress.exe'
Copy-Item -LiteralPath $stub -Destination $bridge -Force
$source = [IO.File]::OpenRead($installer)
try {
    $destination = [IO.File]::Open($bridge, [IO.FileMode]::Append)
    try {
        $source.CopyTo($destination)
        $destination.Write([BitConverter]::GetBytes([uint64]$source.Length), 0, 8)
        $destination.Write([Text.Encoding]::ASCII.GetBytes('EMCSETUP'), 0, 8)
    } finally { $destination.Dispose() }
} finally { $source.Dispose() }
Write-Host "Legacy upgrade bridge: $bridge"

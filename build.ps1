[CmdletBinding()]
param([string]$VcVars)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-Checked {
    param([string]$Exe, [string[]]$Arguments)
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed with exit code $LASTEXITCODE" }
}

if ($env:OS -ne 'Windows_NT' -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw 'This script creates the Windows x64 portable package.'
}
if ($VcVars) { $VcVars = (Resolve-Path -LiteralPath $VcVars).Path }
Push-Location -LiteralPath $PSScriptRoot
try {
    if ($VcVars) {
        # vcvars sets compiler/SDK environment variables in the child shell.
        Invoke-Checked 'cmd.exe' @('/d', '/c', ('call "{0}" && cargo build --locked --release' -f $VcVars))
    } else {
        Invoke-Checked 'cargo' @('build', '--locked', '--release')
    }

    $dist = (New-Item -ItemType Directory -Path 'dist\windows\EasyMediaCompress' -Force).FullName
    Copy-Item -LiteralPath 'target\release\EasyMediaCompress.exe', 'target\release\compress.exe' -Destination $dist -Force
    Copy-Item -LiteralPath 'README.md', 'THIRD_PARTY.md', 'win_install.ps1' -Destination $dist -Force
    Compress-Archive -LiteralPath (@('EasyMediaCompress.exe', 'compress.exe', 'README.md', 'THIRD_PARTY.md', 'win_install.ps1') | ForEach-Object { Join-Path $dist $_ }) -DestinationPath 'dist/windows/portable.zip' -Force
} finally {
    Pop-Location
}

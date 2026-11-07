[CmdletBinding()]
param([string]$VcVars, [string]$OpenMpDll, [switch]$DevToolsOnly)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-Checked {
    param([string]$Exe, [string[]]$Arguments)
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed with exit code $LASTEXITCODE" }
}

function Get-Archive {
    param([string]$Uri, [string]$File, [string]$Sha256)
    if (!(Test-Path -LiteralPath $File -PathType Leaf)) {
        $pending = "$File.part"
        Invoke-Checked 'curl.exe' @('--fail', '--location', '--silent', '--show-error', '--retry', '2', $Uri, '--output', $pending)
        if ((Get-FileHash -LiteralPath $pending).Hash -ne $Sha256) { throw "Unexpected SHA-256: $Uri" }
        Move-Item -LiteralPath $pending -Destination $File
    }
    if ((Get-FileHash -LiteralPath $File).Hash -ne $Sha256) { throw "Unexpected SHA-256: $File" }
}

if ($env:OS -ne 'Windows_NT' -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw 'This script creates the Windows x64 portable package.'
}
if ($VcVars) { $VcVars = (Resolve-Path -LiteralPath $VcVars).Path }
if ($OpenMpDll) { $OpenMpDll = (Resolve-Path -LiteralPath $OpenMpDll).Path }
Push-Location -LiteralPath $PSScriptRoot
try {
    if (!$DevToolsOnly -and $VcVars) {
        # vcvars sets compiler/SDK environment variables in the child shell.
        Invoke-Checked 'cmd.exe' @('/d', '/c', ('call "{0}" && cargo build --locked --release' -f $VcVars))
    } elseif (!$DevToolsOnly) {
        Invoke-Checked 'cargo' @('build', '--locked', '--release')
    }

    if (!$OpenMpDll) {
        $redist = @($env:VCToolsRedistDir)
        if ($VcVars) {
            $vc = Split-Path (Split-Path (Split-Path $VcVars))
            $redist += Join-Path $vc 'Redist\MSVC'
        }
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        if (Test-Path -LiteralPath $vswhere) {
            $redist += @(& $vswhere -products '*' -property installationPath | ForEach-Object { Join-Path $_ 'VC\Redist\MSVC' })
        }
        $OpenMpDll = $redist | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | ForEach-Object {
            Get-ChildItem -LiteralPath $_ -Filter vcomp140.dll -File -Recurse -ErrorAction SilentlyContinue
        } | Where-Object { $_.FullName -match '\\x64\\Microsoft\.VC\d+\.OpenMP\\vcomp140\.dll$' -and $_.FullName -notmatch '\\onecore\\' } |
            Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
    }
    if (!$OpenMpDll) {
        throw 'vcomp140.dll was not found in the C++ redistributable files. Supply its x64 path with -OpenMpDll.'
    }

    $cache = New-Item -ItemType Directory -Path '.tools' -Force
    $ffVersion = '8.1.2'
    $imVersion = '7.1.2-35'
    $ffFile = "ffmpeg-$ffVersion-essentials_build.zip"
    $imFile = "ImageMagick-$imVersion-portable-Q16-x64.7z"
    $ffArchive = Join-Path $cache.FullName $ffFile
    $imArchive = Join-Path $cache.FullName $imFile
    Get-Archive "https://github.com/GyanD/codexffmpeg/releases/download/$ffVersion/$ffFile" $ffArchive 'DB580001CAA24AC104C8CB856CD113A87B0A443F7BDF47D8C12B1D740584A2EC'
    Get-Archive "https://download.imagemagick.org/archive/binaries/$imFile" $imArchive 'E7C629F3530CCB9D4918A15F0E928254FEDA28E84A7FA794B25873D152C075C1'

    $ffExtract = (New-Item -ItemType Directory -Path ".tools\ffmpeg-$ffVersion" -Force).FullName
    $imExtract = (New-Item -ItemType Directory -Path ".tools\imagemagick-$imVersion" -Force).FullName
    Invoke-Checked 'tar.exe' @('-xf', $ffArchive, '-C', $ffExtract)
    Invoke-Checked 'tar.exe' @('-xf', $imArchive, '-C', $imExtract)

    $ffRoot = Join-Path $ffExtract "ffmpeg-$ffVersion-essentials_build"
    $dist = (New-Item -ItemType Directory -Path 'dist\EasyMediaCompress' -Force).FullName
    $ffOut = (New-Item -ItemType Directory -Path "$dist\tools\ffmpeg" -Force).FullName
    $imOut = (New-Item -ItemType Directory -Path "$dist\tools\imagemagick" -Force).FullName
    foreach ($file in @('ffmpeg.exe', 'ffprobe.exe')) {
        Copy-Item -LiteralPath "$ffRoot\bin\$file" -Destination $ffOut -Force
    }
    Copy-Item -LiteralPath "$ffRoot\LICENSE", "$ffRoot\README.txt" -Destination $ffOut -Force
    foreach ($file in @('magick.exe', 'LICENSE.txt', 'NOTICE.txt')) {
        Copy-Item -LiteralPath "$imExtract\$file" -Destination $imOut -Force
    }
    Copy-Item -LiteralPath $OpenMpDll -Destination "$imOut\vcomp140.dll" -Force
    Copy-Item -LiteralPath 'packaging\policy.xml' -Destination $imOut -Force
    if (!$DevToolsOnly) {
        Copy-Item -LiteralPath 'target\release\EasyMediaCompress.exe', 'target\release\compress.exe' -Destination $dist -Force
        Copy-Item -LiteralPath 'README.md', 'THIRD_PARTY.md', 'win_install.ps1' -Destination $dist -Force
        Write-Host "Portable build: $dist"
    }
} finally {
    Pop-Location
}

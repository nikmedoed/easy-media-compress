[CmdletBinding()]
param([string]$VcVars, [string]$OpenMpDll)

$ErrorActionPreference = 'Stop'
if ($VcVars) { $VcVars = (Resolve-Path -LiteralPath $VcVars).Path }
Push-Location -LiteralPath $PSScriptRoot
$previousFfmpeg = $env:MEDIA_COMPRESS_FFMPEG_DIR
$previousMagick = $env:MEDIA_COMPRESS_MAGICK_DIR
try {
    $tools = Join-Path $PSScriptRoot 'dist\EasyMediaCompress\tools'
    if (!(Test-Path "$tools\ffmpeg\ffmpeg.exe") -or !(Test-Path "$tools\ffmpeg\ffprobe.exe") -or
        !(Test-Path "$tools\imagemagick\magick.exe") -or !(Test-Path "$tools\imagemagick\vcomp140.dll") -or
        !(Test-Path "$tools\imagemagick\policy.xml")) {
        .\build.ps1 -DevToolsOnly -VcVars $VcVars -OpenMpDll $OpenMpDll
    }
    $env:MEDIA_COMPRESS_FFMPEG_DIR = Join-Path $tools 'ffmpeg'
    $env:MEDIA_COMPRESS_MAGICK_DIR = Join-Path $tools 'imagemagick'
    if ($VcVars) {
        cmd.exe /d /c ('call "{0}" && cargo run --locked --bin EasyMediaCompress' -f $VcVars)
    } else {
        cargo run --locked --bin EasyMediaCompress
    }
    if ($LASTEXITCODE -ne 0) { throw "Development launch failed ($LASTEXITCODE)." }
} finally {
    $env:MEDIA_COMPRESS_FFMPEG_DIR = $previousFfmpeg
    $env:MEDIA_COMPRESS_MAGICK_DIR = $previousMagick
    Pop-Location
}

[CmdletBinding()]
param([string]$Package = (Join-Path $PSScriptRoot '..\dist\EasyMediaCompress'))

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Package = (Resolve-Path -LiteralPath $Package).Path
$cli = Join-Path $Package 'compress.exe'
$ffmpeg = Join-Path $Package 'tools\ffmpeg\ffmpeg.exe'
$ffprobe = Join-Path $Package 'tools\ffmpeg\ffprobe.exe'
$magick = Join-Path $Package 'tools\imagemagick\magick.exe'
$fixtures = Join-Path $PSScriptRoot '..\tests\fixtures'
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = [IO.Directory]::CreateTempSubdirectory('compress-smoke-').FullName
$pathBefore = $env:PATH
$ffBefore = $env:MEDIA_COMPRESS_FFMPEG_DIR
$imBefore = $env:MEDIA_COMPRESS_MAGICK_DIR
$success = $false

function Run {
    param([string]$Exe, [string[]]$Arguments)
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed ($LASTEXITCODE): $Arguments" }
}

function Assert {
    param([bool]$Condition, [string]$Message)
    if (!$Condition) { throw $Message }
}

function Video-Info {
    param([string]$File)
    Run $ffprobe @('-v','error','-show_streams','-of','json',$File) | ConvertFrom-Json
}

try {
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:MEDIA_COMPRESS_FFMPEG_DIR = $null
    $env:MEDIA_COMPRESS_MAGICK_DIR = $null
    $nest = (New-Item -ItemType Directory -Path "$root\медиа [1]").FullName
    Copy-Item -LiteralPath "$fixtures\sample.heic", "$fixtures\rotated.jpg", "$fixtures\photo.png" -Destination $nest
    Run $ffmpeg @('-hide_banner','-loglevel','error','-f','lavfi','-i','testsrc2=size=900x810:rate=15:duration=1',
        '-f','lavfi','-i','sine=duration=1','-c:v','libx264','-preset','ultrafast','-c:a','aac',"$nest\clip.mp4")
    $original = Get-FileHash -LiteralPath "$nest\clip.mp4"

    Assert ((Run $cli @('--version')) -match '^\d+\.\d+\.\d+$') 'Missing CLI version'
    Run $cli @($root)
    foreach ($name in @('sample_compressed.jpg','rotated_compressed.jpg','photo_compressed.jpg','clip_compressed.mp4')) {
        Assert (Test-Path -LiteralPath "$nest\$name" -PathType Leaf) "Missing $name"
    }
    $mp4 = Video-Info "$nest\clip_compressed.mp4"
    Assert ($mp4.streams[0].codec_name -eq 'h264' -and $mp4.streams[0].pix_fmt -eq 'yuv420p' -and
        $mp4.streams[1].codec_name -eq 'aac') 'Unexpected default video codecs'
    $jpg = Run $magick @('identify','-format','%wx%h:%Q',"$nest\rotated_compressed.jpg")
    Assert ($jpg -eq '30x40:60') "EXIF rotation or quality was lost: $jpg"
    $heic = Run $magick @('identify','-format','%wx%h',"$nest\sample_compressed.jpg")
    Assert ($heic -eq '40x30') "HEIC output dimensions: $heic"
    $corner = Run $magick @("$nest\photo_compressed.jpg",'-format','%[pixel:p{0,0}]','info:')
    Assert ($corner -eq 'srgb(255,255,255)' -or $corner -eq 'gray(255)') "JPEG alpha was not flattened onto white: $corner"

    Run $cli @('photo',"$nest\rotated.jpg",'--mode','original','--format','jpeg')
    $photo = Run $magick @('identify','-format','%wx%h:%Q:%[profiles]:%[exif:Make]',"$nest\rotated_converted.jpg")
    Assert ($photo -match '^30x40:95:.*exif.*icc.*:Easy Media Compress test$') "Original JPEG metadata: $photo"
    foreach ($format in @('webp','png')) {
        Run $cli @('image',"$nest\photo.png",'--format',$format)
        $info = Run $magick @('identify','-format','%wx%h:%[channels]',"$nest\photo_compressed.$format")
        Assert ($info -match '^64x64:.*a') "Alpha or dimensions lost in ${format}: $info"
    }

    Run $cli @("$nest\clip.mp4",'-crf','28','-preset','fast','--video-profile','crf480')
    $p480 = Video-Info "$nest\clip (2)_compressed.mp4"
    Assert ($p480.streams[0].height -eq 480) '480p short side was not scaled'
    Run $cli @("$nest\clip.mp4",'--video-profile','crf720')
    $p720 = Video-Info "$nest\clip (3)_compressed.mp4"
    Assert ($p720.streams[0].height -eq 720) '720p short side was not scaled'
    Run $cli @('5',"$nest\clip.mp4")
    Assert ((Get-Item -LiteralPath "$nest\clip_smaller.mp4").Length -lt 5MB) '5 MB output exceeds the target'
    foreach ($profile in @('gif120','gif180','gif','gif480')) {
        $short = if ($profile -eq 'gif') { 240 } else { [int]$profile.Substring(3) }
        $gifInput = Join-Path $nest "$profile.mp4"
        Copy-Item -LiteralPath "$nest\clip.mp4" -Destination $gifInput
        Run $cli @($gifInput,'--video-profile',$profile)
        $gif = Video-Info "$nest\${profile}_converted.gif"
        Assert ($gif.streams[0].height -eq $short -and [int]$gif.streams[0].nb_frames -eq 15) "Invalid $profile output"
    }
    Assert ((Get-FileHash -LiteralPath "$nest\clip.mp4").Hash -eq $original.Hash) 'Source video was modified'
    Set-Content -LiteralPath "$nest\broken.jpg" -Value 'broken'
    & $cli image "$nest\broken.jpg"
    Assert ($LASTEXITCODE -eq 1) 'Broken media should return exit code 1'
    Assert (!(Test-Path -LiteralPath "$nest\broken_compressed.jpg")) 'Failed conversion left a result'
    Assert (@(Get-ChildItem -LiteralPath $nest -Filter '.compress-*' -Force).Count -eq 0) 'Temporary file was left behind'
    $success = $true
    Write-Host 'Portable smoke test passed.'
} finally {
    $env:PATH = $pathBefore
    $env:MEDIA_COMPRESS_FFMPEG_DIR = $ffBefore
    $env:MEDIA_COMPRESS_MAGICK_DIR = $imBefore
    if ($success -and ([IO.Path]::GetDirectoryName($root).TrimEnd('\') -eq $tempRoot.TrimEnd('\')) -and
        ([IO.Path]::GetFileName($root) -like 'compress-smoke-*')) {
        Remove-Item -LiteralPath $root -Recurse
    } else {
        Write-Host "Test files: $root"
    }
}

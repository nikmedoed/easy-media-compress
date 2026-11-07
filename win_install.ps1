[CmdletBinding()]
param([switch]$Uninstall)

$ErrorActionPreference = 'Stop'
$folder = $PSScriptRoot
if (!(Test-Path -LiteralPath (Join-Path $folder 'EasyMediaCompress.exe'))) {
    $folder = Join-Path $folder 'dist\EasyMediaCompress'
}
$exe = Join-Path $folder 'EasyMediaCompress.exe'
$cli = Join-Path $folder 'compress.exe'
$entry = 'HKCU:\Software\Classes\AllFileSystemObjects\shell\EasyMediaCompressPortable'
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'Easy Media Compress (Portable).lnk'
if ($Uninstall) {
    # Remove only this application's fixed user-level key and shortcut.
    if (Test-Path -LiteralPath "$entry\command") { Remove-Item -LiteralPath "$entry\command" }
    if (Test-Path -LiteralPath $entry) { Remove-Item -LiteralPath $entry }
    if (Test-Path -LiteralPath $shortcutPath) { Remove-Item -LiteralPath $shortcutPath }
    return
}
if (!(Test-Path -LiteralPath $exe -PathType Leaf) -or !(Test-Path -LiteralPath $cli -PathType Leaf)) {
    throw 'Run build.ps1 first or run this script from the portable folder.'
}
New-Item -Path $entry -Force | Out-Null
Set-Item -LiteralPath $entry -Value 'Compress media (Portable)'
New-ItemProperty -LiteralPath $entry -Name Icon -PropertyType String -Value ('"{0}",0' -f $exe) -Force | Out-Null
New-Item -Path "$entry\command" -Force | Out-Null
Set-Item -LiteralPath "$entry\command" -Value ('"{0}" --gui "%1"' -f $exe)
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcutPath)
$shortcut.TargetPath = $exe
$shortcut.WorkingDirectory = $folder
$shortcut.IconLocation = "$exe,0"
$shortcut.Save()
Write-Host "Added Explorer menu and Start Menu shortcut for $exe"

[CmdletBinding()]
param([string]$VcVars)

$ErrorActionPreference = 'Stop'
if ($VcVars) { $VcVars = (Resolve-Path -LiteralPath $VcVars).Path }
Push-Location -LiteralPath $PSScriptRoot
try {
    if ($VcVars) {
        cmd.exe /d /c ('call "{0}" && cargo run --locked --bin EasyMediaCompress' -f $VcVars)
    } else {
        cargo run --locked --bin EasyMediaCompress
    }
    if ($LASTEXITCODE -ne 0) { throw "Development launch failed ($LASTEXITCODE)." }
} finally {
    Pop-Location
}

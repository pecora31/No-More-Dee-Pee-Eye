$ErrorActionPreference = 'Stop'
$workspace = Split-Path $PSScriptRoot -Parent
$destination = Join-Path $workspace 'runtime'
$revision = '6eb463a6758fb48cd101bc55dfd057e6e9d98af1'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$files = @('winws2.exe', 'cygwin1.dll', 'WinDivert.dll', 'WinDivert64.sys', 'lua/zapret-lib.lua', 'lua/zapret-antidpi.lua')
$manifest = @()
$manifestPath = Join-Path $workspace 'engine-manifest.json'
$expected = if (Test-Path -LiteralPath $manifestPath) { Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json } else { @() }
foreach ($file in $files) {
    $target = Join-Path $destination $file
    New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent) | Out-Null
    Invoke-WebRequest -Uri "https://raw.githubusercontent.com/bol-van/zapret-win-bundle/$revision/zapret-winws/$file" -OutFile $target
    $hash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
    $entry = $expected | Where-Object { $_.path -eq $file }
    if ($expected.Count -gt 0 -and (-not $entry -or $entry.sha256 -ne $hash)) { throw "Pinned hash mismatch: $file" }
    $manifest += [ordered]@{ path = $file; sha256 = $hash }
}
if ($expected.Count -eq 0) { throw 'Missing engine-manifest.json; restore the checked-in manifest before fetching.' }
Write-Output "Downloaded Zapret2 from pinned bundle $revision"
Get-AuthenticodeSignature -LiteralPath (Join-Path $destination 'WinDivert64.sys') | Select-Object Status,StatusMessage

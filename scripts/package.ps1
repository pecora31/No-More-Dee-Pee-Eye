$ErrorActionPreference = 'Stop'
$workspace = Split-Path $PSScriptRoot -Parent
Push-Location $workspace
try {
    if (-not (Test-Path -LiteralPath '.\runtime\winws2.exe')) { & "$PSScriptRoot\fetch-engine.ps1" }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $output = Join-Path $workspace 'dist\NoMoreDeePeeEye'
    New-Item -ItemType Directory -Force -Path $output | Out-Null
    $portableExe = Join-Path $workspace 'dist\NoMoreDeePeeEye-0.1.4-windows-x64.exe'
    Copy-Item -LiteralPath 'target\release\no-more-dee-pee-eye.exe' -Destination $portableExe -Force
    Copy-Item -LiteralPath $portableExe -Destination (Join-Path $output 'NoMoreDeePeeEye.exe') -Force
    New-Item -ItemType Directory -Force -Path (Join-Path $output 'runtime') | Out-Null
    Copy-Item -Path 'runtime\*' -Destination (Join-Path $output 'runtime') -Recurse -Force
    Copy-Item -LiteralPath 'README.md','THIRD-PARTY-NOTICES.md','engine-manifest.json','LICENSE' -Destination $output -Force
    Copy-Item -LiteralPath 'ui\fonts\INTER-OFL.txt' -Destination (Join-Path $output 'FONT-LICENSE.txt') -Force
    Compress-Archive -Path "$output\*" -DestinationPath (Join-Path $workspace 'dist\NoMoreDeePeeEye-0.1.4-windows-x64.zip') -Force
    Get-Item -LiteralPath $portableExe,(Join-Path $workspace 'dist\NoMoreDeePeeEye-0.1.4-windows-x64.zip') | Select-Object FullName,Length
} finally { Pop-Location }

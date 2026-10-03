$ErrorActionPreference = 'Stop'
$workspace = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $workspace 'dist\NoMoreDeePeeEye\NoMoreDeePeeEye.exe'
$artifacts = Join-Path $workspace 'artifacts'
function Run-Check([string]$program, [string]$arguments, [string]$name) {
    $taskProcess = Start-Process -FilePath $program -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $artifacts "$name.stdout.txt") -RedirectStandardError (Join-Path $artifacts "$name.stderr.txt")
    if (-not $taskProcess.WaitForExit(45000)) { throw "$name did not finish within 45 seconds." }
    if ($taskProcess.ExitCode -ne 0) { throw "$name failed: see artifacts/$name.stderr.txt" }
    Write-Output "PASS: $name"
}
Run-Check $exe '--ipc-self-test' 'release-ipc'
$unicode = Join-Path $artifacts 'kiểm thử có dấu\NoMoreDeePeeEye'
New-Item -ItemType Directory -Force -Path $unicode | Out-Null
Copy-Item -Path (Join-Path $workspace 'dist\NoMoreDeePeeEye\*') -Destination $unicode -Recurse -Force
$unicodeExe = Join-Path $unicode 'NoMoreDeePeeEye.exe'
$checkReport = Join-Path $artifacts 'release-engine-check.txt'
Run-Check $unicodeExe ('--check-engine "' + $checkReport + '"') 'release-unicode-check'
if ((Select-String -LiteralPath $checkReport -Pattern '^Validated$').Count -ne 3) { throw 'Expected three validated presets.' }
$smokeReport = Join-Path $artifacts 'release-engine-smoke.txt'
Run-Check $unicodeExe ('--smoke-engine "' + $smokeReport + '"') 'release-engine-smoke'
if (-not (Select-String -LiteralPath $smokeReport -Pattern 'PASS: helper closed events')) { throw 'Missing worker cleanup acknowledgment.' }
$snapshots = Join-Path $artifacts 'release-screenshots'
Run-Check $exe ('--screenshots "' + $snapshots + '"') 'release-ui'
foreach ($page in 0..3) { if (-not (Test-Path -LiteralPath (Join-Path $snapshots "page-$page.png"))) { throw "Missing page $page screenshot" } }
Get-Content -LiteralPath (Join-Path $snapshots 'tray-smoke.txt')

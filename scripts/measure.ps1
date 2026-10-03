$ErrorActionPreference = 'Stop'
$workspace = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $workspace 'dist\NoMoreDeePeeEye\NoMoreDeePeeEye.exe'
if (Get-Process no-more-dee-pee-eye -ErrorAction SilentlyContinue) { throw 'Close the existing No More Dee Pee Eye instance before measuring.' }
$appProcess = Start-Process -FilePath $exe -ArgumentList '--start-hidden' -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Seconds 3
    $appProcess.Refresh()
    if ($appProcess.HasExited) { throw 'No More Dee Pee Eye exited during hidden startup; measurements are invalid.' }
    $appProcess = Get-Process -Id $appProcess.Id
    $hiddenMemory = $appProcess.WorkingSet64
    $cpuStart = $appProcess.TotalProcessorTime.TotalSeconds
    $watch = [Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds 10
    $appProcess.Refresh()
    if ($appProcess.HasExited) { throw 'No More Dee Pee Eye exited during the CPU sample.' }
    $watch.Stop()
    $cpu = ($appProcess.TotalProcessorTime.TotalSeconds - $cpuStart) / $watch.Elapsed.TotalSeconds * 100
    # The second invocation signals the existing instance to show its window.
    $opener = Start-Process -FilePath $exe -WindowStyle Hidden -PassThru
    $opener.WaitForExit(5000) | Out-Null
    Start-Sleep -Seconds 2
    $appProcess.Refresh()
    if ($appProcess.HasExited) { throw 'No More Dee Pee Eye exited while reopening the window.' }
    if ($appProcess.WorkingSet64 -le 0) { throw 'The working-set measurement is invalid.' }
    $result = [ordered]@{
        measuredAt = (Get-Date).ToString('o')
        configuration = 'Release; Zapret2 stopped; software renderer; 1000x710 window'
        hiddenWorkingSetMiB = [math]::Round($hiddenMemory / 1MB, 2)
        visibleWorkingSetMiB = [math]::Round($appProcess.WorkingSet64 / 1MB, 2)
        privateMemoryMiB = [math]::Round($appProcess.PrivateMemorySize64 / 1MB, 2)
        hiddenCpuPercentOfOneLogicalCore = [math]::Round($cpu, 3)
        sampleSeconds = [math]::Round($watch.Elapsed.TotalSeconds, 2)
        executableMiB = [math]::Round((Get-Item -LiteralPath $exe).Length / 1MB, 2)
        zipMiB = [math]::Round((Get-Item -LiteralPath (Join-Path $workspace 'dist\NoMoreDeePeeEye-0.1.1-windows-x64.zip')).Length / 1MB, 2)
    }
    New-Item -ItemType Directory -Force -Path (Join-Path $workspace 'artifacts') | Out-Null
    $result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $workspace 'artifacts\performance.json') -Encoding utf8
    $result | ConvertTo-Json
} finally {
    $stopper = Start-Process -FilePath $exe -ArgumentList '--quit' -WindowStyle Hidden -PassThru
    $stopper.WaitForExit(5000) | Out-Null
    if (-not $appProcess.WaitForExit(5000)) { throw 'No More Dee Pee Eye did not exit after --quit.' }
}

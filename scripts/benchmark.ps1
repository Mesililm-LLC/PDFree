param(
    [string]$Executable = '',
    [string]$Document = 'tests/corpus/office.pdf',
    [ValidateSet('Warm', 'Cold')][string]$Mode = 'Warm',
    [int]$Trials = 20,
    [string]$Output = 'benchmarks/results/warm-office.json',
    [string]$ColdTrialId = '',
    [switch]$Empty
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
if (!$Executable) { $Executable = Join-Path $root 'target/release/pdfree.exe' }
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$Document = (Resolve-Path -LiteralPath $Document).Path
if ($Trials -lt 1 -or $Trials -gt 100) { throw 'Trials must be between 1 and 100.' }
if ($Mode -eq 'Cold' -and ($Trials -ne 1 -or !$ColdTrialId)) { throw 'Cold trials require -Trials 1 and a unique -ColdTrialId after a fresh reboot.' }
$outputPath = [IO.Path]::GetFullPath((Join-Path $root $Output))
$null = New-Item -ItemType Directory -Force -Path (Split-Path -Parent $outputPath)
$runRoot = Join-Path $root ('benchmarks/runs/' + [Guid]::NewGuid().ToString())
$null = New-Item -ItemType Directory -Force -Path $runRoot
$runs = @()
for ($trial = 0; $trial -lt $Trials; $trial++) {
    $trace = Join-Path $runRoot "$trial.jsonl"
    $stderrPath = Join-Path $runRoot "$trial.stderr.txt"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $processArgs = @('--benchmark-ui', '--trace', ('"' + $trace + '"'))
    if (!$Empty) { $processArgs = @('"' + $Document + '"') + $processArgs }
    $process = Start-Process -FilePath $Executable -ArgumentList $processArgs -PassThru -WindowStyle Hidden -RedirectStandardError $stderrPath
    $peakWorkingSet = 0L
    $peakPrivate = 0L
    $lastWorkingSet = 0L
    $cpuMilliseconds = 0.0
    while (!$process.HasExited) {
        $process.Refresh()
        if (!$process.HasExited) {
            $peakWorkingSet = [Math]::Max($peakWorkingSet, $process.WorkingSet64)
            $peakPrivate = [Math]::Max($peakPrivate, $process.PrivateMemorySize64)
            $lastWorkingSet = $process.WorkingSet64
            $cpuMilliseconds = $process.TotalProcessorTime.TotalMilliseconds
        }
        if ($watch.Elapsed.TotalSeconds -gt 30) {
            $process.Kill()
            throw "Benchmark trial $trial timed out. See $trace and $stderrPath"
        }
        Start-Sleep -Milliseconds 25
    }
    $process.WaitForExit()
    $watch.Stop()
    if (!(Test-Path -LiteralPath $trace)) { throw "Trial $trial produced no trace (exit $($process.ExitCode)). See $stderrPath" }
    $events = @(Get-Content -LiteralPath $trace | ForEach-Object { $_ | ConvertFrom-Json })
    $first = $events | Where-Object event -eq 'first_page_pixels_published' | Select-Object -First 1
    $window = $events | Where-Object event -eq 'window_show_requested' | Select-Object -First 1
    $complete = $events | Where-Object event -eq 'benchmark_complete' | Select-Object -First 1
    if ((!$Empty -and !$first) -or !$complete -or $process.ExitCode -ne 0) { throw "Trial $trial failed. See $trace and $stderrPath" }
    $frames = @($events | Where-Object event -eq 'frame_rendered')
    $frameGaps = @()
    for ($i = 1; $i -lt $frames.Count; $i++) { $frameGaps += $frames[$i].elapsed_ms - $frames[$i-1].elapsed_ms }
    $renders = @($events | Where-Object event -eq 'tile_available_to_ui' | ForEach-Object { $_.data.render_ms })
    $runs += [ordered]@{
        trial = $trial + 1; mode = $Mode; cold_trial_id = $ColdTrialId;
        process_elapsed_ms = $watch.Elapsed.TotalMilliseconds;
        window_show_request_ms = $window.elapsed_ms;
        first_pixels_from_main_ms = $first.elapsed_ms;
        open_to_pixels_ms = $first.data.open_to_pixels_ms;
        peak_working_set_bytes = $peakWorkingSet;
        peak_private_bytes = $peakPrivate;
        last_sample_working_set_bytes = $lastWorkingSet;
        sampled_cpu_ms = $cpuMilliseconds;
        empty_window = [bool]$Empty;
        raster_cache_bytes = $complete.data.cache_bytes;
        cache_hits = $complete.data.cache_hits;
        cache_misses = $complete.data.cache_misses;
        tile_render_ms = $renders;
        rendered_frame_gaps_ms = $frameGaps;
        trace_file = $trace.Substring($root.Length + 1).Replace('\','/');
    }
    Write-Host "Trial $($trial+1)/${Trials}: first pixels $([Math]::Round($first.data.open_to_pixels_ms, 1)) ms, peak working set $([Math]::Round($peakWorkingSet / 1MB, 1)) MiB"
}
$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$gpu = @(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion)
$report = [ordered]@{
    captured_utc = [DateTime]::UtcNow.ToString('o');
    executable_sha256 = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash;
    window_style_requested = 'Hidden';
    document = [IO.Path]::GetRelativePath($root, $Document).Replace('\','/');
    document_sha256 = (Get-FileHash -LiteralPath $Document -Algorithm SHA256).Hash;
    renderer_requested = $(if ($env:SLINT_BACKEND) { $env:SLINT_BACKEND } else { 'winit default (FemtoVG with software fallback)' });
    machine = @{ os = $os.Caption; build = $os.BuildNumber; last_boot_utc = $os.LastBootUpTime.ToUniversalTime().ToString('o'); cpu = $cpu.Name; logical_processors = $cpu.NumberOfLogicalProcessors; ram_bytes = $os.TotalVisibleMemorySize * 1024; gpu = $gpu };
    limitations = @('First pixels means UI image publication, not verified physical display presentation.', 'Frame gaps include idle periods and do not measure scroll frame-time percentiles.', 'Peak memory is sampled at 25 ms; raster cache excludes Slint images and native allocations.', 'Warm means filesystem/driver caches were not cleared; process initialization is repeated.', 'No cold classification without an explicit post-reboot single trial.');
    runs = $runs;
}
$report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $outputPath -Encoding utf8
Write-Host "Saved $outputPath"

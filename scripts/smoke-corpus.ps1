param([string]$Executable = 'dist/PDFree/pdfree.exe')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$previousRenderer = $env:SLINT_BACKEND
$previousCapture = $env:PDFREE_CAPTURE_DIR
$results = @()
Add-Type -AssemblyName System.Drawing
$captureRoot = Join-Path $root ('benchmarks/captures/corpus/' + [Guid]::NewGuid().ToString())
try {
    $env:SLINT_BACKEND = 'winit-software'
    foreach ($name in @('w3c-dummy','tracemonkey','transparency','vector','scan','mixed','thousand')) {
        $folder = Join-Path $captureRoot $name
        $null = New-Item -ItemType Directory -Force -Path $folder
        $env:PDFREE_CAPTURE_DIR = $folder
        $trace = Join-Path $folder 'trace.jsonl'
        $document = Join-Path $root ('tests/corpus/' + $name + '.pdf')
        $processArgs = @(('"' + $document + '"'), '--benchmark-ui', '--trace', ('"' + $trace + '"'))
        $process = Start-Process -FilePath $Executable -ArgumentList $processArgs -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $folder 'stderr.txt')
        if (!$process.WaitForExit(30000)) { $process.Kill(); throw "$name smoke run timed out." }
        $events = @(Get-Content -LiteralPath $trace | ForEach-Object { $_ | ConvertFrom-Json })
        $firstCapture = $events | Where-Object { $_.event -eq 'snapshot' -and $_.data.stage -eq 'first-page' -and $_.data.success }
        if ($process.ExitCode -ne 0 -or !($events | Where-Object event -eq 'benchmark_complete') -or !$firstCapture) { throw "$name smoke run failed." }
        foreach ($stage in @('first-page','after-zoom')) {
            $imagePath = Join-Path $folder ($stage + '.png')
            if (!(Test-Path -LiteralPath $imagePath)) { throw "$name missing current $stage capture." }
            $bitmap = [Drawing.Bitmap]::new($imagePath)
            try {
                if ($bitmap.Width -lt 640 -or $bitmap.Height -lt 400) { throw "$name invalid $stage dimensions." }
                $null = $bitmap.GetPixel(0,0)
            } finally { $bitmap.Dispose() }
        }
        $results += @{ document = $name; document_sha256 = (Get-FileHash -LiteralPath $document).Hash; capture_directory = [IO.Path]::GetRelativePath($root,$folder); completed = $true }
        Write-Host "${name}: rendered and captured before/after zoom"
    }
} finally {
    $env:SLINT_BACKEND = $previousRenderer
    $env:PDFREE_CAPTURE_DIR = $previousCapture
}
@{ executable_sha256 = (Get-FileHash -LiteralPath $Executable).Hash; renderer = 'winit-software'; kind = 'functional smoke; not a controlled timing run'; results = $results } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath benchmarks/results/corpus-ui-smoke.json -Encoding utf8

# Run the recompiled build with the diagnostics on, logging everything to run.log.
#   powershell -ExecutionPolicy Bypass -File tools\run.ps1
# Extra Kirikae switches can be passed as NAME=VALUE pairs:
#   powershell -ExecutionPolicy Bypass -File tools\run.ps1 KIRIKAE_TRACE_IPC=1

param([string[]]$Extra = @())

$ErrorActionPreference = 'Continue'
$Root = Split-Path -Parent $PSScriptRoot
$Exe = Join-Path $Root 'build\cruisn-blast.exe'
$Log = Join-Path $Root 'run.log'
if (-not (Test-Path $Exe)) { Write-Host "No build at $Exe - run tools\build_windows.ps1 first" -ForegroundColor Red; exit 1 }

$env:KIRIKAE_TRACE_EXIT = '1'   # guest backtrace if the game quits
$env:KIRIKAE_STATS = '1'        # fps, recompiled vs interpreted calls
foreach ($kv in $Extra + $args) {
    $name, $value = $kv -split '=', 2
    Set-Item -Path "env:$name" -Value $value
    Write-Host "set $name=$value"
}

Write-Host "Starting $Exe (log: $Log)"
Push-Location (Split-Path $Exe)
try {
    & $Exe 2>&1 | ForEach-Object { "$_" } | Tee-Object -FilePath $Log
    Write-Host "`nexit code $LASTEXITCODE - send me run.log" -ForegroundColor Cyan
} finally { Pop-Location }

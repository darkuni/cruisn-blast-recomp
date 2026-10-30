# Build Cruis'n Blast natively on Windows with patched Kirikae.
#
# Run from the project folder (the one holding extracted_v2\ and tools\):
#   powershell -ExecutionPolicy Bypass -File tools\build_windows.ps1
#   powershell -ExecutionPolicy Bypass -File tools\build_windows.ps1 -Relink   (after runtime-only patches)
#
# Needs: git, Rust (rustup), Visual Studio Build Tools with the C++ desktop workload,
# a Vulkan 1.3 GPU. The script checks for each and stops with instructions if one is missing.

param(
    [string]$Root = (Split-Path -Parent $PSScriptRoot),
    [int]$Jobs = 0,
    # Only the runtime or shader translator changed: rebuild them and relink the game
    # without regenerating its code. Needs a previous full build.
    [switch]$Relink,
    # Keep guest CPU registers in memory between instructions (the slower code generation used
    # before guest registers were kept in host registers). For comparing; off by default.
    [switch]$SlowRegs
)

# Continue, not Stop: Windows PowerShell 5.1 turns native stderr (Kirikae's progress
# lines) into terminating errors under Stop. Exit codes are checked explicitly instead.
$ErrorActionPreference = 'Continue'
$Tools = Join-Path $Root 'tools'
$Patches = Join-Path $Tools 'kirikae-patches'
if (-not (Test-Path $Patches)) { $Patches = Join-Path $Root 'patches\kirikae' }   # repo checkout layout
$Kirikae = Join-Path $Root 'kirikae'
$Target = 'C:\kt'          # short CARGO_TARGET_DIR; long paths break some MSVC builds
$Dump = Join-Path $Root 'extracted_v2'
$Out = Join-Path $Root 'build'
$SeedProfile = Join-Path $Tools 'cruisn-blast.profile'

function Step($msg) { Write-Host "`n== $msg" -ForegroundColor Cyan }
function Fail($msg) { Write-Host "`nSTOP: $msg" -ForegroundColor Red; exit 1 }

# Run a native command through cmd so its stderr comes back as plain text (PowerShell 5.1
# wraps redirected stderr in red error records) and log it as UTF-8.
function Run-Logged([string]$exe, [string[]]$argv, [string]$log) {
    $line = '"' + $exe + '" ' + (($argv | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' ')
    $lines = New-Object System.Collections.Generic.List[string]
    cmd /s /c "`"$line 2>&1`"" | ForEach-Object { Write-Host $_; $lines.Add($_) }
    $code = $LASTEXITCODE
    [IO.File]::WriteAllLines($log, $lines, (New-Object Text.UTF8Encoding $false))
    return $code
}

Step 'Checking prerequisites'
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Fail 'git not found. Install it:  winget install --id Git.Git -e'
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Fail 'Rust not found. Install it:  winget install --id Rustlang.Rustup -e   then open a NEW terminal and rerun.'
}
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vc = $null
if (Test-Path $vswhere) {
    $vc = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $vc) {
    Fail ('MSVC C++ build tools not found. Install them:' +
        "`n  winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override `"--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended`"" +
        "`nthen rerun this script.")
}
foreach ($p in @($Dump, (Join-Path $Dump 'exefs\main'), (Join-Path $Dump 'romfs\Data'), $SeedProfile,
                 (Join-Path $Patches 'BASE_COMMIT'))) {
    if (-not (Test-Path $p)) { Fail "missing $p" }
}
Write-Host "git, cargo ($(cargo --version)), MSVC at $vc - OK"

Step 'Getting Kirikae'
$base = (Get-Content (Join-Path $Patches 'BASE_COMMIT')).Trim()
if (-not (Test-Path (Join-Path $Kirikae '.git'))) {
    git clone -c core.autocrlf=false https://github.com/Cor3Duo/kirikae $Kirikae
    if ($LASTEXITCODE) { Fail 'git clone failed' }
}
Push-Location $Kirikae
try {
    git config core.autocrlf false   # LF checkout so the patches apply
    git fetch --quiet origin
    git checkout --quiet --force $base
    git reset --quiet --hard $base
    git clean -fdq   # files that earlier patch runs added (patches create them fresh)
    foreach ($patch in Get-ChildItem $Patches -Filter '*.patch' | Sort-Object Name) {
        Write-Host "applying $($patch.Name)"
        git apply --whitespace=nowarn $patch.FullName
        if ($LASTEXITCODE) { Fail "patch $($patch.Name) did not apply" }
    }

    Step 'Building Kirikae (first time takes a few minutes)'
    $env:CARGO_TARGET_DIR = $Target
    $env:RUST_MIN_STACK = '268435456'   # rustc's LLVM recurses deep on the vp9 decoder crate
    cargo build --release -p kirikae
    if ($LASTEXITCODE) { Fail 'Kirikae build failed - send me the error above' }
} finally { Pop-Location }

$sw = [Diagnostics.Stopwatch]::StartNew()
$project = Join-Path $Out 'cruisn-blast-build'
if ($Relink) {
    Step 'Relinking Cruis''n Blast against the patched runtime'
    if (-not (Test-Path (Join-Path $project 'Cargo.toml'))) { Fail 'no previous build to relink - run without -Relink first' }
    Push-Location $project
    try { $code = Run-Logged 'cargo' @('build', '--release') (Join-Path $Root 'build.log') } finally { Pop-Location }
    if ($code) { Fail 'relink failed - build.log has the details' }
    Copy-Item (Join-Path $Target 'release\cruisn-blast.exe') (Join-Path $Out 'cruisn-blast.exe') -Force
} else {
    Step 'Recompiling Cruis''n Blast'
    $k = Join-Path $Target 'release\kirikae.exe'
    # Seed profile (IL2CPP method entries) plus whatever previous runs had to interpret,
    # which the game writes next to the exe.
    $runProfile = Join-Path $Out 'cruisn-blast.profile'
    $merged = Join-Path $Root 'merged.profile'
    $all = @(Get-Content $SeedProfile)
    if (Test-Path $runProfile) {
        $all += Get-Content $runProfile
        Write-Host "merging $((Get-Content $runProfile | Where-Object { $_ -notmatch '^#' }).Count) interpreted addresses from previous runs"
    }
    [IO.File]::WriteAllLines($merged, [string[]]$all, (New-Object Text.UTF8Encoding $false))
    $kargs = @('build', $Dump, '-o', $Out, '--name', 'cruisn-blast', '--profile', $merged)
    if (-not $SlowRegs) { $kargs += '--fast-regs' }   # guest registers stay in host registers within a function
    if ($Jobs -gt 0) { $kargs += @('-j', "$Jobs") }
    $code = Run-Logged $k $kargs (Join-Path $Root 'build.log')
    if ($code) { Fail 'recompile failed - build.log has the details' }
}
Step 'Shader programs for startup compilation'
# Shaders the game is known to use: compiled behind the start screen, the rest in the background.
$usedSeed = Join-Path $Tools 'cruisn-blast.shaders-used'
$used = Join-Path $Out 'cruisn-blast.shaders-used'
if ((Test-Path $usedSeed) -and -not (Test-Path $used)) { Copy-Item $usedSeed $used }
$shaders = Join-Path $Out 'cruisn-blast.shaders'
if (Test-Path $shaders) {
    Write-Host "already there: $shaders"
} else {
    $py = Get-Command py -ErrorAction SilentlyContinue
    if (-not $py) { $py = Get-Command python -ErrorAction SilentlyContinue }
    if ($py) {
        & $py.Source -m pip install --quiet UnityPy lz4
        & $py.Source (Join-Path $Tools 'extract_shaders.py') (Join-Path $Dump 'romfs\Data') $shaders
    }
    if (-not (Test-Path $shaders)) {
        Write-Host ('No shader file: the game will compile shaders while it runs (stutter on first sight).' +
            "`nInstall Python (winget install --id Python.Python.3.12 -e) and rerun, or run tools\extract_shaders.py yourself.") -ForegroundColor Yellow
    }
}
Write-Host ("`nDone in {0:N1} min: {1}" -f $sw.Elapsed.TotalMinutes, (Join-Path $Out 'cruisn-blast.exe')) -ForegroundColor Green
Write-Host 'Run it with:  powershell -ExecutionPolicy Bypass -File tools\run.ps1'
Write-Host 'Package it for testers with:  powershell -ExecutionPolicy Bypass -File tools\package.ps1'

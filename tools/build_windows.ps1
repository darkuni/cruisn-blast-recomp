# Build Cruis'n Blast natively on Windows with patched Kirikae.
#
# Run from the project folder (the one holding extracted_v2\ and tools\):
#   powershell -ExecutionPolicy Bypass -File tools\build_windows.ps1
#
# Needs: git, Rust (rustup), Visual Studio Build Tools with the C++ desktop workload,
# a Vulkan 1.3 GPU. The script checks for each and stops with instructions if one is missing.

param(
    [string]$Root = (Split-Path -Parent $PSScriptRoot),
    [int]$Jobs = 0
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

Step 'Recompiling Cruis''n Blast'
$k = Join-Path $Target 'release\kirikae.exe'
$kargs = @('build', $Dump, '-o', $Out, '--name', 'cruisn-blast', '--profile', $SeedProfile)
if ($Jobs -gt 0) { $kargs += @('-j', "$Jobs") }
$sw = [Diagnostics.Stopwatch]::StartNew()
& $k @kargs 2>&1 | Tee-Object -FilePath (Join-Path $Root 'build.log')
if ($LASTEXITCODE) { Fail 'recompile failed - build.log has the details' }
Write-Host ("`nDone in {0:N1} min: {1}" -f $sw.Elapsed.TotalMinutes, (Join-Path $Out 'cruisn-blast.exe')) -ForegroundColor Green
Write-Host 'Run it with:  powershell -ExecutionPolicy Bypass -File tools\run.ps1'

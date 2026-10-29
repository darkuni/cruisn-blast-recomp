# Package the native PC build for testers, without any of the game in it.
#
# Run after tools\build_windows.ps1 (full build or -Relink), from the project folder:
#   powershell -ExecutionPolicy Bypass -File tools\package.ps1
#
# Makes dist\CruisnBlast\ and dist\CruisnBlast-<date>.zip holding:
#   cruisn-blast.exe            the recompiled game code and runtime (no console window). It does
#                               not contain the game's files: it reads them from game\ next to it
#   setup.exe                   makes game\ and the shader file from the player's own NSPs + keys
#                               (tools\setup, Rust; no Python needed)
#   cruisn-blast.shaders-used   which shaders to compile behind the start screen (hashes only)
#   README.txt, THIRD_PARTY_NOTICES.txt

param(
    [string]$Root = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = 'Continue'
$Tools = Join-Path $Root 'tools'
$Target = 'C:\kt'
$Project = Join-Path $Root 'build\cruisn-blast-build'
$Kirikae = Join-Path $Target 'release\kirikae.exe'
$Dist = Join-Path $Root 'dist'
$Out = Join-Path $Dist 'CruisnBlast'

function Step($msg) { Write-Host "`n== $msg" -ForegroundColor Cyan }
function Fail($msg) { Write-Host "`nSTOP: $msg" -ForegroundColor Red; exit 1 }

foreach ($p in @((Join-Path $Project 'Cargo.toml'), (Join-Path $Project 'src\package.bin'), (Join-Path $Project 'objects.txt'), $Kirikae)) {
    if (-not (Test-Path $p)) { Fail "missing $p - run tools\build_windows.ps1 first" }
}

# Only these files are packaged. Anything else in dist\CruisnBlast (your NSPs, game\, logs from
# trying it) is left alone and never goes into the zip.
$Files = @('cruisn-blast.exe', 'setup.exe', 'cruisn-blast.shaders-used', 'README.txt', 'THIRD_PARTY_NOTICES.txt')
New-Item -ItemType Directory -Force $Out | Out-Null
foreach ($f in $Files) { Remove-Item (Join-Path $Out $f) -Force -ErrorAction SilentlyContinue }

Step 'Linking the distribution exe (game files read from game\ beside it, no console)'
$pkg = Join-Path $Project 'src\package.bin'
$objs = Join-Path $Project 'objects.txt'
$pkgSave = "$pkg.dev"
$objsSave = "$objs.dev"
Copy-Item $pkg $pkgSave -Force
Copy-Item $objs $objsSave -Force
$env:CARGO_TARGET_DIR = $Target
$ok = $false
try {
    & $Kirikae dev external-package $pkgSave $pkg
    if ($LASTEXITCODE) { throw 'kirikae dev external-package failed' }
    # Windows (GUI) subsystem: no console window behind the game. The log goes to cruisn-blast.log.
    $lines = @(Get-Content $objsSave) + @('/SUBSYSTEM:WINDOWS', '/ENTRY:mainCRTStartup')
    [IO.File]::WriteAllLines($objs, [string[]]$lines, (New-Object Text.UTF8Encoding $false))
    Push-Location $Project
    try { cargo build --release } finally { Pop-Location }
    if ($LASTEXITCODE) { throw 'cargo build failed' }
    Copy-Item (Join-Path $Target 'release\cruisn-blast.exe') (Join-Path $Out 'cruisn-blast.exe') -Force
    $ok = $true
} catch {
    Write-Host $_ -ForegroundColor Red
} finally {
    # Back to the development build (embedded game, console), with fresh timestamps so cargo relinks.
    Move-Item $pkgSave $pkg -Force
    Move-Item $objsSave $objs -Force
    (Get-Item $pkg).LastWriteTime = Get-Date
    (Get-Item $objs).LastWriteTime = Get-Date
    Push-Location $Project
    try { cargo build --release } finally { Pop-Location }
    if (-not $LASTEXITCODE) {
        Copy-Item (Join-Path $Target 'release\cruisn-blast.exe') (Join-Path $Root 'build\cruisn-blast.exe') -Force
    }
}
if (-not $ok) { Fail 'the distribution exe was not built - see the error above' }

Step 'Building setup.exe'
Push-Location (Join-Path $Tools 'setup')
try { cargo build --release } finally { Pop-Location }
if ($LASTEXITCODE) { Fail 'setup.exe build failed - see the error above' }
Copy-Item (Join-Path $Target 'release\setup.exe') (Join-Path $Out 'setup.exe') -Force

Step 'Readme and notices'
# The list grows as the game is played: take the one next to the development build if there is one.
$used = Join-Path $Root 'build\cruisn-blast.shaders-used'
if (-not (Test-Path $used)) { $used = Join-Path $Tools 'cruisn-blast.shaders-used' }
Copy-Item $used (Join-Path $Out 'cruisn-blast.shaders-used')
Copy-Item (Join-Path $Tools 'README-testers.txt') (Join-Path $Out 'README.txt')
$notices = Join-Path $Root 'THIRD_PARTY_NOTICES.md'
if (Test-Path $notices) { Copy-Item $notices (Join-Path $Out 'THIRD_PARTY_NOTICES.txt') }

Step 'Zip'
$zip = Join-Path $Dist ("CruisnBlast-{0}.zip" -f (Get-Date -Format 'yyyy-MM-dd'))
if (Test-Path $zip) { Remove-Item $zip -Force }
$members = $Files | Where-Object { Test-Path (Join-Path $Out $_) } | ForEach-Object { "CruisnBlast/$_" }
Push-Location $Dist
try { tar.exe -a -c -f $zip @members } finally { Pop-Location }
if ($LASTEXITCODE -or -not (Test-Path $zip)) { Fail 'zip failed' }
$listed = tar.exe -t -f $zip
Write-Host ($listed -join "`n")

$mb = [math]::Round((Get-Item $zip).Length / 1MB)
Write-Host ("`nDone: $zip ($mb MB)") -ForegroundColor Green
Write-Host "To try it yourself: drag your two NSPs and prod.keys onto $Out\setup.exe, then start cruisn-blast.exe there."

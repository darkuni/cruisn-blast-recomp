# Package the native PC build for testers, without any of the game in it.
#
# Run after tools\build_windows.ps1 (full build or -Relink), from the project folder:
#   powershell -ExecutionPolicy Bypass -File tools\package.ps1
#
# Makes dist\CruisnBlast\ and dist\CruisnBlast-<date>.zip holding:
#   cruisn-blast.exe            the recompiled game code and runtime (no console window). It does
#                               not contain the game's files: it reads them from game\ next to it
#   setup.exe                   makes game\ and the shader file from the player's own NSPs + keys
#   cruisn-blast.shaders-used   which shaders to compile behind the start screen (hashes only)
#   README.txt, THIRD_PARTY_NOTICES.txt
#
# Needs Python (winget install --id Python.Python.3.12 -e) to build setup.exe.

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
$py = Get-Command py -ErrorAction SilentlyContinue
if (-not $py) { $py = Get-Command python -ErrorAction SilentlyContinue }
if (-not $py) { Fail 'Python not found (needed for setup.exe): winget install --id Python.Python.3.12 -e' }

if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null

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
$work = Join-Path $Dist 'setup-work'
& $py.Source -m pip install --quiet --upgrade pyinstaller UnityPy lz4 cryptography
if ($LASTEXITCODE) { Fail 'pip install failed' }
Push-Location $Tools
try {
    & $py.Source -m PyInstaller --onefile --console --name setup --collect-all UnityPy --collect-all lz4 `
        --distpath $Out --workpath $work --specpath $work -y (Join-Path $Tools 'setup_game.py')
} finally { Pop-Location }
if (-not (Test-Path (Join-Path $Out 'setup.exe'))) { Fail 'PyInstaller did not produce setup.exe' }
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue

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
Push-Location $Dist
try { tar.exe -a -c -f $zip CruisnBlast } finally { Pop-Location }
if ($LASTEXITCODE -or -not (Test-Path $zip)) { Fail 'zip failed' }

# Nothing from the game may be in the package.
$bad = Get-ChildItem -Recurse $Out | Where-Object { $_.Name -match '\.(shaders|nsp|nca|keys|pipelines|profile)$' -or $_.Name -eq 'romfs.bin' -or $_.Name -eq 'game' }
if ($bad) { Fail ("game-derived files in the package: " + ($bad.Name -join ', ')) }

$mb = [math]::Round((Get-Item $zip).Length / 1MB)
Write-Host ("`nDone: $zip ($mb MB)") -ForegroundColor Green
Write-Host "To try it yourself: drag your two NSPs and prod.keys onto $Out\setup.exe, then start cruisn-blast.exe there."

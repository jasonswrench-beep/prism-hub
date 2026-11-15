# Build a release of Prism Hub: tests, notices, the per-user installer and its SHA-256 file.
# Output: dist\  (Prism-Hub-<version>-setup.exe and SHA256SUMS.txt). Nothing here publishes anything.
#
# -Sign signs the program, the installer's parts, the uninstaller and the setup file with Azure Artifact
# Signing during the build (tools\sign_windows.py; the account names come from the OPENSTAGE_SIGN_*
# environment variables shared with the other apps, and the sign-in from `az login`). Without -Sign the
# build is unsigned, and no hub will install it as an update.
#
# The published catalog (if there is one) goes inside the installer, so a first start without a network
# still lists the apps. It is checked against the hub's key at every start, like any other copy.
param([switch]$Sign)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$brand = Get-Content (Join-Path $root "brand\brand.json") -Raw | ConvertFrom-Json

Write-Host "== Tools"
if (-not (Test-Path node_modules)) { npm install --no-audit --no-fund; if ($LASTEXITCODE -ne 0) { throw "npm install failed." } }

Write-Host "== Brand files"
node tools/apply-brand.mjs
if ($LASTEXITCODE -ne 0) { throw "The brand files could not be written." }
if (-not $brand.catalog_public_key) {
  Write-Warning "brand\brand.json has no catalog_public_key: this hub will refuse every catalog. Run Make Hub Key.bat first."
}

# Build paths would otherwise be written into the program (in panic messages and debug data),
# including the builder's Windows user name. Map them to neutral names.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE ".rustup" }
$env:RUSTFLAGS = "--remap-path-prefix=$($env:USERPROFILE)=user --remap-path-prefix=$rustupHome=rustup --remap-path-prefix=$cargoHome=cargo --remap-path-prefix=$root=src"

Write-Host "== Tests"
cargo test --release --manifest-path src-tauri/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw "Tests failed; release stopped." }

Write-Host "== Third-party notices and license check"
python tools/third_party.py
if ($LASTEXITCODE -ne 0) { throw "A dependency license needs review; release stopped." }

$signScript = Join-Path $root "tools\sign_windows.py"
if ($Sign) {
  Write-Host "== Signing: checking everything it needs"
  python -I $signScript --ready
  if ($LASTEXITCODE -ne 0) { throw "Signing is not ready (see above); nothing was built." }
}

Write-Host "== Catalog for the installer"
$snapshot = Join-Path $root "target\catalog-snapshot"
if (Test-Path $snapshot) { Remove-Item -Recurse -Force $snapshot }
New-Item -ItemType Directory -Force $snapshot | Out-Null
$catalogFile = $brand.catalog_file
# No catalog release yet is normal before the first publish, so gh's complaint is not an error here.
$ErrorActionPreference = "Continue"
& gh release download catalog -R $brand.repository -p $catalogFile -p "$catalogFile.sig" -D $snapshot 2>&1 | Out-Null
$ErrorActionPreference = "Stop"
$haveSnapshot = (Test-Path (Join-Path $snapshot $catalogFile)) -and (Test-Path (Join-Path $snapshot "$catalogFile.sig"))
if ($haveSnapshot) { Write-Host "   the published catalog goes inside the installer" } else { Write-Host "   no published catalog yet: the installer carries none" }

Write-Host "== Installer"
$bundle = [ordered]@{}
if ($haveSnapshot) { $bundle.resources = [ordered]@{ "../LICENSE" = "LICENSE.txt"; "../NOTICE" = "NOTICE.txt"; "../THIRD-PARTY.md" = "THIRD-PARTY.md"; "../target/catalog-snapshot/$catalogFile" = "catalog/$catalogFile"; "../target/catalog-snapshot/$catalogFile.sig" = "catalog/$catalogFile.sig" } }
# Tauri runs the sign command for each file it signs (the program, NSIS plugins, the uninstaller, the setup).
if ($Sign) { $bundle.windows = @{ signCommand = @{ cmd = "python"; args = @("-I", $signScript, "--description", $brand.product_name, "%1") } } }
if ($bundle.Count) {
  $cfg = Join-Path $env:TEMP "prism-hub-build-config.json"
  [IO.File]::WriteAllText($cfg, (@{ bundle = $bundle } | ConvertTo-Json -Depth 6), (New-Object Text.UTF8Encoding $false))
  npx tauri build --config $cfg
} else {
  npx tauri build
}
if ($LASTEXITCODE -ne 0) { throw "Installer build failed." }

$version = (Select-String -Path "src-tauri\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force $dist | Out-Null
$setup = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
$setupName = ($brand.product_name -replace ' ', '-') + "-$version-setup.exe"

# The program must not carry the builder's profile folder (and so their user name).
Write-Host "== Checking the program for personal paths"
$bytes = [IO.File]::ReadAllBytes((Join-Path $root "src-tauri\target\release\$($brand.binary_name).exe"))
$text = [Text.Encoding]::ASCII.GetString($bytes)
$wide = [Text.Encoding]::Unicode.GetString($bytes)
foreach ($needle in @($env:USERPROFILE, $env:USERPROFILE.Replace("\", "/"), "\Users\$($env:USERNAME)\", "/Users/$($env:USERNAME)/")) {
  if ($text.IndexOf($needle, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or $wide.IndexOf($needle, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
    throw "The program file contains the builder's profile path ($needle); nothing was copied to dist."
  }
}
if ($Sign) {
  Write-Host "== Signing: checking the installer"
  $env:OPENSTAGE_SIGN_PUBLISHER = $brand.expected_signer
  python -I $signScript --check $setup.FullName
  if ($LASTEXITCODE -ne 0) { throw "The installer is not signed and timestamped by $($brand.expected_signer); nothing was copied to dist." }
}
Copy-Item $setup.FullName (Join-Path $dist $setupName) -Force

$size = (Get-Item (Join-Path $dist $setupName)).Length
Write-Host ("   installer size: {0:N1} MB (target: under 8 MB)" -f ($size / 1MB))

"{0}  {1}" -f (Get-FileHash (Join-Path $dist $setupName) -Algorithm SHA256).Hash.ToLower(), $setupName | Set-Content (Join-Path $dist "SHA256SUMS.txt") -Encoding ascii
Get-ChildItem $dist -File
if ($Sign) { Write-Host "Done. The program and installer are signed and timestamped." } else { Write-Host "Done. This build is unsigned (for testing only)." }

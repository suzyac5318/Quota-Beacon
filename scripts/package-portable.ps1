$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
$version = (Get-Content VERSION -Raw).Trim()
$identity = Get-Content PRODUCT_LINE.json -Raw | ConvertFrom-Json
if ($identity.productLine -ne 'windows') { throw 'Portable packaging requires the Windows product line.' }
function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
    finally { $stream.Dispose(); $sha.Dispose() }
}

# Always build before packaging: never silently distribute a stale target/release EXE.
& npm.cmd run tauri -- build --no-bundle
if ($LASTEXITCODE -ne 0) { throw 'Windows desktop build failed.' }
$exe = Join-Path $root 'src-tauri/target/release/quota-beacon.exe'
$actualVersion = (Get-Item $exe).VersionInfo.ProductVersion
if ($actualVersion -ne $version) { throw "EXE version $actualVersion does not match VERSION $version." }
$bytes = [IO.File]::ReadAllBytes($exe)
$peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
if ([BitConverter]::ToUInt16($bytes, $peOffset + 4) -ne 0x8664) { throw 'Expected a Windows x64 executable.' }

$stage = Join-Path $root ('work/portable-' + [Guid]::NewGuid().ToString('N'))
$output = Join-Path $root "outputs/windows-v$version"
$archive = Join-Path $output 'quota-beacon-windows-unsigned.zip'
if (Test-Path $archive) { throw "Output already exists; preserve it before repackaging: $archive" }
New-Item -ItemType Directory -Path $stage, $output -Force | Out-Null
Copy-Item -LiteralPath $exe -Destination (Join-Path $stage 'Quota-Beacon.exe')
Copy-Item -LiteralPath LICENSE, UPSTREAM-NOTICE.md -Destination $stage
@"
Quota Beacon Windows Portable v$version (x64, unsigned)

1. Extract the entire ZIP to a writable folder. Do not run inside the ZIP.
2. Sign in to Codex Desktop on this Windows user account.
3. Run Quota-Beacon.exe. No installer or administrator rights are required.

Requires Windows 10/11 x64 and Microsoft Edge WebView2 Runtime.
If WebView2 is missing, install Microsoft's Evergreen Runtime:
https://developer.microsoft.com/microsoft-edge/webview2/
Node.js, Rust and Visual Studio are NOT required on the user's computer.

This unsigned build may trigger SmartScreen. Verify the download source and SHA-256.
Settings, account vault and window state are stored in the Windows user data
directory, not beside the EXE. Removing this folder does not remove those data.
Exit from the system tray before replacing the executable. If startup at login
is enabled, disable it before moving the folder and re-enable it afterwards.
Only Windows 11 has been tested locally; clean-machine testing is separate.
"@ | Set-Content -LiteralPath (Join-Path $stage 'README.txt') -Encoding utf8
$stagedArchive = Join-Path $stage 'quota-beacon-windows-unsigned.zip'
Compress-Archive -LiteralPath (Join-Path $stage 'Quota-Beacon.exe'), (Join-Path $stage 'README.txt'), (Join-Path $stage 'LICENSE'), (Join-Path $stage 'UPSTREAM-NOTICE.md') -DestinationPath $stagedArchive
$verification = Join-Path $stage 'verify'
Expand-Archive -LiteralPath $stagedArchive -DestinationPath $verification
$originalHash = Get-Sha256 $exe
if ((Get-Sha256 (Join-Path $verification 'Quota-Beacon.exe')) -ne $originalHash) {
    throw 'Packaged EXE hash differs from the freshly built executable.'
}
$hash = Get-Sha256 $stagedArchive
Copy-Item -LiteralPath $stagedArchive -Destination $archive
"$hash  quota-beacon-windows-unsigned.zip" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
Write-Output "Portable Windows v$version verified: $archive"
Write-Output "SHA-256: $hash"

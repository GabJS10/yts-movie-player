# Downloads the msedgedriver that matches the installed WebView2 runtime (tauri-driver needs
# the same version) into the folder given as the first argument, and prints the .exe path.
# Usage: pwsh e2e/scripts/get-msedgedriver.ps1 <out-dir>
param([Parameter(Mandatory = $true)][string]$OutDir)
$ErrorActionPreference = 'Stop'

$webview2 = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$keys = @(
  "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$webview2",
  "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$webview2",
  "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$webview2"
)
$version = $null
foreach ($k in $keys) {
  $pv = (Get-ItemProperty -Path $k -Name pv -ErrorAction SilentlyContinue).pv
  if ($pv -and $pv -ne '0.0.0.0') { $version = $pv; break }
}
if (-not $version) { throw 'WebView2 runtime not found in the registry' }
Write-Host "WebView2 $version"

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$zip = Join-Path $OutDir 'edgedriver.zip'
Invoke-WebRequest -Uri "https://msedgedriver.microsoft.com/$version/edgedriver_win64.zip" -OutFile $zip
Expand-Archive -Path $zip -DestinationPath $OutDir -Force
$exe = Join-Path $OutDir 'msedgedriver.exe'
if (-not (Test-Path $exe)) { throw "msedgedriver.exe missing in $OutDir" }
Write-Output $exe

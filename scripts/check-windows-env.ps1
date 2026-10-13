# Probe Windows environment for TrayAsk. Probe only: always exits 0.
# Usage: pwsh scripts/check-windows-env.ps1

$BootstrapperUrl = "https://go.microsoft.com/fwlink/p/?LinkId=2124703"

$Keys = @(
  "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
  "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
)

$Found = $false
foreach ($Key in $Keys) {
  if (Test-Path $Key) {
    $Version = (Get-ItemProperty -Path $Key -Name "pv" -ErrorAction SilentlyContinue).pv
    if ($Version) {
      Write-Output "FOUND: WebView2 Runtime $Version ($Key)"
    } else {
      Write-Output "FOUND: WebView2 registry key present ($Key)"
    }
    $Found = $true
  } else {
    Write-Output "INFO: not present: $Key"
  }
}

if ($Found) {
  Write-Output "OK: WebView2 Runtime detected"
} else {
  Write-Output "MISSING: WebView2 Runtime not detected (download: $BootstrapperUrl)"
}

exit 0

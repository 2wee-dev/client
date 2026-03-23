$ErrorActionPreference = 'Stop'

$repo = "2wee-dev/client"
$bin = "2wee.exe"
$installDir = "$env:LOCALAPPDATA\Programs\2wee"

# Detect architecture
$arch = if ([System.Environment]::Is64BitOperatingSystem) {
    if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x86_64' }
} else {
    Write-Error "32-bit Windows is not supported."
    exit 1
}

$artifact = "2wee-windows-${arch}.exe"
$url = "https://github.com/${repo}/releases/latest/download/${artifact}"

Write-Host "Downloading 2wee for Windows ${arch}..."
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Invoke-WebRequest -Uri $url -OutFile "$installDir\$bin" -UseBasicParsing

# Add to PATH if not already there
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$installDir*") {
    Write-Host "Adding $installDir to PATH..."
    [Environment]::SetEnvironmentVariable("PATH", "$userPath;$installDir", "User")
    $env:PATH += ";$installDir"
}

Write-Host "Done. Run: 2wee https://your-app.com/terminal"
Write-Host "Note: restart your terminal for PATH changes to take effect."

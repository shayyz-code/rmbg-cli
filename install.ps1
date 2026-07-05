$ErrorActionPreference = "Stop"

$Architecture = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($Architecture -ne "AMD64") {
    throw "rmbg installer: only Windows x64 is supported"
}

$Repository = "shayyz-code/rmbg-cli"
$InstallDir = if ($env:RMBG_INSTALL_DIR) { $env:RMBG_INSTALL_DIR } else { Join-Path $HOME ".local\bin" }
$Version = if ($env:RMBG_VERSION) { $env:RMBG_VERSION.TrimStart("v") } else { "latest" }
$BaseUrl = if ($Version -eq "latest") {
    "https://github.com/$Repository/releases/latest/download"
} else {
    "https://github.com/$Repository/releases/download/v$Version"
}

$Archive = "rmbg-x86_64-pc-windows-msvc.zip"
$Temporary = Join-Path ([IO.Path]::GetTempPath()) ("rmbg-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $Temporary | Out-Null

try {
    Invoke-WebRequest "$BaseUrl/$Archive" -OutFile (Join-Path $Temporary $Archive)
    Invoke-WebRequest "$BaseUrl/SHA256SUMS" -OutFile (Join-Path $Temporary "SHA256SUMS")

    $ChecksumLine = Get-Content (Join-Path $Temporary "SHA256SUMS") |
        Where-Object { $_ -match "^[0-9a-fA-F]{64}\s+$([regex]::Escape($Archive))$" } |
        Select-Object -First 1
    if (-not $ChecksumLine) { throw "rmbg installer: checksum for $Archive is missing" }

    $Expected = ($ChecksumLine -split "\s+")[0].ToLowerInvariant()
    $Actual = (Get-FileHash (Join-Path $Temporary $Archive) -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) { throw "rmbg installer: checksum verification failed" }

    Expand-Archive (Join-Path $Temporary $Archive) -DestinationPath $Temporary -Force
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item (Join-Path $Temporary "rmbg.exe") (Join-Path $InstallDir "rmbg.exe") -Force
    Copy-Item (Join-Path $Temporary "uv.exe") (Join-Path $InstallDir "uv.exe") -Force
    Copy-Item (Join-Path $Temporary "THIRD_PARTY_NOTICES.md") (Join-Path $InstallDir "rmbg-THIRD-PARTY-NOTICES.md") -Force
} finally {
    Remove-Item -Recurse -Force $Temporary -ErrorAction SilentlyContinue
}

Write-Host "rmbg installed at $(Join-Path $InstallDir 'rmbg.exe')"
Write-Host "bundled uv installed at $(Join-Path $InstallDir 'uv.exe')"
if (($env:PATH -split ";") -notcontains $InstallDir) {
    Write-Host "Add $InstallDir to PATH before using rmbg."
}
Write-Host "Run 'rmbg setup' to install the local model runtime."

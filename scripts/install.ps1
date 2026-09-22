[CmdletBinding()]
param(
    [string]$Version,
    [string]$InstallDir,
    [string]$Repository = "sircalch/Ostrin",
    [switch]$AddToPath
)

$ErrorActionPreference = "Stop"

function Stop-Installer([string]$Message) {
    throw "ostrinc installer: $Message"
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = if ([string]::IsNullOrWhiteSpace($env:OSTRIN_VERSION)) { "latest" } else { $env:OSTRIN_VERSION }
}
if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $base = if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { $env:USERPROFILE } else { $env:LOCALAPPDATA }
    $InstallDir = Join-Path $base "Ostrin\bin"
}
if (-not $Repository -match "^[^/]+/[^/]+$") {
    Stop-Installer "repository must look like OWNER/REPO"
}

if ($Version -eq "latest") {
    try {
        $release = Invoke-RestMethod -Headers @{ Accept = "application/vnd.github+json" } -Uri "https://api.github.com/repos/$Repository/releases/latest"
        $tag = [string]$release.tag_name
    } catch {
        Stop-Installer "could not resolve the latest published release: $($_.Exception.Message)"
    }
    if ([string]::IsNullOrWhiteSpace($tag)) {
        Stop-Installer "the repository has no published release"
    }
} else {
    $tag = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
}

if ($tag -notmatch "^v[0-9A-Za-z._-]+$") {
    Stop-Installer "version must be a release tag such as v0.1.0"
}

$target = "x86_64-pc-windows-msvc"
$archive = "ostrinc-$tag-$target.zip"
$baseUrl = "https://github.com/$Repository/releases/download/$tag"
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ostrinc-install-" + [guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $temporaryRoot $archive
$checksumPath = "$archivePath.sha256"
$extractRoot = Join-Path $temporaryRoot "extracted"

try {
    New-Item -ItemType Directory -Path $temporaryRoot -Force | Out-Null
    try {
        Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/$archive" -OutFile $archivePath
        Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/$archive.sha256" -OutFile $checksumPath
    } catch {
        Stop-Installer "could not download release $tag; confirm that the tagged release exists: $($_.Exception.Message)"
    }

    $expectedHash = ((Get-Content -Raw -LiteralPath $checksumPath) -split "\s+")[0].ToLowerInvariant()
    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    if ($expectedHash -ne $actualHash) {
        Stop-Installer "checksum verification failed for $archive"
    }

    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractRoot -Force
    $binary = Get-ChildItem -LiteralPath $extractRoot -Filter "ostrinc.exe" -File -Recurse | Select-Object -First 1
    if ($null -eq $binary) {
        Stop-Installer "release archive did not contain ostrinc.exe"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $destination = Join-Path $InstallDir "ostrinc.exe"
    $staged = Join-Path $InstallDir (".ostrinc-$([guid]::NewGuid().ToString('N')).exe")
    Copy-Item -LiteralPath $binary.FullName -Destination $staged -Force
    Move-Item -LiteralPath $staged -Destination $destination -Force

    $expectedVersion = "ostrinc " + $tag.Substring(1)
    $actualVersion = (& $destination --version 2>$null | Out-String).Trim()
    if ($actualVersion -ne $expectedVersion) {
        Stop-Installer "installed compiler reported '$actualVersion', expected '$expectedVersion'"
    }

    if ($AddToPath) {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $parts = @($userPath -split ";" | Where-Object { $_ })
        if ($parts -notcontains $InstallDir) {
            [Environment]::SetEnvironmentVariable("Path", (($parts + $InstallDir) -join ";"), "User")
        }
        $env:Path = "$InstallDir;$env:Path"
        Write-Output "Added $InstallDir to the user PATH."
    }
    Write-Output "Installed $actualVersion to $destination (checksum verified)."
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

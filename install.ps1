<#
.SYNOPSIS
    CodeGraph Windows installer

.DESCRIPTION
    One-click installation for CodeGraph MCP server on Windows.
    Downloads pre-built binary from GitHub releases and configures PATH.

.PARAMETER FromSource
    Build from source instead of downloading pre-built binary

.PARAMETER Force
    Force reinstallation even if already installed

.PARAMETER InstallDir
    Custom installation directory (default: $env:LOCALAPPDATA\Programs\codegraph)

.EXAMPLE
    irm https://raw.githubusercontent.com/nickolay-grechkin/codegraph/main/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -FromSource

.EXAMPLE
    .\install.ps1 -Force -InstallDir "C:\Tools\codegraph"
#>

[CmdletBinding()]
param(
    [switch]$FromSource,
    [switch]$Force,
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\codegraph"
)

$ErrorActionPreference = "Stop"

# Configuration
$Repo = "nickolay-grechkin/codegraph"
$BinaryName = "codegraph.exe"

# Colors for output
function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "White"
    )
    Write-Host $Message -ForegroundColor $Color
}

function Write-Success { param([string]$Message) Write-ColorOutput $Message "Green" }
function Write-Info { param([string]$Message) Write-ColorOutput $Message "Cyan" }
function Write-Warn { param([string]$Message) Write-ColorOutput $Message "Yellow" }
function Write-Err { param([string]$Message) Write-ColorOutput $Message "Red" }

# Banner
function Show-Banner {
    Write-Success ""
    Write-Success "  ____          _       ____                 _     "
    Write-Success " / ___|___   __| | ___ / ___|_ __ __ _ _ __ | |__  "
    Write-Success "| |   / _ \ / _  |/ _ \ |  _| '__/ _  | '_ \| '_ \ "
    Write-Success "| |__| (_) | (_| |  __/ |_| | | | (_| | |_) | | | |"
    Write-Success " \____\___/ \__,_|\___|\____|_|  \__,_| .__/|_| |_|"
    Write-Success "                                       |_|          "
    Write-Success ""
    Write-Info    "  Lightning-fast codebase intelligence MCP server"
    Write-Success ""
}

# Detect architecture
function Get-Architecture {
    $arch = $env:PROCESSOR_ARCHITECTURE
    switch ($arch) {
        "AMD64" { return "x86_64" }
        "ARM64" { return "aarch64" }
        default {
            Write-Err "Unsupported architecture: $arch"
            exit 1
        }
    }
}

# Get latest release version
function Get-LatestVersion {
    try {
        $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -ErrorAction Stop
        return $response.tag_name
    }
    catch {
        Write-Warn "Could not fetch latest version from GitHub API"
        return $null
    }
}

# Cargo bin directory for source installs
$CargoBinDir = "$env:USERPROFILE\.cargo\bin"

# Check if already installed
function Test-Installed {
    $binaryPath = Join-Path $InstallDir $BinaryName
    $cargoBinaryPath = Join-Path $CargoBinDir $BinaryName
    return (Test-Path $binaryPath) -or (Test-Path $cargoBinaryPath)
}

# Download and install binary
function Install-Binary {
    param(
        [string]$Version,
        [string]$Architecture
    )

    $artifactName = "codegraph-${Architecture}-pc-windows-msvc.tar.gz"
    $downloadUrl = "https://github.com/$Repo/releases/download/$Version/$artifactName"
    $tempFile = Join-Path $env:TEMP $artifactName
    $tempExtractDir = Join-Path $env:TEMP "codegraph-extract"

    Write-Info "Downloading CodeGraph $Version for Windows $Architecture..."
    Write-Info "URL: $downloadUrl"

    try {
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -Uri $downloadUrl -OutFile $tempFile -ErrorAction Stop
        $ProgressPreference = 'Continue'
    }
    catch {
        Write-Warn "Pre-built binary not available for this platform"
        Write-Info "Error: $_"
        return $false
    }

    # Create install directory
    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    # Extract archive
    Write-Info "Extracting archive..."
    if (Test-Path $tempExtractDir) {
        Remove-Item -Path $tempExtractDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $tempExtractDir -Force | Out-Null

    # Use tar to extract .tar.gz
    tar -xzf $tempFile -C $tempExtractDir 2>$null

    # Find and install binary
    $extractedBinary = Get-ChildItem -Path $tempExtractDir -Filter $BinaryName -Recurse | Select-Object -First 1
    if ($extractedBinary) {
        $binaryPath = Join-Path $InstallDir $BinaryName
        Move-Item -Path $extractedBinary.FullName -Destination $binaryPath -Force
    }
    else {
        Write-Warn "Binary not found in archive"
        return $false
    }

    # Cleanup
    Remove-Item -Path $tempFile -Force -ErrorAction SilentlyContinue
    Remove-Item -Path $tempExtractDir -Recurse -Force -ErrorAction SilentlyContinue

    Write-Success "Installed CodeGraph to $binaryPath"
    return $true
}

# Check for Visual Studio Build Tools
function Test-MSVCInstalled {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($vsPath) {
            return $true
        }
    }

    try {
        $null = Get-Command cl.exe -ErrorAction Stop
        return $true
    }
    catch {
        return $false
    }
}

# Install from source
function Install-FromSource {
    Write-Info "Installing from source..."

    # Check for cargo
    try {
        $null = Get-Command cargo -ErrorAction Stop
    }
    catch {
        Write-Warn "Rust not found. Installing Rust..."
        Write-Info "Downloading rustup-init.exe..."

        $rustupUrl = "https://win.rustup.rs/x86_64"
        $rustupPath = Join-Path $env:TEMP "rustup-init.exe"

        Invoke-WebRequest -Uri $rustupUrl -OutFile $rustupPath

        Write-Info "Running Rust installer..."
        & $rustupPath -y --default-toolchain stable

        if ($LASTEXITCODE -ne 0) {
            Write-Err "Rust installation failed with exit code $LASTEXITCODE"
            return $false
        }

        # Refresh environment
        $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    }

    # Check for MSVC Build Tools
    if (-not (Test-MSVCInstalled)) {
        Write-Err "Visual Studio Build Tools not found!"
        Write-Warn ""
        Write-Warn "You need Visual Studio Build Tools to compile Rust programs."
        Write-Warn ""
        Write-Warn "Download from: https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022"
        Write-Warn ""
        Write-Warn "In the installer, select 'Desktop development with C++'."
        Write-Warn "After installation, restart your terminal and run this script again."
        return $false
    }

    Write-Info "Building CodeGraph (this may take a few minutes)..."

    cargo install --git "https://github.com/$Repo" --locked --force

    if ($LASTEXITCODE -ne 0) {
        Write-Err "Cargo build failed with exit code $LASTEXITCODE"
        Write-Warn ""
        Write-Warn "Common causes:"
        Write-Warn "  - Missing Visual Studio Build Tools or Windows SDK"
        Write-Warn "  - Run from 'Developer Command Prompt for VS 2022' for proper environment"
        Write-Warn "  - Ensure 'Desktop development with C++' workload is installed"
        return $false
    }

    Write-Success "Installed CodeGraph via cargo"
    return $true
}

# Add to PATH
function Add-ToPath {
    param(
        [switch]$FromSource
    )

    $targetDir = if ($FromSource) { $CargoBinDir } else { $InstallDir }

    # Check if already in PATH
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -like "*$targetDir*") {
        Write-Info "Install directory already in PATH"
        return
    }

    # Add to user PATH
    $newPath = "$userPath;$targetDir"
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")

    # Also update current session
    $env:PATH = "$env:PATH;$targetDir"

    Write-Success "Added $targetDir to PATH"
    Write-Info "Restart your terminal to use 'codegraph' command"
}

# Main installation
function Main {
    Show-Banner

    # Check if already installed
    if ((Test-Installed) -and (-not $Force)) {
        Write-Warn "CodeGraph is already installed"
        Write-Info "Use -Force to reinstall"
        exit 0
    }

    $arch = Get-Architecture
    Write-Info "Detected architecture: $arch"

    $success = $false
    $installedFromSource = $false

    if ($FromSource) {
        $success = Install-FromSource
        $installedFromSource = $success
    }
    else {
        $version = Get-LatestVersion

        if ($version) {
            Write-Info "Latest version: $version"
            $success = Install-Binary -Version $version -Architecture $arch
        }

        if (-not $success) {
            Write-Warn "Falling back to source build..."
            $success = Install-FromSource
            $installedFromSource = $success
        }
    }

    if (-not $success) {
        Write-Err "Installation failed"
        exit 1
    }

    if ($installedFromSource) {
        Add-ToPath -FromSource
    } else {
        Add-ToPath
    }

    Write-Success ""
    Write-Success "Installation complete!"
    Write-Success ""
    Write-Info "Quick start:"
    Write-Info "  1. Restart your terminal"
    Write-Info "  2. Navigate to your project: cd C:\path\to\project"
    Write-Info "  3. Run: codegraph init ."
    Write-Info ""
    Write-Info "This will index your codebase, register the MCP server,"
    Write-Info "install hooks, and generate CLAUDE.md."
    Write-Info ""
    Write-Info "For full documentation:"
    Write-Info "  https://github.com/$Repo"
}

# Run
Main

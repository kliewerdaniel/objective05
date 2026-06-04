# Installation

## Purpose

Design the installation process for Objective such that a non-technical user can install the system in minutes on macOS, Linux, or Windows.

## Scope

This document covers platform-specific installation procedures, system requirements, dependency management, model download, first-run setup, and verification steps.

## Responsibilities

- Define installation procedures for all supported platforms
- Specify system requirements
- Define dependency management strategy
- Document first-run setup
- Specify installation verification

## Assumptions

- Users have administrative access to their machine (or can install user-level software)
- Target systems have internet connectivity for initial installation and model download
- Users may not have technical expertise (installation must be straightforward)
- Package managers (Homebrew, apt, winget) are the preferred distribution mechanism

## Design

### Distribution Methods

| Platform | Primary Method | Secondary Method |
|----------|---------------|------------------|
| macOS | Homebrew (`brew install objective`) | Direct download (.tar.gz) |
| Linux | APT/RPM repository (`apt install objective`) | AppImage / direct binary |
| Windows | Winget (`winget install objective`) | Direct download (.zip) |
| All | Docker image (`docker run objective/objective`) | — |

### System Requirements

**Minimum:**
- CPU: 4 cores, x86_64 (AVX2) or Apple Silicon
- RAM: 16 GB
- Storage: 20 GB free
- OS: macOS 13+ / Ubuntu 22.04+ / Windows 10+ / Debian 12+
- No GPU required (CPU-only mode)

**Recommended:**
- CPU: 8 cores
- RAM: 32 GB
- GPU: 8 GB VRAM (NVIDIA, AMD, or Apple Silicon)
- Storage: 100 GB SSD

**Unsupported:**
- 32-bit architectures
- ARM Linux (except via Docker with emulation)
- Windows on ARM (except via x64 emulation)

### macOS Installation

**Homebrew installation:**

```bash
# Add the Objective tap
brew tap objective/tap

# Install Objective
brew install objective

# Verify installation
objective --version

# Run setup
objective setup
```

**Manual installation:**

```bash
# Download
curl -L -o objective.tar.gz \
  https://github.com/anomalyco/objective/releases/latest/download/objective-macos-amd64.tar.gz

# Extract
tar -xzf objective.tar.gz

# Move to PATH
sudo mv objective /usr/local/bin/

# Verify
objective --version
```

**Apple Silicon note:**
- The binary is universal (x86_64 + arm64)
- Homebrew handles architecture automatically
- Metal GPU acceleration is enabled by default on M-series

### Linux Installation

**APT repository (Ubuntu/Debian):**

```bash
# Add GPG key
curl -fsSL https://objective.ai/apt/gpg.key | sudo gpg --dearmor -o /usr/share/keyrings/objective-archive-keyring.gpg

# Add repository
echo "deb [signed-by=/usr/share/keyrings/objective-archive-keyring.gpg] https://objective.ai/apt stable main" \
  | sudo tee /etc/apt/sources.list.d/objective.list

# Install
sudo apt update && sudo apt install objective

# Verify
objective --version
```

**Manual installation:**

```bash
# Download
curl -L -o objective.tar.gz \
  https://github.com/anomalyco/objective/releases/latest/download/objective-linux-amd64.tar.gz

# Extract and install
tar -xzf objective.tar.gz
sudo mv objective /usr/local/bin/

# Verify
objective --version
```

### Windows Installation

**Winget:**

```powershell
# Install
winget install Objective

# Verify
objective --version
```

**Manual installation:**

```powershell
# Download
Invoke-WebRequest -Uri "https://github.com/anomalyco/objective/releases/latest/download/objective-windows-amd64.zip" -OutFile "objective.zip"

# Extract
Expand-Archive -Path "objective.zip" -DestinationPath "C:\Program Files\Objective"

# Add to PATH (or run from installation directory)
[Environment]::SetEnvironmentVariable("Path", "$env:Path;C:\Program Files\Objective", "User")

# Verify
objective --version
```

### Docker Installation

```bash
# Pull
docker pull objective/objective:latest

# Run
docker run -d \
  --name objective \
  -p 8080:8080 \
  -p 8081:8081 \
  -v objective-data:/var/lib/objective \
  -v objective-models:/var/lib/objective/models \
  objective/objective:latest
```

### First-Run Setup

The `objective setup` command performs initial configuration:

```
$ objective setup

  ╔══════════════════════════════════════════════════╗
  ║         Objective — First-Time Setup            ║
  ╚══════════════════════════════════════════════════╝

  Welcome to Objective! Let's get you set up.

  Step 1/5: System Requirements Check
    ✓ CPU: 8 cores
    ✓ RAM: 32 GB
    ✓ Disk: 200 GB free
    ✓ GPU: Apple M3 (Metal) — 16 GB unified memory
    ✓ Operating System: macOS 14.5

  Step 2/5: Download Models
    This will download approximately 6 GB of model files.
    □ bge-small-en-v1.5 (embedding) — 34 MB
    □ Mistral 7B Instruct (extraction) — 4.1 GB
    □ Mixtral 8x7B Instruct (generation) — 8.0 GB
    □ Piper TTS (voice) — 45 MB

    Downloading [====================>----------] 60%
    Estimated time remaining: 8 minutes
    (Downloads are resumable)

  Step 3/5: Add Your First Source
    Objective needs at least one information source.
    Popular choices:
    ▶ RSS Feed (news websites, blogs)
    ▶ Reddit Subreddit
    ▶ YouTube Channel

    Enter a source URL (or press Enter to skip and configure later):
    > https://feeds.bbci.co.uk/news/rss.xml

    ✓ Source validated: BBC News RSS

  Step 4/5: Configure Broadcast Schedule
    How often should Objective generate broadcasts?
    ▶ Every 2 hours (default)
    ▶ Every 4 hours
    ▶ Every 6 hours
    ▶ Custom schedule

    Selection: Every 2 hours

  Step 5/5: Start Objective
    ✓ Configuration saved to ~/.objective/config/objective.yaml
    ✓ Models downloaded: 3/3
    ✓ Source configured: BBC News RSS

    To start Objective now, run:
      objective start

    To access the dashboard:
      http://localhost:8080

    Thank you for installing Objective!
```

### Post-Installation Verification

```bash
# Check version
objective --version
# Expected: objective 1.0.0 (rev abc1234, built 2026-06-01)

# Check status
objective status
# Expected: System is running. Uptime: 1h 23m. All services healthy.

# Check health
curl http://localhost:8080/api/v1/health
# Expected: {"status": "healthy", "services": {"ingestion": "healthy", ...}}

# Open dashboard
open http://localhost:8080
```

### Uninstallation

```bash
# macOS (Homebrew)
brew uninstall objective

# Linux (APT)
sudo apt remove objective

# Windows (Winget)
winget uninstall Objective

# Clean up data (optional)
rm -rf ~/.objective
```

## Interfaces

- `from-source.md` — building and running Objective from a source checkout
  (current Rust workspace + Vite/React dashboard layout).
- `operations.md` — running and updating Objective
- `docs/architecture/architecture-decisions.md` — single binary distribution (ADR-012)

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Incompatible CPU (no AVX2) | Cannot run | Check at setup; provide clear error with minimum requirements |
| Insufficient RAM | Crash on model load | Check at setup; warn and offer CPU-only model config |
| Disk space insufficient for models | Download fails | Check free space before download; stream download |
| Download interrupted | Partial models | Resume download; verify checksums |
| Missing system dependencies (Linux) | Binary won't run | Static linking; document required libraries |
| Antivirus flags binary | Installation blocked | Code signing; Windows Authenticode |
| Permission denied on data directory | Setup fails | Create directory with user permissions; clear error message |

## Future Extensions

- GUI installer (macOS .dmg, Windows .msi, Linux .deb/.rpm)
- Package manager for all platforms (Snap, Flatpak, Nix)
- Model cache optimization (pre-download popular models)
- Air-gapped installation guide (USB transfer of models)
- Multi-instance orchestration (install Objective across machines)
- Silent/unattended installation for enterprise deployment

# Installation Guide

CodeGraph can be installed on macOS, Linux, and Windows through multiple methods.

## Quick Install

### macOS / Linux (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/nickolay-grechkin/codegraph/main/install.sh | bash
```

### macOS (Homebrew)

```bash
brew install nickolay-grechkin/codegraph/codegraph
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/nickolay-grechkin/codegraph/main/install.ps1 | iex
```

### All platforms (Cargo)

```bash
cargo install --git https://github.com/nickolay-grechkin/codegraph
```

## Platform-Specific Instructions

### macOS

Supported architectures:
- Apple Silicon (aarch64 / M1-M4)
- Intel (x86_64)

**Option 1: Homebrew** (recommended)
```bash
brew install nickolay-grechkin/codegraph/codegraph
```

**Option 2: Install script**
```bash
curl -fsSL https://raw.githubusercontent.com/nickolay-grechkin/codegraph/main/install.sh | bash
```

**Option 3: Binary download**

Download the correct binary for your architecture from the [releases page](https://github.com/nickolay-grechkin/codegraph/releases):
- `codegraph-aarch64-apple-darwin.tar.gz` (Apple Silicon)
- `codegraph-x86_64-apple-darwin.tar.gz` (Intel)

```bash
tar xzf codegraph-*.tar.gz
chmod +x codegraph
mv codegraph ~/.local/bin/
```

**Option 4: From source**
```bash
cargo install --git https://github.com/nickolay-grechkin/codegraph
```

### Linux

Supported architectures:
- x86_64 (64-bit)

**Option 1: Install script** (recommended)
```bash
curl -fsSL https://raw.githubusercontent.com/nickolay-grechkin/codegraph/main/install.sh | bash
```

**Option 2: Binary download**

Download `codegraph-x86_64-unknown-linux-gnu.tar.gz` from the [releases page](https://github.com/nickolay-grechkin/codegraph/releases).

```bash
tar xzf codegraph-x86_64-unknown-linux-gnu.tar.gz
chmod +x codegraph
sudo mv codegraph /usr/local/bin/
```

**Option 3: From source**
```bash
cargo install --git https://github.com/nickolay-grechkin/codegraph
```

### Windows

Supported architectures:
- x86_64 (64-bit)

Requires Windows 10 or later.

**Option 1: PowerShell installer** (recommended)
```powershell
irm https://raw.githubusercontent.com/nickolay-grechkin/codegraph/main/install.ps1 | iex
```

The installer automatically downloads the binary, configures PATH, and detects installed AI editors.

**Option 2: PowerShell installer with options**
```powershell
# Build from source instead of downloading binary
.\install.ps1 -FromSource

# Force reinstall to a custom directory
.\install.ps1 -Force -InstallDir "C:\Tools\codegraph"
```

**Option 3: From source**

Prerequisites:
- Rust 1.75+ (`rustup.rs`)
- Visual Studio Build Tools with "Desktop development with C++"

```bash
cargo install --git https://github.com/nickolay-grechkin/codegraph
```

## Building from Source

Prerequisites:
- Rust 1.75 or later
- C compiler (for tree-sitter and SQLite native compilation)
- On Windows: Visual Studio Build Tools with "Desktop development with C++"

```bash
# Clone the repository
git clone https://github.com/nickolay-grechkin/codegraph.git
cd codegraph

# Full build with embeddings (~45MB binary)
cargo build --release

# Without embeddings (keyword-only search, ~29MB binary)
cargo build --release --no-default-features

# Install to cargo bin
cargo install --path .
```

### Feature Flags

| Feature | Description | Binary Size |
|---------|-------------|-------------|
| `embedding` (default) | Jina v2 Base Code embeddings for semantic search | ~45 MB |
| No features | Keyword-only search via FTS5 | ~29 MB |

```bash
# Build with embeddings (default)
cargo build --release

# Build without embeddings
cargo build --release --no-default-features
```

## Verification

After installation, verify it works:

```bash
codegraph --version
# codegraph 0.2.0
```

Then initialize your first project:

```bash
cd /path/to/your/project
codegraph init .
```

## Editor Configuration

After installing the binary, configure your AI editor:

### Claude Code

CodeGraph auto-configures itself during `codegraph init`. The `.mcp.json` file is written to your project root. No manual configuration needed.

### Claude Desktop

Copy the config template or add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "codegraph",
      "args": ["serve"],
      "env": {}
    }
  }
}
```

### Cursor

Create `.cursor/mcp.json` in your project root:

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "codegraph",
      "args": ["serve"],
      "enabled": true
    }
  }
}
```

### VS Code (GitHub Copilot)

Create `.vscode/mcp.json` in your workspace:

```json
{
  "servers": {
    "codegraph": {
      "command": "codegraph",
      "args": ["serve"]
    }
  }
}
```

Pre-made config templates are available in the `configs/` directory of the repository.

## Updating

### Homebrew
```bash
brew update
brew upgrade codegraph
```

### Cargo
```bash
cargo install --git https://github.com/nickolay-grechkin/codegraph --force
```

### PowerShell
```powershell
.\install.ps1 -Force
```

## Uninstalling

```bash
# Remove the binary
rm ~/.local/bin/codegraph  # or wherever you installed it

# Remove project data (run inside each initialized project)
rm -rf .codegraph/
codegraph git-hooks uninstall

# Remove Claude Code integration files (check before deleting)
rm .mcp.json
rm -rf .claude/
```

### Homebrew
```bash
brew uninstall codegraph
```

## Troubleshooting

### "codegraph: command not found"

Ensure the binary is in your PATH:
```bash
# Check where it's installed
which codegraph

# If using the install script, it should be at:
ls ~/.local/bin/codegraph
```

Add to your shell profile if needed:
```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Build fails with tree-sitter errors

Ensure you have a C compiler installed:
```bash
# macOS
xcode-select --install

# Ubuntu/Debian
sudo apt install build-essential

# Fedora
sudo dnf install gcc
```

### Large binary size

The default build includes the Jina v2 Base Code ONNX model for semantic embeddings. For a smaller binary:
```bash
cargo build --release --no-default-features
```
This produces a ~29MB binary with keyword-only search (FTS5 BM25). Semantic vector search is disabled but all other features work.

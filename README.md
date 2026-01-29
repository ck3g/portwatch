# portwatch

A fast, interactive TUI for monitoring which processes are listening on which ports.

## Features

- **Interactive TUI** with real-time updates
- **Live filtering** by process name, PID, port, or protocol
- **Kill processes** with SIGTERM/SIGKILL from the UI
- **Table view** with sorting and navigation
- **Auto-refresh** or manual update modes
- **Linux & macOS** support with automatic backend detection

## Installation

### From source

```bash
git clone https://github.com/ck3g/portwatch.git
cd portwatch
cargo build --release
./target/release/portwatch
```

Or install directly:

```bash
cargo install --path .
```

## Usage

### Interactive TUI (default)

```bash
portwatch
```

**Keybindings:**
- `↑/↓` or `j/k` - Navigate
- `/` - Filter (search by process, port, etc.)
- `s` - Send signal (SIGTERM/SIGKILL) to selected process
- `?` - Show help
- `Esc` - Clear filter / Cancel action
- `q` - Quit

### CLI Modes

**One-shot table:**
```bash
portwatch --once
```

**Refresh loop (plain text):**
```bash
portwatch --interval 5
```

## Requirements

One of the following tools must be installed:

- **Linux**: `ss` (from `iproute2`, preferred) or `lsof`
- **macOS**: `lsof` (pre-installed)

portwatch will automatically detect and use the available tool.

### Installing requirements

**Debian/Ubuntu:**
```bash
sudo apt install iproute2
```

**Arch Linux:**
```bash
sudo pacman -S iproute2
```

## Platform Support

- **Linux** (tested on Arch, Ubuntu, Debian)
- **macOS** (via lsof)
- **Windows** - Use [WSL2](https://learn.microsoft.com/en-us/windows/wsl/)

## License

MIT

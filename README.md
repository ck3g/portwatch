# portwatch

A fast, interactive TUI for monitoring which processes are listening on which ports.

## Features

- **Interactive TUI** with real-time updates
- **Live filtering** by process name, PID, port, or protocol
- **Send signals** to processes (SIGTERM, SIGKILL, SIGHUP, SIGINT, and more)
- **Table view** with sorting and navigation
- **Run modes** for the interactive TUI, `--once`, and `--interval`
- **Linux-first** support, with macOS support through `lsof`

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
- `s` - Send signal to selected process
- `?` - Show help
- `Esc` - Clear filter / Cancel action
- `q` - Quit

> **Safety:** Sending SIGTERM or SIGKILL terminates the selected process and may require elevated permissions. Review the selected PID and process name before confirming.

### CLI Modes

**One-shot table:**
```bash
portwatch --once
```

**Refresh loop (plain text):**
```bash
portwatch --interval 5
```

## Supported Signals

PortWatch includes a curated set of signals useful for managing network services
and daemons, rather than the full list of POSIX signals:

| Signal | Description |
|--------|-------------|
| SIGTERM | Graceful shutdown (default) |
| SIGKILL | Force kill (cannot be caught) |
| SIGHUP | Hangup — many daemons reload configuration (e.g., nginx, PostgreSQL) |
| SIGINT | Interrupt, equivalent to Ctrl+C |
| SIGQUIT | Quit with core dump, useful for debugging hung services |
| SIGSTOP | Pause (freeze) a process |
| SIGCONT | Resume a paused process |
| SIGUSR1 | User-defined — used by many servers for app-specific actions (e.g., log reopening) |
| SIGUSR2 | User-defined — used by many servers for app-specific actions (e.g., graceful restart) |

Signals like SIGILL, SIGSEGV, SIGBUS, and SIGFPE are intentionally excluded.
These are hardware/OS fault signals not meant for manual process management — sending
them to a process simulates a crash rather than controlling it.

## Requirements

One of the following tools must be installed:

- **Linux**: `lsof` or `ss` (from `iproute2`)
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

- **Linux** (primary platform)
- **macOS** (supported through the `lsof` backend)
- **Windows** - Use [WSL2](https://learn.microsoft.com/en-us/windows/wsl/)

## License

[MIT](LICENSE)

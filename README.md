# project-manager

A keyboard-driven project launcher and its benchmark playground.

The goal is a resident launcher that feels reliable under fast keyboard use:
press a shortcut, type, move the selection, press Enter, open the project — with
no key lost, repeated, or applied to the wrong selection. Several GUI/TUI
frontends are explored side by side, all sharing the same configuration file
(`~/.project-manager.json`).

## Layout

| Path | Description |
| --- | --- |
| `tui-bench/` | **`pm`** — a keyboard-only TUI to quickly register the current directory as a project. |
| `mock-app/tauri-bench/` | Tauri (web + Rust) launcher prototype. |
| `mock-app/appkit-bench/` | Native macOS AppKit launcher prototype. |
| `mock-app/iced-bench/` | Rust `iced` launcher prototype. |
| `mock-app/wails-bench/` | Go + web (Wails) launcher prototype. |
| `mock-app/shared/projects.json` | Sample fixture data shared by the prototypes. |
| `mock-app/tools/` | Build and metrics scripts for the benchmarks. |

## Shared configuration

All frontends read and write a single JSON file at `~/.project-manager.json`.
Each project record looks like:

```json
{
  "id": "manual-1700000000000",
  "name": "my-project",
  "path": "/path/to/my-project",
  "openPaths": [],
  "aliases": ["mp"],
  "tags": ["manual"],
  "language": "Project",
  "lastOpenedAt": ""
}
```

## Installation

To build and install the TUI project manager (`pm`) on your Mac:

```bash
# Clone the repository
git clone <repository_url> project-manager
cd project-manager

# Run the installation script
./install.sh
```

The script will:
- Check and install required runtimes (`rust`, `bun`) using `mise` if available.
- Build the `pm` tool in release mode.
- Install the `pm` binary to `~/.local/bin`.
- Guide you on adding `~/.local/bin` to your `PATH` if it is not already there.

See [`tui-bench/README.md`](tui-bench/README.md) for keybindings and usage details.

> **Note:** `mock-app/shared/projects.json` and
> `mock-app/wails-bench/resources/projects.json` contain synthetic sample data
> (paths under `/Users/example/...`), not real projects.

## License

[MIT](LICENSE)

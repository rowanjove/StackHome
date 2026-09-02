# StackHome

[中文说明](README.md)

StackHome is a local-first Windows desktop application for file indexing, organization, duplicate detection, backup, and restore. It requires no account and uploads no files. Every batch file change is planned and previewed before execution.

![StackHome desktop interface](assets/screenshots/stackhome-overview.png)

## Core capabilities

| Capability | What it does |
| --- | --- |
| File catalog | Scans folders into a local SQLite index and queries by name, path, and file type |
| File organizer | Builds move, copy, rename, tag, or ignore plans from nested AND / OR / NOT rules |
| Duplicate finder | Confirms exact duplicates with file size, a partial pre-hash, and a full BLAKE3 hash |
| Similar images | Finds visually similar images with perceptual hashes and Hamming distance |
| Metadata extraction | Reads file signatures, image dimensions, EXIF/GPS, audio tags, and MP4 metadata |
| Local backup | Supports custom sources, exclusions, capacity checks, incremental snapshots, manifests, ZIP/7Z, and verification |
| Restore and undo | Checks restore conflicts and journals file operations so safe moves and renames can be undone |
| Automation | Provides watch folders, explicitly enabled rule execution, scheduled backups, and a Windows tray mode |

## Safety model

- Scanning, indexing, and planning do not change files.
- Batch operations require a preview and recheck source size and modification time before execution.
- Cross-volume moves use Copy → Verify → Journal → Delete Source. A failed verification leaves the source intact.
- Destination and undo conflicts never overwrite existing files.
- Duplicate cleanup uses the Windows Recycle Bin by default instead of permanent deletion.
- All data stays on the device. There are no accounts, cloud sync, or telemetry uploads.

## Download

Download the current Windows build from [GitHub Releases](https://github.com/rowanjove/stackhome/releases/latest):

- `StackHome-Portable-Windows-x64-v0.6.2.exe` — portable executable.
- `StackHome-Setup-Windows-x64-v0.6.2.exe` — NSIS installer.
- `SHA256SUMS.txt` — checksums for both executables.

StackHome supports Windows 10/11 x64. The current binaries are not commercially code-signed, so Windows SmartScreen may display an unknown-publisher warning.

## Project facts

| Field | Value |
| --- | --- |
| Current version | v0.6.2 |
| Desktop framework | Tauri 2 |
| Frontend | React 19, TypeScript, Vite |
| Backend | Rust |
| Local storage | SQLite WAL for indexes, tasks, rules, plans, and history; file contents are never stored in the database |
| Supported platform | Windows 10/11 x64 |
| License | MIT |

The existing application identifier and data directory remain unchanged for upgrade compatibility. Public branding, window titles, and release files use StackHome.

## Run from source

Install Node.js, Rust, and the Windows prerequisites for Tauri 2, then run:

```bash
npm install
npm run tauri dev
```

Validation and release builds:

```bash
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
npm run build:windows:portable
npm run build:windows:installer
```

## Current boundaries

StackHome does not currently provide PDF/DOCX text extraction, OCR, cloud sync, user accounts, or cross-platform builds. A browser-based Vite preview can validate rendering only; file operations and system integration require the Tauri desktop runtime.

## License

[MIT](LICENSE)

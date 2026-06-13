# WebDAV Backup

A CLI tool for backing up local files and MySQL / PostgreSQL databases via WebDAV.

## Directory Structure

- `rust/` — Rust implementation
- (Reserved) Implementations in other languages

## Features

- Full file backup with `.gitignore`-style exclude rules
- MySQL database backup via `mysqldump`
- PostgreSQL database backup via `pg_dump`
- Multiple WebDAV remote sources, each backup can choose which source to use
- ZIP compression with optional AES-256 password protection
- Three-level configuration inheritance: Global → Source → Backup
- Remote retention policy (keep the latest N backups, 0 means keep all)
- Beautiful console log output with log level support

## Requirements

- Rust 1.75+
- `mysqldump` (required for MySQL backups)
- `pg_dump` (required for PostgreSQL backups)

## Configuration

Copy the example configuration file and edit it:

```bash
cp config.example.json config.json
```

If no configuration file is specified explicitly, the program defaults to `config.json` in the current working directory. A sample configuration is provided at `config.example.json` in the project root.

### Configuration Items

- `global` — Global default settings
  - `zip_password` — Global ZIP password; leave unset for no encryption
  - `retain_count` — Global retention count; `0` means never delete old backups
  - `log_level` — Log level: `trace`, `debug`, `info`, `warn`, `error`
  - `mysqldump_path` — Custom path to `mysqldump`; uses system `mysqldump` if omitted
  - `pg_dump_path` — Custom path to `pg_dump`; uses system `pg_dump` if omitted

- `source` — Array of remote WebDAV sources (multiple sources supported)
  - `name` — Source name, referenced by backups
  - `url`, `username`, `password` — WebDAV connection credentials
  - `sub_dir` — Default remote subdirectory for this source
  - `zip_password` — Overrides the global ZIP password
  - `retain_count` — Overrides the global retention count
  - `proxy` — HTTP/HTTPS proxy URL for this source

- `backup` — Array of backup items (each item must be exactly one of: file / MySQL / PgSQL)
  - `source` — Which remote source to use
  - `sub_dir` — Remote subdirectory for this backup; if omitted, defaults to `"backup"`
  - `zip_password` / `retain_count` — Override source-level settings
  - `file` / `mysql` / `pgsql` — Type-specific settings

## Development Commands

```bash
cd rust
cargo run
```

Or specify a custom configuration file:

```bash
cargo run -- --config /path/to/config.json
```

Run in background mode (logs written to `webdav_backup.log`):

```bash
cargo run -- --background
```

## Build

```bash
cd rust
cargo build --release
```

For static compilation on Windows (no external MSVC runtime dependency), run in PowerShell:

```powershell
$env:RUSTFLAGS='-C target-feature=+crt-static'
cargo build --release
```

## Scheduling

After manual testing, you can add the tool to your system scheduler:

- **Linux**: Add a line to `crontab -e`
- **Windows**: Use Task Scheduler to create a basic task

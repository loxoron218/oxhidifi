# Quickstart

## Prerequisites

- **Rust**: stable toolchain (edition 2024), install via `rustup`
- **GTK4/Libadwaita**: development libraries
  ```bash
  # Fedora
  sudo dnf install gtk4-devel libadwaita-devel

  # Ubuntu/Debian
  sudo apt install libgtk-4-dev libadwaita-1-dev

  # Arch
  sudo pacman -S gtk4 libadwaita
  ```
- **SQLite**: development headers (for `sqlx` compile-time checking)
  ```bash
  # Fedora
  sudo dnf install sqlite-devel

  # Ubuntu/Debian
  sudo apt install libsqlite3-dev

  # Arch
  sudo pacman -S sqlite
  ```

## Build & Run

```bash
# Build (debug)
cargo build

# Build (release with optimizations)
cargo build --release

# Run
cargo run --release
```

## Test

```bash
# Run all tests
cargo test

# Run benchmarks
cargo bench

# Run specific test
cargo test scanner::tests::test_scan_directory
```

## Lint

```bash
# Full lint suite
cargo clippy --fix --allow-dirty --all-targets -- -W clippy::pedantic && cargo fmt
```

## Project Structure

```
src/
├── main.rs              # Entry point
├── app.rs               # Application setup
├── library/             # Scanner, metadata, dedup, watcher
├── storage/             # Database and settings persistence
├── playback/            # Decoder, resampler, output, queue
├── ui/                  # Window, library views, detail pages, player
└── metrics/             # Performance observability
```

## Configuration

- **Library directories**: Add via Settings → Library paths
- **Database**: `$XDG_DATA_HOME/oxhidifi/library.db`
- **Settings**: `$XDG_CONFIG_HOME/oxhidifi/settings.json`
- **Cached artwork**: `$XDG_CACHE_HOME/oxhidifi/artwork/`

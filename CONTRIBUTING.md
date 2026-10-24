# Contributing

Thanks for your interest in Phone Audio.

## Before you start

- The project targets **Windows 11** and **Linux x86_64**.
- Use the Rust toolchain pinned by `rust-toolchain.toml`.
- The phone side requires Android 11+ with USB debugging / wireless debugging;
  no app is installed on the phone.

## Development setup

```bash
# Windows (PowerShell)
powershell -ExecutionPolicy Bypass -File scripts/fetch-windows-deps.ps1
cargo run --release

# Linux
bash scripts/fetch-linux-deps.sh
cargo run --release
```

`scripts/fetch-*` downloads adb and scrcpy into `bin/` (git-ignored). The
Windows bundle additionally overlays the patched scrcpy binary from
`vendor/scrcpy-patched/win64/` so the output-device selector works.

## Required checks

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

For packaging:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1
```

CI runs the same checks on Windows and Linux (`.github/workflows/build.yml`).

## Pull requests

- Keep changes focused; separate UI, engine, and packaging changes when possible.
- Update `README.md` / `TESTING.md` / `CHANGELOG.md` when behaviour changes.
- Do not commit `bin/`, `dist/`, `target/`, downloaded vendor archives, or
  personal settings/screenshots.
- Describe the test environment (OS, phone model, Android version, connection
  type) without posting private device addresses or serial numbers.

## Licensing

By contributing you agree that your changes are licensed under the project's
MIT license. Vendored third-party components keep their original licenses; see
`THIRD_PARTY_LICENSES.md` and `NOTICE`.

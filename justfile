# Spacedrive development commands

# Install JS dependencies and set up native deps + cargo config
setup:
    bun install
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" xtask setup

# Run the daemon (default dev workflow: just dev-daemon + just dev-desktop)
dev-daemon *ARGS:
	powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" run --features ffmpeg,heif --bin sd-daemon {{ARGS}}

# Run the desktop app in dev mode
dev-desktop:
    cd apps/tauri && bun run tauri:dev

# Run the mobile app in dev mode
dev-mobile:
	cd apps/mobile && bun run start

# Run the mobile app on iOS
dev-mobile-ios:
	cd apps/mobile && bun run ios

# Run the mobile app on Android
dev-mobile-android:
	cd apps/mobile && bun run android

# Build the native mobile core
build-mobile:
	powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" xtask build-mobile

# Run the headless server (web UI, no desktop app)
dev-server *ARGS:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" run --bin sd-server {{ARGS}}

# Run all workspace tests
test:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" test --workspace

# Build everything (default members)
build:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" build

# Build in release mode
build-release:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" build --release

# Report / prune Rust target + optional registry cache (Windows PowerShell)
clean-rust:
    powershell -NoProfile -ExecutionPolicy Bypass -File ./clean-rust-cache.ps1

# Drop entire target dir (Windows)
clean-rust-all:
    powershell -NoProfile -ExecutionPolicy Bypass -File ./clean-rust-cache.ps1 -AllTarget

# Format and lint
check:
    cargo fmt --check
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" clippy --workspace

# Format code
fmt:
    cargo fmt

# Regenerate the TypeScript client types from Rust `Type`-deriving structs
generate-types:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" run --bin generate_typescript_types --manifest-path "{{justfile_directory()}}/core/Cargo.toml"

# Fail if committed TS types have drifted from Rust (used in CI)
check-types:
    ./scripts/check-ts-types.sh

# Link SpaceUI packages for local development.
spaceui-link:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -d ../spaceui/packages ]; then
        echo "Error: ../spaceui not found. Clone it adjacent to this repo:"
        echo "  git clone https://github.com/spacedriveapp/spaceui ../spaceui"
        exit 1
    fi
    cd ../spaceui
    bun install && bun run build --filter='@spacedrive/primitives' --filter='@spacedrive/ai' --filter='@spacedrive/forms' --filter='@spacedrive/explorer' --filter='@spacedrive/tokens'
    for pkg in primitives ai forms explorer tokens; do
        cd packages/$pkg && bun link && cd ../..
    done
    cd "{{justfile_directory()}}"
    bun link @spacedrive/primitives @spacedrive/ai @spacedrive/forms @spacedrive/explorer @spacedrive/tokens
    echo "SpaceUI packages linked successfully."

# Unlink SpaceUI packages and restore npm versions.
spaceui-unlink:
    cd packages/interface && bun unlink @spacedrive/primitives @spacedrive/ai @spacedrive/forms @spacedrive/explorer @spacedrive/tokens && bun install

# Run the CLI
cli *ARGS:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/invoke-spacedrive-cargo.ps1" run --bin sd-cli -- {{ARGS}}

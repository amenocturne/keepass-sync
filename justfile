# Default: show available commands
default:
    @just --list

# Build the CLI
build mode="dev":
    @if [ "{{mode}}" = "release" ]; then \
        cargo build --release; \
    else \
        cargo build; \
    fi

# Run the CLI
run *args:
    cargo run -- {{args}}

# Run tests
test *args:
    cargo test {{args}}

# Format source
fmt:
    cargo fmt

# Lint source
lint:
    cargo clippy -- -D warnings

# Build Android app
mobile-build:
    cd mobile/android; ANDROID_HOME="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}" ./gradlew assembleDebug

# Generate local Android release signing key/config
mobile-release-key:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -e mobile/android/keystores/keepass-sync-release.jks || -e mobile/android/keystore.properties ]]; then
        echo "Release key/config already exists. Keep it, or remove mobile/android/keystores/keepass-sync-release.jks and mobile/android/keystore.properties before regenerating."
        exit 1
    fi
    password="$(uuidgen | tr -d '-')"
    mkdir -p mobile/android/keystores
    keytool -genkeypair -v -keystore mobile/android/keystores/keepass-sync-release.jks -storepass "$password" -keypass "$password" -alias keepass-sync-release -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=Keepass Sync, OU=Personal, O=Personal, L=Nowhere, ST=None, C=XX" -noprompt
    printf 'storeFile=keystores/keepass-sync-release.jks\nstorePassword=%s\nkeyAlias=keepass-sync-release\nkeyPassword=%s\n' "$password" "$password" > mobile/android/keystore.properties
    echo "Created mobile/android/keystores/keepass-sync-release.jks and mobile/android/keystore.properties. Back them up; they are required for app updates."

# Build signed Android release APK
mobile-release:
    #!/usr/bin/env bash
    set -euo pipefail
    android_home="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
    ANDROID_HOME="$android_home" mobile/android/gradlew --project-dir mobile/android :app:assembleRelease
    version="${KEEPASS_SYNC_VERSION_NAME:-0.1.0}"
    version="${version#v}"
    apk="mobile/android/dist/keepass-sync-$version.apk"
    signer="$(find "$android_home/build-tools" -type f -name apksigner | sort | tail -n 1)"
    test -n "$signer"
    mkdir -p mobile/android/dist
    cp mobile/android/app/build/outputs/apk/release/app-release.apk "$apk"
    "$signer" verify --verbose "$apk"
    printf 'Release APK: %s\n' "$apk"

# Remove build artifacts
clean:
    cargo clean

# Full reset
reset: clean build

# KeePass Sync

Local-first OSS synchronization for KeePass-compatible `.kdbx` vaults.

KeePass Sync is not a password manager. It coordinates sync around existing
KeePass clients:

- KeePassXC owns desktop password editing and semantic merge.
- KeePassDX or KeePass2Android owns Android password editing.
- This project owns revision-aware transport, locking, manifests, backups, and
  conflict surfacing.

## Setup

```bash
just build
just test
```

## Usage

Filesystem-backed sync commands:

```bash
just run hash ./passwords.kdbx
just run decide --local sha256:... --base sha256:... --remote sha256:...
just run sync \
  --local ./passwords.kdbx \
  --remote-root ./server \
  --state ./device-state.json \
  --device macbook-pro
just run watch \
  --local ./passwords.kdbx \
  --remote-root ./server \
  --state ./device-state.json \
  --device macbook-pro
just run merge-incoming \
  --remote-root ./server \
  --device macbook-pro \
  --password-file ./password.txt
just run manifest read ./manifest.json
just run doctor
```

The remote root uses this layout:

```text
server/
  canonical/passwords.kdbx
  canonical/manifest.json
  incoming/<device-id>/*.kdbx
  backups/*.kdbx
```

`sync` publishes, pulls, or preserves a divergent local database under
`incoming/`. `merge-incoming` runs KeePassXC merge on desktop and republishes the
merged canonical database. `watch` is the Rust daemon substrate intended to be
wrapped by a macOS LaunchAgent.

The Android app is intentionally separate Kotlin code. It should implement the
same manifest/base-revision protocol, but it should not merge KDBX files.

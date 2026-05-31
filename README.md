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

Current foundation commands:

```bash
just run hash ./passwords.kdbx
just run decide --local sha256:... --base sha256:... --remote sha256:...
just run manifest read ./manifest.json
just run doctor
```

Full SSH sync and desktop merge commands are planned in
`projects/software/active/keepass-sync/spec-keepass-sync.md` in the Mirror
vault.

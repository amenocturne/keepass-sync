# KeePass Sync

Rust sync coordinator for KeePass-compatible `.kdbx` vaults. This repo owns
non-mobile implementation: core sync protocol, CLI, transport, and macOS
integration. Android is Kotlin and should stay outside this Rust repo unless a
shared protocol fixture is useful.

## Commands

Run `just` to see all commands. Key ones:

```bash
just build
just test
just lint
just fmt
just run --help
```

## Architecture

- `src/sync.rs` - pure sync decision state machine.
- `src/revision.rs` - SHA-256 revision values.
- `src/manifest.rs` - remote manifest format.
- `src/local_state.rs` - per-device base revision metadata.
- `src/remote_fs.rs` - filesystem-backed remote transaction implementation.
- `src/keepassxc.rs` - KeePassXC CLI adapter for desktop merge.
- `src/main.rs` - CLI boundary.
- `tests/keepass_sync.rs` - integration tests using real `keepassxc-cli`
  databases.

## Key Patterns

- Core sync logic is pure and tested.
- IO stays at CLI/transport boundaries.
- Do not parse or merge KDBX manually; shell out to KeePassXC for semantic
  desktop merge.
- Android transport must never overwrite canonical when remote revision differs
  from the device base revision.
- Filesystem transport is the first concrete backend; SSH/SFTP should preserve
  the same transaction semantics instead of changing sync rules.
- Packaged builds should bundle `keepassxc-cli` under
  `tools/keepassxc/bin/keepassxc-cli`. Do not rely on or overwrite a system
  KeePassXC install.

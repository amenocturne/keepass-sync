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
- `src/main.rs` - CLI boundary.

## Key Patterns

- Core sync logic is pure and tested.
- IO stays at CLI/transport boundaries.
- Do not parse or merge KDBX manually; shell out to KeePassXC for semantic
  desktop merge.
- Android transport must never overwrite canonical when remote revision differs
  from the device base revision.

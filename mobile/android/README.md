# KeePass Sync Android

Transport-only Android client for KeePass Sync.

The app does not parse or merge KDBX files. It only moves bytes according to the
same manifest/base-revision protocol as the Rust sync engine:

- publish local when the remote canonical revision still matches the local base
- pull remote when local is unchanged
- preserve local under `incoming/<device-id>/` when local and remote diverged

Current backend: Android Storage Access Framework folder selected by the user.
This is useful for validating the protocol and UI. Homelab SSH/SFTP transport
should preserve the same sync rules when added.

## Build

```bash
just mobile-build
```

`ANDROID_HOME` defaults to `/opt/homebrew/share/android-commandlinetools`.
Override `ANDROID_HOME` if the SDK lives elsewhere.

## Release APK

Generate a local release key/config once:

```bash
just mobile-release-key
```

Build the signed release APK:

```bash
just mobile-release
```

The generated keystore and `keystore.properties` are ignored by git. Back them
up; future APK updates require the same key.

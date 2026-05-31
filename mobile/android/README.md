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
gradle -p mobile/android :app:assembleDebug
```

Requires an Android SDK, either through `ANDROID_HOME` or
`mobile/android/local.properties` with `sdk.dir=/path/to/sdk`.

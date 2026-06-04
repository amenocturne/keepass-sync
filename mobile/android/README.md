# KeePass Sync Android

Transport-only Android client for KeePass Sync.

The app does not parse or merge KDBX files. It only moves bytes according to the
same manifest/base-revision protocol as the Rust sync engine:

- publish local when the remote canonical revision still matches the local base
- pull remote when local is unchanged
- preserve local under `incoming/<device-id>/` when local and remote diverged

Current backend: bearer-token HTTP against the Rust homelab server. Configure
the app with:

- local KDBX file URI
- sync endpoint, for example `https://passwords.example.internal`
- sync token

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

Release versions follow git tags. Push a semver tag like `v0.1.1` to build and
publish a GitHub release automatically.

Required GitHub secrets:

- `ANDROID_KEYSTORE_BASE64`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

Populate them from the local release key:

```bash
gh secret set ANDROID_KEYSTORE_BASE64 --repo amenocturne/keepass-sync \
  --body "$(base64 -i mobile/android/keystores/keepass-sync-release.jks)"

gh secret set ANDROID_KEYSTORE_PASSWORD --repo amenocturne/keepass-sync \
  --body "$(awk -F= '$1 == "storePassword" { print $2; exit }' mobile/android/keystore.properties)"

gh secret set ANDROID_KEY_ALIAS --repo amenocturne/keepass-sync \
  --body "$(awk -F= '$1 == "keyAlias" { print $2; exit }' mobile/android/keystore.properties)"

gh secret set ANDROID_KEY_PASSWORD --repo amenocturne/keepass-sync \
  --body "$(awk -F= '$1 == "keyPassword" { print $2; exit }' mobile/android/keystore.properties)"
```

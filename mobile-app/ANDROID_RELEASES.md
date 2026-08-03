# Android Releases

Android downloads are published to the repository's GitHub Releases page:

`https://github.com/Tsohnle95/musicPlayer/releases/latest`

## Local Testing

Keep using the existing debug workflow:

```bash
cd mobile-app
./gradlew installDebug
```

The debug variant does not use the release keystore or release secrets.

## Create The Signing Key

Generate the key once and keep the keystore and passwords backed up securely. Losing this key prevents future APKs from updating an installed app.

```bash
keytool -genkeypair -v \
  -keystore luno-release.jks \
  -alias luno \
  -keyalg RSA \
  -keysize 4096 \
  -validity 10000
```

Do not commit the keystore. The repository ignores `*.jks`, `*.keystore`, and `*.p12` files.

## Configure GitHub Secrets

In the repository, open **Settings → Secrets and variables → Actions → New repository secret** and add:

| Secret | Value |
|---|---|
| `ANDROID_RELEASE_KEYSTORE_BASE64` | Base64 contents of `luno-release.jks` |
| `ANDROID_RELEASE_STORE_PASSWORD` | Keystore password |
| `ANDROID_RELEASE_KEY_ALIAS` | `luno` or the alias chosen above |
| `ANDROID_RELEASE_KEY_PASSWORD` | Key password |

On macOS, copy the encoded keystore with:

```bash
base64 < luno-release.jks | pbcopy
```

## Publish A Release

1. Update `versionName` and increment `versionCode` in `app/build.gradle.kts`.
2. Run the normal debug tests locally with `./gradlew installDebug` or `./gradlew testDebugUnitTest`.
3. Commit and push the version change.
4. Create and push a matching tag, for example:

```bash
git tag v0.1.0
git push origin v0.1.0
```

5. GitHub Actions builds `testDebugUnitTest`, `lintDebug`, and the release APK.
6. The workflow verifies that the tag matches `versionName` and verifies the APK signature.
7. A GitHub Release is created automatically with `Luno-Android-v0.1.0.apk` attached.

The Android app's **Settings → App → Check for updates** entry reads this release tag and finds the attached APK. **Download APK** downloads it into the app cache and opens Android's package installer; the first install may require enabling **Allow Luno to install unknown apps**. Users can also download it directly from the release page.

## Important Signing Rule

Every future release must use the same keystore and key alias. Never generate a replacement key for an existing application ID unless you intentionally want Android to treat it as a different app.

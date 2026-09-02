# Neomacs Android package

This Gradle project is the thin Android application host for the
`neomacs-android` Rust library.  It deliberately does not build or discover
Rust outputs: callers must supply one target-built native library and one
complete portable runtime asset directory.

Build those inputs from the repository root, then assemble the package:

```sh
cargo xtask fresh-build --release \
  --portable-seed \
  --portable-runtime-image ./tmp/neomacs.portable \
  --low-memory

cargo xtask package-portable-assets \
  --portable-runtime-image ./tmp/neomacs.portable \
  --output-dir ./tmp/neomacs-android-assets

cargo build --release \
  -p neomacs-android \
  --target aarch64-linux-android

crates/neomacs-android/android/gradlew \
  -p crates/neomacs-android/android \
  -PneomacsNativeLibrary="$PWD/target/aarch64-linux-android/release/libneomacs_android.so" \
  -PneomacsPortableAssets="$PWD/tmp/neomacs-android-assets" \
  assembleRelease
```

The Android NDK compiler/linker environment must be configured for the Cargo
step.  CI uses NDK `28.2.13676358`, API level 24, and `arm64-v8a`.  Gradle pins
the same NDK and refuses to assemble when either generated input is absent.

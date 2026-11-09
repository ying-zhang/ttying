# Copy this file to build.config.psd1 and adjust paths for your machine.
# build.config.psd1 is ignored by Git.
@{
    # Optional. Environment variables CARGO_HOME / RUSTUP_HOME are used if omitted.
    CargoHome      = '<path-to-cargo-home>'
    RustupHome     = '<path-to-rustup-home>'

    # Android toolchain. Environment variables ANDROID_HOME and
    # ANDROID_NDK_HOME are used if omitted.
    AndroidHome    = '<path-to-android-sdk>'
    AndroidNdkHome = '<path-to-android-ndk>'

    # Optional. If omitted, the newest build-tools/platform under AndroidHome is used.
    # BuildTools   = '<path-to-android-sdk>\build-tools\<version>'
    # AndroidJar   = '<path-to-android-sdk>\platforms\android-<version>\android.jar'

    # Required when the tools are not already on PATH.
    JdkBin         = '<path-to-jdk>\bin'
    Python         = '<path-to-python.exe>'

    # Optional native toolchains.
    ZigBin         = '<path-to-zig-directory>'
    MingwBin       = '<path-to-mingw>\bin'

# Android release signing. Set these in the ignored build.config.psd1,
# or via TTYING_ANDROID_KEYSTORE / TTYING_ANDROID_KEY_ALIAS /
# TTYING_ANDROID_KEYSTORE_PASSWORD / TTYING_ANDROID_KEY_PASSWORD.
# AndroidKeystore          = '<path-to-release.keystore>'
# AndroidKeyAlias          = 'ttying'
# AndroidKeyStorePassword  = '<store-password>'
# AndroidKeyPassword       = '<key-password>'
}

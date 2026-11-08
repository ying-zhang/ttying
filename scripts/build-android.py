#!/usr/bin/env python3
"""Build the arm64 Android APK with the stable release signing key."""
import hashlib
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ANDROID = ROOT / "android"
BUILD = ANDROID / "build"
DIST = ROOT / "dist"


def fail(message: str) -> None:
    raise SystemExit(message)


def env(name: str) -> str | None:
    value = os.environ.get(name)
    return value if value else None


def tool(name: str) -> str:
    path = shutil.which(name)
    if not path:
        fail(f"required tool not found on PATH: {name}")
    return path


def version_key(name: str) -> tuple[int, ...]:
    return tuple(int(part) for part in re.findall(r"\d+", name)) or (0,)


def latest_dir(parent: Path, minimum: int | None = None) -> Path:
    candidates = []
    for item in parent.iterdir() if parent.exists() else []:
        if not item.is_dir():
            continue
        key = version_key(item.name)
        if minimum is None or key[0] >= minimum:
            candidates.append((key, item))
    if not candidates:
        fail(f"no matching directory under {parent}")
    return max(candidates, key=lambda pair: pair[0])[1]


def run(command: list[str], *, cwd: Path = ROOT, env_extra: dict[str, str] | None = None) -> None:
    printable = " ".join(command)
    print(f"+ {printable}", flush=True)
    child_env = os.environ.copy()
    if env_extra:
        child_env.update(env_extra)
    subprocess.run(command, cwd=str(cwd), env=child_env, check=True)


def main() -> None:
    sdk = env("ANDROID_HOME") or env("ANDROID_SDK_ROOT")
    if not sdk:
        fail("ANDROID_HOME or ANDROID_SDK_ROOT is required")
    sdk_path = Path(sdk)

    ndk = env("ANDROID_NDK_HOME") or env("ANDROID_NDK_ROOT")
    if not ndk:
        ndk_path = latest_dir(sdk_path / "ndk")
    else:
        ndk_path = Path(ndk)
    if not ndk_path.is_dir():
        fail(f"Android NDK not found: {ndk_path}")

    build_tools = env("ANDROID_BUILD_TOOLS")
    if build_tools:
        build_tools_path = Path(build_tools)
    else:
        build_tools_path = latest_dir(sdk_path / "build-tools")
    android_jar = env("ANDROID_PLATFORM_JAR")
    if android_jar:
        jar_path = Path(android_jar)
    else:
        jar_path = latest_dir(sdk_path / "platforms", 35) / "android.jar"
    if not jar_path.is_file():
        fail(f"Android platform jar not found: {jar_path}")

    suffix = ".exe" if os.name == "nt" else ""
    aapt2 = build_tools_path / f"aapt2{suffix}"
    d8 = build_tools_path / ("d8.bat" if os.name == "nt" else "d8")
    zipalign = build_tools_path / f"zipalign{suffix}"
    apksigner = build_tools_path / ("apksigner.bat" if os.name == "nt" else "apksigner")
    javac = tool("javac")
    jar = tool("jar")
    cargo = tool("cargo")
    if not shutil.which("cargo-ndk"):
        fail("cargo-ndk is required (cargo install cargo-ndk --locked)")

    keystore = env("TTYING_ANDROID_KEYSTORE")
    if not keystore:
        fail("TTYING_ANDROID_KEYSTORE is required")
    alias = env("TTYING_ANDROID_KEY_ALIAS") or "ttying"
    store_password = env("TTYING_ANDROID_KEYSTORE_PASSWORD")
    key_password = env("TTYING_ANDROID_KEY_PASSWORD") or store_password
    if not store_password:
        fail("TTYING_ANDROID_KEYSTORE_PASSWORD is required")

    for item in [aapt2, d8, zipalign, apksigner, jar_path, Path(keystore)]:
        if not item.exists():
            fail(f"required file not found: {item}")

    BUILD.mkdir(parents=True, exist_ok=True)
    DIST.mkdir(parents=True, exist_ok=True)
    for directory in [BUILD / "classes", BUILD / "dex"]:
        if directory.exists():
            shutil.rmtree(directory)
        directory.mkdir(parents=True)
    for filename in ["resources.zip", "base.apk", "classes.jar", "unsigned.apk", "aligned.apk"]:
        path = BUILD / filename
        if path.exists():
            path.unlink()

    run(
        [cargo, "ndk", "-t", "arm64-v8a", "-o", str(ANDROID / "jniLibs"),
         "build", "--locked", "--release", "-p", "ttying-jni"],
        env_extra={"ANDROID_NDK_HOME": str(ndk_path), "ANDROID_NDK_ROOT": str(ndk_path)},
    )

    java_files = [str(path) for path in sorted((ANDROID / "src").rglob("*.java"))]
    run([javac, "--release", "11", "-classpath", str(jar_path), "-d", str(BUILD / "classes"), *java_files])
    run([jar, "cf", str(BUILD / "classes.jar"), "-C", str(BUILD / "classes"), "."])
    run([str(aapt2), "compile", "--dir", str(ANDROID / "res"), "-o", str(BUILD / "resources.zip")])
    run([str(d8), "--lib", str(jar_path), "--min-api", "26", "--output", str(BUILD / "dex"), str(BUILD / "classes.jar")])
    run([str(aapt2), "link", "-o", str(BUILD / "base.apk"), "-I", str(jar_path),
         "-R", str(BUILD / "resources.zip"), "--manifest", str(ANDROID / "AndroidManifest.xml"),
         "--min-sdk-version", "26", "--target-sdk-version", "35", "--auto-add-overlay"])
    run([sys.executable, str(ROOT / "scripts" / "package-apk.py")])
    run([str(zipalign), "-P", "16", "-f", "4", str(BUILD / "unsigned.apk"), str(BUILD / "aligned.apk")])
    output = DIST / "ttying-android-arm64.apk"
    run(
        [str(apksigner), "sign", "--ks", keystore, "--ks-key-alias", alias,
         "--ks-pass", "env:TTYING_APKSIGNER_STORE_PASSWORD",
         "--key-pass", "env:TTYING_APKSIGNER_KEY_PASSWORD",
         "--out", str(output), str(BUILD / "aligned.apk")],
        env_extra={
            "TTYING_APKSIGNER_STORE_PASSWORD": store_password,
            "TTYING_APKSIGNER_KEY_PASSWORD": key_password,
        },
    )
    run([str(apksigner), "verify", "--print-certs", str(output)])
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    print(f"{digest}  {output.name}")


if __name__ == "__main__":
    main()

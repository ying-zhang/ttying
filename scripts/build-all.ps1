param([switch]$ValidateOnly)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ConfigPath = Join-Path $PSScriptRoot "build.config.psd1"
$Config = if (Test-Path $ConfigPath) { Import-PowerShellDataFile $ConfigPath } else { @{} }

function Get-Setting([string]$Name, [string]$EnvName) {
    if ($Config.ContainsKey($Name) -and $Config[$Name] -and -not ([string]$Config[$Name]).StartsWith("<")) {
        return $Config[$Name]
    }
    if ($EnvName) {
        $value = [Environment]::GetEnvironmentVariable($EnvName)
        if ($value) { return $value }
    }
    return $null
}

function Add-BinPath([string]$PathValue) {
    if ($PathValue) {
        $resolved = Require-Path $PathValue "Tool directory"
        if (-not (Test-Path -LiteralPath $resolved -PathType Container)) { throw "Not a directory: $resolved" }
        $env:Path = "$resolved;$env:Path"
    }
}

function Require-Path([string]$PathValue, [string]$Label) {
    if ($PathValue -and -not [IO.Path]::IsPathRooted($PathValue)) { $PathValue = Join-Path $Root $PathValue }
    if (-not $PathValue -or -not (Test-Path -LiteralPath $PathValue)) {
        throw "$Label not found: $PathValue. Set it in scripts/build.config.psd1 or the documented environment variable."
    }
    return (Resolve-Path -LiteralPath $PathValue).Path
}

$CargoHome = Get-Setting "CargoHome" "CARGO_HOME"
$RustupHome = Get-Setting "RustupHome" "RUSTUP_HOME"
if ($CargoHome) { $env:CARGO_HOME = $CargoHome; Add-BinPath (Join-Path $CargoHome "bin") }
if ($RustupHome) { $env:RUSTUP_HOME = $RustupHome }
Add-BinPath (Get-Setting "ZigBin" "TTYING_ZIG_BIN")
Add-BinPath (Get-Setting "MingwBin" "TTYING_MINGW_BIN")

$AndroidHome = Get-Setting "AndroidHome" "ANDROID_HOME"
if (-not $AndroidHome -or -not (Test-Path -LiteralPath $AndroidHome -PathType Container)) {
    if ($Config.ContainsKey("AndroidHome") -and $Config.AndroidHome -and -not $Config.AndroidHome.StartsWith("<")) {
        throw "Configured AndroidHome is invalid: $AndroidHome"
    }
    $AndroidHome = $env:ANDROID_SDK_ROOT
}
$AndroidHome = Require-Path $AndroidHome "Android SDK"
$AndroidNdk = Get-Setting "AndroidNdkHome" "ANDROID_NDK_HOME"
$AndroidNdk = Require-Path $AndroidNdk "Android NDK"
$env:ANDROID_HOME = $AndroidHome
$env:ANDROID_SDK_ROOT = $AndroidHome
$env:ANDROID_NDK_HOME = $AndroidNdk

$BuildTools = Get-Setting "BuildTools" "ANDROID_BUILD_TOOLS"
if (-not $BuildTools) {
    $BuildTools = Get-ChildItem (Join-Path $AndroidHome "build-tools") -Directory |
        Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
        Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1 -ExpandProperty FullName
}
$BuildTools = Require-Path $BuildTools "Android build-tools"
$AndroidJar = Get-Setting "AndroidJar" "ANDROID_PLATFORM_JAR"
if (-not $AndroidJar) {
    $AndroidJar = Get-ChildItem (Join-Path $AndroidHome "platforms") -Filter android.jar -Recurse |
        Where-Object { $_.Directory.Name -match '^android-(\d+)(?:\.\d+)*$' -and [int]$Matches[1] -ge 35 } |
        Sort-Object { $v = $_.Directory.Name -replace '^android-', ''; if ($v -notmatch '\.') { $v += '.0' }; [version]$v } -Descending | Select-Object -First 1 -ExpandProperty FullName
}
$AndroidJar = Require-Path $AndroidJar "Android platform jar"
$AndroidKeystore = Get-Setting "AndroidKeystore" "TTYING_ANDROID_KEYSTORE"
$AndroidKeyAlias = Get-Setting "AndroidKeyAlias" "TTYING_ANDROID_KEY_ALIAS"
$AndroidKeyStorePassword = Get-Setting "AndroidKeyStorePassword" "TTYING_ANDROID_KEYSTORE_PASSWORD"
$AndroidKeyPassword = Get-Setting "AndroidKeyPassword" "TTYING_ANDROID_KEY_PASSWORD"
if (-not $AndroidKeyAlias) { $AndroidKeyAlias = "ttying" }
if (-not $AndroidKeyPassword) { $AndroidKeyPassword = $AndroidKeyStorePassword }
$AndroidKeystore = Require-Path $AndroidKeystore "Android release keystore"
if (-not $AndroidKeyStorePassword) {
    throw "AndroidKeyStorePassword is required for release signing. Set it in scripts/build.config.psd1 or TTYING_ANDROID_KEYSTORE_PASSWORD."
}

$JdkBin = Get-Setting "JdkBin" "TTYING_JDK_BIN"
if (-not $JdkBin -and $env:JAVA_HOME) { $JdkBin = Join-Path $env:JAVA_HOME "bin" }
Add-BinPath $JdkBin
if ($JdkBin) { $env:JAVA_HOME = Split-Path (Require-Path $JdkBin "JDK bin") -Parent }
$Python = Get-Setting "Python" "TTYING_PYTHON"
if (-not $Python) { $Python = (Get-Command python -ErrorAction Stop).Source }
$Node = Get-Setting "Node" "TTYING_NODE"
if (-not $Node) { $Node = (Get-Command node -ErrorAction Stop).Source }
$Npm = Get-Setting "Npm" "TTYING_NPM"
if (-not $Npm) { $Npm = (Get-Command npm -ErrorAction Stop).Source }
$Cargo = (Get-Command cargo -ErrorAction Stop).Source
$Javac = (Get-Command javac -ErrorAction Stop).Source
$JarExe = (Get-Command jar -ErrorAction Stop).Source
$Aapt2 = Join-Path $BuildTools "aapt2.exe"
$D8 = Join-Path $BuildTools "d8.bat"
$ZipAlign = Join-Path $BuildTools "zipalign.exe"
$ApkSigner = Join-Path $BuildTools "apksigner.bat"
$Python = Require-Path $Python "Python"
$Node = Require-Path $Node "Node"
$Npm = Require-Path $Npm "npm"
foreach ($tool in @($Python, $Node, $Npm, $Cargo, $Javac, $JarExe, $Aapt2, $D8, $ZipAlign, $ApkSigner)) {
    if (-not (Test-Path -LiteralPath $tool -PathType Leaf)) { throw "Tool not found: $tool" }
}
$TargetDir = Join-Path $Root "target"
if ($ValidateOnly) {
    [pscustomobject]@{ AndroidSdk=$AndroidHome; BuildTools=$BuildTools; AndroidJar=$AndroidJar; AndroidKeystore=$AndroidKeystore; Cargo=$Cargo; Javac=$Javac; TargetDir=$TargetDir }
    return
}

$Dist = Join-Path $Root "dist"
$Assets = Join-Path $Root "crates\ttying-core\assets"
$Build = Join-Path $Root "android\build"
New-Item -ItemType Directory -Force $Dist, $Assets, $Build, "$Build\classes", "$Build\dex" | Out-Null

Write-Host "== web =="
Push-Location (Join-Path $Root "web")
try {
    & $Npm ci --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
    & $Node .\node_modules\esbuild\bin\esbuild src\main.js --bundle --minify --outfile="$Assets\app.js"
    if ($LASTEXITCODE -ne 0) { throw "web build failed" }
    Copy-Item .\src\index.html "$Assets\index.html" -Force
} finally { Pop-Location }

Write-Host "== windows =="
Push-Location $Root
try {
    & $Cargo build --locked --release --target x86_64-pc-windows-gnu --target-dir $TargetDir -p ttying-cli
    if ($LASTEXITCODE -ne 0) { throw "windows build failed" }
    Copy-Item "$TargetDir\x86_64-pc-windows-gnu\release\ttying.exe" "$Dist\ttying-windows-x86_64.exe" -Force
} finally { Pop-Location }

Write-Host "== linux =="
Push-Location $Root
try {
    & $Cargo zigbuild --locked --release --target-dir $TargetDir --target x86_64-unknown-linux-musl -p ttying-cli
    if ($LASTEXITCODE -ne 0) { throw "linux build failed" }
    Copy-Item "$TargetDir\x86_64-unknown-linux-musl\release\ttying" "$Dist\ttying-linux-x86_64" -Force
} finally { Pop-Location }

Write-Host "== android jni =="
Push-Location $Root
try {
    & $Cargo ndk -t arm64-v8a -o android\jniLibs build --locked --release --target-dir $TargetDir -p ttying-jni
    if ($LASTEXITCODE -ne 0) { throw "android jni build failed" }
} finally { Pop-Location }

Write-Host "== apk =="
foreach ($cleanup in @("$Build\classes", "$Build\dex", "$Build\gen")) {
    $absolute = [IO.Path]::GetFullPath($cleanup)
    $buildPrefix = [IO.Path]::GetFullPath($Build).TrimEnd('\') + '\'
    if (-not $absolute.StartsWith($buildPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsafe cleanup path: $absolute" }
    foreach ($directory in @($Build, $absolute)) {
        if (Test-Path -LiteralPath $directory) {
            if ((Get-Item -LiteralPath $directory).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Refusing cleanup through a reparse point: $directory" }
        }
    }
    if (Test-Path -LiteralPath $absolute) { Remove-Item -LiteralPath $absolute -Recurse -Force }
}
New-Item -ItemType Directory -Force "$Build\classes", "$Build\dex", "$Build\gen" | Out-Null
Push-Location $Root
try {
    & $Aapt2 compile --dir android\res -o "$Build\resources.zip"
    if ($LASTEXITCODE -ne 0) { throw "aapt2 compile failed" }
    & $Aapt2 link -o "$Build\base.apk" -I $AndroidJar -R "$Build\resources.zip" --manifest android\AndroidManifest.xml --java "$Build\gen" --min-sdk-version 26 --target-sdk-version 35 --auto-add-overlay
    if ($LASTEXITCODE -ne 0) { throw "aapt2 link failed" }
    $javaFiles = @(Get-ChildItem "$Build\gen" -Recurse -Filter *.java | ForEach-Object FullName) + @(Get-ChildItem android\src\cn\z\ttying\app\*.java | ForEach-Object FullName)
    & $Javac --release 11 -classpath $AndroidJar -d "$Build\classes" @javaFiles
    if ($LASTEXITCODE -ne 0) { throw "javac failed" }
    & $JarExe cf "$Build\classes.jar" -C "$Build\classes" .
    if ($LASTEXITCODE -ne 0) { throw "jar failed" }
    & $D8 --lib $AndroidJar --min-api 26 --output "$Build\dex" "$Build\classes.jar"
    if ($LASTEXITCODE -ne 0) { throw "d8 failed" }
} finally { Pop-Location }

& $Python (Join-Path $Root "scripts\package-apk.py")
if ($LASTEXITCODE -ne 0) { throw "package-apk failed" }
& $ZipAlign -P 16 -f 4 "$Build\unsigned.apk" "$Build\aligned.apk"
if ($LASTEXITCODE -ne 0) { throw "zipalign failed" }
$env:TTYING_APKSIGNER_STORE_PASSWORD = $AndroidKeyStorePassword
$env:TTYING_APKSIGNER_KEY_PASSWORD = $AndroidKeyPassword
try {
    & $ApkSigner sign --ks $AndroidKeystore --ks-key-alias $AndroidKeyAlias --ks-pass "env:TTYING_APKSIGNER_STORE_PASSWORD" --key-pass "env:TTYING_APKSIGNER_KEY_PASSWORD" --out "$Dist\ttying-android-arm64.apk" "$Build\aligned.apk"
    if ($LASTEXITCODE -ne 0) { throw "apksigner sign failed" }
    & $ApkSigner verify --print-certs "$Dist\ttying-android-arm64.apk"
    if ($LASTEXITCODE -ne 0) { throw "apksigner verify failed" }
} finally {
    Remove-Item Env:TTYING_APKSIGNER_STORE_PASSWORD -ErrorAction SilentlyContinue
    Remove-Item Env:TTYING_APKSIGNER_KEY_PASSWORD -ErrorAction SilentlyContinue
}

Get-ChildItem $Dist -Filter "*.idsig" -File -ErrorAction SilentlyContinue | Remove-Item -Force

Write-Host "== hashes =="
Get-ChildItem $Dist -File | Where-Object { $_.Name -notlike "*.idsig" -and $_.Name -ne "SHA256SUMS.txt" } | ForEach-Object {
    $h = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
    "$h  $($_.Name)"
} | Set-Content "$Dist\SHA256SUMS.txt"
Write-Host "done"

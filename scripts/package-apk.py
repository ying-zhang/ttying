import os, shutil, sys, zipfile
root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
out = os.path.join(root, "android", "build")
base = os.path.join(out, "base.apk")
unsigned = os.path.join(out, "unsigned.apk")
shutil.copyfile(base, unsigned)
with zipfile.ZipFile(unsigned, "a", compression=zipfile.ZIP_STORED) as z:
    z.write(os.path.join(out, "dex", "classes.dex"), "classes.dex")
    z.write(os.path.join(root, "android", "jniLibs", "arm64-v8a", "libttying_jni.so"),
            "lib/arm64-v8a/libttying_jni.so")
print(unsigned)

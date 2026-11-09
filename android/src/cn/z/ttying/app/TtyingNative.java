package cn.z.ttying;

public final class TtyingNative {
    static { System.loadLibrary("ttying_jni"); }
    private TtyingNative() {}
    public static native String start(String bind, int port, String commandJson, String token);
    public static native String stop();
    public static native String status();
}

package com.latexsnipper.core;

/**
 * JNI transport for the existing Core v3 envelopes and session ABI v1.
 * The host loads its packaged latexsnipper_ffi library before using this class.
 * Call blocking operations off the UI thread. Always inspect the JSON envelope:
 * a returned String is not proof that the Core action succeeded.
 */
public final class NativeSessionBridge {
    private NativeSessionBridge() {}

    public static native int abiVersion();
    public static native String create(String requestJson);
    public static native String health(long handle);
    public static native String warmup(long handle, String requestJson);
    public static native String recognizeBytes(long handle, String requestJson, byte[] encodedImage);
    public static native String reloadModels(long handle);
    public static native String close(long handle);
    public static native String formulaCapabilities();
    public static native String convertFormula(String requestJson);
}

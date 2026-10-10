# JNI application session transport

Build `latexsnipper-ffi` with `--features android-jni` to export genuine JNI
methods for `com.latexsnipper.core.NativeSessionBridge`. This feature is explicit
and is also supported on desktop platforms for JVM transport conformance tests.
It does not enable an Android inference runtime by itself.

The host must load its packaged native library before calling the class, for
example `System.loadLibrary("latexsnipper_ffi")` on Android. Run blocking calls
off the UI thread. Do not load an untrusted library or download a binary based
on arbitrary note content or model metadata.

`create`, `health`, `warmup`, `recognizeBytes`, `reloadModels`, `close`,
`formulaCapabilities`, and `convertFormula` return the existing Core v3 JSON
envelope. Check `ok`, the error code and versions; a Java String is not proof
of successful recognition or conversion. `abiVersion()` reports C session ABI v1.
`create` returns the same opaque handle as the C ABI: retain its 64-bit bit pattern
in a Java `long`, using `Long.parseUnsignedLong` if parsing an unsigned JSON number.

Recognition input is an encoded PNG/JPEG/WebP/BMP/TIFF/GIF byte array, not raw RGB.
Each handle owns one persistent Core `RecognitionSession`; no stub engine or
second session registry is created in the JNI layer. Warmup and model reload use
the same Core caches. Explicit close invalidates the handle and frees session
resources. Close every created handle; JVM garbage collection does not close a
native session. Independent sessions remain independent after a failed call.

JSON requests are bounded before JNI decoding by 1 MiB UTF-16 units and then by
1 MiB UTF-8 bytes. Encoded arrays must contain 1 to 100 MiB bytes, with their length
checked before copying. Core applies its source, decoder, nesting and output
budgets separately. The JNI-owned image copy moves into the shared session path,
without a second C-ABI copy or a Java array pinned throughout inference. These
limits do not provide a hard memory sandbox or cancellation
of synchronous native inference. Supervision and UI task cancellation belong
to the host; never replay a document modification after a timeout.

Null, empty or oversized JNI transport arguments throw `IllegalArgumentException`.
JNI allocation/internal failures throw `IllegalStateException`, preserving any
pending JVM exception. Declared Core errors remain failed JSON envelopes.
The JNI adapter consumes and frees every native C response string, then returns
a JVM-owned String; Java callers must not pass it to a native free-string method.

Historical `Java_*NativeBridge*` exports are raw C compatibility shims, not valid
JNI signatures. They remain unchanged; use the new class instead of calling
those functions as JVM native methods. The old iOS global-engine API also remains
separate; new C integrations can use [the opaque session header](../include/latexsnipper_session.h).

```console
cargo build --locked -p latexsnipper-ffi --features android-jni --release
python crates/ffi/examples/jni_smoke.py --library-dir target/release
```

The smoke uses a real JDK and `-Xcheck:jni`, with symbol resolution, Unicode,
null/oversized buffers, warmup reuse, reload, independent handles and close/error
recovery. Missing JDK or native symbols is a failure, not a passing skip.
Desktop JVM conformance is not Android ABI/ART, device-model accuracy, NDK/APK
packaging or iOS acceptance; those remain separate release gates.

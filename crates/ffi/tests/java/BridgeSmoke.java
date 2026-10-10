import com.latexsnipper.core.NativeSessionBridge;
import java.nio.file.Path;
import java.util.Base64;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/** Actual JVM symbol resolution and JNI transport checks, not Android device acceptance. */
public final class BridgeSmoke {
    private static final Pattern HANDLE = Pattern.compile("\"handle\":([0-9]+)");

    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }

    private static String quote(String value) {
        return "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }

    private static void ok(String json) {
        require(json != null && json.contains("\"ok\":true"), "Expected successful Core envelope: " + json);
        require(json.contains("\"apiEnvelopeVersion\":3"), "Missing v3 contract version");
    }

    private static void error(String json, String code) {
        require(json != null && json.contains("\"ok\":false"), "Expected failure envelope: " + json);
        require(json.contains("\"code\":\"" + code + "\""), "Expected " + code + ": " + json);
    }

    private static void invalid(Runnable action) {
        try {
            action.run();
        } catch (IllegalArgumentException expected) {
            return;
        }
        throw new AssertionError("Expected an invalid JNI argument exception");
    }

    private static long create(String directory) {
        String json = NativeSessionBridge.create("{\"modelsDir\":" + quote(directory) + ",\"maxThreads\":1}");
        ok(json);
        Matcher matcher = HANDLE.matcher(json);
        require(matcher.find(), "No opaque handle returned");
        return Long.parseUnsignedLong(matcher.group(1));
    }

    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        require(NativeSessionBridge.abiVersion() == 1, "ABI version changed");
        ok(NativeSessionBridge.formulaCapabilities());
        invalid(() -> NativeSessionBridge.convertFormula(null));
        invalid(() -> NativeSessionBridge.create(""));
        invalid(() -> NativeSessionBridge.convertFormula("x".repeat(1024 * 1024 + 1)));
        invalid(() -> NativeSessionBridge.convertFormula("\u4e2d".repeat(400000)));
        error(NativeSessionBridge.convertFormula("{}"), "INVALID_JSON");
        error(NativeSessionBridge.convertFormula("{\"content\":\"x\u0000y\"}"), "INVALID_JSON");
        String unicode = NativeSessionBridge.convertFormula(
            "{\"content\":\"\\\\text{\u4e2d\u6587\ud83d\ude00}\",\"inputFormat\":\"latex\",\"outputFormat\":\"latex\",\"mode\":\"best-effort\"}");
        ok(unicode);
        require(unicode.contains("\u4e2d\u6587\ud83d\ude00"), "Modified UTF-8 conversion lost Unicode");
        String bare = NativeSessionBridge.convertFormula(
            "{\"content\":\"frac(a,b)\",\"inputFormat\":\"typst\",\"outputFormat\":\"latex-fragment\",\"mode\":\"best-effort\"}");
        ok(bare);
        require(bare.contains("\"contentKind\":\"latex-fragment\""), "No bare-formula projection marker");
        error(NativeSessionBridge.convertFormula(
            "{\"content\":\"x\",\"inputFormat\":\"mtef\",\"outputFormat\":\"omml\"}"), "UNSUPPORTED_FORMAT");

        long first = create(args[1]);
        long second = create(args[2]);
        require(first != second, "Sessions must have independent handles");
        try {
            ok(NativeSessionBridge.health(first));
            error(NativeSessionBridge.create("{\"modelsDir\":\"\"}"), "INVALID_ARGUMENT");
            ok(NativeSessionBridge.health(first));
            error(NativeSessionBridge.health(-1), "SESSION_NOT_FOUND");
            String warmup = "{\"profile\":\"croppedFormula\"}";
            String once = NativeSessionBridge.warmup(first, warmup);
            String twice = NativeSessionBridge.warmup(first, warmup);
            ok(once);
            ok(twice);
            require(once.contains("\"alreadyWarm\":false"), "First warmup was not new");
            require(twice.contains("\"alreadyWarm\":true"), "Warmup was not reused");
            require(NativeSessionBridge.warmup(second, warmup).contains("\"alreadyWarm\":false"), "Warmup leaked across sessions");
            invalid(() -> NativeSessionBridge.recognizeBytes(first, warmup, null));
            invalid(() -> NativeSessionBridge.recognizeBytes(first, warmup, new byte[0]));
            invalid(() -> NativeSessionBridge.recognizeBytes(first, warmup, new byte[100 * 1024 * 1024 + 1]));
            error(NativeSessionBridge.recognizeBytes(first, warmup, new byte[] {1, 2, 3}), "UNSUPPORTED_FORMAT");
            byte[] png = Base64.getDecoder().decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=");
            error(NativeSessionBridge.recognizeBytes(first, "{\"profile\":\"formula\",\"timeoutMs\":0}", png), "TIMEOUT");
            ok(NativeSessionBridge.health(first));
            ok(NativeSessionBridge.reloadModels(first));
            require(NativeSessionBridge.warmup(first, warmup).contains("\"alreadyWarm\":false"), "Reload did not clear warmup state");
            ExecutorService executor = Executors.newFixedThreadPool(2);
            try {
                Future<String> a = executor.submit(() -> NativeSessionBridge.health(first));
                Future<String> b = executor.submit(() -> NativeSessionBridge.health(second));
                ok(a.get());
                ok(b.get());
            } finally {
                executor.shutdownNow();
            }
            ok(NativeSessionBridge.close(first));
            error(NativeSessionBridge.health(first), "SESSION_NOT_FOUND");
            error(NativeSessionBridge.close(first), "SESSION_NOT_FOUND");
            ok(NativeSessionBridge.health(second));
        } finally {
            NativeSessionBridge.close(first);
            NativeSessionBridge.close(second);
        }
        System.out.println("Actual JVM JNI session, Unicode, byte-array, exception and lifecycle smoke passed");
    }
}

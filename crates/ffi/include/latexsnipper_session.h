#ifndef LATEXSNIPPER_SESSION_H
#define LATEXSNIPPER_SESSION_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32) && defined(LATEXSNIPPER_BUILD_DLL)
#define LATEXSNIPPER_API __declspec(dllexport)
#elif defined(_WIN32)
#define LATEXSNIPPER_API __declspec(dllimport)
#else
#define LATEXSNIPPER_API
#endif

/* Every char pointer returned below is UTF-8 JSON and must be passed exactly
 * once to latexsnipper_string_free. A null return means allocation failed. */

LATEXSNIPPER_API uint32_t latexsnipper_session_abi_version(void);

/* Additive model-free formula API. Both functions return the existing v3
 * envelope and use latexsnipper_string_free; no session handle is required.
 * Capabilities data is an array of native input/output/mode rows.
 * Conversion request example (mode defaults to strict):
 * {"content":"x","inputFormat":"latex","outputFormat":"omml"}
 * Only LaTeX-to-OMML currently supports strict. Select "best-effort" explicitly
 * for other available routes; UnicodeMath/AsciiMath/MTEF remain unsupported.
 * Conversion data contains content and the executed capability row.
 * Explicit outputFormat "latex-fragment" returns one bare formula plus
 * contentKind "latex-fragment"; capability describes the latex_display route.
 * Legacy "latex" still exports a complete document. Projection is not a new
 * semantic format or a guarantee of editor rendering/fidelity.
 * JSON is capped at 1 MiB. Source and reconstructed LaTeX are capped at 64 KiB,
 * 64 lexical nesting levels and 512 structural tokens; XML DTDs are rejected.
 * Serialized conversion data is capped at 256 KiB, excluding the envelope.
 * Exceeding this budget returns OUTPUT_TOO_LARGE, not a partial result.
 * This budget is not a peak memory allocation or execution deadline guarantee.
 * Older v1 libraries may lack these symbols; feature-detect before using them.
 */
LATEXSNIPPER_API char *latexsnipper_formula_capabilities(void);
LATEXSNIPPER_API char *latexsnipper_formula_convert(
    const uint8_t *request_json,
    size_t request_json_len);

/* Additive instance-local registered conversion ABI v1, distinct from OCR
 * session handles. Every response uses the v3 envelope and string_free.
 * Create JSON: {"schemaVersion":1}, or the same plus "pluginStore" (existing
 * store path) and "profile" (closed WASI v1 profile object). No install/download
 * or package enablement. WASI requires the wasi-formula build feature; its
 * prepared backend is disabled until set_enabled explicitly enables the route.
 * Creation JSON <=32 KiB; profile <=16 KiB; at most 64 live/preparing registries.
 * Requests are closed JSON <=1 MiB; serialized response data <=2 MiB.
 * Convert JSON has schemaVersion/input/output/mode/backend/content/contextSha256
 * and optional policy {"schemaVersion":1,"backends":[]}. Default backend is
 * core:builtin. Success data is a converted/rejected audited report; ok=true
 * alone does NOT indicate conversion. Inspect data.outcome.kind. Candidates are
 * not write permits. contextSha256 must bind the actual Core binary, definitions,
 * fonts, styles and host dependencies; a package version is insufficient.
 * Execution failures never retry. Concurrent operations return BUSY, not a
 * hidden queue. Cancellation is terminal for this instance (create a new one).
 * Close/cancel do not wait for the conversion mutex. Native work already entered
 * cannot be undone; WASI retains its existing interruption budgets/checkpoints.
 * Changes/closure observed before response suppress completed candidates.
 * Approval is a TRUSTED HOST interface, never expose it to guest/MCP/plugin code.
 * Snapshots can contain sensitive paths; do not log/upload them automatically.
 * Approval JSON kinds: once(requestId), approve(requestId, scope, uses,
 * validForMs), batch(requestIds, validForMs), revoke(requestId), clear.
 * scope is {"kind":"exact"} or {"kind":"sameTarget","maxPayloadBytes":N}.
 * Only call once/approve/batch AFTER explicit human consent. Existing finite
 * grants execute without re-prompting; no wildcards, permanent approval or
 * persistence. Core validates quotas/TTL and package/host/target identities.
 * Older libraries can lack these additive symbols; feature-detect first.
 */
LATEXSNIPPER_API char *latexsnipper_formula_registry_create(const uint8_t *request_json, size_t request_json_len);
LATEXSNIPPER_API char *latexsnipper_formula_registry_capabilities(uint64_t handle);
LATEXSNIPPER_API char *latexsnipper_formula_registry_set_enabled(uint64_t handle, const uint8_t *request_json, size_t request_json_len);
LATEXSNIPPER_API char *latexsnipper_formula_registry_convert(uint64_t handle, const uint8_t *request_json, size_t request_json_len);
LATEXSNIPPER_API char *latexsnipper_formula_registry_cancel(uint64_t handle);
LATEXSNIPPER_API char *latexsnipper_formula_registry_close(uint64_t handle);
LATEXSNIPPER_API char *latexsnipper_formula_registry_approvals(uint64_t handle);
LATEXSNIPPER_API char *latexsnipper_formula_registry_approval(uint64_t handle, const uint8_t *request_json, size_t request_json_len);

/* request_json example:
 * {"modelsDir":"models","runtimePreference":"auto","maxThreads":4}
 */
LATEXSNIPPER_API char *latexsnipper_session_create(
    const uint8_t *request_json,
    size_t request_json_len);

LATEXSNIPPER_API char *latexsnipper_session_health(uint64_t handle);

/* request_json example: {"profile":"croppedFormula"} */
LATEXSNIPPER_API char *latexsnipper_session_warmup(
    uint64_t handle,
    const uint8_t *request_json,
    size_t request_json_len);

/* data contains an encoded PNG, JPEG, WebP, BMP, TIFF, or GIF image. */
LATEXSNIPPER_API char *latexsnipper_session_recognize_bytes(
    uint64_t handle,
    const uint8_t *request_json,
    size_t request_json_len,
    const uint8_t *data,
    size_t data_len);

LATEXSNIPPER_API char *latexsnipper_session_reload_models(uint64_t handle);

LATEXSNIPPER_API char *latexsnipper_session_close(uint64_t handle);

LATEXSNIPPER_API void latexsnipper_string_free(char *ptr);

#ifdef __cplusplus
}
#endif

#endif /* LATEXSNIPPER_SESSION_H */

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
 * for other available routes. UnicodeMath/AsciiMath have finite formula-only
 * input/output routes; MTEF is not supported by this string API.
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

/* Additive, model-free structured completion. Feature-detect these symbols;
 * existing session ABI v1 and formula_capabilities array are unchanged.
 * Complete request: {"source":"\\fr","inputFormat":"latex","cursor":3,
 * "revision":"42"}. Only finite LaTeX editing grammar is supported.
 * Offsets/ranges are UTF-8 bytes, not UTF-16 editor positions. Revisions are
 * canonical decimal u64 strings (no JSON numbers, leading zeros or exponent).
 * Complete data has snapshot and completion (context/candidates).
 * Apply sends CURRENT source/inputFormat/cursor/revision, the previous snapshot
 * verbatim, and candidateId; do not send insert/replace or the result tree.
 * Core rechecks schema/hash/revision/cursor and regenerates the selected edit.
 * Apply data has source/cursor/absolute tabStops. Snapshot is not authorization.
 * The host owns explicit acceptance, undo, revision updates and document writes.
 * No model/session/file access; no retries, network, macro execution or deadline.
 * Request <=1 MiB, source <=64 KiB, editing nodes <=4096, depth <64,
 * candidates <=128, serialized data <=1 MiB (excluding envelope).
 * Failures: INVALID_ARGUMENT (buffer/length), INVALID_JSON (closed schema),
 * INVALID_CURSOR, LIMIT_EXCEEDED, STALE_CONTEXT, UNKNOWN_CANDIDATE,
 * OUTPUT_TOO_LARGE. Free every response once using latexsnipper_string_free.
 */
LATEXSNIPPER_API char *latexsnipper_formula_completion_capabilities(void);
LATEXSNIPPER_API char *latexsnipper_formula_complete(const uint8_t *request_json, size_t request_json_len);
LATEXSNIPPER_API char *latexsnipper_formula_completion_apply(const uint8_t *request_json, size_t request_json_len);

/* Additive schema-v2 scoped completion; feature-detect these separate symbols.
 * Complete adds catalog {scope, definitions}; data is snapshot/context/candidates
 * (no v1 completion nesting). Candidates contain edit and nullable definition.
 * Scope has userSha256, nullable projectSha256/documentSha256, generation;
 * definitions have id/revision/userSha256/nullable projectSha256/command/arguments.
 * Scope IDs are opaque lowercase SHA-256 strings, not authorization credentials.
 * All revisions/generations are canonical decimal u64 strings. <=64 declarations,
 * ASCII IDs <=64 bytes (letters/digits/-_.), ASCII-letter commands <=64 bytes
 * without backslash/digits, <=4 arguments; empty for symbols. Capabilities expose
 * argumentKinds and maxLeadingOptionalArguments for required math/text and one
 * leading optional-math/optional-text. Defaults are not expanded.
 * Only current-user global/current-project definitions participate. Ambiguous or
 * builtin-colliding effective definitions fail INVALID_CATALOG. No macro bodies,
 * expansion, retrieval, saved history or implicit execution.
 * Apply adds the scoped snapshot and candidateId; Core also binds actual effective
 * signatures, revisions, user/project/document and reset generation. Host owns
 * authorization and must advance generation on clear/reset. Stale replies cannot
 * be accepted through v1. Other budgets/errors and string_free ownership above
 * remain unchanged. Existing v1 capability data and session ABI are unchanged.
 */
LATEXSNIPPER_API char *latexsnipper_formula_scoped_completion_capabilities(void);
LATEXSNIPPER_API char *latexsnipper_formula_complete_scoped(const uint8_t *request_json, size_t request_json_len);
LATEXSNIPPER_API char *latexsnipper_formula_completion_apply_scoped(const uint8_t *request_json, size_t request_json_len);

/* Additive schema-v3 private fragments; distinct from v1/v2 editing and the v3
 * JSON envelope. Feature-detect these symbols; never downgrade a failed accept.
 * Query adds catalog {commands, policy, fragments}; commands is the v2 catalog.
 * Policy requires retrievalEnabled/sensitiveDocument/allowInSensitiveDocument.
 * Fragments have id/revision/userSha256/nullable projectSha256/documentSha256,
 * trigger/label/source. Current-user global/current-project/current-document
 * filtering and sensitive-document policy run before private semantic checks or
 * hashing. Core does not read history, files, databases or network resources.
 * <=64 fragments, <=8 KiB source each; IDs <=64 ASCII letters/digits/-_., triggers
 * <=64 ASCII letters (no backslash); nonempty single-line labels <=128 bytes.
 * Sources must be closed finite editing templates with bare formula shape: no
 * document/math wrapper, lexical/package commands or unterminated final comment.
 * Shape checks do not grant permission to execute arbitrary TeX commands.
 * Data: snapshot/context/candidates. Each candidate has edit and nullable
 * definition/fragment references (id/revision). Snapshot uses catalogSha256 and
 * binds actual effective content, scope/generation and all policy fields.
 * Apply adds CURRENT catalog/editor state, snapshot, candidateId. The host owns
 * retrieval consent, real current policy, reset epochs, explicit acceptance,
 * undo and document writes. Hashes/booleans are not authorization credentials.
 * Other request/source/result limits and string_free ownership above apply.
 * Calls are synchronous: no internal retries, sessions, learning or persistence.
 */
LATEXSNIPPER_API char *latexsnipper_formula_fragment_completion_capabilities(void);
LATEXSNIPPER_API char *latexsnipper_formula_complete_fragments(const uint8_t *request_json, size_t request_json_len);
LATEXSNIPPER_API char *latexsnipper_formula_completion_apply_fragments(const uint8_t *request_json, size_t request_json_len);

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

/* Additive volatile preference owners, schema 1. Every returned v3 JSON envelope
 * must be freed once with latexsnipper_string_free. No document or disk writes.
 * create: {schemaVersion:1,scope:{userSha256,projectSha256,documentSha256,
 * generation},keyHex}. keyHex is PRIVATE random 32-byte host key material encoded
 * as 64 lowercase hex characters: never log it or expose these APIs to guests.
 * Defaults: ranking/capture OFF. Returned data.handle is a canonical u64 decimal
 * STRING; parse losslessly before passing as uint64_t, never through a double.
 * command kinds (closed JSON; nested request uses fragment editing schema 3):
 * info; set-policy {policy:{rankingEnabled,feedbackEnabled,allowSensitiveFeedback}};
 * capture -> {captureId}; discard-capture {captureId};
 * rank-fragments {request} -> {completion,order,features};
 * record-acceptance {captureId,eventId,request} -> {receiptId:string|null};
 * undo {receiptId}; clear {generation,keyHex}.
 * IDs and generation/eventId use canonical decimal u64 STRINGS, not numbers.
 * Capture BEFORE dispatch; record only AFTER a real host acceptance succeeded.
 * Successful record (including disabled/null) consumes the capture; errors leave
 * it available for explicit retry/discard. Undo consumes its receipt once.
 * Policy transitions/clear invalidate outstanding tokens; tokens are globally
 * unique in this loaded library and cannot alias another live owner. No wrap.
 * Clear requires newer scope generation and a fresh nonzero key. Update actual
 * catalog generation and requery; do not forge a capture for late queued work.
 * Retrieval allow does not authorize sensitive feedback. Handles/tokens and
 * policy booleans are not permission credentials. Trusted host owns all consent.
 * Limits: 64 owners, 128 pending captures/owner, 1024 events including undos;
 * create/metadata 4 KiB, editing command/result data 1 MiB, source/tree unchanged.
 * Busy is prompt PREFERENCE_BUSY without removal/replay; close only succeeds when
 * it can remove the owner. Repeated close/unknown handle: PREFERENCE_NOT_FOUND.
 * Discard all handles after library unload or process restart. Explicit
 * authenticated checkpoints are separate below; no automatic persistence,
 * secure heap erasure, user-level merging, model ranking or OLE IO is provided.
 * Feature-detect these additive symbols in older libraries.
 */
LATEXSNIPPER_API char *latexsnipper_formula_preferences_capabilities(void);
LATEXSNIPPER_API char *latexsnipper_formula_preferences_create(const uint8_t *request_json, size_t request_json_len);
/* Explicit authenticated restore creates a NEW default-off owner. Request:
 * {schemaVersion:1, scope, keyHex, storageKeyHex, expectedCheckpoint, snapshotJson,
 *  permission:{storageEnabled,allowSensitiveStorage}}.
 * Scope/checkpoint MUST come from current trusted host state, not the snapshot.
 * Keys must be separate nonzero host keys (64 lowercase hex characters).
 * Canonical decimal-u64 expectedCheckpoint; snapshotJson <=256 KiB UTF-8;
 * entire restore request <=1 MiB. Captures/receipts/consent are never restored.
 * command export-checkpoint takes storageKeyHex, checkpoint and permission;
 * returns {snapshotJson,checkpoint}. Save explicitly in host-protected storage.
 * HMAC is not encryption. No file IO, automatic retry or rollback protection
 * independent of host freshness. persistence:false continues to mean no disk IO.
 */
LATEXSNIPPER_API char *latexsnipper_formula_preferences_restore(const uint8_t *request_json, size_t request_json_len);
LATEXSNIPPER_API char *latexsnipper_formula_preferences_command(uint64_t handle, const uint8_t *request_json, size_t request_json_len);
LATEXSNIPPER_API char *latexsnipper_formula_preferences_close(uint64_t handle);

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

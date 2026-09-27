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

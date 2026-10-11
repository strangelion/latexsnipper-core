#ifndef LATEXSNIPPER_LEGACY_H
#define LATEXSNIPPER_LEGACY_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Compatibility API: one serialized process-default session, not an iOS SDK.
 * New applications should use latexsnipper_session.h for independent handles.
 * models_dir must be valid NUL-terminated UTF-8 naming an existing directory.
 * Invalid arguments or construction failure leave the previous session intact.
 * An internal panic requires reinitialization. Success means a
 * session exists, not that models, providers or recognition quality are ready.
 */
uint32_t latexsnipper_ffi_response_version(void);
int32_t latexsnipper_init(const char *models_dir);

/* Tightly packed RGB, width/height in 1..10000, active data <=100 MiB.
 * data must reference len readable bytes for the duration of the call.
 * Padding is accepted but only width*height*3 active bytes are consumed.
 * Returns legacy FfiResponse JSON; free exactly once with free_string.
 * Recognition is synchronous: do not call from a UI or Tokio runtime thread.
 */
char *latexsnipper_recognize_formula(const uint8_t *data, size_t len, uint32_t width, uint32_t height);
char *latexsnipper_recognize_cropped_formula(const uint8_t *data, size_t len, uint32_t width, uint32_t height);
char *latexsnipper_recognize_text(const uint8_t *data, size_t len, uint32_t width, uint32_t height);
char *latexsnipper_recognize_mixed(const uint8_t *data, size_t len, uint32_t width, uint32_t height);

/* Release waits for in-flight legacy work and does not close generic handles. */
void latexsnipper_release(void);
/* ptr must be NULL or a still-owned result allocated by this library. */
void latexsnipper_free_string(char *ptr);

#ifdef __cplusplus
}
#endif
#endif

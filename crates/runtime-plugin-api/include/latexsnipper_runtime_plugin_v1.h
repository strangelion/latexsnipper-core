#ifndef LATEXSNIPPER_RUNTIME_PLUGIN_V1_H
#define LATEXSNIPPER_RUNTIME_PLUGIN_V1_H

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#define LS_RUNTIME_PLUGIN_EXPORT __declspec(dllexport)
#else
#define LS_RUNTIME_PLUGIN_EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

#define LATEXSNIPPER_RUNTIME_PLUGIN_ABI_V1 1u
#define LS_RUNTIME_OK 0
#define LS_RUNTIME_ERROR (-1)

enum LatexSnipperDataTypeV1 {
    LS_DTYPE_FLOAT32 = 0,
    LS_DTYPE_FLOAT16 = 1,
    LS_DTYPE_INT64 = 2,
    LS_DTYPE_INT32 = 3,
    LS_DTYPE_UINT8 = 4,
    LS_DTYPE_BOOL = 5,
};

enum LatexSnipperDeviceKindV1 {
    LS_DEVICE_AUTO = 0,
    LS_DEVICE_CPU = 1,
    LS_DEVICE_GPU = 2,
    LS_DEVICE_NPU = 3,
};

typedef struct LatexSnipperBytesV1 {
    /* Borrowed UTF-8/JSON bytes. An empty view may use data == NULL. */
    const uint8_t *data;
    size_t len;
} LatexSnipperBytesV1;

typedef struct LatexSnipperTensorViewV1 {
    LatexSnipperBytesV1 name;
    int32_t dtype;
    const int64_t *shape;
    size_t rank;
    const void *data;
    size_t byte_len;
} LatexSnipperTensorViewV1;

typedef struct LatexSnipperArtifactV1 {
    LatexSnipperBytesV1 role;
    LatexSnipperBytesV1 path;
} LatexSnipperArtifactV1;

typedef struct LatexSnipperSessionCreateRequestV1 {
    const LatexSnipperArtifactV1 *artifacts;
    size_t artifact_count;
    LatexSnipperBytesV1 artifact_options_json;
    LatexSnipperBytesV1 runtime_options_json;
} LatexSnipperSessionCreateRequestV1;

typedef struct LatexSnipperSessionV1 {
    /* On success the host owns handle and calls destroy_session exactly once.
       On failure, a non-NULL handle is also transferred to the host. */
    void *handle;
    /* Session-owned JSON, valid until destroy_session. */
    LatexSnipperBytesV1 metadata_json;
} LatexSnipperSessionV1;

typedef struct LatexSnipperRunRequestV1 {
    uint8_t has_method;
    LatexSnipperBytesV1 method;
    const LatexSnipperTensorViewV1 *inputs;
    size_t input_count;
    uint8_t has_requested_outputs;
    const LatexSnipperBytesV1 *requested_outputs;
    size_t requested_output_count;
} LatexSnipperRunRequestV1;

typedef struct LatexSnipperOwnedTensorListV1 {
    /* Any non-empty ownership representation, including owner != NULL with
       zero tensors, is released by one free_output call. This also applies
       when run returns an error after allocating output. */
    void *owner;
    const LatexSnipperTensorViewV1 *tensors;
    size_t tensor_count;
} LatexSnipperOwnedTensorListV1;

typedef struct LatexSnipperRuntimeDeviceV1 {
    LatexSnipperBytesV1 name;
    int32_t kind;
    uint8_t has_memory_bytes;
    uint64_t memory_bytes;
} LatexSnipperRuntimeDeviceV1;

typedef struct LatexSnipperRuntimeProbeV1 {
    uint8_t available;
    LatexSnipperBytesV1 version;
    LatexSnipperBytesV1 reason_unavailable;
    const LatexSnipperRuntimeDeviceV1 *devices;
    size_t device_count;
    LatexSnipperBytesV1 capabilities_json;
} LatexSnipperRuntimeProbeV1;

typedef int32_t (*LatexSnipperRuntimeProbeFnV1)(LatexSnipperRuntimeProbeV1 *output);
typedef int32_t (*LatexSnipperRuntimeCreateSessionFnV1)(
    const LatexSnipperSessionCreateRequestV1 *request,
    LatexSnipperSessionV1 *output);
typedef void (*LatexSnipperRuntimeDestroySessionFnV1)(void *session);
typedef int32_t (*LatexSnipperRuntimeRunFnV1)(
    void *session,
    const LatexSnipperRunRequestV1 *request,
    LatexSnipperOwnedTensorListV1 *output);
typedef void (*LatexSnipperRuntimeFreeOutputFnV1)(
    void *session,
    LatexSnipperOwnedTensorListV1 *output);
typedef LatexSnipperBytesV1 (*LatexSnipperRuntimeLastErrorFnV1)(void);

typedef struct LatexSnipperRuntimePluginV1 {
    size_t struct_size;
    uint32_t abi_version;
    LatexSnipperBytesV1 runtime_id;
    LatexSnipperBytesV1 plugin_version;
    LatexSnipperRuntimeProbeFnV1 probe;
    LatexSnipperRuntimeCreateSessionFnV1 create_session;
    LatexSnipperRuntimeDestroySessionFnV1 destroy_session;
    LatexSnipperRuntimeRunFnV1 run;
    LatexSnipperRuntimeFreeOutputFnV1 free_output;
    LatexSnipperRuntimeLastErrorFnV1 last_error;
} LatexSnipperRuntimePluginV1;

/* All request/tensor views are borrowed only for the synchronous call.
   Plugins must not retain host pointers and must not unwind C++ exceptions or
   Rust panics across this ABI. A hard native crash terminates the in-process
   host; crash isolation requires a future out-of-process runtime host. */

LS_RUNTIME_PLUGIN_EXPORT const LatexSnipperRuntimePluginV1 *
latexsnipper_runtime_plugin_entry_v1(void);

#ifdef __cplusplus
}
#endif

#endif

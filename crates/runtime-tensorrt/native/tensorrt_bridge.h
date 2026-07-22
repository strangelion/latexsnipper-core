#ifndef LATEXSNIPPER_TENSORRT_BRIDGE_H
#define LATEXSNIPPER_TENSORRT_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#define LS_TRT_EXPORT __declspec(dllexport)
#else
#define LS_TRT_EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

enum LsTrtDataType {
    LS_TRT_FLOAT32 = 0,
    LS_TRT_FLOAT16 = 1,
    LS_TRT_INT64 = 2,
    LS_TRT_INT32 = 3,
    LS_TRT_UINT8 = 4,
    LS_TRT_BOOL = 5,
};

typedef struct LsTrtShapeProfile {
    const char* input_name;
    const int64_t* min_shape;
    const int64_t* opt_shape;
    const int64_t* max_shape;
    size_t rank;
} LsTrtShapeProfile;

typedef struct LsTrtBuildOptions {
    int32_t device_id;
    int32_t precision;
    uint64_t workspace_bytes;
    const LsTrtShapeProfile* profiles;
    size_t profile_count;
} LsTrtBuildOptions;

typedef struct LsTrtTensorView {
    const char* name;
    int32_t dtype;
    const int64_t* shape;
    size_t rank;
    const void* data;
    size_t byte_len;
} LsTrtTensorView;

typedef struct LsTrtTensorInfo {
    const char* name;
    int32_t dtype;
    const int64_t* shape;
    size_t rank;
    const void* data;
    size_t byte_len;
} LsTrtTensorInfo;

typedef struct LsTrtBuffer LsTrtBuffer;
typedef struct LsTrtSession LsTrtSession;
typedef struct LsTrtOutputs LsTrtOutputs;

LS_TRT_EXPORT uint32_t ls_trt_abi_version(void);
LS_TRT_EXPORT const char* ls_trt_runtime_id(void);
LS_TRT_EXPORT const char* ls_trt_last_error(void);
LS_TRT_EXPORT const char* ls_trt_runtime_version(void);
LS_TRT_EXPORT const char* ls_trt_device_fingerprint(int32_t device_id);
LS_TRT_EXPORT uint64_t ls_trt_device_memory(int32_t device_id);

LS_TRT_EXPORT LsTrtBuffer* ls_trt_build_engine(
    const char* onnx_path,
    const LsTrtBuildOptions* options);
LS_TRT_EXPORT const uint8_t* ls_trt_buffer_data(const LsTrtBuffer* buffer);
LS_TRT_EXPORT size_t ls_trt_buffer_size(const LsTrtBuffer* buffer);
LS_TRT_EXPORT void ls_trt_buffer_destroy(LsTrtBuffer* buffer);

LS_TRT_EXPORT LsTrtSession* ls_trt_session_load(
    const uint8_t* engine_data,
    size_t engine_size,
    int32_t device_id);
LS_TRT_EXPORT void ls_trt_session_destroy(LsTrtSession* session);
LS_TRT_EXPORT size_t ls_trt_tensor_count(const LsTrtSession* session, int32_t direction);
LS_TRT_EXPORT int32_t ls_trt_tensor_info(
    const LsTrtSession* session,
    int32_t direction,
    size_t index,
    LsTrtTensorInfo* output);
LS_TRT_EXPORT LsTrtOutputs* ls_trt_session_run(
    LsTrtSession* session,
    const LsTrtTensorView* inputs,
    size_t input_count);
LS_TRT_EXPORT void ls_trt_outputs_destroy(LsTrtOutputs* outputs);
LS_TRT_EXPORT size_t ls_trt_outputs_count(const LsTrtOutputs* outputs);
LS_TRT_EXPORT int32_t ls_trt_output_info(
    const LsTrtOutputs* outputs,
    size_t index,
    LsTrtTensorInfo* output);

#ifdef __cplusplus
}
#endif

#endif

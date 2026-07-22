#ifndef LATEXSNIPPER_EXECUTORCH_BRIDGE_H
#define LATEXSNIPPER_EXECUTORCH_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#define LS_ET_API __declspec(dllexport)
#else
#define LS_ET_API __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct LsEtSession LsEtSession;
typedef struct LsEtOutputs LsEtOutputs;

enum LsEtDataType {
  LS_ET_FLOAT32 = 0,
  LS_ET_FLOAT16 = 1,
  LS_ET_INT64 = 2,
  LS_ET_INT32 = 3,
  LS_ET_UINT8 = 4,
  LS_ET_BOOL = 5,
};

typedef struct LsEtTensorView {
  int32_t dtype;
  const int64_t* shape;
  size_t rank;
  const void* data;
  size_t byte_len;
} LsEtTensorView;

typedef struct LsEtTensorInfo {
  const char* name;
  int32_t dtype;
  const int64_t* shape;
  size_t rank;
  const void* data;
  size_t byte_len;
} LsEtTensorInfo;

LS_ET_API uint32_t ls_et_abi_version(void);
LS_ET_API const char* ls_et_last_error(void);
LS_ET_API const char* ls_et_runtime_version(void);

LS_ET_API LsEtSession* ls_et_session_load(const char* program_path);
LS_ET_API void ls_et_session_destroy(LsEtSession* session);
LS_ET_API size_t ls_et_method_count(const LsEtSession* session);
LS_ET_API const char* ls_et_method_name(
    const LsEtSession* session,
    size_t index);

// `input` is non-zero for inputs and zero for outputs.
LS_ET_API size_t ls_et_tensor_count(
    const LsEtSession* session,
    const char* method,
    int32_t input);
LS_ET_API int32_t ls_et_tensor_info(
    const LsEtSession* session,
    const char* method,
    int32_t input,
    size_t index,
    LsEtTensorInfo* info);

LS_ET_API LsEtOutputs* ls_et_session_run(
    LsEtSession* session,
    const char* method,
    const LsEtTensorView* inputs,
    size_t input_count);
LS_ET_API void ls_et_outputs_destroy(LsEtOutputs* outputs);
LS_ET_API size_t ls_et_outputs_count(const LsEtOutputs* outputs);
LS_ET_API int32_t ls_et_output_info(
    const LsEtOutputs* outputs,
    size_t index,
    LsEtTensorInfo* info);

#ifdef __cplusplus
}
#endif

#endif

#ifndef LATEXSNIPPER_COREML_BRIDGE_H
#define LATEXSNIPPER_COREML_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define LS_COREML_BRIDGE_ABI_VERSION 1u

enum LsCoreMlDataType {
    LS_COREML_FLOAT32 = 0,
    LS_COREML_FLOAT16 = 1,
    LS_COREML_INT32 = 2,
};

enum LsCoreMlComputeUnits {
    LS_COREML_COMPUTE_ALL = 0,
    LS_COREML_COMPUTE_CPU_ONLY = 1,
    LS_COREML_COMPUTE_CPU_AND_GPU = 2,
    LS_COREML_COMPUTE_CPU_AND_NEURAL_ENGINE = 3,
};

typedef struct LsCoreMlSession LsCoreMlSession;
typedef struct LsCoreMlOutputs LsCoreMlOutputs;

typedef struct LsCoreMlTensorView {
    const char *name;
    int32_t dtype;
    const int64_t *shape;
    size_t rank;
    const void *data;
    size_t byte_len;
} LsCoreMlTensorView;

typedef struct LsCoreMlTensorInfo {
    const char *name;
    int32_t dtype;
    const int64_t *shape;
    size_t rank;
    const void *data;
    size_t byte_len;
} LsCoreMlTensorInfo;

uint32_t ls_coreml_bridge_abi_version(void);
const char *ls_coreml_last_error(void);
const char *ls_coreml_runtime_version(void);

int32_t ls_coreml_compile_model(const char *source_path,
                                const char *destination_path);

LsCoreMlSession *ls_coreml_session_create(const char *compiled_model_path,
                                          int32_t compute_units);
void ls_coreml_session_destroy(LsCoreMlSession *session);
size_t ls_coreml_tensor_count(const LsCoreMlSession *session, int32_t input);
int32_t ls_coreml_tensor_info(const LsCoreMlSession *session,
                              int32_t input,
                              size_t index,
                              LsCoreMlTensorInfo *info);

LsCoreMlOutputs *ls_coreml_session_run(LsCoreMlSession *session,
                                       const LsCoreMlTensorView *inputs,
                                       size_t input_count);
void ls_coreml_outputs_destroy(LsCoreMlOutputs *outputs);
size_t ls_coreml_outputs_count(const LsCoreMlOutputs *outputs);
int32_t ls_coreml_output_info(const LsCoreMlOutputs *outputs,
                              size_t index,
                              LsCoreMlTensorInfo *info);

#ifdef __cplusplus
}
#endif

#endif

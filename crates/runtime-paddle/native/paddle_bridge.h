#ifndef LATEXSNIPPER_PADDLE_BRIDGE_H_
#define LATEXSNIPPER_PADDLE_BRIDGE_H_

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#define LS_PADDLE_EXPORT __declspec(dllexport)
#else
#define LS_PADDLE_EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

enum LsPaddleDataType {
  LS_PADDLE_FLOAT32 = 0,
  LS_PADDLE_FLOAT16 = 1,
  LS_PADDLE_INT64 = 2,
  LS_PADDLE_INT32 = 3,
  LS_PADDLE_UINT8 = 4,
  LS_PADDLE_BOOL = 5,
};

typedef struct LsPaddleConfig LsPaddleConfig;
typedef struct LsPaddlePredictor LsPaddlePredictor;
typedef struct LsPaddleTensor LsPaddleTensor;

LS_PADDLE_EXPORT uint32_t ls_paddle_abi_version(void);
LS_PADDLE_EXPORT const char* ls_paddle_last_error(void);
LS_PADDLE_EXPORT const char* ls_paddle_runtime_version(void);

LS_PADDLE_EXPORT LsPaddleConfig* ls_paddle_config_create(void);
LS_PADDLE_EXPORT void ls_paddle_config_destroy(LsPaddleConfig* config);
LS_PADDLE_EXPORT int32_t ls_paddle_config_set_model(
    LsPaddleConfig* config, const char* model, const char* params);
LS_PADDLE_EXPORT int32_t ls_paddle_config_set_cpu_threads(
    LsPaddleConfig* config, int32_t threads);
LS_PADDLE_EXPORT int32_t ls_paddle_config_set_ir_optim(
    LsPaddleConfig* config, int32_t enabled);
LS_PADDLE_EXPORT int32_t ls_paddle_config_set_memory_optim(
    LsPaddleConfig* config, int32_t enabled);
LS_PADDLE_EXPORT int32_t ls_paddle_config_enable_gpu(
    LsPaddleConfig* config, uint64_t memory_pool_mb, int32_t device_id);

LS_PADDLE_EXPORT LsPaddlePredictor* ls_paddle_predictor_create(
    const LsPaddleConfig* config);
LS_PADDLE_EXPORT void ls_paddle_predictor_destroy(
    LsPaddlePredictor* predictor);
LS_PADDLE_EXPORT size_t ls_paddle_predictor_input_count(
    const LsPaddlePredictor* predictor);
LS_PADDLE_EXPORT size_t ls_paddle_predictor_output_count(
    const LsPaddlePredictor* predictor);
LS_PADDLE_EXPORT const char* ls_paddle_predictor_input_name(
    const LsPaddlePredictor* predictor, size_t index);
LS_PADDLE_EXPORT const char* ls_paddle_predictor_output_name(
    const LsPaddlePredictor* predictor, size_t index);
LS_PADDLE_EXPORT LsPaddleTensor* ls_paddle_predictor_input(
    LsPaddlePredictor* predictor, const char* name);
LS_PADDLE_EXPORT LsPaddleTensor* ls_paddle_predictor_output(
    LsPaddlePredictor* predictor, const char* name);
LS_PADDLE_EXPORT int32_t ls_paddle_predictor_run(
    LsPaddlePredictor* predictor);

LS_PADDLE_EXPORT void ls_paddle_tensor_destroy(LsPaddleTensor* tensor);
LS_PADDLE_EXPORT int32_t ls_paddle_tensor_reshape(
    LsPaddleTensor* tensor, const int64_t* shape, size_t rank);
LS_PADDLE_EXPORT size_t ls_paddle_tensor_rank(const LsPaddleTensor* tensor);
LS_PADDLE_EXPORT int64_t ls_paddle_tensor_dimension(
    const LsPaddleTensor* tensor, size_t index);
LS_PADDLE_EXPORT int32_t ls_paddle_tensor_dtype(
    const LsPaddleTensor* tensor);
LS_PADDLE_EXPORT int32_t ls_paddle_tensor_copy_from_cpu(
    LsPaddleTensor* tensor, int32_t dtype, const void* data, size_t byte_len);
LS_PADDLE_EXPORT int32_t ls_paddle_tensor_copy_to_cpu(
    const LsPaddleTensor* tensor, int32_t dtype, void* data, size_t byte_len);

#ifdef __cplusplus
}
#endif

#endif  // LATEXSNIPPER_PADDLE_BRIDGE_H_

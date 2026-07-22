#include "paddle_bridge.h"

#include <cstring>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include "paddle/phi/common/data_type.h"
#include "paddle_inference_api.h"

struct LsPaddleConfig {
  paddle_infer::Config value;
};

struct LsPaddlePredictor {
  std::shared_ptr<paddle_infer::Predictor> value;
  std::vector<std::string> inputs;
  std::vector<std::string> outputs;
};

struct LsPaddleTensor {
  std::unique_ptr<paddle_infer::Tensor> value;
};

namespace {

thread_local std::string g_last_error;
thread_local std::string g_runtime_version;

void ClearError() { g_last_error.clear(); }

void SetError(const char* operation, const std::exception& error) {
  g_last_error = std::string(operation) + ": " + error.what();
}

void SetUnknownError(const char* operation) {
  g_last_error = std::string(operation) + ": unknown native exception";
}

template <typename Function>
int32_t Status(const char* operation, Function&& function) noexcept {
  try {
    ClearError();
    std::forward<Function>(function)();
    return 1;
  } catch (const std::exception& error) {
    SetError(operation, error);
  } catch (...) {
    SetUnknownError(operation);
  }
  return 0;
}

template <typename Result, typename Function>
Result* Pointer(const char* operation, Function&& function) noexcept {
  try {
    ClearError();
    return std::forward<Function>(function)();
  } catch (const std::exception& error) {
    SetError(operation, error);
  } catch (...) {
    SetUnknownError(operation);
  }
  return nullptr;
}

template <typename Function>
size_t Size(const char* operation, Function&& function) noexcept {
  try {
    ClearError();
    return std::forward<Function>(function)();
  } catch (const std::exception& error) {
    SetError(operation, error);
  } catch (...) {
    SetUnknownError(operation);
  }
  return 0;
}

void Require(bool condition, const char* message) {
  if (!condition) {
    throw std::invalid_argument(message);
  }
}

int32_t ToBridgeDtype(paddle_infer::DataType dtype) {
  switch (dtype) {
    case paddle_infer::FLOAT32:
      return LS_PADDLE_FLOAT32;
    case paddle_infer::FLOAT16:
      return LS_PADDLE_FLOAT16;
    case paddle_infer::INT64:
      return LS_PADDLE_INT64;
    case paddle_infer::INT32:
      return LS_PADDLE_INT32;
    case paddle_infer::UINT8:
      return LS_PADDLE_UINT8;
    case paddle_infer::BOOL:
      return LS_PADDLE_BOOL;
    default:
      throw std::invalid_argument("unsupported Paddle tensor data type");
  }
}

size_t ElementSize(int32_t dtype) {
  switch (dtype) {
    case LS_PADDLE_FLOAT32:
    case LS_PADDLE_INT32:
      return 4;
    case LS_PADDLE_FLOAT16:
      return 2;
    case LS_PADDLE_INT64:
      return 8;
    case LS_PADDLE_UINT8:
    case LS_PADDLE_BOOL:
      return 1;
    default:
      throw std::invalid_argument("unknown bridge tensor data type");
  }
}

size_t ElementCount(const paddle_infer::Tensor& tensor) {
  size_t count = 1;
  for (int dimension : tensor.shape()) {
    Require(dimension >= 0, "tensor has a dynamic dimension");
    const auto value = static_cast<size_t>(dimension);
    Require(value == 0 || count <= std::numeric_limits<size_t>::max() / value,
            "tensor element count overflow");
    count *= value;
  }
  return count;
}

void ValidateCopy(const paddle_infer::Tensor& tensor,
                  int32_t dtype,
                  const void* data,
                  size_t byte_len) {
  Require(data != nullptr || byte_len == 0, "tensor data pointer is null");
  Require(ToBridgeDtype(tensor.type()) == dtype,
          "requested data type does not match Paddle tensor");
  const size_t expected = ElementCount(tensor) * ElementSize(dtype);
  Require(byte_len == expected, "tensor byte length does not match its shape");
}

}  // namespace

extern "C" {

uint32_t ls_paddle_abi_version(void) { return 1; }

const char* ls_paddle_last_error(void) { return g_last_error.c_str(); }

const char* ls_paddle_runtime_version(void) {
  try {
    ClearError();
    g_runtime_version = paddle_infer::GetVersion();
    return g_runtime_version.c_str();
  } catch (const std::exception& error) {
    SetError("get Paddle version", error);
  } catch (...) {
    SetUnknownError("get Paddle version");
  }
  return nullptr;
}

LsPaddleConfig* ls_paddle_config_create(void) {
  return Pointer<LsPaddleConfig>("create config", [] {
    return new LsPaddleConfig{};
  });
}

void ls_paddle_config_destroy(LsPaddleConfig* config) { delete config; }

int32_t ls_paddle_config_set_model(LsPaddleConfig* config,
                                   const char* model,
                                   const char* params) {
  return Status("set model", [&] {
    Require(config != nullptr, "config is null");
    Require(model != nullptr, "model path is null");
    Require(params != nullptr, "params path is null");
    config->value.SetModel(model, params);
    // Paddle 3.0's Windows CPU package enables oneDNN by default. The
    // PP-FormulaNet complete inference program contains a while instruction
    // whose scale op fails after oneDNN placement for non-trivial sequences.
    // Keep the first runtime version on the portable CPU kernels; acceleration
    // can be reintroduced as an explicit, model-qualified option later.
    config->value.DisableMKLDNN();
  });
}

int32_t ls_paddle_config_set_cpu_threads(LsPaddleConfig* config,
                                         int32_t threads) {
  return Status("set CPU threads", [&] {
    Require(config != nullptr, "config is null");
    Require(threads > 0, "CPU thread count must be positive");
    config->value.SetCpuMathLibraryNumThreads(threads);
  });
}

int32_t ls_paddle_config_set_ir_optim(LsPaddleConfig* config,
                                      int32_t enabled) {
  return Status("set IR optimization", [&] {
    Require(config != nullptr, "config is null");
    config->value.SwitchIrOptim(enabled != 0);
  });
}

int32_t ls_paddle_config_set_memory_optim(LsPaddleConfig* config,
                                          int32_t enabled) {
  return Status("set memory optimization", [&] {
    Require(config != nullptr, "config is null");
    config->value.EnableMemoryOptim(enabled != 0);
  });
}

int32_t ls_paddle_config_enable_gpu(LsPaddleConfig* config,
                                    uint64_t memory_pool_mb,
                                    int32_t device_id) {
  return Status("enable GPU", [&] {
    Require(config != nullptr, "config is null");
    config->value.EnableUseGpu(memory_pool_mb, device_id,
                               paddle_infer::PrecisionType::kFloat32);
    config->value.EnableCUDNN();
  });
}

LsPaddlePredictor* ls_paddle_predictor_create(
    const LsPaddleConfig* config) {
  return Pointer<LsPaddlePredictor>("create predictor", [&] {
    Require(config != nullptr, "config is null");
    auto value = paddle_infer::CreatePredictor(config->value);
    Require(value != nullptr, "Paddle returned a null predictor");
    auto* predictor = new LsPaddlePredictor{};
    predictor->value = std::move(value);
    predictor->inputs = predictor->value->GetInputNames();
    predictor->outputs = predictor->value->GetOutputNames();
    return predictor;
  });
}

void ls_paddle_predictor_destroy(LsPaddlePredictor* predictor) {
  delete predictor;
}

size_t ls_paddle_predictor_input_count(const LsPaddlePredictor* predictor) {
  return Size("get input count", [&] {
    Require(predictor != nullptr, "predictor is null");
    return predictor->inputs.size();
  });
}

size_t ls_paddle_predictor_output_count(const LsPaddlePredictor* predictor) {
  return Size("get output count", [&] {
    Require(predictor != nullptr, "predictor is null");
    return predictor->outputs.size();
  });
}

const char* ls_paddle_predictor_input_name(
    const LsPaddlePredictor* predictor, size_t index) {
  try {
    ClearError();
    Require(predictor != nullptr, "predictor is null");
    return predictor->inputs.at(index).c_str();
  } catch (const std::exception& error) {
    SetError("get input name", error);
  } catch (...) {
    SetUnknownError("get input name");
  }
  return nullptr;
}

const char* ls_paddle_predictor_output_name(
    const LsPaddlePredictor* predictor, size_t index) {
  try {
    ClearError();
    Require(predictor != nullptr, "predictor is null");
    return predictor->outputs.at(index).c_str();
  } catch (const std::exception& error) {
    SetError("get output name", error);
  } catch (...) {
    SetUnknownError("get output name");
  }
  return nullptr;
}

LsPaddleTensor* ls_paddle_predictor_input(LsPaddlePredictor* predictor,
                                          const char* name) {
  return Pointer<LsPaddleTensor>("get input tensor", [&] {
    Require(predictor != nullptr, "predictor is null");
    Require(name != nullptr, "tensor name is null");
    auto value = predictor->value->GetInputHandle(name);
    Require(value != nullptr, "Paddle returned a null input tensor");
    return new LsPaddleTensor{std::move(value)};
  });
}

LsPaddleTensor* ls_paddle_predictor_output(LsPaddlePredictor* predictor,
                                           const char* name) {
  return Pointer<LsPaddleTensor>("get output tensor", [&] {
    Require(predictor != nullptr, "predictor is null");
    Require(name != nullptr, "tensor name is null");
    auto value = predictor->value->GetOutputHandle(name);
    Require(value != nullptr, "Paddle returned a null output tensor");
    return new LsPaddleTensor{std::move(value)};
  });
}

int32_t ls_paddle_predictor_run(LsPaddlePredictor* predictor) {
  return Status("run predictor", [&] {
    Require(predictor != nullptr, "predictor is null");
    Require(predictor->value->Run(), "Paddle Predictor::Run returned false");
  });
}

void ls_paddle_tensor_destroy(LsPaddleTensor* tensor) { delete tensor; }

int32_t ls_paddle_tensor_reshape(LsPaddleTensor* tensor,
                                 const int64_t* shape,
                                 size_t rank) {
  return Status("reshape tensor", [&] {
    Require(tensor != nullptr, "tensor is null");
    Require(shape != nullptr || rank == 0, "shape pointer is null");
    std::vector<int> dimensions;
    dimensions.reserve(rank);
    for (size_t index = 0; index < rank; ++index) {
      Require(shape[index] >= 0 &&
                  shape[index] <= std::numeric_limits<int>::max(),
              "tensor dimension is outside Paddle's int range");
      dimensions.push_back(static_cast<int>(shape[index]));
    }
    tensor->value->Reshape(dimensions);
  });
}

size_t ls_paddle_tensor_rank(const LsPaddleTensor* tensor) {
  return Size("get tensor rank", [&] {
    Require(tensor != nullptr, "tensor is null");
    return tensor->value->shape().size();
  });
}

int64_t ls_paddle_tensor_dimension(const LsPaddleTensor* tensor,
                                   size_t index) {
  try {
    ClearError();
    Require(tensor != nullptr, "tensor is null");
    return tensor->value->shape().at(index);
  } catch (const std::exception& error) {
    SetError("get tensor dimension", error);
  } catch (...) {
    SetUnknownError("get tensor dimension");
  }
  return std::numeric_limits<int64_t>::min();
}

int32_t ls_paddle_tensor_dtype(const LsPaddleTensor* tensor) {
  try {
    ClearError();
    Require(tensor != nullptr, "tensor is null");
    return ToBridgeDtype(tensor->value->type());
  } catch (const std::exception& error) {
    SetError("get tensor data type", error);
  } catch (...) {
    SetUnknownError("get tensor data type");
  }
  return -1;
}

int32_t ls_paddle_tensor_copy_from_cpu(LsPaddleTensor* tensor,
                                       int32_t dtype,
                                       const void* data,
                                       size_t byte_len) {
  return Status("copy tensor from CPU", [&] {
    Require(tensor != nullptr, "tensor is null");
    ValidateCopy(*tensor->value, dtype, data, byte_len);
    switch (dtype) {
      case LS_PADDLE_FLOAT32:
        tensor->value->CopyFromCpu(static_cast<const float*>(data));
        break;
      case LS_PADDLE_FLOAT16: {
        const size_t count = byte_len / sizeof(uint16_t);
        std::vector<paddle::float16> values(count);
        std::memcpy(values.data(), data, byte_len);
        tensor->value->CopyFromCpu(values.data());
        break;
      }
      case LS_PADDLE_INT64:
        tensor->value->CopyFromCpu(static_cast<const int64_t*>(data));
        break;
      case LS_PADDLE_INT32:
        tensor->value->CopyFromCpu(static_cast<const int32_t*>(data));
        break;
      case LS_PADDLE_UINT8:
        tensor->value->CopyFromCpu(static_cast<const uint8_t*>(data));
        break;
      case LS_PADDLE_BOOL: {
        const auto* bytes = static_cast<const uint8_t*>(data);
        std::unique_ptr<bool[]> values(new bool[byte_len]);
        for (size_t index = 0; index < byte_len; ++index) {
          values[index] = bytes[index] != 0;
        }
        tensor->value->CopyFromCpu(values.get());
        break;
      }
      default:
        throw std::invalid_argument("unknown bridge tensor data type");
    }
  });
}

int32_t ls_paddle_tensor_copy_to_cpu(const LsPaddleTensor* tensor,
                                     int32_t dtype,
                                     void* data,
                                     size_t byte_len) {
  return Status("copy tensor to CPU", [&] {
    Require(tensor != nullptr, "tensor is null");
    ValidateCopy(*tensor->value, dtype, data, byte_len);
    switch (dtype) {
      case LS_PADDLE_FLOAT32:
        tensor->value->CopyToCpu(static_cast<float*>(data));
        break;
      case LS_PADDLE_FLOAT16: {
        const size_t count = byte_len / sizeof(uint16_t);
        std::vector<paddle::float16> values(count);
        tensor->value->CopyToCpu(values.data());
        std::memcpy(data, values.data(), byte_len);
        break;
      }
      case LS_PADDLE_INT64:
        tensor->value->CopyToCpu(static_cast<int64_t*>(data));
        break;
      case LS_PADDLE_INT32:
        tensor->value->CopyToCpu(static_cast<int32_t*>(data));
        break;
      case LS_PADDLE_UINT8:
        tensor->value->CopyToCpu(static_cast<uint8_t*>(data));
        break;
      case LS_PADDLE_BOOL: {
        std::unique_ptr<bool[]> values(new bool[byte_len]);
        tensor->value->CopyToCpu(values.get());
        auto* bytes = static_cast<uint8_t*>(data);
        for (size_t index = 0; index < byte_len; ++index) {
          bytes[index] = values[index] ? 1 : 0;
        }
        break;
      }
      default:
        throw std::invalid_argument("unknown bridge tensor data type");
    }
  });
}

}  // extern "C"

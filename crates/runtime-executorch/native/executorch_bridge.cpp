#include "executorch_bridge.h"

#include <algorithm>
#include <cstring>
#include <exception>
#include <limits>
#include <memory>
#include <new>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <executorch/extension/module/module.h>
#include <executorch/extension/tensor/tensor_ptr.h>
#include <executorch/runtime/core/error.h>
#include <executorch/runtime/core/evalue.h>
#include <executorch/runtime/core/tag.h>

#ifndef LS_EXECUTORCH_VERSION
#define LS_EXECUTORCH_VERSION "unknown"
#endif

namespace {

using executorch::aten::ScalarType;
using executorch::extension::Module;
using executorch::extension::TensorPtr;
using executorch::runtime::EValue;
using executorch::runtime::Error;
using executorch::runtime::MethodMeta;
using executorch::runtime::Tag;
using executorch::runtime::TensorInfo;

thread_local std::string last_error;

struct TensorDescriptor {
  std::string name;
  int32_t dtype = -1;
  std::vector<int64_t> shape;
};

struct MethodDescriptor {
  std::string name;
  std::vector<TensorDescriptor> inputs;
  std::vector<TensorDescriptor> outputs;
};

struct OwnedOutput {
  std::string name;
  int32_t dtype = -1;
  std::vector<int64_t> shape;
  std::vector<uint8_t> bytes;
};

void set_error(std::string message) {
  last_error = std::move(message);
}

std::string error_code(std::string_view operation, Error error) {
  return std::string(operation) + " failed with ExecuTorch error " +
      std::to_string(static_cast<uint32_t>(error));
}

int32_t bridge_dtype(ScalarType type) {
  switch (type) {
    case ScalarType::Float:
      return LS_ET_FLOAT32;
    case ScalarType::Half:
      return LS_ET_FLOAT16;
    case ScalarType::Long:
      return LS_ET_INT64;
    case ScalarType::Int:
      return LS_ET_INT32;
    case ScalarType::Byte:
      return LS_ET_UINT8;
    case ScalarType::Bool:
      return LS_ET_BOOL;
    default:
      return -1;
  }
}

bool executorch_dtype(int32_t type, ScalarType* output) {
  if (output == nullptr) {
    return false;
  }
  switch (type) {
    case LS_ET_FLOAT32:
      *output = ScalarType::Float;
      return true;
    case LS_ET_FLOAT16:
      *output = ScalarType::Half;
      return true;
    case LS_ET_INT64:
      *output = ScalarType::Long;
      return true;
    case LS_ET_INT32:
      *output = ScalarType::Int;
      return true;
    case LS_ET_UINT8:
      *output = ScalarType::Byte;
      return true;
    case LS_ET_BOOL:
      *output = ScalarType::Bool;
      return true;
    default:
      return false;
  }
}

bool describe_tensor(
    const TensorInfo& info,
    std::string fallback_name,
    TensorDescriptor* descriptor) {
  if (descriptor == nullptr) {
    set_error("tensor descriptor destination is null");
    return false;
  }
  descriptor->dtype = bridge_dtype(info.scalar_type());
  if (descriptor->dtype < 0) {
    set_error("program uses an unsupported tensor dtype");
    return false;
  }
  const auto name = info.name();
  descriptor->name = name.empty() ? std::move(fallback_name)
                                  : std::string(name.data(), name.size());
  descriptor->shape.clear();
  descriptor->shape.reserve(info.sizes().size());
  for (const auto dimension : info.sizes()) {
    descriptor->shape.push_back(static_cast<int64_t>(dimension));
  }
  return true;
}

bool describe_values(
    const MethodMeta& metadata,
    bool input,
    std::vector<TensorDescriptor>* descriptors) {
  const size_t count = input ? metadata.num_inputs() : metadata.num_outputs();
  descriptors->clear();
  descriptors->reserve(count);
  for (size_t index = 0; index < count; ++index) {
    auto tag = input ? metadata.input_tag(index) : metadata.output_tag(index);
    if (!tag.ok()) {
      set_error(error_code("query value tag", tag.error()));
      return false;
    }
    if (*tag != Tag::Tensor) {
      set_error(
          std::string(input ? "input" : "output") + " " +
          std::to_string(index) + " is not a tensor");
      return false;
    }
    auto tensor = input ? metadata.input_tensor_meta(index)
                        : metadata.output_tensor_meta(index);
    if (!tensor.ok()) {
      set_error(error_code("query tensor metadata", tensor.error()));
      return false;
    }
    TensorDescriptor descriptor;
    const auto fallback = std::string(input ? "input_" : "output_") +
        std::to_string(index);
    if (!describe_tensor(*tensor, fallback, &descriptor)) {
      return false;
    }
    descriptors->push_back(std::move(descriptor));
  }
  return true;
}

const MethodDescriptor* find_method(
    const std::vector<MethodDescriptor>& methods,
    const char* name) {
  if (name == nullptr) {
    return nullptr;
  }
  const auto iterator = std::find_if(
      methods.begin(), methods.end(),
      [name](const MethodDescriptor& method) { return method.name == name; });
  return iterator == methods.end() ? nullptr : &*iterator;
}

void fill_info(
    const TensorDescriptor& descriptor,
    const void* data,
    size_t byte_len,
    LsEtTensorInfo* info) {
  info->name = descriptor.name.c_str();
  info->dtype = descriptor.dtype;
  info->shape = descriptor.shape.empty() ? nullptr : descriptor.shape.data();
  info->rank = descriptor.shape.size();
  info->data = data;
  info->byte_len = byte_len;
}

bool checked_input_bytes(
    const LsEtTensorView& input,
    ScalarType type,
    size_t* expected) {
  size_t width = 0;
  switch (type) {
    case ScalarType::Float:
    case ScalarType::Int:
      width = 4;
      break;
    case ScalarType::Half:
      width = 2;
      break;
    case ScalarType::Long:
      width = 8;
      break;
    case ScalarType::Byte:
    case ScalarType::Bool:
      width = 1;
      break;
    default:
      set_error("unsupported input tensor dtype");
      return false;
  }
  size_t elements = 1;
  for (size_t index = 0; index < input.rank; ++index) {
    if (input.shape[index] < 0) {
      set_error("input tensor has a negative dimension");
      return false;
    }
    const auto dimension = static_cast<size_t>(input.shape[index]);
    if (dimension != 0 && elements > std::numeric_limits<size_t>::max() / dimension) {
      set_error("input tensor element count overflows size_t");
      return false;
    }
    elements *= dimension;
  }
  if (width != 0 && elements > std::numeric_limits<size_t>::max() / width) {
    set_error("input tensor byte length overflows size_t");
    return false;
  }
  *expected = elements * width;
  return true;
}

} // namespace

struct LsEtSession {
  explicit LsEtSession(const char* path)
      : module(path, Module::LoadMode::File) {}

  Module module;
  std::vector<MethodDescriptor> methods;
};

struct LsEtOutputs {
  std::vector<OwnedOutput> values;
};

extern "C" {

uint32_t ls_et_abi_version(void) {
  return 1;
}

const char* ls_et_last_error(void) {
  return last_error.c_str();
}

const char* ls_et_runtime_version(void) {
  return LS_EXECUTORCH_VERSION;
}

LsEtSession* ls_et_session_load(const char* program_path) {
  last_error.clear();
  if (program_path == nullptr || program_path[0] == '\0') {
    set_error("program path is empty");
    return nullptr;
  }
  try {
    auto session = std::make_unique<LsEtSession>(program_path);
    const auto load_error = session->module.load();
    if (load_error != Error::Ok) {
      set_error(error_code("load program", load_error));
      return nullptr;
    }
    auto method_names = session->module.method_names();
    if (!method_names.ok()) {
      set_error(error_code("query method names", method_names.error()));
      return nullptr;
    }
    std::vector<std::string> names(method_names->begin(), method_names->end());
    std::sort(names.begin(), names.end());
    session->methods.reserve(names.size());
    for (const auto& name : names) {
      auto metadata = session->module.method_meta(name);
      if (!metadata.ok()) {
        set_error(error_code("query method metadata", metadata.error()));
        return nullptr;
      }
      MethodDescriptor method;
      method.name = name;
      if (!describe_values(*metadata, true, &method.inputs) ||
          !describe_values(*metadata, false, &method.outputs)) {
        return nullptr;
      }
      session->methods.push_back(std::move(method));
    }
    return session.release();
  } catch (const std::exception& exception) {
    set_error(std::string("load program threw an exception: ") + exception.what());
  } catch (...) {
    set_error("load program threw an unknown exception");
  }
  return nullptr;
}

void ls_et_session_destroy(LsEtSession* session) {
  delete session;
}

size_t ls_et_method_count(const LsEtSession* session) {
  return session == nullptr ? 0 : session->methods.size();
}

const char* ls_et_method_name(const LsEtSession* session, size_t index) {
  if (session == nullptr || index >= session->methods.size()) {
    set_error("method index is out of range");
    return nullptr;
  }
  return session->methods[index].name.c_str();
}

size_t ls_et_tensor_count(
    const LsEtSession* session,
    const char* method,
    int32_t input) {
  if (session == nullptr) {
    set_error("session is null");
    return 0;
  }
  const auto* descriptor = find_method(session->methods, method);
  if (descriptor == nullptr) {
    set_error("method is not present in the program");
    return 0;
  }
  return input != 0 ? descriptor->inputs.size() : descriptor->outputs.size();
}

int32_t ls_et_tensor_info(
    const LsEtSession* session,
    const char* method,
    int32_t input,
    size_t index,
    LsEtTensorInfo* info) {
  if (session == nullptr || info == nullptr) {
    set_error("session or tensor info destination is null");
    return 0;
  }
  const auto* descriptor = find_method(session->methods, method);
  if (descriptor == nullptr) {
    set_error("method is not present in the program");
    return 0;
  }
  const auto& tensors = input != 0 ? descriptor->inputs : descriptor->outputs;
  if (index >= tensors.size()) {
    set_error("tensor index is out of range");
    return 0;
  }
  fill_info(tensors[index], nullptr, 0, info);
  return 1;
}

LsEtOutputs* ls_et_session_run(
    LsEtSession* session,
    const char* method_name,
    const LsEtTensorView* inputs,
    size_t input_count) {
  last_error.clear();
  if (session == nullptr) {
    set_error("session is null");
    return nullptr;
  }
  const auto* method = find_method(session->methods, method_name);
  if (method == nullptr) {
    set_error("method is not present in the program");
    return nullptr;
  }
  if (input_count != method->inputs.size() || (input_count != 0 && inputs == nullptr)) {
    set_error("input count does not match method metadata");
    return nullptr;
  }
  try {
    std::vector<TensorPtr> tensor_owners;
    std::vector<EValue> values;
    tensor_owners.reserve(input_count);
    values.reserve(input_count);
    for (size_t index = 0; index < input_count; ++index) {
      const auto& input = inputs[index];
      if (input.rank != 0 && input.shape == nullptr) {
        set_error("input tensor shape is null");
        return nullptr;
      }
      ScalarType scalar_type;
      if (!executorch_dtype(input.dtype, &scalar_type)) {
        set_error("input tensor dtype code is unsupported");
        return nullptr;
      }
      size_t expected_bytes = 0;
      if (!checked_input_bytes(input, scalar_type, &expected_bytes)) {
        return nullptr;
      }
      if (input.byte_len != expected_bytes ||
          (input.byte_len != 0 && input.data == nullptr)) {
        set_error("input tensor byte length does not match its shape and dtype");
        return nullptr;
      }
      std::vector<executorch::aten::SizesType> shape;
      shape.reserve(input.rank);
      for (size_t dimension = 0; dimension < input.rank; ++dimension) {
        const auto value = input.shape[dimension];
        if (value < std::numeric_limits<executorch::aten::SizesType>::min() ||
            value > std::numeric_limits<executorch::aten::SizesType>::max()) {
          set_error("input tensor dimension is outside ExecuTorch's range");
          return nullptr;
        }
        shape.push_back(static_cast<executorch::aten::SizesType>(value));
      }
      std::vector<uint8_t> bytes(input.byte_len);
      if (input.byte_len != 0) {
        std::memcpy(bytes.data(), input.data, input.byte_len);
      }
      auto tensor = executorch::extension::make_tensor_ptr(
          std::move(shape), std::move(bytes), scalar_type);
      values.emplace_back(*tensor);
      tensor_owners.push_back(std::move(tensor));
    }

    auto result = session->module.execute(method->name, values);
    if (!result.ok()) {
      set_error(error_code("execute method", result.error()));
      return nullptr;
    }
    if (result->size() != method->outputs.size()) {
      set_error("method output count does not match program metadata");
      return nullptr;
    }

    auto outputs = std::make_unique<LsEtOutputs>();
    outputs->values.reserve(result->size());
    for (size_t index = 0; index < result->size(); ++index) {
      const auto& value = result->at(index);
      if (!value.isTensor()) {
        set_error("method returned a non-tensor output");
        return nullptr;
      }
      const auto& tensor = value.toTensor();
      OwnedOutput output;
      output.name = method->outputs[index].name;
      output.dtype = bridge_dtype(tensor.scalar_type());
      if (output.dtype < 0) {
        set_error("method returned an unsupported tensor dtype");
        return nullptr;
      }
      output.shape.reserve(tensor.sizes().size());
      for (const auto dimension : tensor.sizes()) {
        output.shape.push_back(static_cast<int64_t>(dimension));
      }
      output.bytes.resize(tensor.nbytes());
      if (!output.bytes.empty()) {
        const void* data = tensor.const_data_ptr();
        if (data == nullptr) {
          set_error("method returned a null tensor data pointer");
          return nullptr;
        }
        std::memcpy(output.bytes.data(), data, output.bytes.size());
      }
      outputs->values.push_back(std::move(output));
    }
    return outputs.release();
  } catch (const std::exception& exception) {
    set_error(std::string("execute method threw an exception: ") + exception.what());
  } catch (...) {
    set_error("execute method threw an unknown exception");
  }
  return nullptr;
}

void ls_et_outputs_destroy(LsEtOutputs* outputs) {
  delete outputs;
}

size_t ls_et_outputs_count(const LsEtOutputs* outputs) {
  return outputs == nullptr ? 0 : outputs->values.size();
}

int32_t ls_et_output_info(
    const LsEtOutputs* outputs,
    size_t index,
    LsEtTensorInfo* info) {
  if (outputs == nullptr || info == nullptr || index >= outputs->values.size()) {
    set_error("output index is out of range or destination is null");
    return 0;
  }
  const auto& output = outputs->values[index];
  // Output strings/shapes must be borrowed from the owner, not this temporary.
  info->name = output.name.c_str();
  info->dtype = output.dtype;
  info->shape = output.shape.empty() ? nullptr : output.shape.data();
  info->rank = output.shape.size();
  info->data = output.bytes.empty() ? nullptr : output.bytes.data();
  info->byte_len = output.bytes.size();
  return 1;
}

} // extern "C"

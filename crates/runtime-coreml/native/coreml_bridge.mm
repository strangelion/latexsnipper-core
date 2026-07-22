#import <CoreML/CoreML.h>
#import <Foundation/Foundation.h>
#import <dispatch/dispatch.h>

#include "coreml_bridge.h"

#include <algorithm>
#include <cstring>
#include <limits>
#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace {

thread_local std::string g_last_error;

void set_error(const std::string &message) { g_last_error = message; }

void clear_error() { g_last_error.clear(); }

std::string ns_error(NSError *error, const char *fallback) {
    if (error == nil) {
        return fallback;
    }
    NSString *description = error.localizedDescription;
    return description == nil ? fallback : description.UTF8String;
}

NSString *utf8_string(const char *value, const char *label) {
    if (value == nullptr) {
        set_error(std::string(label) + " is null");
        return nil;
    }
    NSString *result = [NSString stringWithUTF8String:value];
    if (result == nil) {
        set_error(std::string(label) + " is not valid UTF-8");
    }
    return result;
}

size_t dtype_width(int32_t dtype) {
    switch (dtype) {
    case LS_COREML_FLOAT32:
    case LS_COREML_INT32:
        return 4;
    case LS_COREML_FLOAT16:
        return 2;
    default:
        return 0;
    }
}

bool coreml_dtype(MLMultiArrayDataType data_type, int32_t &dtype) {
    switch (data_type) {
    case MLMultiArrayDataTypeFloat32:
        dtype = LS_COREML_FLOAT32;
        return true;
    case MLMultiArrayDataTypeFloat16:
        dtype = LS_COREML_FLOAT16;
        return true;
    case MLMultiArrayDataTypeInt32:
        dtype = LS_COREML_INT32;
        return true;
    default:
        return false;
    }
}

bool mlmultiarray_dtype(int32_t dtype, MLMultiArrayDataType &data_type) {
    switch (dtype) {
    case LS_COREML_FLOAT32:
        data_type = MLMultiArrayDataTypeFloat32;
        return true;
    case LS_COREML_FLOAT16:
        data_type = MLMultiArrayDataTypeFloat16;
        return true;
    case LS_COREML_INT32:
        data_type = MLMultiArrayDataTypeInt32;
        return true;
    default:
        return false;
    }
}

bool checked_element_count(const std::vector<int64_t> &shape, size_t &count) {
    count = 1;
    for (int64_t dimension : shape) {
        if (dimension < 0 ||
            static_cast<uint64_t>(dimension) > std::numeric_limits<size_t>::max()) {
            return false;
        }
        const size_t value = static_cast<size_t>(dimension);
        if (value != 0 && count > std::numeric_limits<size_t>::max() / value) {
            return false;
        }
        count *= value;
    }
    return true;
}

std::vector<int64_t> number_vector(NSArray<NSNumber *> *numbers) {
    std::vector<int64_t> result;
    result.reserve(numbers.count);
    for (NSNumber *number in numbers) {
        result.push_back(number.longLongValue);
    }
    return result;
}

NSArray<NSNumber *> *number_array(const int64_t *values, size_t count) {
    NSMutableArray<NSNumber *> *result = [NSMutableArray arrayWithCapacity:count];
    for (size_t index = 0; index < count; ++index) {
        [result addObject:@(values[index])];
    }
    return result;
}

struct TensorMetadata {
    std::string name;
    int32_t dtype = -1;
    std::vector<int64_t> shape;
};

struct OwnedTensor {
    std::string name;
    int32_t dtype = -1;
    std::vector<int64_t> shape;
    std::vector<uint8_t> data;
};

bool copy_row_major_to_multiarray(const void *source,
                                  size_t source_bytes,
                                  MLMultiArray *destination,
                                  int32_t dtype,
                                  std::string &failure) {
    const size_t width = dtype_width(dtype);
    const std::vector<int64_t> shape = number_vector(destination.shape);
    const std::vector<int64_t> strides = number_vector(destination.strides);
    size_t count = 0;
    if (width == 0 || !checked_element_count(shape, count) ||
        count > std::numeric_limits<size_t>::max() / width ||
        source_bytes != count * width || (source_bytes != 0 && source == nullptr)) {
        failure = "input tensor has an invalid shape or byte length";
        return false;
    }
    uint8_t *target = static_cast<uint8_t *>(destination.dataPointer);
    const uint8_t *bytes = static_cast<const uint8_t *>(source);
    for (size_t flat = 0; flat < count; ++flat) {
        size_t remainder = flat;
        size_t target_index = 0;
        for (size_t axis = shape.size(); axis-- > 0;) {
            const size_t dimension = static_cast<size_t>(shape[axis]);
            const size_t coordinate = dimension == 0 ? 0 : remainder % dimension;
            remainder = dimension == 0 ? 0 : remainder / dimension;
            target_index += coordinate * static_cast<size_t>(strides[axis]);
        }
        std::memcpy(target + target_index * width, bytes + flat * width, width);
    }
    return true;
}

bool copy_multiarray_to_row_major(MLMultiArray *source,
                                  OwnedTensor &destination,
                                  std::string &failure) {
    if (!coreml_dtype(source.dataType, destination.dtype)) {
        failure = "Core ML output MLMultiArray uses an unsupported dtype";
        return false;
    }
    destination.shape = number_vector(source.shape);
    const std::vector<int64_t> strides = number_vector(source.strides);
    const size_t width = dtype_width(destination.dtype);
    size_t count = 0;
    if (!checked_element_count(destination.shape, count) ||
        count > std::numeric_limits<size_t>::max() / width) {
        failure = "Core ML output has an invalid or overflowing shape";
        return false;
    }
    destination.data.resize(count * width);
    const uint8_t *bytes = static_cast<const uint8_t *>(source.dataPointer);
    for (size_t flat = 0; flat < count; ++flat) {
        size_t remainder = flat;
        size_t source_index = 0;
        for (size_t axis = destination.shape.size(); axis-- > 0;) {
            const size_t dimension = static_cast<size_t>(destination.shape[axis]);
            const size_t coordinate = dimension == 0 ? 0 : remainder % dimension;
            remainder = dimension == 0 ? 0 : remainder / dimension;
            source_index += coordinate * static_cast<size_t>(strides[axis]);
        }
        std::memcpy(destination.data.data() + flat * width,
                    bytes + source_index * width,
                    width);
    }
    return true;
}

bool describe_features(NSDictionary<NSString *, MLFeatureDescription *> *descriptions,
                       std::vector<TensorMetadata> &metadata,
                       std::string &failure) {
    NSArray<NSString *> *names =
        [descriptions.allKeys sortedArrayUsingSelector:@selector(compare:)];
    metadata.reserve(names.count);
    for (NSString *name in names) {
        MLFeatureDescription *description = descriptions[name];
        if (description.type != MLFeatureTypeMultiArray ||
            description.multiArrayConstraint == nil) {
            failure = "feature '" + std::string(name.UTF8String) +
                      "' is not an MLMultiArray; image, dictionary, and sequence features are not supported";
            return false;
        }
        TensorMetadata tensor;
        tensor.name = name.UTF8String;
        if (!coreml_dtype(description.multiArrayConstraint.dataType, tensor.dtype)) {
            failure = "feature '" + tensor.name + "' uses an unsupported MLMultiArray dtype";
            return false;
        }
        MLMultiArrayConstraint *constraint = description.multiArrayConstraint;
        tensor.shape = number_vector(constraint.shape);
        if (constraint.shapeConstraint.type != MLMultiArrayShapeConstraintTypeUnspecified) {
            std::fill(tensor.shape.begin(), tensor.shape.end(), -1);
        }
        metadata.push_back(std::move(tensor));
    }
    return true;
}

MLComputeUnits compute_units(int32_t value, bool &valid) {
    valid = true;
    switch (value) {
    case LS_COREML_COMPUTE_ALL:
        return MLComputeUnitsAll;
    case LS_COREML_COMPUTE_CPU_ONLY:
        return MLComputeUnitsCPUOnly;
    case LS_COREML_COMPUTE_CPU_AND_GPU:
        return MLComputeUnitsCPUAndGPU;
    case LS_COREML_COMPUTE_CPU_AND_NEURAL_ENGINE:
        return MLComputeUnitsCPUAndNeuralEngine;
    default:
        valid = false;
        return MLComputeUnitsAll;
    }
}

} // namespace

struct LsCoreMlSession {
    __strong MLModel *model = nil;
    dispatch_queue_t queue = nullptr;
    std::vector<TensorMetadata> inputs;
    std::vector<TensorMetadata> outputs;
};

struct LsCoreMlOutputs {
    std::vector<OwnedTensor> values;
};

extern "C" uint32_t ls_coreml_bridge_abi_version(void) {
    return LS_COREML_BRIDGE_ABI_VERSION;
}

extern "C" const char *ls_coreml_last_error(void) {
    return g_last_error.empty() ? nullptr : g_last_error.c_str();
}

extern "C" const char *ls_coreml_runtime_version(void) {
    @autoreleasepool {
        static std::string version;
        static dispatch_once_t once_token;
        dispatch_once(&once_token, ^{
          NSString *text = NSProcessInfo.processInfo.operatingSystemVersionString;
          version = "Core ML / " + std::string(text.UTF8String);
        });
        return version.c_str();
    }
}

extern "C" int32_t ls_coreml_compile_model(const char *source_path,
                                             const char *destination_path) {
    @autoreleasepool {
        clear_error();
        NSString *source = utf8_string(source_path, "source path");
        NSString *destination = utf8_string(destination_path, "destination path");
        if (source == nil || destination == nil) {
            return -1;
        }
        NSError *error = nil;
        NSURL *compiled = [MLModel compileModelAtURL:[NSURL fileURLWithPath:source]
                                               error:&error];
        if (compiled == nil) {
            set_error(ns_error(error, "Core ML model compilation failed"));
            return -1;
        }
        NSURL *target = [NSURL fileURLWithPath:destination];
        if (![[NSFileManager defaultManager] copyItemAtURL:compiled
                                                    toURL:target
                                                    error:&error]) {
            set_error(ns_error(error, "copy compiled Core ML model failed"));
            return -1;
        }
        return 0;
    }
}

extern "C" LsCoreMlSession *
ls_coreml_session_create(const char *compiled_model_path, int32_t compute_units_value) {
    @autoreleasepool {
        clear_error();
        NSString *path = utf8_string(compiled_model_path, "compiled model path");
        if (path == nil) {
            return nullptr;
        }
        bool valid_units = false;
        MLComputeUnits units = compute_units(compute_units_value, valid_units);
        if (!valid_units) {
            set_error("invalid Core ML compute-units value");
            return nullptr;
        }
        MLModelConfiguration *configuration = [[MLModelConfiguration alloc] init];
        configuration.computeUnits = units;
        NSError *error = nil;
        MLModel *model = [MLModel modelWithContentsOfURL:[NSURL fileURLWithPath:path]
                                          configuration:configuration
                                                  error:&error];
        if (model == nil) {
            set_error(ns_error(error, "load compiled Core ML model failed"));
            return nullptr;
        }
        std::unique_ptr<LsCoreMlSession> session(new LsCoreMlSession());
        session->model = model;
        session->queue = dispatch_queue_create("com.latexsnipper.coreml.session",
                                               DISPATCH_QUEUE_SERIAL);
        std::string failure;
        if (!describe_features(model.modelDescription.inputDescriptionsByName,
                               session->inputs,
                               failure) ||
            !describe_features(model.modelDescription.outputDescriptionsByName,
                               session->outputs,
                               failure)) {
            set_error(failure);
            return nullptr;
        }
        return session.release();
    }
}

extern "C" void ls_coreml_session_destroy(LsCoreMlSession *session) {
    delete session;
}

extern "C" size_t ls_coreml_tensor_count(const LsCoreMlSession *session, int32_t input) {
    if (session == nullptr) {
        set_error("Core ML session is null");
        return 0;
    }
    return input != 0 ? session->inputs.size() : session->outputs.size();
}

extern "C" int32_t ls_coreml_tensor_info(const LsCoreMlSession *session,
                                           int32_t input,
                                           size_t index,
                                           LsCoreMlTensorInfo *info) {
    clear_error();
    if (session == nullptr || info == nullptr) {
        set_error("Core ML session or tensor-info output is null");
        return -1;
    }
    const std::vector<TensorMetadata> &values = input != 0 ? session->inputs : session->outputs;
    if (index >= values.size()) {
        set_error("Core ML tensor metadata index is out of range");
        return -1;
    }
    const TensorMetadata &value = values[index];
    info->name = value.name.c_str();
    info->dtype = value.dtype;
    info->shape = value.shape.data();
    info->rank = value.shape.size();
    info->data = nullptr;
    info->byte_len = 0;
    return 0;
}

extern "C" LsCoreMlOutputs *
ls_coreml_session_run(LsCoreMlSession *session,
                      const LsCoreMlTensorView *inputs,
                      size_t input_count) {
    @autoreleasepool {
        clear_error();
        if (session == nullptr || (input_count != 0 && inputs == nullptr)) {
            set_error("Core ML session or input list is null");
            return nullptr;
        }

        __block LsCoreMlOutputs *result = nullptr;
        __block std::string failure;
        dispatch_sync(session->queue, ^{
          @autoreleasepool {
              NSMutableDictionary<NSString *, MLFeatureValue *> *features =
                  [NSMutableDictionary dictionaryWithCapacity:input_count];
              for (size_t index = 0; index < input_count; ++index) {
                  const LsCoreMlTensorView &view = inputs[index];
                  NSString *name = view.name == nullptr
                                       ? nil
                                       : [NSString stringWithUTF8String:view.name];
                  MLMultiArrayDataType data_type;
                  if (name == nil || !mlmultiarray_dtype(view.dtype, data_type) ||
                      (view.rank != 0 && view.shape == nullptr)) {
                      failure = "Core ML input has an invalid name, dtype, or shape";
                      return;
                  }
                  NSError *error = nil;
                  MLMultiArray *array =
                      [[MLMultiArray alloc] initWithShape:number_array(view.shape, view.rank)
                                                dataType:data_type
                                                   error:&error];
                  if (array == nil) {
                      failure = ns_error(error, "allocate Core ML input MLMultiArray failed");
                      return;
                  }
                  if (!copy_row_major_to_multiarray(view.data,
                                                    view.byte_len,
                                                    array,
                                                    view.dtype,
                                                    failure)) {
                      return;
                  }
                  features[name] = [MLFeatureValue featureValueWithMultiArray:array];
              }
              NSError *error = nil;
              MLDictionaryFeatureProvider *provider =
                  [[MLDictionaryFeatureProvider alloc] initWithDictionary:features
                                                                    error:&error];
              if (provider == nil) {
                  failure = ns_error(error, "create Core ML feature provider failed");
                  return;
              }
              id<MLFeatureProvider> prediction =
                  [session->model predictionFromFeatures:provider error:&error];
              if (prediction == nil) {
                  failure = ns_error(error, "Core ML prediction failed");
                  return;
              }

              std::unique_ptr<LsCoreMlOutputs> outputs(new LsCoreMlOutputs());
              outputs->values.reserve(session->outputs.size());
              for (const TensorMetadata &metadata : session->outputs) {
                  NSString *name = [NSString stringWithUTF8String:metadata.name.c_str()];
                  MLFeatureValue *feature = [prediction featureValueForName:name];
                  if (feature == nil || feature.type != MLFeatureTypeMultiArray ||
                      feature.multiArrayValue == nil) {
                      failure = "Core ML output '" + metadata.name + "' is not an MLMultiArray";
                      return;
                  }
                  OwnedTensor value;
                  value.name = metadata.name;
                  if (!copy_multiarray_to_row_major(feature.multiArrayValue,
                                                    value,
                                                    failure)) {
                      return;
                  }
                  outputs->values.push_back(std::move(value));
              }
              result = outputs.release();
          }
        });
        if (!failure.empty()) {
            set_error(failure);
            return nullptr;
        }
        if (result == nullptr) {
            set_error("Core ML prediction returned no output container");
            return nullptr;
        }
        return result;
    }
}

extern "C" void ls_coreml_outputs_destroy(LsCoreMlOutputs *outputs) {
    delete outputs;
}

extern "C" size_t ls_coreml_outputs_count(const LsCoreMlOutputs *outputs) {
    if (outputs == nullptr) {
        set_error("Core ML outputs are null");
        return 0;
    }
    return outputs->values.size();
}

extern "C" int32_t ls_coreml_output_info(const LsCoreMlOutputs *outputs,
                                           size_t index,
                                           LsCoreMlTensorInfo *info) {
    clear_error();
    if (outputs == nullptr || info == nullptr) {
        set_error("Core ML outputs or tensor-info output is null");
        return -1;
    }
    if (index >= outputs->values.size()) {
        set_error("Core ML output index is out of range");
        return -1;
    }
    const OwnedTensor &value = outputs->values[index];
    info->name = value.name.c_str();
    info->dtype = value.dtype;
    info->shape = value.shape.data();
    info->rank = value.shape.size();
    info->data = value.data.data();
    info->byte_len = value.data.size();
    return 0;
}

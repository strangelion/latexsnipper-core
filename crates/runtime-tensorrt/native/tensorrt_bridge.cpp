#include "tensorrt_bridge.h"

#include <NvInfer.h>
#if !defined(LATEXSNIPPER_TENSORRT_RTX)
#include <NvInferPlugin.h>
#endif
#include <NvInferVersion.h>
#include <NvOnnxParser.h>
#include <cuda_runtime_api.h>

#include <algorithm>
#include <cstring>
#include <limits>
#include <memory>
#include <new>
#include <sstream>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

#if defined(LATEXSNIPPER_TENSORRT_RTX)
#if !defined(TRT_MAJOR_RTX) || !defined(TRT_MINOR_RTX) || TRT_MAJOR_RTX != 1 || TRT_MINOR_RTX < 5
#error "latexsnipper_tensorrt_rtx_bridge requires the TensorRT-RTX 1.5+ C++ ABI"
#endif
#else
#if NV_TENSORRT_MAJOR != 10
#error "latexsnipper_tensorrt_bridge currently targets the TensorRT 10 C++ ABI"
#endif
#endif

namespace {

thread_local std::string g_last_error;
thread_local std::string g_device_fingerprint;

void set_error(std::string message) {
    g_last_error = std::move(message);
}

std::string cuda_error(const char* operation, cudaError_t status) {
    std::ostringstream message;
    message << operation << " failed: " << cudaGetErrorName(status) << " ("
            << cudaGetErrorString(status) << ")";
    return message.str();
}

bool check_cuda(const char* operation, cudaError_t status) {
    if (status == cudaSuccess) {
        return true;
    }
    set_error(cuda_error(operation, status));
    return false;
}

class BridgeLogger final : public nvinfer1::ILogger {
public:
    void log(Severity severity, const char* message) noexcept override {
        if (severity <= Severity::kWARNING && message != nullptr) {
            last_message_ = message;
        }
    }

    const std::string& last_message() const noexcept {
        return last_message_;
    }

private:
    std::string last_message_;
};

BridgeLogger g_logger;

template <typename T>
using TrtPtr = std::unique_ptr<T>;

struct TensorMetadata {
    std::string name;
    int32_t dtype{};
    nvinfer1::DataType native_dtype{};
    nvinfer1::TensorLocation location{};
    std::vector<int64_t> shape;
};

struct OwnedTensor {
    std::string name;
    int32_t dtype{};
    std::vector<int64_t> shape;
    std::vector<uint8_t> data;
};

int32_t bridge_dtype(nvinfer1::DataType dtype) {
    switch (dtype) {
    case nvinfer1::DataType::kFLOAT:
        return LS_TRT_FLOAT32;
    case nvinfer1::DataType::kHALF:
        return LS_TRT_FLOAT16;
    case nvinfer1::DataType::kINT64:
        return LS_TRT_INT64;
    case nvinfer1::DataType::kINT32:
        return LS_TRT_INT32;
    case nvinfer1::DataType::kUINT8:
        return LS_TRT_UINT8;
    case nvinfer1::DataType::kBOOL:
        return LS_TRT_BOOL;
    default:
        return -1;
    }
}

size_t dtype_size(nvinfer1::DataType dtype) {
    switch (dtype) {
    case nvinfer1::DataType::kFLOAT:
        return sizeof(float);
    case nvinfer1::DataType::kHALF:
        return sizeof(uint16_t);
    case nvinfer1::DataType::kINT64:
        return sizeof(int64_t);
    case nvinfer1::DataType::kINT32:
        return sizeof(int32_t);
    case nvinfer1::DataType::kUINT8:
    case nvinfer1::DataType::kBOOL:
        return sizeof(uint8_t);
    default:
        return 0;
    }
}

std::vector<int64_t> copy_dims(const nvinfer1::Dims& dims) {
    if (dims.nbDims < 0) {
        return {};
    }
    return std::vector<int64_t>(dims.d, dims.d + dims.nbDims);
}

bool has_dynamic_dimension(const nvinfer1::Dims& dims) {
    return std::any_of(dims.d, dims.d + dims.nbDims, [](int64_t value) { return value < 0; });
}

bool dims_from_view(const int64_t* values, size_t rank, nvinfer1::Dims& output) {
    if (rank > static_cast<size_t>(nvinfer1::Dims::MAX_DIMS)) {
        set_error("tensor rank exceeds TensorRT Dims::MAX_DIMS");
        return false;
    }
    if (rank > 0 && values == nullptr) {
        set_error("tensor shape pointer is null");
        return false;
    }
    output.nbDims = static_cast<int32_t>(rank);
    for (size_t index = 0; index < rank; ++index) {
        if (values[index] < 0) {
            set_error("runtime tensor shape contains a negative dimension");
            return false;
        }
        output.d[index] = values[index];
    }
    return true;
}

bool checked_bytes(const nvinfer1::Dims& dims, nvinfer1::DataType dtype, size_t& output) {
    const size_t component_size = dtype_size(dtype);
    if (component_size == 0) {
        set_error("TensorRT tensor uses an unsupported data type");
        return false;
    }
    size_t elements = 1;
    for (int32_t index = 0; index < dims.nbDims; ++index) {
        if (dims.d[index] < 0) {
            set_error("tensor shape remains dynamic when allocating a concrete buffer");
            return false;
        }
        const auto dimension = static_cast<uint64_t>(dims.d[index]);
        if (dimension != 0 && elements > std::numeric_limits<size_t>::max() / dimension) {
            set_error("tensor element count overflow");
            return false;
        }
        elements *= static_cast<size_t>(dimension);
    }
    if (component_size != 0 && elements > std::numeric_limits<size_t>::max() / component_size) {
        set_error("tensor byte length overflow");
        return false;
    }
    output = elements * component_size;
    return true;
}

class Allocation {
public:
    Allocation() = default;
    Allocation(const Allocation&) = delete;
    Allocation& operator=(const Allocation&) = delete;

    Allocation(Allocation&& other) noexcept
        : pointer_(std::exchange(other.pointer_, nullptr)),
          host_(std::exchange(other.host_, false)) {}

    Allocation& operator=(Allocation&& other) noexcept {
        if (this != &other) {
            release();
            pointer_ = std::exchange(other.pointer_, nullptr);
            host_ = std::exchange(other.host_, false);
        }
        return *this;
    }

    ~Allocation() {
        release();
    }

    bool allocate(size_t size, bool host) {
        release();
        host_ = host;
        const size_t allocation_size = std::max<size_t>(size, 1);
        cudaError_t status = host ? cudaMallocHost(&pointer_, allocation_size)
                                  : cudaMalloc(&pointer_, allocation_size);
        if (status != cudaSuccess) {
            pointer_ = nullptr;
            set_error(cuda_error(host ? "cudaMallocHost" : "cudaMalloc", status));
            return false;
        }
        return true;
    }

    void* get() const noexcept {
        return pointer_;
    }

private:
    void release() noexcept {
        if (pointer_ == nullptr) {
            return;
        }
        if (host_) {
            cudaFreeHost(pointer_);
        } else {
            cudaFree(pointer_);
        }
        pointer_ = nullptr;
    }

    void* pointer_{};
    bool host_{};
};

class DynamicOutputAllocator final : public nvinfer1::IOutputAllocator {
public:
    explicit DynamicOutputAllocator(bool host) : host_(host) {}

    void* reallocateOutputAsync(
        const char*, void*, uint64_t size, uint64_t, cudaStream_t) noexcept override {
        if (size <= capacity_ && allocation_.get() != nullptr) {
            return allocation_.get();
        }
        if (size > std::numeric_limits<size_t>::max()
            || !allocation_.allocate(static_cast<size_t>(size), host_)) {
            return nullptr;
        }
        capacity_ = size;
        return allocation_.get();
    }

    void notifyShape(const char*, const nvinfer1::Dims& dims) noexcept override {
        shape_ = dims;
        shape_known_ = true;
    }

    void* data() const noexcept {
        return allocation_.get();
    }

    const nvinfer1::Dims& shape() const noexcept {
        return shape_;
    }

    bool shape_known() const noexcept {
        return shape_known_;
    }

private:
    bool host_{};
    Allocation allocation_;
    uint64_t capacity_{};
    nvinfer1::Dims shape_{};
    bool shape_known_{};
};

const LsTrtShapeProfile* find_profile(
    const LsTrtBuildOptions& options, const std::string& input_name) {
    for (size_t index = 0; index < options.profile_count; ++index) {
        const auto& profile = options.profiles[index];
        if (profile.input_name != nullptr && input_name == profile.input_name) {
            return &profile;
        }
    }
    return nullptr;
}

bool profile_dims(
    const LsTrtShapeProfile& profile,
    const int64_t* values,
    const nvinfer1::Dims& network_dims,
    const char* selector,
    nvinfer1::Dims& output) {
    if (!dims_from_view(values, profile.rank, output)) {
        return false;
    }
    if (output.nbDims != network_dims.nbDims) {
        set_error(std::string("profile '") + profile.input_name + "' " + selector
                  + " rank does not match the ONNX input rank");
        return false;
    }
    for (int32_t index = 0; index < output.nbDims; ++index) {
        if (output.d[index] <= 0) {
            set_error(std::string("profile '") + profile.input_name + "' " + selector
                      + " contains a non-positive dimension");
            return false;
        }
        if (network_dims.d[index] >= 0 && output.d[index] != network_dims.d[index]) {
            set_error(std::string("profile '") + profile.input_name + "' " + selector
                      + " changes a static ONNX dimension");
            return false;
        }
    }
    return true;
}

bool configure_profiles(
    nvinfer1::IBuilder& builder,
    nvinfer1::INetworkDefinition& network,
    nvinfer1::IBuilderConfig& config,
    const LsTrtBuildOptions& options) {
    if (options.profile_count > 0 && options.profiles == nullptr) {
        set_error("profile_count is nonzero but profiles is null");
        return false;
    }
    std::unordered_set<std::string> used_profiles;
    nvinfer1::IOptimizationProfile* profile = nullptr;
    bool has_dynamic_input = false;
    for (int32_t index = 0; index < network.getNbInputs(); ++index) {
        nvinfer1::ITensor* input = network.getInput(index);
        if (input == nullptr || input->getName() == nullptr) {
            set_error("ONNX network contains an unnamed input");
            return false;
        }
        const std::string name = input->getName();
        const nvinfer1::Dims network_dims = input->getDimensions();
        if (!has_dynamic_dimension(network_dims)) {
            continue;
        }
        has_dynamic_input = true;
        if (input->isShapeTensor()) {
            set_error("dynamic shape-tensor input '" + name
                      + "' requires explicit shape values, which are not inferred from dimension profiles");
            return false;
        }
        const LsTrtShapeProfile* source = find_profile(options, name);
        if (source == nullptr) {
            set_error("dynamic ONNX input '" + name
                      + "' has no manifest optimization profile; shape guessing is disabled");
            return false;
        }
        if (source->input_name == nullptr || source->min_shape == nullptr
            || source->opt_shape == nullptr || source->max_shape == nullptr) {
            set_error("profile for input '" + name + "' contains a null field");
            return false;
        }
        if (profile == nullptr) {
            profile = builder.createOptimizationProfile();
            if (!profile) {
                set_error("TensorRT failed to create an optimization profile");
                return false;
            }
        }
        nvinfer1::Dims minimum{};
        nvinfer1::Dims optimum{};
        nvinfer1::Dims maximum{};
        if (!profile_dims(*source, source->min_shape, network_dims, "min", minimum)
            || !profile_dims(*source, source->opt_shape, network_dims, "opt", optimum)
            || !profile_dims(*source, source->max_shape, network_dims, "max", maximum)) {
            return false;
        }
        for (int32_t dimension = 0; dimension < minimum.nbDims; ++dimension) {
            if (!(minimum.d[dimension] <= optimum.d[dimension]
                    && optimum.d[dimension] <= maximum.d[dimension])) {
                set_error("profile for input '" + name
                          + "' must satisfy min <= opt <= max for every dimension");
                return false;
            }
        }
        if (!profile->setDimensions(name.c_str(), nvinfer1::OptProfileSelector::kMIN, minimum)
            || !profile->setDimensions(name.c_str(), nvinfer1::OptProfileSelector::kOPT, optimum)
            || !profile->setDimensions(name.c_str(), nvinfer1::OptProfileSelector::kMAX, maximum)) {
            set_error("TensorRT rejected the optimization profile for input '" + name + "'");
            return false;
        }
        used_profiles.insert(name);
    }
    for (size_t index = 0; index < options.profile_count; ++index) {
        const char* name = options.profiles[index].input_name;
        if (name == nullptr || used_profiles.count(name) == 0) {
            set_error(std::string("manifest profile '") + (name == nullptr ? "<null>" : name)
                      + "' does not name a dynamic execution input");
            return false;
        }
    }
    if (has_dynamic_input) {
        if (!profile || !profile->isValid()) {
            set_error("TensorRT optimization profile is invalid");
            return false;
        }
        if (config.addOptimizationProfile(profile) < 0) {
            set_error("TensorRT failed to add the optimization profile");
            return false;
        }
    }
    return true;
}

bool fill_tensor_info(const TensorMetadata& metadata, LsTrtTensorInfo* output) {
    if (output == nullptr) {
        set_error("tensor info output pointer is null");
        return false;
    }
    output->name = metadata.name.c_str();
    output->dtype = metadata.dtype;
    output->shape = metadata.shape.data();
    output->rank = metadata.shape.size();
    output->data = nullptr;
    output->byte_len = 0;
    return true;
}

bool fill_output_info(const OwnedTensor& tensor, LsTrtTensorInfo* output) {
    if (output == nullptr) {
        set_error("output tensor info pointer is null");
        return false;
    }
    output->name = tensor.name.c_str();
    output->dtype = tensor.dtype;
    output->shape = tensor.shape.data();
    output->rank = tensor.shape.size();
    output->data = tensor.data.data();
    output->byte_len = tensor.data.size();
    return true;
}

} // namespace

struct LsTrtBuffer {
    std::vector<uint8_t> bytes;
};

struct LsTrtSession {
    int32_t device_id{};
    TrtPtr<nvinfer1::IRuntime> runtime;
    TrtPtr<nvinfer1::ICudaEngine> engine;
    TrtPtr<nvinfer1::IExecutionContext> context;
    cudaStream_t stream{};
    std::vector<TensorMetadata> inputs;
    std::vector<TensorMetadata> outputs;

    ~LsTrtSession() {
        if (stream != nullptr) {
            cudaStreamDestroy(stream);
        }
    }
};

struct LsTrtOutputs {
    std::vector<OwnedTensor> tensors;
};

extern "C" {

uint32_t ls_trt_abi_version(void) {
    return 1;
}

const char* ls_trt_runtime_id(void) {
#if defined(LATEXSNIPPER_TENSORRT_RTX)
    return "tensorrt-rtx";
#else
    return "tensorrt";
#endif
}

const char* ls_trt_last_error(void) {
    return g_last_error.c_str();
}

const char* ls_trt_runtime_version(void) {
    static const std::string version = std::to_string(NV_TENSORRT_MAJOR) + "."
        + std::to_string(NV_TENSORRT_MINOR) + "." + std::to_string(NV_TENSORRT_PATCH) + "."
        + std::to_string(NV_TENSORRT_BUILD);
    return version.c_str();
}

const char* ls_trt_device_fingerprint(int32_t device_id) {
    g_last_error.clear();
    if (device_id < 0 || !check_cuda("cudaSetDevice", cudaSetDevice(device_id))) {
        return nullptr;
    }
    cudaDeviceProp properties{};
    if (!check_cuda("cudaGetDeviceProperties", cudaGetDeviceProperties(&properties, device_id))) {
        return nullptr;
    }
    char pci_bus_id[32]{};
    if (!check_cuda(
            "cudaDeviceGetPCIBusId",
            cudaDeviceGetPCIBusId(pci_bus_id, static_cast<int>(sizeof(pci_bus_id)), device_id))) {
        return nullptr;
    }
    std::ostringstream fingerprint;
    fingerprint << properties.name << "|pci=" << pci_bus_id << "|sm=" << properties.major << "."
                << properties.minor << "|memory=" << properties.totalGlobalMem;
    g_device_fingerprint = fingerprint.str();
    return g_device_fingerprint.c_str();
}

uint64_t ls_trt_device_memory(int32_t device_id) {
    cudaDeviceProp properties{};
    if (device_id < 0
        || cudaGetDeviceProperties(&properties, device_id) != cudaSuccess) {
        return 0;
    }
    return static_cast<uint64_t>(properties.totalGlobalMem);
}

LsTrtBuffer* ls_trt_build_engine(const char* onnx_path, const LsTrtBuildOptions* options) {
    g_last_error.clear();
    try {
        if (onnx_path == nullptr || options == nullptr) {
            set_error("ONNX path and build options are required");
            return nullptr;
        }
        if (options->device_id < 0
            || !check_cuda("cudaSetDevice", cudaSetDevice(options->device_id))) {
            return nullptr;
        }
#if !defined(LATEXSNIPPER_TENSORRT_RTX)
        if (!initLibNvInferPlugins(&g_logger, "")) {
            set_error("TensorRT plugin initialization failed");
            return nullptr;
        }
#endif
        TrtPtr<nvinfer1::IBuilder> builder{nvinfer1::createInferBuilder(g_logger)};
        if (!builder) {
            set_error("TensorRT failed to create IBuilder");
            return nullptr;
        }
        const uint32_t flags =
#if defined(LATEXSNIPPER_TENSORRT_RTX)
            0U;
#else
            1U << static_cast<uint32_t>(nvinfer1::NetworkDefinitionCreationFlag::kEXPLICIT_BATCH);
#endif
        TrtPtr<nvinfer1::INetworkDefinition> network{builder->createNetworkV2(flags)};
        TrtPtr<nvinfer1::IBuilderConfig> config{builder->createBuilderConfig()};
        if (!network || !config) {
            set_error("TensorRT failed to create network/build configuration");
            return nullptr;
        }
        TrtPtr<nvonnxparser::IParser> parser{nvonnxparser::createParser(*network, g_logger)};
        if (!parser) {
            set_error("TensorRT failed to create the ONNX parser");
            return nullptr;
        }
        if (!parser->parseFromFile(
                onnx_path, static_cast<int32_t>(nvinfer1::ILogger::Severity::kWARNING))) {
            std::ostringstream message;
            message << "failed to parse ONNX model";
            for (int32_t index = 0; index < parser->getNbErrors(); ++index) {
                const nvonnxparser::IParserError* error = parser->getError(index);
                if (error != nullptr) {
                    message << "; " << error->desc();
                }
            }
            set_error(message.str());
            return nullptr;
        }
        if (options->workspace_bytes > 0) {
            config->setMemoryPoolLimit(
                nvinfer1::MemoryPoolType::kWORKSPACE, options->workspace_bytes);
        }
#if defined(LATEXSNIPPER_TENSORRT_RTX)
        if (options->precision != 0) {
            set_error(
                "TensorRT-RTX 1.5 uses strongly typed models; encode FP16/quantization in ONNX "
                "instead of setting the TensorRT 10 precision option");
            return nullptr;
        }
        if (!config->setNbComputeCapabilities(1)
            || !config->setComputeCapability(nvinfer1::ComputeCapability::kCURRENT, 0)) {
            set_error("TensorRT-RTX failed to target the current GPU compute capability");
            return nullptr;
        }
#else
        switch (options->precision) {
        case 0:
            break;
        case 1:
            if (!builder->platformHasFastFp16()) {
                set_error("FP16 precision was requested but the selected GPU has no fast FP16 support");
                return nullptr;
            }
            config->setFlag(nvinfer1::BuilderFlag::kFP16);
            break;
        case 2:
            if (!builder->platformHasFastInt8()) {
                set_error("INT8 precision was requested but the selected GPU has no fast INT8 support");
                return nullptr;
            }
            config->setFlag(nvinfer1::BuilderFlag::kINT8);
            break;
        default:
            set_error("unknown TensorRT precision option");
            return nullptr;
        }
#endif
        if (!configure_profiles(*builder, *network, *config, *options)) {
            return nullptr;
        }
        TrtPtr<nvinfer1::IHostMemory> serialized{
            builder->buildSerializedNetwork(*network, *config)};
        if (!serialized || serialized->data() == nullptr || serialized->size() == 0) {
            set_error("TensorRT failed to build a serialized engine: " + g_logger.last_message());
            return nullptr;
        }
        auto buffer = std::make_unique<LsTrtBuffer>();
        const auto* begin = static_cast<const uint8_t*>(serialized->data());
        buffer->bytes.assign(begin, begin + serialized->size());
        return buffer.release();
    } catch (const std::exception& error) {
        set_error(std::string("engine build threw an exception: ") + error.what());
        return nullptr;
    } catch (...) {
        set_error("engine build threw an unknown exception");
        return nullptr;
    }
}

const uint8_t* ls_trt_buffer_data(const LsTrtBuffer* buffer) {
    return buffer == nullptr ? nullptr : buffer->bytes.data();
}

size_t ls_trt_buffer_size(const LsTrtBuffer* buffer) {
    return buffer == nullptr ? 0 : buffer->bytes.size();
}

void ls_trt_buffer_destroy(LsTrtBuffer* buffer) {
    delete buffer;
}

LsTrtSession* ls_trt_session_load(
    const uint8_t* engine_data, size_t engine_size, int32_t device_id) {
    g_last_error.clear();
    try {
        if (engine_data == nullptr || engine_size == 0 || device_id < 0) {
            set_error("engine bytes and a non-negative device id are required");
            return nullptr;
        }
        if (!check_cuda("cudaSetDevice", cudaSetDevice(device_id))) {
            return nullptr;
        }
#if !defined(LATEXSNIPPER_TENSORRT_RTX)
        if (!initLibNvInferPlugins(&g_logger, "")) {
            set_error("TensorRT plugin initialization failed");
            return nullptr;
        }
#endif
        auto session = std::make_unique<LsTrtSession>();
        session->device_id = device_id;
        session->runtime.reset(nvinfer1::createInferRuntime(g_logger));
        if (!session->runtime) {
            set_error("TensorRT failed to create IRuntime");
            return nullptr;
        }
        session->engine.reset(session->runtime->deserializeCudaEngine(engine_data, engine_size));
        if (!session->engine) {
            set_error("TensorRT failed to deserialize engine: " + g_logger.last_message());
            return nullptr;
        }
        session->context.reset(session->engine->createExecutionContext());
        if (!session->context) {
            set_error("TensorRT failed to create IExecutionContext");
            return nullptr;
        }
        if (!check_cuda("cudaStreamCreate", cudaStreamCreate(&session->stream))) {
            return nullptr;
        }
        for (int32_t index = 0; index < session->engine->getNbIOTensors(); ++index) {
            const char* name = session->engine->getIOTensorName(index);
            if (name == nullptr) {
                set_error("TensorRT engine contains an unnamed I/O tensor");
                return nullptr;
            }
            const nvinfer1::DataType native_dtype = session->engine->getTensorDataType(name);
            const int32_t dtype = bridge_dtype(native_dtype);
            if (dtype < 0) {
                set_error(std::string("TensorRT I/O tensor '") + name
                          + "' uses a data type not supported by latexsnipper-tensor");
                return nullptr;
            }
            TensorMetadata metadata{
                name,
                dtype,
                native_dtype,
                session->engine->getTensorLocation(name),
                copy_dims(session->engine->getTensorShape(name)),
            };
            const auto mode = session->engine->getTensorIOMode(name);
            if (mode == nvinfer1::TensorIOMode::kINPUT) {
                session->inputs.push_back(std::move(metadata));
            } else if (mode == nvinfer1::TensorIOMode::kOUTPUT) {
                session->outputs.push_back(std::move(metadata));
            }
        }
        if (session->inputs.empty() || session->outputs.empty()) {
            set_error("TensorRT engine must expose at least one input and one output");
            return nullptr;
        }
        return session.release();
    } catch (const std::exception& error) {
        set_error(std::string("engine load threw an exception: ") + error.what());
        return nullptr;
    } catch (...) {
        set_error("engine load threw an unknown exception");
        return nullptr;
    }
}

void ls_trt_session_destroy(LsTrtSession* session) {
    delete session;
}

size_t ls_trt_tensor_count(const LsTrtSession* session, int32_t direction) {
    if (session == nullptr) {
        return 0;
    }
    return direction == 0 ? session->inputs.size()
                          : direction == 1 ? session->outputs.size() : 0;
}

int32_t ls_trt_tensor_info(
    const LsTrtSession* session, int32_t direction, size_t index, LsTrtTensorInfo* output) {
    g_last_error.clear();
    if (session == nullptr) {
        set_error("session is null");
        return 0;
    }
    const auto* tensors = direction == 0 ? &session->inputs
                                         : direction == 1 ? &session->outputs : nullptr;
    if (tensors == nullptr || index >= tensors->size()) {
        set_error("tensor metadata index is out of range");
        return 0;
    }
    return fill_tensor_info((*tensors)[index], output) ? 1 : 0;
}

LsTrtOutputs* ls_trt_session_run(
    LsTrtSession* session, const LsTrtTensorView* inputs, size_t input_count) {
    g_last_error.clear();
    try {
        if (session == nullptr || (input_count > 0 && inputs == nullptr)) {
            set_error("session or input array is null");
            return nullptr;
        }
        if (input_count != session->inputs.size()) {
            set_error("input count does not match the TensorRT engine");
            return nullptr;
        }
        if (!check_cuda("cudaSetDevice", cudaSetDevice(session->device_id))) {
            return nullptr;
        }
        std::unordered_map<std::string, const LsTrtTensorView*> provided;
        for (size_t index = 0; index < input_count; ++index) {
            if (inputs[index].name == nullptr
                || !provided.emplace(inputs[index].name, &inputs[index]).second) {
                set_error("input names must be non-null and unique");
                return nullptr;
            }
        }
        std::vector<Allocation> allocations;
        allocations.reserve(session->inputs.size() + session->outputs.size());
        for (const TensorMetadata& metadata : session->inputs) {
            const auto found = provided.find(metadata.name);
            if (found == provided.end()) {
                set_error("missing TensorRT input '" + metadata.name + "'");
                return nullptr;
            }
            const LsTrtTensorView& input = *found->second;
            if (input.dtype != metadata.dtype) {
                set_error("input '" + metadata.name + "' dtype does not match the engine");
                return nullptr;
            }
            nvinfer1::Dims dims{};
            if (!dims_from_view(input.shape, input.rank, dims)) {
                return nullptr;
            }
            const nvinfer1::Dims declared = session->engine->getTensorShape(metadata.name.c_str());
            if (dims.nbDims != declared.nbDims) {
                set_error("input '" + metadata.name + "' rank does not match the engine");
                return nullptr;
            }
            for (int32_t dimension = 0; dimension < dims.nbDims; ++dimension) {
                if (declared.d[dimension] >= 0 && declared.d[dimension] != dims.d[dimension]) {
                    set_error("input '" + metadata.name + "' changes a static engine dimension");
                    return nullptr;
                }
            }
            if (!session->context->setInputShape(metadata.name.c_str(), dims)) {
                set_error("input '" + metadata.name
                          + "' shape is outside the engine optimization profile");
                return nullptr;
            }
            size_t expected_bytes{};
            if (!checked_bytes(dims, metadata.native_dtype, expected_bytes)
                || expected_bytes != input.byte_len
                || (input.byte_len > 0 && input.data == nullptr)) {
                if (g_last_error.empty()) {
                    set_error("input '" + metadata.name + "' byte length is invalid");
                }
                return nullptr;
            }
            const bool host = metadata.location == nvinfer1::TensorLocation::kHOST;
            allocations.emplace_back();
            if (!allocations.back().allocate(expected_bytes, host)) {
                return nullptr;
            }
            if (host) {
                if (expected_bytes > 0) {
                    std::memcpy(allocations.back().get(), input.data, expected_bytes);
                }
            } else if (expected_bytes > 0
                       && !check_cuda(
                           "cudaMemcpyAsync(input)",
                           cudaMemcpyAsync(
                               allocations.back().get(), input.data, expected_bytes,
                               cudaMemcpyHostToDevice, session->stream))) {
                return nullptr;
            }
            if (!session->context->setTensorAddress(
                    metadata.name.c_str(), allocations.back().get())) {
                set_error("TensorRT rejected the address for input '" + metadata.name + "'");
                return nullptr;
            }
        }

        const int32_t missing_count = session->context->inferShapes(0, nullptr);
        if (missing_count < 0) {
            set_error("TensorRT shape inference failed");
            return nullptr;
        }
        if (missing_count > 0) {
            std::vector<const char*> missing(static_cast<size_t>(missing_count));
            session->context->inferShapes(missing_count, missing.data());
            std::ostringstream message;
            message << "TensorRT shape inference is missing inputs";
            for (const char* name : missing) {
                message << " " << (name == nullptr ? "<unknown>" : name);
            }
            set_error(message.str());
            return nullptr;
        }

        struct OutputBinding {
            const TensorMetadata* metadata{};
            nvinfer1::Dims shape{};
            Allocation* allocation{};
            std::unique_ptr<DynamicOutputAllocator> dynamic;
        };
        std::vector<OutputBinding> output_bindings;
        output_bindings.reserve(session->outputs.size());
        for (const TensorMetadata& metadata : session->outputs) {
            OutputBinding binding;
            binding.metadata = &metadata;
            binding.shape = session->context->getTensorShape(metadata.name.c_str());
            const bool host = metadata.location == nvinfer1::TensorLocation::kHOST;
            if (has_dynamic_dimension(binding.shape)) {
                binding.dynamic = std::make_unique<DynamicOutputAllocator>(host);
                if (!session->context->setOutputAllocator(
                        metadata.name.c_str(), binding.dynamic.get())) {
                    set_error("TensorRT rejected the dynamic output allocator for '"
                              + metadata.name + "'");
                    return nullptr;
                }
            } else {
                size_t bytes{};
                if (!checked_bytes(binding.shape, metadata.native_dtype, bytes)) {
                    return nullptr;
                }
                allocations.emplace_back();
                if (!allocations.back().allocate(bytes, host)) {
                    return nullptr;
                }
                binding.allocation = &allocations.back();
                if (!session->context->setTensorAddress(
                        metadata.name.c_str(), binding.allocation->get())) {
                    set_error("TensorRT rejected the address for output '" + metadata.name + "'");
                    return nullptr;
                }
            }
            output_bindings.push_back(std::move(binding));
        }
        if (!session->context->enqueueV3(session->stream)) {
            set_error("TensorRT enqueueV3 failed: " + g_logger.last_message());
            return nullptr;
        }
        if (!check_cuda("cudaStreamSynchronize", cudaStreamSynchronize(session->stream))) {
            return nullptr;
        }

        auto outputs = std::make_unique<LsTrtOutputs>();
        outputs->tensors.reserve(output_bindings.size());
        for (OutputBinding& binding : output_bindings) {
            const TensorMetadata& metadata = *binding.metadata;
            const void* source = nullptr;
            if (binding.dynamic) {
                if (!binding.dynamic->shape_known() || binding.dynamic->data() == nullptr) {
                    set_error("TensorRT did not allocate dynamic output '" + metadata.name + "'");
                    return nullptr;
                }
                binding.shape = binding.dynamic->shape();
                source = binding.dynamic->data();
            } else {
                source = binding.allocation->get();
            }
            size_t bytes{};
            if (!checked_bytes(binding.shape, metadata.native_dtype, bytes)) {
                return nullptr;
            }
            OwnedTensor output;
            output.name = metadata.name;
            output.dtype = metadata.dtype;
            output.shape = copy_dims(binding.shape);
            output.data.resize(bytes);
            if (bytes > 0) {
                if (metadata.location == nvinfer1::TensorLocation::kHOST) {
                    std::memcpy(output.data.data(), source, bytes);
                } else if (!check_cuda(
                               "cudaMemcpy(output)",
                               cudaMemcpy(
                                   output.data.data(), source, bytes, cudaMemcpyDeviceToHost))) {
                    return nullptr;
                }
            }
            outputs->tensors.push_back(std::move(output));
        }
        return outputs.release();
    } catch (const std::exception& error) {
        set_error(std::string("inference threw an exception: ") + error.what());
        return nullptr;
    } catch (...) {
        set_error("inference threw an unknown exception");
        return nullptr;
    }
}

void ls_trt_outputs_destroy(LsTrtOutputs* outputs) {
    delete outputs;
}

size_t ls_trt_outputs_count(const LsTrtOutputs* outputs) {
    return outputs == nullptr ? 0 : outputs->tensors.size();
}

int32_t ls_trt_output_info(
    const LsTrtOutputs* outputs, size_t index, LsTrtTensorInfo* output) {
    g_last_error.clear();
    if (outputs == nullptr || index >= outputs->tensors.size()) {
        set_error("output index is out of range");
        return 0;
    }
    return fill_output_info(outputs->tensors[index], output) ? 1 : 0;
}

} // extern "C"

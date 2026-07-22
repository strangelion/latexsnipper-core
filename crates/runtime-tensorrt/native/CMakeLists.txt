cmake_minimum_required(VERSION 3.24)
project(latexsnipper_tensorrt_bridge LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

option(LATEXSNIPPER_TENSORRT_RTX "Build the TensorRT-RTX 1.5 bridge" OFF)
set(LATEXSNIPPER_TENSORRT_ROOT "" CACHE PATH "TensorRT SDK root")
if(NOT LATEXSNIPPER_TENSORRT_ROOT AND DEFINED ENV{TENSORRT_ROOT})
    set(LATEXSNIPPER_TENSORRT_ROOT "$ENV{TENSORRT_ROOT}")
endif()

find_package(CUDAToolkit REQUIRED)
find_path(TENSORRT_INCLUDE_DIR NvInfer.h
    HINTS "${LATEXSNIPPER_TENSORRT_ROOT}/include")
set(TENSORRT_LIBRARY_HINTS
    "${LATEXSNIPPER_TENSORRT_ROOT}/lib"
    "${LATEXSNIPPER_TENSORRT_ROOT}/lib64")

if(LATEXSNIPPER_TENSORRT_RTX)
    find_library(TENSORRT_NVINFER_LIBRARY NAMES tensorrt_rtx tensorrt_rtx_1_5
        HINTS ${TENSORRT_LIBRARY_HINTS})
    find_library(TENSORRT_ONNX_PARSER_LIBRARY
        NAMES tensorrt_onnxparser_rtx tensorrt_onnxparser_rtx_1_5
        HINTS ${TENSORRT_LIBRARY_HINTS})
else()
    find_library(TENSORRT_NVINFER_LIBRARY NAMES nvinfer nvinfer_10
        HINTS ${TENSORRT_LIBRARY_HINTS})
    find_library(TENSORRT_ONNX_PARSER_LIBRARY NAMES nvonnxparser nvonnxparser_10
        HINTS ${TENSORRT_LIBRARY_HINTS})
    find_library(TENSORRT_PLUGIN_LIBRARY NAMES nvinfer_plugin nvinfer_plugin_10
        HINTS ${TENSORRT_LIBRARY_HINTS})
endif()

if(NOT TENSORRT_INCLUDE_DIR OR NOT TENSORRT_NVINFER_LIBRARY OR
   NOT TENSORRT_ONNX_PARSER_LIBRARY OR
   (NOT LATEXSNIPPER_TENSORRT_RTX AND NOT TENSORRT_PLUGIN_LIBRARY))
    message(FATAL_ERROR
        "Requested TensorRT headers/libraries were not found. Set LATEXSNIPPER_TENSORRT_ROOT or TENSORRT_ROOT.")
endif()

add_library(latexsnipper_tensorrt_bridge SHARED tensorrt_bridge.cpp)
target_include_directories(latexsnipper_tensorrt_bridge PRIVATE "${TENSORRT_INCLUDE_DIR}")
target_link_libraries(latexsnipper_tensorrt_bridge PRIVATE
    "${TENSORRT_NVINFER_LIBRARY}"
    "${TENSORRT_ONNX_PARSER_LIBRARY}"
    CUDA::cudart)
if(LATEXSNIPPER_TENSORRT_RTX)
    target_compile_definitions(latexsnipper_tensorrt_bridge PRIVATE
        LATEXSNIPPER_TENSORRT_RTX=1)
    set_target_properties(latexsnipper_tensorrt_bridge PROPERTIES
        OUTPUT_NAME latexsnipper_tensorrt_rtx_bridge)
else()
    target_link_libraries(latexsnipper_tensorrt_bridge PRIVATE
        "${TENSORRT_PLUGIN_LIBRARY}")
endif()
target_compile_definitions(latexsnipper_tensorrt_bridge PRIVATE
    LATEXSNIPPER_TENSORRT_BRIDGE_BUILD=1)

if(MSVC)
    target_compile_options(latexsnipper_tensorrt_bridge PRIVATE /W4 /EHsc /permissive-)
else()
    target_compile_options(latexsnipper_tensorrt_bridge PRIVATE
        -Wall -Wextra -Wpedantic -Werror=return-type)
    set_target_properties(latexsnipper_tensorrt_bridge PROPERTIES
        BUILD_RPATH "$ORIGIN"
        INSTALL_RPATH "$ORIGIN")
endif()

install(TARGETS latexsnipper_tensorrt_bridge
    RUNTIME DESTINATION bin
    LIBRARY DESTINATION lib)
install(FILES tensorrt_bridge.h DESTINATION include)

cmake_minimum_required(VERSION 3.18)
project(latexsnipper_paddle_bridge LANGUAGES CXX)

set(PADDLE_INFERENCE_ROOT "" CACHE PATH "Path to an extracted Paddle Inference SDK")
if(NOT PADDLE_INFERENCE_ROOT)
  message(FATAL_ERROR "Set PADDLE_INFERENCE_ROOT to the extracted Paddle Inference SDK")
endif()

find_path(PADDLE_INFERENCE_INCLUDE_DIR
  NAMES paddle_inference_api.h
  PATHS "${PADDLE_INFERENCE_ROOT}/paddle/include"
  NO_DEFAULT_PATH)
find_library(PADDLE_INFERENCE_LIBRARY
  NAMES paddle_inference
  PATHS "${PADDLE_INFERENCE_ROOT}/paddle/lib"
  NO_DEFAULT_PATH)
if(NOT PADDLE_INFERENCE_INCLUDE_DIR OR NOT PADDLE_INFERENCE_LIBRARY)
  message(FATAL_ERROR "Paddle headers or library were not found below PADDLE_INFERENCE_ROOT")
endif()

add_library(latexsnipper_paddle_bridge SHARED
  paddle_bridge.cpp
  paddle_bridge.h)
target_compile_features(latexsnipper_paddle_bridge PRIVATE cxx_std_17)
target_include_directories(latexsnipper_paddle_bridge PRIVATE
  "${PADDLE_INFERENCE_INCLUDE_DIR}")
target_link_libraries(latexsnipper_paddle_bridge PRIVATE
  "${PADDLE_INFERENCE_LIBRARY}")
target_compile_definitions(latexsnipper_paddle_bridge PRIVATE
  NOMINMAX WIN32_LEAN_AND_MEAN)

if(MSVC)
  target_compile_options(latexsnipper_paddle_bridge PRIVATE /W4 /EHsc /utf-8)
else()
  target_compile_options(latexsnipper_paddle_bridge PRIVATE
    -Wall -Wextra -Wpedantic)
endif()

install(TARGETS latexsnipper_paddle_bridge
  RUNTIME DESTINATION .
  LIBRARY DESTINATION .)

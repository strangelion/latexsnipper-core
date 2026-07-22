cmake_minimum_required(VERSION 3.24)
project(latexsnipper_executorch_bridge LANGUAGES CXX)

find_package(executorch CONFIG REQUIRED)

set(
  LATEXSNIPPER_EXECUTORCH_VERSION
  "unknown"
  CACHE STRING
  "Version string reported by the packaged ExecuTorch bridge"
)

# Source-tree builds expose namespaced aliases while the installed SDK exports
# the underlying interface targets. Support both official integration modes.
if(TARGET executorch::backends)
  set(_executorch_backends executorch::backends)
  set(_executorch_extensions executorch::extensions)
  set(_executorch_kernels executorch::kernels)
else()
  set(_executorch_backends executorch_backends)
  set(_executorch_extensions executorch_extensions)
  set(_executorch_kernels executorch_kernels)
endif()

add_library(latexsnipper_executorch_bridge SHARED executorch_bridge.cpp)
target_compile_features(latexsnipper_executorch_bridge PRIVATE cxx_std_17)
target_compile_definitions(
  latexsnipper_executorch_bridge
  PRIVATE LS_EXECUTORCH_VERSION="${LATEXSNIPPER_EXECUTORCH_VERSION}"
)
target_link_libraries(
  latexsnipper_executorch_bridge
  PRIVATE
    executorch
    ${_executorch_backends}
    ${_executorch_extensions}
    ${_executorch_kernels}
)

if(MSVC)
  target_compile_options(latexsnipper_executorch_bridge PRIVATE /EHsc /W4)
else()
  target_compile_options(
    latexsnipper_executorch_bridge PRIVATE -Wall -Wextra -Wpedantic
  )
endif()

install(
  TARGETS latexsnipper_executorch_bridge
  RUNTIME DESTINATION bin
  LIBRARY DESTINATION lib
  ARCHIVE DESTINATION lib
)
install(FILES executorch_bridge.h DESTINATION include)

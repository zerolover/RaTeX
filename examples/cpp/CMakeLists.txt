cmake_minimum_required(VERSION 3.20)

project(ratex_cpp_example LANGUAGES CXX)

set(RATEX_DIST_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../dist/ratex-ffi" CACHE PATH "Path to the ratex FFI bundle")
set(RATEX_INCLUDE_DIR "${RATEX_DIST_DIR}/inc/ratex")
set(RATEX_LIB_DIR "${RATEX_DIST_DIR}/libs")

if(WIN32)
    set(RATEX_SHARED_LIB "${RATEX_LIB_DIR}/ratex_ffi.dll")
    set(RATEX_IMPORT_LIB "${RATEX_LIB_DIR}/ratex_ffi.lib")
elseif(APPLE)
    set(RATEX_SHARED_LIB "${RATEX_LIB_DIR}/libratex_ffi.dylib")
else()
    set(RATEX_SHARED_LIB "${RATEX_LIB_DIR}/libratex_ffi.so")
endif()

if(NOT EXISTS "${RATEX_INCLUDE_DIR}/ratex.h")
    message(FATAL_ERROR "RaTeX headers not found under ${RATEX_INCLUDE_DIR}. Build the FFI bundle into ${RATEX_DIST_DIR} first.")
endif()

if(NOT EXISTS "${RATEX_SHARED_LIB}")
    message(FATAL_ERROR "RaTeX shared library not found at ${RATEX_SHARED_LIB}.")
endif()

add_executable(render render.cpp)
target_compile_features(render PRIVATE cxx_std_17)

if(MSVC)
    target_compile_options(render PRIVATE /utf-8)
endif()

add_library(ratex_ffi SHARED IMPORTED GLOBAL)
set_target_properties(
    ratex_ffi
    PROPERTIES
        IMPORTED_LOCATION "${RATEX_SHARED_LIB}"
        INTERFACE_INCLUDE_DIRECTORIES "${RATEX_INCLUDE_DIR}"
)

if(WIN32)
    if(NOT EXISTS "${RATEX_IMPORT_LIB}")
        message(FATAL_ERROR "RaTeX import library not found at ${RATEX_IMPORT_LIB}.")
    endif()

    set_target_properties(ratex_ffi PROPERTIES IMPORTED_IMPLIB "${RATEX_IMPORT_LIB}")
endif()

get_filename_component(RATEX_SHARED_LIB_NAME "${RATEX_SHARED_LIB}" NAME)

target_include_directories(render PRIVATE "${RATEX_INCLUDE_DIR}")
target_link_libraries(render PRIVATE ratex_ffi)

add_custom_command(
    TARGET render
    POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
            "${RATEX_SHARED_LIB}"
            "$<TARGET_FILE_DIR:render>/${RATEX_SHARED_LIB_NAME}"
)

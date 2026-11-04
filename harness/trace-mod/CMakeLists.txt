cmake_minimum_required(VERSION 3.21)
set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_OSX_ARCHITECTURES "arm64")

project(CadenceTrace VERSION 1.0.0)

add_library(${PROJECT_NAME} SHARED src/main.cpp)

if (NOT DEFINED ENV{GEODE_SDK})
    message(FATAL_ERROR "Set GEODE_SDK (see harness/README.md)")
endif()

add_subdirectory($ENV{GEODE_SDK} ${CMAKE_CURRENT_BINARY_DIR}/geode)
setup_geode_mod(${PROJECT_NAME} DONT_INSTALL)

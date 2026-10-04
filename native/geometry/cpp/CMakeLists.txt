cmake_minimum_required(VERSION 3.18)
project(ReShikiGeometry LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
  set(CMAKE_BUILD_TYPE Release CACHE STRING "Build configuration" FORCE)
endif()

# All sources are checked in, compressed, and checksum pinned. This project has
# no FetchContent/ExternalProject/find-package path to third-party libraries.
set(VENDOR_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../vendor")
include("${VENDOR_DIR}/source-hashes.cmake")
foreach(SOURCE_KIND RDKIT BOOST)
  if(SOURCE_KIND STREQUAL "RDKIT")
    set(SOURCE_ARCHIVE "${VENDOR_DIR}/rdkit-geometry-2026.03.6.tar.gz")
  else()
    set(SOURCE_ARCHIVE "${VENDOR_DIR}/boost-geometry-1.92.0.tar.gz")
  endif()
  file(SHA256 "${SOURCE_ARCHIVE}" ACTUAL_SHA256)
  if(NOT ACTUAL_SHA256 STREQUAL "${${SOURCE_KIND}_SOURCE_SHA256}")
    message(FATAL_ERROR "Pinned ${SOURCE_KIND} geometry source archive checksum mismatch")
  endif()
  file(ARCHIVE_EXTRACT INPUT "${SOURCE_ARCHIVE}" DESTINATION "${CMAKE_CURRENT_BINARY_DIR}/sources")
endforeach()

set(RDKIT_CODE "${CMAKE_CURRENT_BINARY_DIR}/sources/rdkit/Code")
set(BOOST_INCLUDE "${CMAKE_CURRENT_BINARY_DIR}/sources/boost")
set(GENERATED_CODE "${CMAKE_CURRENT_BINARY_DIR}/generated/Code")
file(MAKE_DIRECTORY "${GENERATED_CODE}/RDGeneral" "${GENERATED_CODE}/GraphMol/SmilesParse")

# Match the documented geometry-only options. Eigen, URF, dynamic libraries,
# Boost serialization/iostreams, Python, test frameworks and CPU-specific
# instructions are deliberately absent from this build.
set(RDKit_Year 2026)
set(RDKit_intMonth 3)
set(RDKit_Revision 6)
set(RDKit_RELEASENAME "2026.03.6")
set(RDKit_BUILDNAME "${CMAKE_SYSTEM_NAME}|${CMAKE_CXX_COMPILER_ID}|${CMAKE_SYSTEM_PROCESSOR}")
set(Boost_LIB_VERSION "1_92")
set(RDK_BUILD_THREADSAFE_SSS ON)
set(RDK_USE_STRICT_ROTOR_DEFINITION ON)
foreach(NAME versions.h versions.cpp RDConfig.h)
  configure_file("${RDKIT_CODE}/RDGeneral/${NAME}.cmake" "${GENERATED_CODE}/RDGeneral/${NAME}")
endforeach()
# Upstream RDGeneral .cpp files include versions.h/RDConfig.h by sibling name.
# Put generated siblings in this disposable source stage, never upstream/git.
file(COPY "${GENERATED_CODE}/RDGeneral/versions.h" "${GENERATED_CODE}/RDGeneral/RDConfig.h"
     DESTINATION "${RDKIT_CODE}/RDGeneral")
foreach(NAME lex.yysmiles.cpp lex.yysmarts.cpp smiles.tab.cpp smiles.tab.hpp smarts.tab.cpp smarts.tab.hpp)
  configure_file("${RDKIT_CODE}/GraphMol/SmilesParse/${NAME}.cmake" "${GENERATED_CODE}/GraphMol/SmilesParse/${NAME}" COPYONLY)
endforeach()

include(sources.cmake)
set(EXPORT_HEADER "// Static geometry build: exports are internal to the application.\n#pragma once\n")
foreach(NAME ${RDKIT_EXPORT_NAMES})
  string(TOUPPER "${NAME}" NAME_UPPER)
  string(APPEND EXPORT_HEADER "#define RDKIT_${NAME_UPPER}_EXPORT\n")
endforeach()
string(APPEND EXPORT_HEADER "#define RDKIT_QUERY_EXPORT\n")
# Preserve mtime when content is unchanged, so Rust-only edits and enabling the
# proof suite do not needlessly recompile all upstream translation units.
file(CONFIGURE OUTPUT "${GENERATED_CODE}/RDGeneral/export.h" CONTENT "${EXPORT_HEADER}" @ONLY)

find_package(Threads REQUIRED)
add_library(reshiki_geometry STATIC geometry.cpp ${RDKIT_GEOMETRY_SOURCES})
target_include_directories(reshiki_geometry PRIVATE
  "${GENERATED_CODE}" "${GENERATED_CODE}/GraphMol/SmilesParse"
  "${RDKIT_CODE}" "${BOOST_INCLUDE}")
target_compile_definitions(reshiki_geometry PRIVATE
  RDK_BUILD_THREADSAFE_SSS RDK_USE_STRICT_ROTOR_DEFINITION
  BOOST_ALL_NO_LIB BOOST_SYSTEM_NO_DEPRECATED)
target_link_libraries(reshiki_geometry PUBLIC Threads::Threads)
if(MSVC)
  target_compile_definitions(reshiki_geometry PRIVATE YY_NO_UNISTD_H _HAS_AUTO_PTR_ETC=0 BOOST_NO_CXX98_FUNCTION_BASE)
  target_compile_options(reshiki_geometry PRIVATE /wd4267)
elseif(CMAKE_CXX_COMPILER_ID MATCHES "Clang")
  set_source_files_properties(
    "${GENERATED_CODE}/GraphMol/SmilesParse/smarts.tab.cpp"
    "${GENERATED_CODE}/GraphMol/SmilesParse/smiles.tab.cpp"
    PROPERTIES COMPILE_OPTIONS "-Wno-unused-but-set-variable")
endif()
install(TARGETS reshiki_geometry ARCHIVE DESTINATION lib)

option(RESHIKI_GEOMETRY_BUILD_TESTS "Build independent native geometry validation" OFF)
if(RESHIKI_GEOMETRY_BUILD_TESTS)
  enable_testing()
  add_executable(geometry_native_tests geometry_test.cpp)
  target_link_libraries(geometry_native_tests PRIVATE reshiki_geometry)
  add_test(NAME geometry_native_tests COMMAND geometry_native_tests)
endif()

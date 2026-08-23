cmake_minimum_required(VERSION 3.20)
project(cesar NONE)

include(ExternalProject)

set(CSR_CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo")
set(CSR_PROFILE release)

# Auto-detect architecture, mirroring scripts/crossgen.sh / toolchain.cmake
execute_process(COMMAND uname -m OUTPUT_VARIABLE CSR_ARCH OUTPUT_STRIP_TRAILING_WHITESPACE)
if(CSR_ARCH STREQUAL "x86_64")
  set(CSR_RUST_TARGET x86_64-unknown-linux-musl)
elseif(CSR_ARCH STREQUAL "aarch64")
  set(CSR_RUST_TARGET aarch64-unknown-linux-musl)
else()
  message(FATAL_ERROR "Unsupported architecture: ${CSR_ARCH}. Supported: x86_64, aarch64")
endif()

set(CSR_BINARY "${CSR_CARGO_TARGET_DIR}/${CSR_RUST_TARGET}/${CSR_PROFILE}/csr")
set(CSL_BINARY "${CSR_CARGO_TARGET_DIR}/${CSR_RUST_TARGET}/${CSR_PROFILE}/csl")

ExternalProject_Add(csr_build
  SOURCE_DIR "${CMAKE_CURRENT_SOURCE_DIR}"
  BUILD_IN_SOURCE TRUE
  CONFIGURE_COMMAND ""
  BUILD_COMMAND env CARGO_TARGET_DIR=${CSR_CARGO_TARGET_DIR}
    cargo build --profile ${CSR_PROFILE} --locked --target ${CSR_RUST_TARGET}
  BUILD_ALWAYS TRUE
  INSTALL_COMMAND ""
  BUILD_BYPRODUCTS "${CSR_BINARY}" "${CSL_BINARY}"
)

install(FILES "${CSR_BINARY}" "${CSL_BINARY}"
  DESTINATION "${CMAKE_INSTALL_PREFIX}/bin"
  PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE GROUP_READ GROUP_EXECUTE WORLD_READ WORLD_EXECUTE
)
install(FILES docs/csr.1 docs/csl.1
  DESTINATION "${CMAKE_INSTALL_PREFIX}/share/man/man1"
)

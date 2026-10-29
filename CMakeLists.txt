cmake_minimum_required(VERSION 3.21)
project(ZoruMiner VERSION 0.1.1 LANGUAGES CXX)
find_program(CARGO_EXECUTABLE cargo HINTS "$ENV{HOME}/.cargo/bin" REQUIRED)
set(ZORU_CARGO_TARGET_DIR "${CMAKE_CURRENT_SOURCE_DIR}/miner/target" CACHE PATH "Rust build directory")
set(ZORU_CLI_BINARY "${ZORU_CARGO_TARGET_DIR}/release/zoru-miner-cli${CMAKE_EXECUTABLE_SUFFIX}")
file(GLOB_RECURSE ZORU_RUST_SOURCES CONFIGURE_DEPENDS "miner/src/*.rs")
add_custom_command(OUTPUT "${ZORU_CLI_BINARY}"
  COMMAND ${CMAKE_COMMAND} -E env "CARGO_BUILD_JOBS=1" "CARGO_TARGET_DIR=${ZORU_CARGO_TARGET_DIR}" "${CARGO_EXECUTABLE}" build --release --locked --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/miner/Cargo.toml"
  DEPENDS ${ZORU_RUST_SOURCES} miner/Cargo.toml miner/Cargo.lock rust-toolchain.toml
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}" VERBATIM)
add_custom_target(zoru-cli ALL
  COMMAND ${CMAKE_COMMAND} -E copy_if_different "${ZORU_CLI_BINARY}" "${CMAKE_CURRENT_BINARY_DIR}/zoru-miner-cli${CMAKE_EXECUTABLE_SUFFIX}"
  DEPENDS "${ZORU_CLI_BINARY}" VERBATIM)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
find_package(Qt6 6.2 REQUIRED COMPONENTS Core Gui Widgets Network Qml Quick QuickControls2)

set(ZORU_WALLET_TARGET_DIR "${CMAKE_CURRENT_BINARY_DIR}/wallet-target")
if(WIN32)
  set(ZORU_WALLET_FILENAME "zoru_wallet.dll")
elseif(APPLE)
  set(ZORU_WALLET_FILENAME "libzoru_wallet.dylib")
else()
  set(ZORU_WALLET_FILENAME "libzoru_wallet.so")
endif()
set(ZORU_WALLET_LIBRARY "${ZORU_WALLET_TARGET_DIR}/release/${ZORU_WALLET_FILENAME}")
file(GLOB_RECURSE ZORU_WALLET_RUST CONFIGURE_DEPENDS "core/src/*.rs")
add_custom_command(OUTPUT "${ZORU_WALLET_LIBRARY}"
  COMMAND ${CMAKE_COMMAND} -E env "CARGO_BUILD_JOBS=1" "CARGO_TARGET_DIR=${ZORU_WALLET_TARGET_DIR}" "${CARGO_EXECUTABLE}" build --release --locked --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/core/Cargo.toml"
  DEPENDS ${ZORU_WALLET_RUST} core/Cargo.toml core/Cargo.lock
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}" VERBATIM)
add_custom_target(zoru-wallet DEPENDS "${ZORU_WALLET_LIBRARY}")

# Restore the original Qt Quick / QML onboarding and wallet experience.
# Production PoW, proof submission and reward accounting remain in zoru-miner-cli.
qt_add_executable(zoru-miner-gui
  app/src/qml_main.cpp app/src/AppState.cpp app/src/AppState.h)
qt_add_qml_module(zoru-miner-gui URI Zoru VERSION 1.0
  QML_FILES app/qml/Main.qml)
qt_add_resources(zoru-miner-gui "zoru-artwork" PREFIX "/"
  FILES assets/zoru-panda.png assets/eye.png assets/eye-off.png)
target_link_libraries(zoru-miner-gui PRIVATE
  Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Network Qt6::Qml Qt6::Quick Qt6::QuickControls2)
add_dependencies(zoru-miner-gui zoru-cli zoru-wallet)
add_custom_command(TARGET zoru-miner-gui POST_BUILD
  COMMAND ${CMAKE_COMMAND} -E copy_if_different "${ZORU_WALLET_LIBRARY}"
    "$<TARGET_FILE_DIR:zoru-miner-gui>/${ZORU_WALLET_FILENAME}")

# Retain the existing transport regression suite for the production backend.
add_library(zoru-window app/src/Window.cpp app/src/Window.h)
target_link_libraries(zoru-window PUBLIC Qt6::Core Qt6::Gui Qt6::Widgets)
target_include_directories(zoru-window PUBLIC app/src)
include(CTest)
if(BUILD_TESTING)
  find_package(Qt6 REQUIRED COMPONENTS Test)
  add_executable(zoru-gui-tests app/tests/SmokeTest.cpp)
  add_dependencies(zoru-gui-tests zoru-cli)
  target_link_libraries(zoru-gui-tests PRIVATE zoru-window Qt6::Test Qt6::Network)
  add_test(NAME gui-transport COMMAND zoru-gui-tests)
  set_tests_properties(gui-transport PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endif()
install(TARGETS zoru-miner-gui RUNTIME DESTINATION bin)
install(PROGRAMS "${ZORU_CLI_BINARY}" "${ZORU_WALLET_LIBRARY}" DESTINATION bin)

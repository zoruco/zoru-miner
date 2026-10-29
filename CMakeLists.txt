cmake_minimum_required(VERSION 3.21)
file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/miner/Cargo.toml" ZORU_VERSION_LINE REGEX "^version = ")
string(REGEX MATCH "[0-9]+\\.[0-9]+\\.[0-9]+" ZORU_VERSION "${ZORU_VERSION_LINE}")
project(ZORUMiner VERSION ${ZORU_VERSION} LANGUAGES CXX)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/bin")
foreach(config DEBUG RELEASE RELWITHDEBINFO MINSIZEREL)
  set(CMAKE_RUNTIME_OUTPUT_DIRECTORY_${config} "${CMAKE_CURRENT_BINARY_DIR}/bin")
endforeach()
find_program(CARGO_EXECUTABLE cargo HINTS "$ENV{HOME}/.cargo/bin" REQUIRED)
set(ZORU_CARGO_TARGET_DIR "${CMAKE_CURRENT_SOURCE_DIR}/miner/target" CACHE PATH "Rust build directory")
set(ZORU_UPDATE_BINARY "${ZORU_CARGO_TARGET_DIR}/release/zoru-update${CMAKE_EXECUTABLE_SUFFIX}")
set(ZORU_CLI_BINARY "${ZORU_CARGO_TARGET_DIR}/release/zoru-miner-cli${CMAKE_EXECUTABLE_SUFFIX}")
file(GLOB_RECURSE ZORU_RUST_SOURCES CONFIGURE_DEPENDS "miner/src/*.rs")
add_custom_command(OUTPUT "${ZORU_CLI_BINARY}" "${ZORU_UPDATE_BINARY}"
  COMMAND ${CMAKE_COMMAND} -E env "CARGO_BUILD_JOBS=1" "CARGO_TARGET_DIR=${ZORU_CARGO_TARGET_DIR}" "${CARGO_EXECUTABLE}" build --release --locked --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/miner/Cargo.toml"
  DEPENDS ${ZORU_RUST_SOURCES} miner/Cargo.toml miner/Cargo.lock rust-toolchain.toml config/update-public-key.hex
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}" VERBATIM)
add_custom_target(zoru-cli ALL
  COMMAND ${CMAKE_COMMAND} -E make_directory "${CMAKE_CURRENT_BINARY_DIR}/bin"
  COMMAND ${CMAKE_COMMAND} -E copy_if_different "${ZORU_CLI_BINARY}" "${CMAKE_CURRENT_BINARY_DIR}/bin/zoru-miner-cli${CMAKE_EXECUTABLE_SUFFIX}"
  COMMAND ${CMAKE_COMMAND} -E copy_if_different "${ZORU_UPDATE_BINARY}" "${CMAKE_CURRENT_BINARY_DIR}/bin/zoru-update${CMAKE_EXECUTABLE_SUFFIX}"
  DEPENDS "${ZORU_CLI_BINARY}" "${ZORU_UPDATE_BINARY}" VERBATIM)


set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
configure_file(app/src/AppVersion.h.in "${CMAKE_CURRENT_BINARY_DIR}/generated/AppVersion.h" @ONLY)
find_package(Qt6 6.2 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 Network Widgets)
qt_add_executable(zoru-miner-gui
  app/src/main.cpp app/src/AppState.h app/src/AppState.cpp
  app/src/WalletService.cpp app/src/WalletService.h
  app/src/UpdateService.cpp app/src/UpdateService.h)
target_include_directories(zoru-miner-gui PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/generated")
set(ZORU_QML_FILES
  app/qml/Main.qml
  app/qml/components/GlobalStats.qml
  app/qml/components/StatusChip.qml
  app/qml/components/HalvingCard.qml
  app/qml/components/WalletDialog.qml
  app/qml/components/Icon.qml
  app/qml/components/EmptyState.qml
  app/qml/components/Metric.qml
  app/qml/components/LineChart.qml
  app/qml/components/MiningCard.qml
  app/qml/components/AddressText.qml
  app/qml/components/WalletForm.qml
  app/qml/components/ProofTable.qml
  app/qml/components/PasswordField.qml
  app/qml/components/Card.qml
  app/qml/components/ActionButton.qml
  app/qml/components/PageBase.qml
  app/qml/components/NetworkCard.qml
  app/qml/components/UpdateDialog.qml
  app/qml/pages/Nodes.qml
  app/qml/pages/Overview.qml
  app/qml/pages/Help.qml
  app/qml/pages/Miner.qml
  app/qml/pages/Proofs.qml
  app/qml/pages/Wallet.qml
  app/qml/pages/Rewards.qml
  app/qml/pages/ProofDetail.qml
  app/qml/pages/Welcome.qml
  app/qml/pages/Security.qml
  app/qml/pages/Settings.qml
  app/qml/Palette.js)
qt_add_qml_module(zoru-miner-gui URI Zoru VERSION 1.0 QML_FILES ${ZORU_QML_FILES})
set_source_files_properties(assets/zoru-panda.png PROPERTIES QT_RESOURCE_ALIAS "branding/app.png")
set_source_files_properties(assets/zoru-token.png PROPERTIES QT_RESOURCE_ALIAS "branding/token.png")
set_source_files_properties(assets/vex-native.png PROPERTIES QT_RESOURCE_ALIAS "branding/vex.png")
qt_add_resources(zoru-miner-gui "zoru_brand" PREFIX "/"
  FILES assets/zoru-panda.png assets/zoru-token.png assets/vex-native.png)
qt_add_resources(zoru-miner-gui "zoru_help" PREFIX "/docs"
  BASE "${CMAKE_CURRENT_SOURCE_DIR}/docs"
  FILES docs/APP_GUIDE.md docs/ABOUT_ZORU.md docs/UPDATES.md)
target_link_libraries(zoru-miner-gui PRIVATE
  Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::Network Qt6::Widgets)
add_dependencies(zoru-miner-gui zoru-cli)
set(ZORU_DEPLOYMENT_MANIFEST "" CACHE FILEPATH "Reviewed mainnet deployment manifest")
if(ZORU_DEPLOYMENT_MANIFEST)
  configure_file("${ZORU_DEPLOYMENT_MANIFEST}" "${CMAKE_CURRENT_BINARY_DIR}/share/zoru-miner/deployment.json" COPYONLY)
  install(FILES "${ZORU_DEPLOYMENT_MANIFEST}" DESTINATION share/zoru-miner RENAME deployment.json)
endif()
include(CTest)
if(BUILD_TESTING)
  find_package(Qt6 REQUIRED COMPONENTS Test)
  add_executable(zoru-gui-tests app/tests/AppStateTest.cpp app/src/AppState.cpp app/src/AppState.h app/src/WalletService.cpp app/src/WalletService.h app/src/UpdateService.cpp app/src/UpdateService.h)
  target_include_directories(zoru-gui-tests PRIVATE app/src "${CMAKE_CURRENT_BINARY_DIR}/generated")
  target_link_libraries(zoru-gui-tests PRIVATE Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Network Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::Test)
  add_dependencies(zoru-gui-tests zoru-cli)
  qt_add_resources(zoru-gui-tests "zoru-test-qml" PREFIX "/Zoru" FILES ${ZORU_QML_FILES})
  qt_add_resources(zoru-gui-tests "zoru-test-brand" PREFIX "/" FILES assets/zoru-panda.png assets/zoru-token.png assets/vex-native.png)
  qt_add_resources(zoru-gui-tests "zoru-test-help" PREFIX "/docs" BASE "${CMAKE_CURRENT_SOURCE_DIR}/docs" FILES docs/APP_GUIDE.md docs/ABOUT_ZORU.md docs/UPDATES.md)
  add_executable(zoru-update-tests app/tests/UpdateServiceTest.cpp app/src/UpdateService.cpp app/src/UpdateService.h)
  target_include_directories(zoru-update-tests PRIVATE app/src "${CMAKE_CURRENT_BINARY_DIR}/generated")
  target_link_libraries(zoru-update-tests PRIVATE Qt6::Core Qt6::Gui Qt6::Network Qt6::Test)
  add_test(NAME gui-updates COMMAND zoru-update-tests)
  add_executable(zoru-update-dialog-tests app/tests/UpdateDialogTest.cpp)
  qt_add_resources(zoru-update-dialog-tests "zoru-update-test-qml" PREFIX "/Zoru" FILES ${ZORU_QML_FILES})
  target_link_libraries(zoru-update-dialog-tests PRIVATE Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::Test)
  add_test(NAME gui-update-dialog COMMAND zoru-update-dialog-tests)
  set_tests_properties(gui-update-dialog PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
  add_test(NAME gui-state COMMAND zoru-gui-tests)
  set_tests_properties(gui-state PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
  add_executable(zoru-file-dialog-tests app/tests/FileDialogTest.cpp)
  target_link_libraries(zoru-file-dialog-tests PRIVATE Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::Test)
  add_test(NAME gui-file-dialog COMMAND zoru-file-dialog-tests)
  set_tests_properties(gui-file-dialog PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
endif()
install(TARGETS zoru-miner-gui RUNTIME DESTINATION bin)
install(PROGRAMS "${ZORU_CLI_BINARY}" "${ZORU_UPDATE_BINARY}" DESTINATION bin)

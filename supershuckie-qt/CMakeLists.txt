cmake_minimum_required(VERSION 4.0)

project("SuperShuckie"
    VERSION 1.0
    LANGUAGES CXX
)

include(corrosion.cmake)

corrosion_import_crate(MANIFEST_PATH "../supershuckie-frontend-c/Cargo.toml")

set(CMAKE_AUTOMOC ON)

if(${WIN32})
    set(EXE_TYPE "WIN32")
endif()

add_library(supershuckie-cpp-static STATIC
    src/main.cpp
    src/main_window.cpp
    src/render_widget.cpp
    src/file_rw.cpp
    src/game_speed_dialog.cpp
    src/ask_for_text_dialog.cpp
    src/select_item_dialog.cpp
    src/controller_settings_window.cpp
    src/sdl_event_wrapper.cpp
    src/theme.cpp
    src/replay_playback_controls.cpp
)

set_property(TARGET supershuckie-cpp-static PROPERTY CXX_STANDARD 20)

target_include_directories(supershuckie-cpp-static
    PRIVATE "${CMAKE_SOURCE_DIR}/../supershuckie-frontend-c/include"
)

find_package(Qt6 REQUIRED COMPONENTS Core Widgets)
find_package(SDL3 REQUIRED CONFIG REQUIRED COMPONENTS SDL3)

target_link_libraries(supershuckie-cpp-static
    PUBLIC Qt6::Widgets
    PUBLIC Qt6::Core
    PUBLIC SDL3::SDL3
    PUBLIC supershuckie_frontend_c
)

if(SCRIPT_BUILD)
    find_library(MELONDS_CORE NAMES core PATHS ${CMAKE_BINARY_DIR}/melonDS/src/)
    find_library(MELONDS_TEAKRA NAMES teakra PATHS ${CMAKE_BINARY_DIR}/melonDS/src/teakra/src)
    target_link_libraries(supershuckie-cpp-static
        PUBLIC ${MELONDS_CORE}
        PUBLIC ${MELONDS_TEAKRA}
    )
endif()


if(${WIN32})
    target_link_libraries(supershuckie-cpp-static PUBLIC dwmapi)
endif()

add_executable(supershuckie-debug)
target_link_libraries(supershuckie-debug supershuckie-cpp-static)

if(${APPLE})
    set(supershuckie_ICON "${CMAKE_SOURCE_DIR}/supershuckie.icns")
    set_source_files_properties(${supershuckie_ICON} PROPERTIES MACOSX_PACKAGE_LOCATION "Resources")
    add_executable(supershuckie MACOSX_BUNDLE ${supershuckie_ICON})
    set_target_properties(supershuckie PROPERTIES
        MACOSX_BUNDLE_BUNDLE_NAME "SuperShuckie"
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_SOURCE_DIR}/Info.plist.in"
        MACOSX_BUNDLE_ICON_FILE "supershuckie.icns"
        MACOSX_BUNDLE_COPYRIGHT "Snowy"
        MACOSX_BUNDLE_EXECUTABLE_NAME "supershuckie"
        MACOSX_BUNDLE_GUI_IDENTIFIER "com.snowymouse.supershuckie"
        MACOSX_BUNDLE_LONG_VERSION_STRING "2.0"
        MACOSX_BUNDLE_BUNDLE_VERSION "2.0"
        MACOSX_BUNDLE_SHORT_VERSION_STRING "2"
        OUTPUT_NAME "SuperShuckie"
    )
elseif(${WIN32})
    add_executable(supershuckie WIN32
        src/windows.rc
    )
else()
    add_executable(supershuckie)
    set_target_properties(supershuckie-debug PROPERTIES
        OUTPUT_NAME "supershuckie"
    )
endif()

target_link_libraries(supershuckie supershuckie-cpp-static)

cmake_minimum_required(VERSION 3.16)
project(buddy-click-effect VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(ECM REQUIRED NO_MODULE)
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} ${ECM_MODULE_PATH})
include(KDEInstallDirs)

find_package(Qt6 REQUIRED COMPONENTS Core Gui DBus)
find_package(KWin REQUIRED)

add_library(buddy_click MODULE buddy_click.cpp)
target_link_libraries(buddy_click PRIVATE Qt6::Core Qt6::Gui Qt6::DBus KWin::kwin)
configure_file(metadata.json ${CMAKE_CURRENT_BINARY_DIR}/metadata.json COPYONLY)

install(TARGETS buddy_click DESTINATION ${KDE_INSTALL_PLUGINDIR}/kwin/effects/plugins)

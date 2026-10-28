# Relocatable CMake package for the bundled C++ Visualizer adapter.
get_filename_component(_ad_visualization_prefix "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)
if(NOT TARGET AdVisualization::Visualizer)
  add_library(AdVisualization::Visualizer SHARED IMPORTED)
  set_target_properties(AdVisualization::Visualizer PROPERTIES
    IMPORTED_LOCATION "${_ad_visualization_prefix}/lib/libad_visualization_ffi.so"
    INTERFACE_INCLUDE_DIRECTORIES "${_ad_visualization_prefix}/include")
endif()
unset(_ad_visualization_prefix)

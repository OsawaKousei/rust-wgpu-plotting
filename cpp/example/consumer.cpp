#include <ad_visualization/visualizer.hpp>

#include <cstdint>
#include <iostream>
#include <optional>
#include <span>
#include <string>
#include <vector>

// Application-owned types: no Rust or C ABI structs are needed here.
struct AppPoint { double x; double y; };
struct AppPose { double x; double y; double theta; };
struct AppFootprint { std::vector<AppPoint> vertices; };
struct AppMap {
  int width;
  int height;
  double resolution;
  std::vector<std::int8_t> grid;
};
struct AppScan {
  std::vector<double> ranges;
  double minAngle;
  double angleIncrement;
  double maxRange;
};

int main(int argc, char **argv) {
  const std::string output = argc > 1 ? argv[1] : "cpp_visualizer.png";
  const bool show_window = argc > 2 && std::string(argv[2]) == "--window";
  AppMap map{8, 8, 0.25, std::vector<std::int8_t>(64, 0)};
  for (int x = 0; x < map.width; ++x) {
    map.grid[static_cast<std::size_t>(x)] = 1;
    map.grid[static_cast<std::size_t>(56 + x)] = 1;
  }
  auto prepared = ad::visualization::Visualizer::prepareMap(map);
  if (!prepared) {
    std::cerr << prepared.error().message << '\n';
    return 1;
  }
  std::optional<ad::visualization::Visualizer::PreparedMap> prepared_copy;
  prepared_copy.emplace(*prepared);
  const ad::visualization::Visualizer visualizer;
  const auto &geometry = prepared_copy->geometry;
  const std::vector<AppPoint> path{{0.3, 0.4}, {0.7, 0.8}, {1.3, 1.1}, {1.7, 1.5}};
  const AppPose pose{1.3, 1.1, 0.4};
  const AppFootprint footprint{{{-0.1, -0.1}, {0.15, 0.0}, {-0.1, 0.1}}};
  const AppScan scan{{0.3, 0.4, 0.5}, -0.5, 0.5, 2.0};
  const auto check = [](ad::visualization::Status status) {
    if (status) return true;
    std::cerr << status.error().message << '\n';
    return false;
  };
  if (!check(visualizer.renderFrame(*prepared_copy)) ||
      !check(visualizer.renderPath(std::span{path}, geometry, "k--")) ||
      !check(visualizer.renderRobot(pose, footprint, geometry)) ||
      !check(visualizer.renderMarker(AppPoint{0.4, 0.4}, geometry, 30.0, "lime")) ||
      !check(visualizer.renderPoints(std::span{path}, geometry, 10.0, "green")) ||
      !check(visualizer.renderScan(pose, scan, geometry))) return 1;
  const auto invalid_style = visualizer.renderPath(std::span{path}, geometry, "unknown");
  if (invalid_style || invalid_style.error().code != ad::visualization::ErrorCode::InvalidInput) {
    std::cerr << "Error conversion failed\n";
    return 1;
  }
  if (show_window && !check(visualizer.presentFrame())) return 1;
  if (!check(visualizer.saveFigure(output))) return 1;
  std::cout << "Saved " << output << '\n';
  return 0;
}

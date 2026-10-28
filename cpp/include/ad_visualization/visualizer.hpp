#pragma once

#include <ad_visualization_ffi.h>

#include <cmath>
#include <cstdint>
#include <memory>
#include <optional>
#include <ranges>
#include <string>
#include <string_view>
#include <utility>
#include <variant>
#include <vector>

namespace ad::visualization {

enum class ErrorCode { InvalidInput, EmptyCollection, SizeMismatch, BackendFailure };
struct Error {
  ErrorCode code;
  std::string message;
};

class Status {
public:
  Status() = default;
  Status(Error error) : error_(std::move(error)) {}
  [[nodiscard]] explicit operator bool() const noexcept { return !error_.has_value(); }
  [[nodiscard]] const Error &error() const { return error_.value(); }

private:
  std::optional<Error> error_;
};

template <class T> class Result {
public:
  Result(T value) : value_(std::in_place_index<0>, std::move(value)) {}
  Result(Error error) : value_(std::in_place_index<1>, std::move(error)) {}
  [[nodiscard]] explicit operator bool() const noexcept { return value_.index() == 0; }
  [[nodiscard]] T &operator*() { return std::get<0>(value_); }
  [[nodiscard]] const T &operator*() const { return std::get<0>(value_); }
  [[nodiscard]] T *operator->() { return &std::get<0>(value_); }
  [[nodiscard]] const T *operator->() const { return &std::get<0>(value_); }
  [[nodiscard]] T &value() { return std::get<0>(value_); }
  [[nodiscard]] const T &value() const { return std::get<0>(value_); }
  [[nodiscard]] const Error &error() const { return std::get<1>(value_); }

private:
  std::variant<T, Error> value_;
};

struct Point { double x; double y; };
struct Pose { double x; double y; double theta; };
struct Footprint { std::vector<Point> vertices; };
struct MapData {
  int width;
  int height;
  double resolution;
  std::vector<std::int8_t> grid;
};
struct LidarScan {
  std::vector<double> ranges;
  double minAngle;
  double angleIncrement;
  double maxRange;
};
struct MapGeometry { int width; int height; double resolution; };

class PreparedMap {
public:
  const MapGeometry geometry;
  PreparedMap(MapGeometry geometry, std::shared_ptr<AdPreparedMap> handle)
      : geometry(geometry), handle_(std::move(handle)) {}

private:
  std::shared_ptr<AdPreparedMap> handle_;
  friend class Visualizer;
};

namespace detail {
inline Error ffi_error(std::int32_t code, const AdVizError &error) {
  const auto kind = [code] {
    switch (code) {
    case AD_VIZ_INVALID_INPUT: return ErrorCode::InvalidInput;
    case AD_VIZ_EMPTY_COLLECTION: return ErrorCode::EmptyCollection;
    case AD_VIZ_SIZE_MISMATCH: return ErrorCode::SizeMismatch;
    default: return ErrorCode::BackendFailure;
    }
  }();
  return {kind, error.message[0] != '\0' ? std::string(error.message)
                                           : std::string("Visualizer backend failed")};
}
inline Status status(std::int32_t code, const AdVizError &error) {
  return code == AD_VIZ_OK ? Status{} : Status{ffi_error(code, error)};
}
inline Result<AdVizGeometry> geometry(MapGeometry map) {
  if (map.width <= 0 || map.height <= 0 || !std::isfinite(map.resolution) ||
      map.resolution <= 0.0) {
    return Error{ErrorCode::InvalidInput, "Map geometry must be positive and finite"};
  }
  return AdVizGeometry{static_cast<std::uint32_t>(map.width),
                       static_cast<std::uint32_t>(map.height), map.resolution};
}
template <class P> AdVizPoint point(const P &value) {
  return {static_cast<double>(value.x), static_cast<double>(value.y)};
}
template <class P> AdVizPose pose(const P &value) {
  return {static_cast<double>(value.x), static_cast<double>(value.y),
          static_cast<double>(value.theta)};
}
template <std::ranges::input_range Range>
std::vector<AdVizPoint> points(const Range &range) {
  std::vector<AdVizPoint> result;
  if constexpr (std::ranges::sized_range<Range>) result.reserve(std::ranges::size(range));
  for (const auto &value : range) result.push_back(point(value));
  return result;
}
inline const std::uint8_t *bytes(std::string_view value) {
  return reinterpret_cast<const std::uint8_t *>(value.data());
}
template <class Scan> double min_angle(const Scan &scan) {
  if constexpr (requires { scan.minAngle; }) return static_cast<double>(scan.minAngle);
  else return static_cast<double>(scan.min_angle);
}
template <class Scan> double angle_increment(const Scan &scan) {
  if constexpr (requires { scan.angleIncrement; }) return static_cast<double>(scan.angleIncrement);
  else return static_cast<double>(scan.angle_increment);
}
template <class Scan> double max_range(const Scan &scan) {
  if constexpr (requires { scan.maxRange; }) return static_cast<double>(scan.maxRange);
  else return static_cast<double>(scan.max_range);
}
} // namespace detail

/// A C++20 adapter over the Rust Visualizer. Inputs are copied at the C ABI boundary.
/// Keep one Visualizer on the window thread; do not call it concurrently.
class Visualizer {
public:
  using MapGeometry = ad::visualization::MapGeometry;
  using PreparedMap = ad::visualization::PreparedMap;

  Visualizer() = default;
  Visualizer(const Visualizer &) = delete;
  Visualizer &operator=(const Visualizer &) = delete;
  Visualizer(Visualizer &&) noexcept = default;
  Visualizer &operator=(Visualizer &&) noexcept = default;

  template <class Map> static Result<PreparedMap> prepareMap(const Map &map) {
    if (ad_viz_abi_version() != AD_VIZ_ABI_VERSION)
      return Error{ErrorCode::BackendFailure, "Visualizer ABI version mismatch"};
    if (!std::in_range<int>(map.width) || !std::in_range<int>(map.height) ||
        map.width <= 0 || map.height <= 0)
      return Error{ErrorCode::InvalidInput, "Map dimensions must be positive"};
    MapGeometry map_geometry{static_cast<int>(map.width), static_cast<int>(map.height),
                             static_cast<double>(map.resolution)};
    auto geometry = detail::geometry(map_geometry);
    if (!geometry) return geometry.error();
    if constexpr (std::ranges::sized_range<decltype(map.grid)>) {
      const auto expected = static_cast<std::uint64_t>(map_geometry.width) *
                            static_cast<std::uint64_t>(map_geometry.height);
      if (std::ranges::size(map.grid) != expected)
        return Error{ErrorCode::SizeMismatch, "Map grid length differs from dimensions"};
    }
    std::vector<std::int8_t> grid;
    if constexpr (std::ranges::sized_range<decltype(map.grid)>)
      grid.reserve(std::ranges::size(map.grid));
    for (const auto &cell : map.grid) grid.push_back(static_cast<std::int8_t>(cell));
    const AdVizMap input{*geometry, grid.data(), grid.size()};
    AdPreparedMap *raw = nullptr;
    AdVizError error{};
    const auto code = ad_viz_prepare_map(input, &raw, &error);
    if (code != AD_VIZ_OK) return detail::ffi_error(code, error);
    return PreparedMap{map_geometry,
                       std::shared_ptr<AdPreparedMap>(raw, ad_viz_prepared_map_destroy)};
  }

  [[nodiscard]] Status renderFrame(const PreparedMap &map) const {
    return invoke([&](AdVisualizer *handle, AdVizError *error) {
      return ad_viz_begin_frame(handle, map.handle_.get(), error);
    });
  }

  template <std::ranges::input_range Range>
  [[nodiscard]] Status renderPath(const Range &path, const MapGeometry &map,
                                  std::string_view style = "b-") const {
    auto geometry = detail::geometry(map);
    if (!geometry) return geometry.error();
    const auto values = detail::points(path);
    return invoke([&](AdVisualizer *handle, AdVizError *error) {
      return ad_viz_render_path(handle, values.data(), values.size(), *geometry,
                                detail::bytes(style), style.size(), error);
    });
  }

  template <class P, class F>
  [[nodiscard]] Status renderRobot(const P &pose, const F &footprint,
                                   const MapGeometry &map) const {
    auto geometry = detail::geometry(map);
    if (!geometry) return geometry.error();
    const auto vertices = detail::points(footprint.vertices);
    return invoke([&](AdVisualizer *handle, AdVizError *error) {
      return ad_viz_render_robot(handle, detail::pose(pose), vertices.data(), vertices.size(),
                                 *geometry, error);
    });
  }

  template <class P>
  [[nodiscard]] Status renderMarker(const P &point, const MapGeometry &map,
                                    double size, std::string_view color) const {
    auto geometry = detail::geometry(map);
    if (!geometry) return geometry.error();
    return invoke([&](AdVisualizer *handle, AdVizError *error) {
      return ad_viz_render_marker(handle, detail::point(point), *geometry, size,
                                  detail::bytes(color), color.size(), error);
    });
  }

  template <std::ranges::input_range Range>
  [[nodiscard]] Status renderPoints(const Range &points, const MapGeometry &map,
                                    double size, std::string_view color) const {
    auto geometry = detail::geometry(map);
    if (!geometry) return geometry.error();
    const auto values = detail::points(points);
    return invoke([&](AdVisualizer *handle, AdVizError *error) {
      return ad_viz_render_points(handle, values.data(), values.size(), *geometry, size,
                                  detail::bytes(color), color.size(), error);
    });
  }

  template <class P, class S>
  [[nodiscard]] Status renderScan(const P &pose, const S &scan,
                                  const MapGeometry &map) const {
    auto geometry = detail::geometry(map);
    if (!geometry) return geometry.error();
    std::vector<double> ranges;
    if constexpr (std::ranges::sized_range<decltype(scan.ranges)>)
      ranges.reserve(std::ranges::size(scan.ranges));
    for (const auto &range : scan.ranges) ranges.push_back(static_cast<double>(range));
    const AdVizScan input{ranges.data(), ranges.size(), detail::min_angle(scan),
                          detail::angle_increment(scan), detail::max_range(scan)};
    return invoke([&](AdVisualizer *handle, AdVizError *error) {
      return ad_viz_render_scan(handle, detail::pose(pose), input, *geometry, error);
    });
  }

  [[nodiscard]] Status presentFrame() const {
    return invoke([](AdVisualizer *handle, AdVizError *error) {
      return ad_viz_present_frame(handle, error);
    });
  }
  [[nodiscard]] Status saveFigure(std::string_view path) const {
    return invoke([&](AdVisualizer *handle, AdVizError *error) {
      return ad_viz_save_figure(handle, detail::bytes(path), path.size(), error);
    });
  }

private:
  struct Deleter {
    void operator()(AdVisualizer *handle) const noexcept { ad_viz_destroy(handle); }
  };
  [[nodiscard]] Status ensureHandle() const {
    if (handle_) return {};
    if (ad_viz_abi_version() != AD_VIZ_ABI_VERSION)
      return Error{ErrorCode::BackendFailure, "Visualizer ABI version mismatch"};
    AdVisualizer *raw = nullptr;
    AdVizError error{};
    const auto code = ad_viz_new(&raw, &error);
    if (code != AD_VIZ_OK) return detail::ffi_error(code, error);
    handle_.reset(raw);
    return {};
  }
  template <class F> Status invoke(F &&function) const {
    if (auto ready = ensureHandle(); !ready) return ready;
    AdVizError error{};
    return detail::status(function(handle_.get(), &error), error);
  }
  mutable std::unique_ptr<AdVisualizer, Deleter> handle_;
};

} // namespace ad::visualization

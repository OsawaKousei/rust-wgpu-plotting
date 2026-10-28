#ifndef AD_VISUALIZATION_FFI_H
#define AD_VISUALIZATION_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define AD_VIZ_ABI_VERSION 1u
#define AD_VIZ_ERROR_CAPACITY 512u

typedef struct AdVisualizer AdVisualizer;
typedef struct AdPreparedMap AdPreparedMap;

typedef enum AdVizStatus {
  AD_VIZ_OK = 0,
  AD_VIZ_INVALID_INPUT = 1,
  AD_VIZ_EMPTY_COLLECTION = 2,
  AD_VIZ_SIZE_MISMATCH = 3,
  AD_VIZ_BACKEND_FAILURE = 4,
  AD_VIZ_PANIC = 5
} AdVizStatus;

typedef struct AdVizError {
  int32_t code;
  char message[AD_VIZ_ERROR_CAPACITY];
} AdVizError;

typedef struct AdVizPoint {
  double x;
  double y;
} AdVizPoint;

typedef struct AdVizPose {
  double x;
  double y;
  double theta;
} AdVizPose;

typedef struct AdVizGeometry {
  uint32_t width;
  uint32_t height;
  double resolution;
} AdVizGeometry;

typedef struct AdVizMap {
  AdVizGeometry geometry;
  const int8_t *grid;
  size_t grid_len;
} AdVizMap;

typedef struct AdVizScan {
  const double *ranges;
  size_t ranges_len;
  double min_angle;
  double angle_increment;
  double max_range;
} AdVizScan;

/* Pointer/length pairs may be null only when length is zero. Input storage is
 * borrowed for the duration of the call. Handles must be destroyed exactly once.
 * A handle must not be used concurrently or from another thread after window
 * presentation begins. No Rust or C++ container crosses this boundary. */
uint32_t ad_viz_abi_version(void);
int32_t ad_viz_new(AdVisualizer **out, AdVizError *error);
void ad_viz_destroy(AdVisualizer *visualizer);
int32_t ad_viz_prepare_map(AdVizMap map, AdPreparedMap **out, AdVizError *error);
void ad_viz_prepared_map_destroy(AdPreparedMap *prepared);
int32_t ad_viz_begin_frame(AdVisualizer *visualizer, const AdPreparedMap *prepared,
                           AdVizError *error);
int32_t ad_viz_render_path(AdVisualizer *visualizer, const AdVizPoint *points,
                           size_t points_len, AdVizGeometry geometry,
                           const uint8_t *style, size_t style_len, AdVizError *error);
int32_t ad_viz_render_robot(AdVisualizer *visualizer, AdVizPose pose,
                            const AdVizPoint *vertices, size_t vertices_len,
                            AdVizGeometry geometry, AdVizError *error);
int32_t ad_viz_render_marker(AdVisualizer *visualizer, AdVizPoint point,
                             AdVizGeometry geometry, double size,
                             const uint8_t *color, size_t color_len, AdVizError *error);
int32_t ad_viz_render_points(AdVisualizer *visualizer, const AdVizPoint *points,
                             size_t points_len, AdVizGeometry geometry, double size,
                             const uint8_t *color, size_t color_len, AdVizError *error);
int32_t ad_viz_render_scan(AdVisualizer *visualizer, AdVizPose pose,
                           AdVizScan scan, AdVizGeometry geometry, AdVizError *error);
int32_t ad_viz_present_frame(AdVisualizer *visualizer, AdVizError *error);
int32_t ad_viz_save_figure(AdVisualizer *visualizer, const uint8_t *path,
                           size_t path_len, AdVizError *error);

#ifdef __cplusplus
}
#endif
#endif

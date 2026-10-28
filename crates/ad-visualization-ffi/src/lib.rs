//! C ABI for the simulation Visualizer. All raw pointers are handled here.
use ad_visualization::{
    Footprint, LidarScan, MapData, MapGeometry, Point, Pose, PreparedMap, RenderError,
    RenderErrorKind, Visualizer,
};
use std::{
    ffi::c_char,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice, str,
};

const CAP: usize = 512;
const OK: i32 = 0;
const INVALID: i32 = 1;
const EMPTY: i32 = 2;
const SIZE: i32 = 3;
const BACKEND: i32 = 4;
const PANIC: i32 = 5;

#[repr(C)]
pub struct AdVizError {
    pub code: i32,
    pub message: [c_char; CAP],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AdVizPoint {
    pub x: f64,
    pub y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AdVizPose {
    pub x: f64,
    pub y: f64,
    pub theta: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AdVizGeometry {
    pub width: u32,
    pub height: u32,
    pub resolution: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AdVizMap {
    pub geometry: AdVizGeometry,
    pub grid: *const i8,
    pub grid_len: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AdVizScan {
    pub ranges: *const f64,
    pub ranges_len: usize,
    pub min_angle: f64,
    pub angle_increment: f64,
    pub max_range: f64,
}
pub struct AdVisualizer(Visualizer);
pub struct AdPreparedMap(PreparedMap);

struct Failure {
    code: i32,
    message: String,
}
type FfiResult<T> = Result<T, Failure>;
fn fail(code: i32, message: impl Into<String>) -> Failure {
    Failure {
        code,
        message: message.into(),
    }
}
impl From<RenderError> for Failure {
    fn from(error: RenderError) -> Self {
        let code = match error.kind() {
            RenderErrorKind::InvalidInput | RenderErrorKind::InvalidSize => INVALID,
            _ => BACKEND,
        };
        fail(code, error.to_string())
    }
}
fn write_error(target: *mut AdVizError, code: i32, message: &str) {
    if target.is_null() {
        return;
    }
    // SAFETY: The C caller supplies a writable error object.
    let target = unsafe { &mut *target };
    target.code = code;
    target.message.fill(0);
    let mut count = message.len().min(CAP - 1);
    while !message.is_char_boundary(count) {
        count -= 1;
    }
    for (dest, byte) in target
        .message
        .iter_mut()
        .zip(message.as_bytes()[..count].iter())
    {
        *dest = *byte as c_char;
    }
}
fn call(error: *mut AdVizError, body: impl FnOnce() -> FfiResult<()>) -> i32 {
    let (code, message) = match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => (OK, String::new()),
        Ok(Err(error)) => (error.code, error.message),
        Err(_) => (PANIC, "Rust panic in visualization ABI".to_owned()),
    };
    write_error(error, code, &message);
    code
}
fn geometry(value: AdVizGeometry) -> FfiResult<MapGeometry> {
    Ok(MapGeometry::new(
        value.width,
        value.height,
        value.resolution,
    )?)
}
fn point(value: AdVizPoint) -> FfiResult<Point> {
    Ok(Point::new(value.x, value.y)?)
}
fn pose(value: AdVizPose) -> FfiResult<Pose> {
    Ok(Pose::new(value.x, value.y, value.theta)?)
}

// SAFETY: Nonempty arrays must be aligned, readable, and alive for the call.
unsafe fn borrowed<'a, T>(pointer: *const T, len: usize) -> FfiResult<&'a [T]> {
    if len == 0 {
        return Ok(&[]);
    }
    if pointer.is_null()
        || !pointer.is_aligned()
        || len > isize::MAX as usize / size_of::<T>().max(1)
    {
        return Err(fail(INVALID, "invalid pointer/length pair"));
    }
    // SAFETY: The C ABI requires valid storage for the duration of the call.
    Ok(unsafe { slice::from_raw_parts(pointer, len) })
}
// SAFETY: Pointer must name a live handle exclusively borrowed for this call.
unsafe fn viz<'a>(pointer: *mut AdVisualizer) -> FfiResult<&'a mut Visualizer> {
    unsafe { pointer.as_mut() }
        .map(|handle| &mut handle.0)
        .ok_or_else(|| fail(INVALID, "visualizer handle is null"))
}
// SAFETY: Pointer must name a live prepared-map handle.
unsafe fn prepared<'a>(pointer: *const AdPreparedMap) -> FfiResult<&'a PreparedMap> {
    unsafe { pointer.as_ref() }
        .map(|handle| &handle.0)
        .ok_or_else(|| fail(INVALID, "prepared map handle is null"))
}
// SAFETY: Pointer/length must name UTF-8 bytes alive for this call.
unsafe fn utf8<'a>(pointer: *const u8, len: usize) -> FfiResult<&'a str> {
    let bytes = unsafe { borrowed(pointer, len)? };
    str::from_utf8(bytes).map_err(|_| fail(INVALID, "text must be UTF-8"))
}
// SAFETY: Pointer/length must name point values alive for this call.
unsafe fn points(pointer: *const AdVizPoint, len: usize) -> FfiResult<Vec<Point>> {
    unsafe { borrowed(pointer, len)? }
        .iter()
        .copied()
        .map(point)
        .collect()
}

#[unsafe(no_mangle)]
pub extern "C" fn ad_viz_abi_version() -> u32 {
    1
}

/// # Safety
/// `out` and `error`, when non-null, must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_new(out: *mut *mut AdVisualizer, error: *mut AdVizError) -> i32 {
    call(error, || {
        let out = unsafe { out.as_mut() }.ok_or_else(|| fail(INVALID, "output is null"))?;
        *out = ptr::null_mut();
        *out = Box::into_raw(Box::new(AdVisualizer(Visualizer::new()?)));
        Ok(())
    })
}
/// # Safety
/// Pass a live handle returned by `ad_viz_new` at most once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_destroy(handle: *mut AdVisualizer) {
    if !handle.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(handle) })));
    }
}
/// # Safety
/// The grid and output pointer must be valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_prepare_map(
    map: AdVizMap,
    out: *mut *mut AdPreparedMap,
    error: *mut AdVizError,
) -> i32 {
    call(error, || {
        let out = unsafe { out.as_mut() }.ok_or_else(|| fail(INVALID, "output is null"))?;
        *out = ptr::null_mut();
        let cells = (map.geometry.width as usize)
            .checked_mul(map.geometry.height as usize)
            .ok_or_else(|| fail(INVALID, "map size overflow"))?;
        if map.grid_len != cells {
            return Err(fail(
                SIZE,
                "map grid length differs from width times height",
            ));
        }
        let grid = unsafe { borrowed(map.grid, map.grid_len)? }.to_vec();
        let map = MapData::new(
            map.geometry.width,
            map.geometry.height,
            map.geometry.resolution,
            grid,
        )?;
        *out = Box::into_raw(Box::new(AdPreparedMap(Visualizer::prepare_map(&map)?)));
        Ok(())
    })
}
/// # Safety
/// Pass a live handle returned by `ad_viz_prepare_map` at most once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_prepared_map_destroy(handle: *mut AdPreparedMap) {
    if !handle.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(handle) })));
    }
}
/// # Safety
/// Handle pointers must be live for this call; the visualizer is borrowed exclusively.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_begin_frame(
    handle: *mut AdVisualizer,
    map: *const AdPreparedMap,
    error: *mut AdVizError,
) -> i32 {
    call(error, || {
        unsafe { viz(handle)? }.begin_frame(unsafe { prepared(map)? })?;
        Ok(())
    })
}
/// # Safety
/// Handle and pointer/length pairs must be valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_render_path(
    handle: *mut AdVisualizer,
    values: *const AdVizPoint,
    len: usize,
    map: AdVizGeometry,
    style: *const u8,
    style_len: usize,
    error: *mut AdVizError,
) -> i32 {
    call(error, || {
        let values = unsafe { points(values, len)? };
        if values.is_empty() {
            return Err(fail(EMPTY, "path is empty"));
        }
        let style = unsafe { utf8(style, style_len)? };
        unsafe { viz(handle)? }.render_path_with_style(&values, &geometry(map)?, style)?;
        Ok(())
    })
}
/// # Safety
/// Handle and footprint pointer/length must be valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_render_robot(
    handle: *mut AdVisualizer,
    robot_pose: AdVizPose,
    vertices: *const AdVizPoint,
    len: usize,
    map: AdVizGeometry,
    error: *mut AdVizError,
) -> i32 {
    call(error, || {
        let vertices = unsafe { points(vertices, len)? };
        if vertices.is_empty() {
            return Err(fail(EMPTY, "footprint is empty"));
        }
        let footprint = Footprint::new(vertices)?;
        unsafe { viz(handle)? }.render_robot(pose(robot_pose)?, &footprint, &geometry(map)?)?;
        Ok(())
    })
}
/// # Safety
/// Handle and color pointer/length must be valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_render_marker(
    handle: *mut AdVisualizer,
    value: AdVizPoint,
    map: AdVizGeometry,
    size: f64,
    color: *const u8,
    color_len: usize,
    error: *mut AdVizError,
) -> i32 {
    call(error, || {
        unsafe { viz(handle)? }.render_marker(point(value)?, &geometry(map)?, size, unsafe {
            utf8(color, color_len)?
        })?;
        Ok(())
    })
}
/// # Safety
/// Handle and pointer/length pairs must be valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_render_points(
    handle: *mut AdVisualizer,
    values: *const AdVizPoint,
    len: usize,
    map: AdVizGeometry,
    size: f64,
    color: *const u8,
    color_len: usize,
    error: *mut AdVizError,
) -> i32 {
    call(error, || {
        let values = unsafe { points(values, len)? };
        if values.is_empty() {
            return Err(fail(EMPTY, "point set is empty"));
        }
        unsafe { viz(handle)? }.render_points(&values, &geometry(map)?, size, unsafe {
            utf8(color, color_len)?
        })?;
        Ok(())
    })
}
/// # Safety
/// Handle and scan range pointer/length must be valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_render_scan(
    handle: *mut AdVisualizer,
    robot_pose: AdVizPose,
    scan: AdVizScan,
    map: AdVizGeometry,
    error: *mut AdVizError,
) -> i32 {
    call(error, || {
        if scan.ranges_len == 0 {
            return Err(fail(EMPTY, "scan is empty"));
        }
        let ranges = unsafe { borrowed(scan.ranges, scan.ranges_len)? }.to_vec();
        let scan = LidarScan {
            ranges,
            min_angle: scan.min_angle,
            angle_increment: scan.angle_increment,
            max_range: scan.max_range,
        };
        unsafe { viz(handle)? }.render_scan(pose(robot_pose)?, &scan, &geometry(map)?)?;
        Ok(())
    })
}
/// # Safety
/// Handle must be live and exclusively borrowed for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_present_frame(
    handle: *mut AdVisualizer,
    error: *mut AdVizError,
) -> i32 {
    call(error, || {
        let visualizer = unsafe { viz(handle)? };
        #[cfg(feature = "native")]
        {
            visualizer.present_frame()?;
            Ok(())
        }
        #[cfg(not(feature = "native"))]
        {
            let _ = visualizer;
            Err(fail(BACKEND, "native window feature is disabled"))
        }
    })
}
/// # Safety
/// Handle and path pointer/length must be valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ad_viz_save_figure(
    handle: *mut AdVisualizer,
    path: *const u8,
    path_len: usize,
    error: *mut AdVizError,
) -> i32 {
    call(error, || {
        let path = unsafe { utf8(path, path_len)? };
        if path.is_empty() {
            return Err(fail(INVALID, "output path is empty"));
        }
        let visualizer = unsafe { viz(handle)? };
        #[cfg(feature = "native")]
        {
            visualizer.save_figure(std::path::Path::new(path))?;
            Ok(())
        }
        #[cfg(not(feature = "native"))]
        {
            let _ = (visualizer, path);
            Err(fail(BACKEND, "native PNG feature is disabled"))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_boundary_handles_map_scene_and_errors() {
        let mut error = AdVizError {
            code: 0,
            message: [0; CAP],
        };
        let mut visualizer = ptr::null_mut();
        let mut prepared = ptr::null_mut();
        let cells = [0_i8, 1, 0, 0];
        let geometry = AdVizGeometry {
            width: 2,
            height: 2,
            resolution: 0.5,
        };
        let map = AdVizMap {
            geometry,
            grid: cells.as_ptr(),
            grid_len: cells.len(),
        };
        // SAFETY: The arrays, output pointers, and handles are valid for each call.
        unsafe {
            assert_eq!(ad_viz_abi_version(), 1);
            assert_eq!(ad_viz_new(&mut visualizer, &mut error), OK);
            assert!(!visualizer.is_null());
            assert_eq!(ad_viz_prepare_map(map, &mut prepared, &mut error), OK);
            assert!(!prepared.is_null());
            assert_eq!(ad_viz_begin_frame(visualizer, prepared, &mut error), OK);
            let path = [
                AdVizPoint { x: 0.25, y: 0.25 },
                AdVizPoint { x: 0.75, y: 0.75 },
            ];
            assert_eq!(
                ad_viz_render_path(
                    visualizer,
                    path.as_ptr(),
                    path.len(),
                    geometry,
                    b"b-".as_ptr(),
                    2,
                    &mut error,
                ),
                OK
            );
            assert_eq!(
                ad_viz_render_path(
                    visualizer,
                    ptr::null(),
                    1,
                    geometry,
                    b"b-".as_ptr(),
                    2,
                    &mut error,
                ),
                INVALID
            );
            assert_ne!(error.message[0], 0);
            assert_eq!(
                ad_viz_render_path(
                    visualizer,
                    path.as_ptr(),
                    path.len(),
                    geometry,
                    b"?".as_ptr(),
                    1,
                    &mut error,
                ),
                INVALID
            );
            assert_eq!(
                ad_viz_render_path(
                    visualizer,
                    ptr::null(),
                    0,
                    geometry,
                    b"b-".as_ptr(),
                    2,
                    &mut error,
                ),
                EMPTY
            );
            ad_viz_prepared_map_destroy(prepared);
            ad_viz_destroy(visualizer);
        }
    }
}

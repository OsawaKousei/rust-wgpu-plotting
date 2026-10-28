#![cfg(feature = "png")]

use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use ad_render::{CpuImage, PixelSize, PngWriter, RenderErrorKind};

static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn replaces_completed_png_and_cleans_failed_temporary_file() {
    let directory = std::env::temp_dir().join(format!(
        "ad-render-png-{}-{}",
        std::process::id(),
        NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("result.png");
    let size = PixelSize::new(2, 1).unwrap();
    let red = CpuImage::new(size, vec![255, 0, 0, 255, 255, 0, 0, 255]).unwrap();
    let blue = CpuImage::new(size, vec![0, 0, 255, 255, 0, 0, 255, 255]).unwrap();
    let writer = PngWriter::new();
    writer.write(&red, &path).unwrap();
    let first = fs::read(&path).unwrap();
    writer.write(&blue, &path).unwrap();
    let second = fs::read(&path).unwrap();
    assert_ne!(first, second);
    assert!(second.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(second.windows(4).any(|chunk| chunk == b"sRGB"));

    let blocked = directory.join("blocked.png");
    fs::create_dir(&blocked).unwrap();
    assert_eq!(
        writer.write(&red, &blocked).unwrap_err().kind(),
        RenderErrorKind::Io
    );
    assert!(blocked.is_dir());
    assert_eq!(fs::read(&path).unwrap(), second);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
    fs::remove_dir_all(directory).unwrap();
}

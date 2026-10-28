use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{CpuImage, RenderError, RenderErrorKind};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn io_error(operation: &'static str, source: io::Error) -> RenderError {
    RenderError::with_source(RenderErrorKind::Io, operation, source.to_string(), source)
}

/// Writes a straight-alpha sRGB CpuImage as RGBA PNG without GPU or GLFW.
pub struct PngWriter;

impl PngWriter {
    pub fn new() -> Self {
        Self
    }

    pub fn write(&self, image: &CpuImage, path: &Path) -> Result<(), RenderError> {
        let file_name = path.file_name().ok_or_else(|| {
            RenderError::new(
                RenderErrorKind::InvalidInput,
                "PngWriter::write",
                "path must name a PNG file",
            )
        })?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = None;
        for _ in 0..100 {
            let mut name = OsString::from(".");
            name.push(file_name);
            name.push(format!(
                ".{}.{}.tmp",
                std::process::id(),
                NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            let temp_path = parent.join(name);
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
            {
                Ok(file) => {
                    temporary = Some((
                        TempPath {
                            path: temp_path,
                            committed: false,
                        },
                        file,
                    ));
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(io_error("PngWriter::write", error)),
            }
        }
        let (mut temp_path, mut file) = temporary.ok_or_else(|| {
            RenderError::new(
                RenderErrorKind::Io,
                "PngWriter::write",
                "unable to create a unique temporary file",
            )
        })?;
        encode_png(&mut file, image)?;
        file.sync_all()
            .map_err(|error| io_error("PngWriter::write", error))?;
        drop(file);
        fs::rename(&temp_path.path, path).map_err(|error| io_error("PngWriter::write", error))?;
        temp_path.committed = true;
        Ok(())
    }
}

impl Default for PngWriter {
    fn default() -> Self {
        Self::new()
    }
}

struct TempPath {
    path: PathBuf,
    committed: bool,
}
impl Drop for TempPath {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn encode_png(writer: &mut File, image: &CpuImage) -> Result<(), RenderError> {
    writer
        .write_all(b"\x89PNG\r\n\x1a\n")
        .map_err(|error| io_error("PngWriter::write", error))?;
    let size = image.size();
    let mut ihdr = [0_u8; 13];
    ihdr[0..4].copy_from_slice(&size.width().to_be_bytes());
    ihdr[4..8].copy_from_slice(&size.height().to_be_bytes());
    ihdr[8] = 8; // 8 bits per channel
    ihdr[9] = 6; // RGBA
    write_chunk(writer, *b"IHDR", &ihdr)?;
    write_chunk(writer, *b"sRGB", &[0])?; // perceptual rendering intent

    let row = (size.width() as usize).checked_mul(4).ok_or_else(|| {
        RenderError::new(
            RenderErrorKind::InvalidSize,
            "PngWriter::write",
            "row size overflow",
        )
    })?;
    let raw_len = row
        .checked_add(1)
        .and_then(|n| n.checked_mul(size.height() as usize))
        .ok_or_else(|| {
            RenderError::new(
                RenderErrorKind::InvalidSize,
                "PngWriter::write",
                "PNG scanline size overflow",
            )
        })?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(raw_len).map_err(|_| {
        RenderError::new(
            RenderErrorKind::ResourceExhausted,
            "PngWriter::write",
            "PNG scanline allocation failed",
        )
    })?;
    for scanline in image.rgba().chunks_exact(row) {
        raw.push(0); // no PNG filter
        raw.extend_from_slice(scanline);
    }
    let zlib = zlib_stored(&raw)?;
    for chunk in zlib.chunks(1 << 20) {
        write_chunk(writer, *b"IDAT", chunk)?;
    }
    write_chunk(writer, *b"IEND", &[])?;
    Ok(())
}

fn zlib_stored(raw: &[u8]) -> Result<Vec<u8>, RenderError> {
    let blocks = raw.len().div_ceil(u16::MAX as usize);
    let capacity = blocks
        .checked_mul(5)
        .and_then(|overhead| raw.len().checked_add(overhead))
        .and_then(|n| n.checked_add(6))
        .ok_or_else(|| {
            RenderError::new(
                RenderErrorKind::InvalidSize,
                "PngWriter::write",
                "zlib size overflow",
            )
        })?;
    let mut output = Vec::new();
    output.try_reserve_exact(capacity).map_err(|_| {
        RenderError::new(
            RenderErrorKind::ResourceExhausted,
            "PngWriter::write",
            "zlib allocation failed",
        )
    })?;
    output.extend_from_slice(&[0x78, 0x01]); // zlib, no compression
    for (index, block) in raw.chunks(u16::MAX as usize).enumerate() {
        output.push(u8::from(index + 1 == blocks)); // BFINAL, stored block
        let length = block.len() as u16;
        output.extend_from_slice(&length.to_le_bytes());
        output.extend_from_slice(&(!length).to_le_bytes());
        output.extend_from_slice(block);
    }
    output.extend_from_slice(&adler32(raw).to_be_bytes());
    Ok(output)
}

fn adler32(bytes: &[u8]) -> u32 {
    let mut a = 1_u32;
    let mut b = 0_u32;
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn write_chunk(writer: &mut File, kind: [u8; 4], data: &[u8]) -> Result<(), RenderError> {
    let length = u32::try_from(data.len()).map_err(|_| {
        RenderError::new(
            RenderErrorKind::PngEncoding,
            "PngWriter::write",
            "PNG chunk exceeds u32",
        )
    })?;
    writer
        .write_all(&length.to_be_bytes())
        .map_err(|error| io_error("PngWriter::write", error))?;
    writer
        .write_all(&kind)
        .map_err(|error| io_error("PngWriter::write", error))?;
    writer
        .write_all(data)
        .map_err(|error| io_error("PngWriter::write", error))?;
    writer
        .write_all(&crc32(&kind, data).to_be_bytes())
        .map_err(|error| io_error("PngWriter::write", error))?;
    Ok(())
}

fn crc32(kind: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in kind.iter().chain(data.iter()) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

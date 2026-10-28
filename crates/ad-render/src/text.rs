use std::{collections::HashMap, sync::Arc};

use ab_glyph::{Font, FontRef, GlyphId, ScaleFont, point};

use crate::{
    ImageData, PixelRect, PixelSize, RenderError, RenderErrorKind, SrgbColor, TextHorizontalAlign,
    TextRotation, TextVerticalAlign,
};

const FONT_BYTES: &[u8] = include_bytes!("../assets/LiberationSans-Regular.ttf");

pub(crate) struct RasterText {
    pub image: Arc<ImageData>,
    pub rect: PixelRect,
}

pub(crate) struct TextPlacement {
    pub x: f64,
    pub y: f64,
    pub size_px: f64,
    pub color: SrgbColor,
    pub horizontal: TextHorizontalAlign,
    pub vertical: TextVerticalAlign,
    pub rotation: TextRotation,
}

#[derive(Debug)]
struct GlyphBitmap {
    left: i32,
    top: i32,
    width: usize,
    alpha: Vec<u8>,
}

/// The font is fixed; the key distinguishes glyph and output pixel size.
#[derive(Debug, Default)]
pub(crate) struct TextRasterizer {
    glyphs: HashMap<(u16, u32), Arc<GlyphBitmap>>,
}

impl TextRasterizer {
    fn glyph(&mut self, font: &FontRef<'_>, id: GlyphId, size: f32) -> Arc<GlyphBitmap> {
        let key = (id.0, size.to_bits());
        if let Some(cached) = self.glyphs.get(&key) {
            return Arc::clone(cached);
        }
        let glyph = id.with_scale_and_position(size, point(0.0, 0.0));
        let bitmap = if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            let width = bounds.width() as usize;
            let height = bounds.height() as usize;
            let mut alpha = vec![0_u8; width * height];
            outline.draw(|x, y, coverage| {
                alpha[y as usize * width + x as usize] = (coverage * 255.0).round() as u8;
            });
            GlyphBitmap {
                left: bounds.min.x as i32,
                top: bounds.min.y as i32,
                width,
                alpha,
            }
        } else {
            GlyphBitmap {
                left: 0,
                top: 0,
                width: 0,
                alpha: Vec::new(),
            }
        };
        if self.glyphs.len() >= 4096 {
            self.glyphs.clear();
        }
        let bitmap = Arc::new(bitmap);
        self.glyphs.insert(key, Arc::clone(&bitmap));
        bitmap
    }

    pub(crate) fn raster_text(
        &mut self,
        text: &str,
        placement: TextPlacement,
    ) -> Result<Option<RasterText>, RenderError> {
        self.raster(text, placement)
    }

    fn raster(
        &mut self,
        text: &str,
        placement: TextPlacement,
    ) -> Result<Option<RasterText>, RenderError> {
        if text.is_empty() {
            return Ok(None);
        }
        let font = FontRef::try_from_slice(FONT_BYTES).map_err(|_| {
            RenderError::new(
                RenderErrorKind::InvalidInput,
                "raster_text",
                "bundled font is invalid",
            )
        })?;
        let px = placement.size_px;
        if !px.is_finite() || px <= 0.0 || px > 4096.0 {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "raster_text",
                "text size is invalid",
            ));
        }
        let scaled = font.as_scaled(px as f32);
        let mut glyphs = Vec::new();
        let mut pen = 2.0_f32;
        let mut previous = None;
        for ch in text.chars() {
            let mut id = scaled.glyph_id(ch);
            if id.0 == 0 {
                id = scaled.glyph_id('\u{fffd}');
            }
            if id.0 == 0 {
                id = scaled.glyph_id('?');
            }
            if let Some(prev) = previous {
                pen += scaled.kern(prev, id);
            }
            glyphs.push((id, pen));
            pen += scaled.h_advance(id);
            previous = Some(id);
        }
        let ascent = scaled.ascent().ceil().max(0.0);
        let descent = (-scaled.descent()).ceil().max(0.0);
        let width = (pen.ceil() + 3.0).max(1.0);
        let height = (ascent + descent + 4.0).max(1.0);
        if width > 8192.0 || height > 8192.0 {
            return Err(RenderError::new(
                RenderErrorKind::InvalidSize,
                "raster_text",
                "text bitmap exceeds 8192 pixels",
            ));
        }
        let width = width as u32;
        let height = height as u32;
        let mut pixels = vec![0_u8; width as usize * height as usize * 4];
        let [r, g, b, a] = placement.color.channels();
        let rgb = [
            (r * 255.0).round() as u8,
            (g * 255.0).round() as u8,
            (b * 255.0).round() as u8,
        ];
        let baseline = ascent + 2.0;
        for (id, x) in glyphs {
            let glyph = self.glyph(&font, id, px as f32);
            for (index, coverage) in glyph.alpha.iter().enumerate() {
                let ix = glyph.left + x.round() as i32 + (index % glyph.width) as i32;
                let iy = glyph.top + baseline.round() as i32 + (index / glyph.width) as i32;
                if ix >= 0 && iy >= 0 && ix < width as i32 && iy < height as i32 {
                    let index = (iy as usize * width as usize + ix as usize) * 4;
                    pixels[index..index + 3].copy_from_slice(&rgb);
                    pixels[index + 3] = pixels[index + 3].max((*coverage as f32 * a).round() as u8);
                }
            }
        }
        let (width, height, pixels, baseline) = if placement.rotation == TextRotation::Clockwise90 {
            let mut turned = vec![0_u8; pixels.len()];
            for y in 0..height {
                for x in 0..width {
                    let src = (y as usize * width as usize + x as usize) * 4;
                    let dst = (x as usize * height as usize + (height - 1 - y) as usize) * 4;
                    turned[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
                }
            }
            (height, width, turned, 0.0)
        } else {
            (width, height, pixels, baseline as f64)
        };
        let image = Arc::new(ImageData::new(
            PixelSize::new(width, height)?,
            Arc::from(pixels),
        )?);
        let x = placement.x
            - match placement.horizontal {
                TextHorizontalAlign::Left => 0.0,
                TextHorizontalAlign::Center => width as f64 / 2.0,
                TextHorizontalAlign::Right => width as f64,
            };
        let y = placement.y
            - match placement.vertical {
                TextVerticalAlign::Top => 0.0,
                TextVerticalAlign::Middle => height as f64 / 2.0,
                TextVerticalAlign::Bottom => height as f64,
                TextVerticalAlign::Baseline => baseline,
            };
        Ok(Some(RasterText {
            image,
            rect: PixelRect::try_new(x, y, width as f64, height as f64)?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_cache_separates_output_pixel_sizes() {
        let mut rasterizer = TextRasterizer::default();
        let placement = |size_px| TextPlacement {
            x: 0.0,
            y: 0.0,
            size_px,
            color: SrgbColor::BLACK,
            horizontal: TextHorizontalAlign::Left,
            vertical: TextVerticalAlign::Top,
            rotation: TextRotation::Zero,
        };
        let first = rasterizer
            .raster_text("A", placement(12.0))
            .unwrap()
            .unwrap();
        assert_eq!(rasterizer.glyphs.len(), 1);
        rasterizer.raster_text("A", placement(12.0)).unwrap();
        assert_eq!(rasterizer.glyphs.len(), 1);
        let second = rasterizer
            .raster_text("A", placement(24.0))
            .unwrap()
            .unwrap();
        assert_eq!(rasterizer.glyphs.len(), 2);
        assert!(second.rect.width() > first.rect.width());
    }
}

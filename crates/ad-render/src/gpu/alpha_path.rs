use std::sync::Arc;

use crate::{
    ImageData, LineCap, LineJoin, PixelPoint, PixelRect, PixelSize, RenderError, RenderErrorKind,
    SrgbColor,
};

/// Rasterizes one translucent path into a single coverage image. Samples are
/// unioned before alpha is applied, so joins cannot darken their own artist.
pub(super) struct PathSpec {
    pub width: f64,
    pub color: SrgbColor,
    pub closed: bool,
    pub cap: LineCap,
    pub join: LineJoin,
    pub clip: PixelRect,
    pub frame: PixelSize,
}

pub(super) fn rasterize(
    points: &[PixelPoint],
    spec: PathSpec,
) -> Result<Option<(Arc<ImageData>, PixelRect)>, RenderError> {
    let PathSpec {
        width,
        color,
        closed,
        cap,
        join,
        clip,
        frame,
    } = spec;
    if points.len() < 2 {
        return Ok(None);
    }
    let radius = width / 2.0;
    let x0 = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let x1 = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let y0 = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let y1 = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    let left = (x0 - radius - 1.0).floor().max(clip.x()).max(0.0);
    let top = (y0 - radius - 1.0).floor().max(clip.y()).max(0.0);
    let right = (x1 + radius + 1.0)
        .ceil()
        .min(clip.x() + clip.width())
        .min(frame.width() as f64);
    let bottom = (y1 + radius + 1.0)
        .ceil()
        .min(clip.y() + clip.height())
        .min(frame.height() as f64);
    if ![left, top, right, bottom].iter().all(|v| v.is_finite()) || right <= left || bottom <= top {
        return Ok(None);
    }
    let left = left.floor() as u32;
    let top = top.floor() as u32;
    let right = right.ceil() as u32;
    let bottom = bottom.ceil() as u32;
    let w = right - left;
    let h = bottom - top;
    let count = (w as usize).checked_mul(h as usize).ok_or_else(|| {
        RenderError::new(
            RenderErrorKind::InvalidSize,
            "alpha_path::rasterize",
            "bitmap size overflow",
        )
    })?;
    if count > 16_000_000 {
        return Err(RenderError::new(
            RenderErrorKind::ResourceExhausted,
            "alpha_path::rasterize",
            "translucent path bitmap is too large",
        ));
    }
    let mut mask = vec![0_u16; count];
    let mut paint =
        |xmin: f64, ymin: f64, xmax: f64, ymax: f64, inside: &dyn Fn(f64, f64) -> bool| {
            let xa = xmin.floor().max(left as f64) as u32;
            let xb = xmax.ceil().min(right as f64) as u32;
            let ya = ymin.floor().max(top as f64) as u32;
            let yb = ymax.ceil().min(bottom as f64) as u32;
            for y in ya..yb {
                for x in xa..xb {
                    let index = (y - top) as usize * w as usize + (x - left) as usize;
                    let mut bits = mask[index];
                    for sy in 0..4 {
                        for sx in 0..4 {
                            let bit = 1_u16 << (sy * 4 + sx);
                            if bits & bit == 0
                                && inside(
                                    x as f64 + (sx as f64 + 0.5) / 4.0,
                                    y as f64 + (sy as f64 + 0.5) / 4.0,
                                )
                            {
                                bits |= bit;
                            }
                        }
                    }
                    mask[index] = bits;
                }
            }
        };
    let segments = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for i in 0..segments {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len2 = dx * dx + dy * dy;
        if !len2.is_finite() || len2 <= 0.0 {
            continue;
        }
        paint(
            a.x.min(b.x) - radius - 1.0,
            a.y.min(b.y) - radius - 1.0,
            a.x.max(b.x) + radius + 1.0,
            a.y.max(b.y) + radius + 1.0,
            &|x, y| {
                let t = ((x - a.x) * dx + (y - a.y) * dy) / len2;
                if !(0.0..=1.0).contains(&t) {
                    return false;
                }
                let cross = (x - a.x) * dy - (y - a.y) * dx;
                cross * cross <= radius * radius * len2
            },
        );
    }
    let mut circles = Vec::new();
    if closed {
        if join == LineJoin::Round {
            circles.extend_from_slice(points);
        }
    } else {
        if cap == LineCap::Round {
            circles.push(points[0]);
            circles.push(points[points.len() - 1]);
        }
        if join == LineJoin::Round && points.len() > 2 {
            circles.extend_from_slice(&points[1..points.len() - 1]);
        }
    }
    for center in circles {
        paint(
            center.x - radius - 1.0,
            center.y - radius - 1.0,
            center.x + radius + 1.0,
            center.y + radius + 1.0,
            &|x, y| (x - center.x).powi(2) + (y - center.y).powi(2) <= radius * radius,
        );
    }
    let [r, g, b, a] = color.channels();
    let rgb = [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ];
    let mut rgba = vec![0_u8; count * 4];
    for (i, bits) in mask.iter().enumerate() {
        rgba[i * 4..i * 4 + 3].copy_from_slice(&rgb);
        rgba[i * 4 + 3] = (a * bits.count_ones() as f32 / 16.0 * 255.0).round() as u8;
    }
    let image = Arc::new(ImageData::new(PixelSize::new(w, h)?, Arc::from(rgba))?);
    Ok(Some((
        image,
        PixelRect::new(left as f64, top as f64, w as f64, h as f64),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translucent_join_uses_one_alpha_coverage() {
        let color = SrgbColor::new(1.0, 0.0, 0.0, 0.5).unwrap();
        let points = [
            PixelPoint { x: 5.0, y: 10.0 },
            PixelPoint { x: 10.0, y: 10.0 },
            PixelPoint { x: 10.0, y: 15.0 },
        ];
        let (image, rect) = rasterize(
            &points,
            PathSpec {
                width: 4.0,
                color,
                closed: false,
                cap: LineCap::Round,
                join: LineJoin::Round,
                clip: PixelRect::new(0.0, 0.0, 20.0, 20.0),
                frame: PixelSize::new(20, 20).unwrap(),
            },
        )
        .unwrap()
        .unwrap();
        let x = 10 - rect.x() as usize;
        let y = 10 - rect.y() as usize;
        let alpha = image.rgba()[(y * image.size().width() as usize + x) * 4 + 3];
        assert_eq!(alpha, 128);
    }
}

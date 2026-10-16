use crate::model::{Crosshair, Shape};
use anyhow::{Context, Result};
use image::RgbaImage;
use resvg::tiny_skia as sk;

pub fn preset(c: &Crosshair) -> Result<RgbaImage> {
    c.validate()?;
    let extent = match c.shape {
        Shape::Dot => c.thickness / 2.,
        Shape::Ring | Shape::Square => c.gap / 2. + c.thickness,
        Shape::Quadrant => c.gap / 2. + c.length.max(c.thickness),
        Shape::Cross => (c.gap / 2. + c.length).max(c.thickness / 2.),
    };
    let n = ((extent + c.outline + 2.).ceil() as u32) * 2;
    let center = n as f32 / 2.;
    let mut foreground = sk::Pixmap::new(n, n).context("无法生成预设")?;
    let mut outline = sk::Pixmap::new(n, n).context("无法生成描边")?;
    for border in [true, false] {
        if border && c.outline == 0. {
            continue;
        }
        let extra = if border { c.outline } else { 0. };
        let p = if border {
            &mut outline
        } else {
            &mut foreground
        };
        let mut paint = sk::Paint::default();
        let rgb = if border { c.outline_color } else { c.color };
        paint.set_color_rgba8(rgb[0], rgb[1], rgb[2], 255);
        let mut b = sk::PathBuilder::new();
        match c.shape {
            Shape::Dot => push_rect(
                &mut b,
                center - c.thickness / 2.,
                center - c.thickness / 2.,
                c.thickness,
                c.thickness,
                extra,
            ),
            Shape::Ring => {
                b.push_circle(center, center, (c.gap + c.thickness) / 2.);
            }
            Shape::Square => {
                let inner = c.gap / 2.;
                let outer = inner + c.thickness;
                for (x, y, w, h) in [
                    (center - outer, center - outer, 2. * outer, c.thickness),
                    (center - outer, center + inner, 2. * outer, c.thickness),
                    (center - outer, center - outer, c.thickness, 2. * outer),
                    (center + inner, center - outer, c.thickness, 2. * outer),
                ] {
                    push_rect(&mut b, x, y, w, h, extra);
                }
            }
            Shape::Quadrant => {
                let gap = c.gap / 2.;
                let arm = c.length.max(c.thickness);
                // Each elbow sits next to the central gap; both legs extend
                // away from the center, matching the four inward-facing corners.
                for (x, arm_x) in [
                    (center - gap - c.thickness, center - gap - arm),
                    (center + gap, center + gap),
                ] {
                    for (y, arm_y) in [
                        (center - gap - c.thickness, center - gap - arm),
                        (center + gap, center + gap),
                    ] {
                        push_rect(&mut b, arm_x, y, arm, c.thickness, extra);
                        push_rect(&mut b, x, arm_y, c.thickness, arm, extra);
                    }
                }
            }
            Shape::Cross => {
                let half = c.thickness / 2. + extra;
                let gap = c.gap / 2.;
                let end = gap + c.length;
                for (index, (x, y, w, h)) in [
                    (
                        center - end - extra,
                        center - half,
                        c.length + 2. * extra,
                        2. * half,
                    ),
                    (
                        center + gap - extra,
                        center - half,
                        c.length + 2. * extra,
                        2. * half,
                    ),
                    (
                        center - half,
                        center - end - extra,
                        2. * half,
                        c.length + 2. * extra,
                    ),
                    (
                        center - half,
                        center + gap - extra,
                        2. * half,
                        c.length + 2. * extra,
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    if c.t_shape && index == 2 {
                        continue;
                    }
                    if let Some(r) = sk::Rect::from_xywh(x, y, w, h) {
                        b.push_rect(r);
                    }
                }
            }
        }
        let path = b.finish().context("无效预设路径")?;
        if c.shape == Shape::Ring {
            p.stroke_path(
                &path,
                &paint,
                &sk::Stroke {
                    width: c.thickness + 2. * extra,
                    ..Default::default()
                },
                sk::Transform::identity(),
                None,
            );
        } else {
            p.fill_path(
                &path,
                &paint,
                sk::FillRule::Winding,
                sk::Transform::identity(),
                None,
            );
        }
        // Draw the dot in the same border/foreground pass, so overlaps do not
        // leave an outline seam or punch a hole through a ring.
        if c.shape.has_center_dot() && c.center_dot {
            let mut dot = sk::PathBuilder::new();
            push_rect(
                &mut dot,
                center - c.thickness / 2.,
                center - c.thickness / 2.,
                c.thickness,
                c.thickness,
                extra,
            );
            p.fill_path(
                &dot.finish().context("无效中心点路径")?,
                &paint,
                sk::FillRule::Winding,
                sk::Transform::identity(),
                None,
            );
        }
    }
    let mut img = RgbaImage::new(n, n);
    for ((pixel, fill), border) in img
        .pixels_mut()
        .zip(foreground.pixels())
        .zip(outline.pixels())
    {
        // Remove the foreground's geometric coverage from the border before
        // applying alpha, so a transparent fill never reveals an outline inside.
        let coverage = f32::from(fill.alpha()) / 255.;
        let fill_alpha = coverage * f32::from(c.opacity);
        let border_alpha =
            f32::from(border.alpha()) / 255. * (1. - coverage) * f32::from(c.outline_opacity);
        let alpha = fill_alpha + border_alpha;
        if alpha > 0. {
            for channel in 0..3 {
                pixel[channel] = ((f32::from(c.color[channel]) * fill_alpha
                    + f32::from(c.outline_color[channel]) * border_alpha)
                    / alpha)
                    .round() as u8;
            }
            pixel[3] = alpha.round() as u8;
        }
    }
    Ok(img)
}
fn push_rect(b: &mut sk::PathBuilder, x: f32, y: f32, w: f32, h: f32, extra: f32) {
    if let Some(rect) = sk::Rect::from_xywh(x - extra, y - extra, w + 2. * extra, h + 2. * extra) {
        b.push_rect(rect);
    }
}
pub(crate) fn opacity(img: &mut RgbaImage, alpha: u8) {
    for p in img.pixels_mut() {
        p[3] = (u16::from(p[3]) * u16::from(alpha) / 255) as u8;
    }
}
/// Remove only matching transparent rows/columns on opposite sides. The
/// original canvas center (including half-pixel centers) remains the anchor.
pub fn crop_centered(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut trim_x = w / 2;
    let mut trim_y = h / 2;
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] != 0 {
            trim_x = trim_x.min(x).min(w - 1 - x);
            trim_y = trim_y.min(y).min(h - 1 - y);
        }
    }
    // Keep a transparent guard pixel, including antialiased alpha=1 pixels.
    trim_x = trim_x.saturating_sub(1);
    trim_y = trim_y.saturating_sub(1);
    image::imageops::crop_imm(img, trim_x, trim_y, w - 2 * trim_x, h - 2 * trim_y).to_image()
}
pub fn texture(img: &RgbaImage, zoom: u32) -> Result<RgbaImage> {
    anyhow::ensure!((1..=16).contains(&zoom), "倍率无效");
    // A 2*zoom multiple gives an integral half-extent in RTSS layout units.
    // Odd source sizes retain a half-pixel anchor, with at most 0.5 px error.
    let quantum = 2 * zoom;
    let width = img.width().max(256).div_ceil(quantum) * quantum;
    let height = img.height().max(256).div_ceil(quantum) * quantum;
    let mut texture = RgbaImage::new(width, height);
    image::imageops::overlay(
        &mut texture,
        img,
        i64::from((width - img.width()) / 2),
        i64::from((height - img.height()) / 2),
    );
    Ok(texture)
}

pub struct AlphaTile {
    pub source_x: u32,
    pub source_y: u32,
    pub atlas_x: u32,
    pub atlas_y: u32,
    pub width: u32,
    pub height: u32,
    pub alpha: u8,
}

/// RTSS 7.3.7 premultiplies PNG RGB while loading, then uses straight-alpha
/// blending. Binary PNG masks and per-layer color alpha avoid that second
/// multiplication without changing the source pixels or their screen positions.
pub fn alpha_texture(source: &RgbaImage, zoom: u32) -> Result<(RgbaImage, Vec<AlphaTile>)> {
    anyhow::ensure!((1..=16).contains(&zoom), "倍率无效");
    anyhow::ensure!(
        source.width().is_multiple_of(2 * zoom) && source.height().is_multiple_of(2 * zoom),
        "图片尺寸与倍率不匹配"
    );
    let tile = 128 / (2 * zoom) * (2 * zoom);
    let mut tiles = Vec::new();
    for sy in (0..source.height()).step_by(tile as usize) {
        for sx in (0..source.width()).step_by(tile as usize) {
            let width = tile.min(source.width() - sx);
            let height = tile.min(source.height() - sy);
            let mut bounds = [None::<(u32, u32, u32, u32)>; 256];
            for y in sy..sy + height {
                for x in sx..sx + width {
                    let alpha = source.get_pixel(x, y)[3] as usize;
                    let b = bounds[alpha].get_or_insert((x, y, x, y));
                    b.0 = b.0.min(x);
                    b.1 = b.1.min(y);
                    b.2 = b.2.max(x);
                    b.3 = b.3.max(y);
                }
            }
            for alpha in 1..=255u8 {
                if let Some((x0, y0, x1, y1)) = bounds[alpha as usize] {
                    // Bound both PNG allocation (at most 1024 x 4096) and
                    // hypertext length, including the 32 KB RTSS slot limit.
                    anyhow::ensure!(
                        tiles.len() < 512,
                        "图片半透明细节过多，超出 RTSS 输出容量；请缩小图片或简化透明度"
                    );
                    let q = 2 * zoom;
                    let x0 = x0 / q * q;
                    let y0 = y0 / q * q;
                    tiles.push(AlphaTile {
                        source_x: x0,
                        source_y: y0,
                        atlas_x: 0,
                        atlas_y: 0,
                        width: (x1 + 1).div_ceil(q) * q - x0,
                        height: (y1 + 1).div_ceil(q) * q - y0,
                        alpha,
                    });
                }
            }
        }
    }
    tiles.sort_by_key(|t| std::cmp::Reverse(t.height));
    let (mut x, mut y, mut row_height, mut atlas_width) = (0, 0, 0, 1);
    for t in &mut tiles {
        if x + t.width > 1024 {
            x = 0;
            y += row_height;
            row_height = 0;
        }
        t.atlas_x = x;
        t.atlas_y = y;
        x += t.width;
        row_height = row_height.max(t.height);
        atlas_width = atlas_width.max(x);
    }
    anyhow::ensure!(y + row_height <= 4096, "图片透明度纹理超出 RTSS 输出容量");
    // RTSS's PNG texture path needs a full 256-pixel allocation quantum on
    // both axes; short packed images otherwise produce corrupted UV samples.
    let mut atlas = RgbaImage::new(
        atlas_width.max(256).div_ceil(256) * 256,
        (y + row_height).max(256).div_ceil(256) * 256,
    );
    for t in &tiles {
        for y in 0..t.height {
            for x in 0..t.width {
                let p = source.get_pixel(t.source_x + x, t.source_y + y);
                if p[3] == t.alpha {
                    atlas.put_pixel(
                        t.atlas_x + x,
                        t.atlas_y + y,
                        image::Rgba([p[0], p[1], p[2], 255]),
                    );
                }
            }
        }
    }
    Ok((atlas, tiles))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alpha_masks_reconstruct_every_alpha_once_without_color_or_anchor_changes() {
        let mut img = RgbaImage::new(256, 2);
        for x in 0..256 {
            img.put_pixel(x, 0, image::Rgba([255, 80, 160, x as u8]));
        }
        for zoom in [1, 2, 3, 4, 16] {
            let expected = texture(&img, zoom).unwrap();
            let (atlas, tiles) = alpha_texture(&expected, zoom).unwrap();
            assert!(atlas.pixels().all(|p| p[3] == 0 || p[3] == 255));
            let mut actual = RgbaImage::new(expected.width(), expected.height());
            for t in tiles {
                assert!(t.width <= 128 && t.height <= 128);
                assert!(t.width.is_multiple_of(2 * zoom) && t.height.is_multiple_of(2 * zoom));
                for y in 0..t.height {
                    for x in 0..t.width {
                        let p = atlas.get_pixel(t.atlas_x + x, t.atlas_y + y);
                        if p[3] == 255 {
                            let dst = actual.get_pixel_mut(t.source_x + x, t.source_y + y);
                            assert_eq!(dst[3], 0, "no pixel may be drawn twice");
                            *dst = image::Rgba([p[0], p[1], p[2], t.alpha]);
                        }
                    }
                }
            }
            for (a, b) in actual.pixels().zip(expected.pixels()) {
                assert_eq!(a[3], b[3]);
                if a[3] != 0 {
                    assert_eq!(a, b);
                }
            }
        }
    }

    #[test]
    fn alpha_mask_allocation_is_bounded_for_dense_transparency() {
        let img = RgbaImage::from_fn(256, 256, |x, y| {
            image::Rgba([255, 255, 255, ((x + y) % 255 + 1) as u8])
        });
        assert!(alpha_texture(&img, 1).is_err());
    }
    #[test]
    fn symmetric_crop_keeps_asymmetric_art_and_faint_edge_at_original_anchor() {
        let mut img = RgbaImage::new(101, 103);
        img.put_pixel(50, 51, image::Rgba([255, 255, 255, 255]));
        img.put_pixel(40, 90, image::Rgba([255, 0, 0, 1]));
        let cropped = crop_centered(&img);
        assert_eq!(cropped.dimensions(), (23, 81));
        assert_eq!(cropped.get_pixel(11, 40)[3], 255);
        assert_eq!(cropped.get_pixel(1, 79)[3], 1);
        for zoom in [1, 2, 3, 4, 16] {
            let texture = texture(&cropped, zoom).unwrap();
            let (w, h) = texture.dimensions();
            assert!(w.is_multiple_of(2 * zoom) && h.is_multiple_of(2 * zoom));
            assert_eq!(texture.get_pixel(w / 2 - 1, h / 2 - 1)[3], 255);
        }
    }
    #[test]
    fn texture_padding_keeps_even_canvas_anchor_exact_for_large_and_small_images() {
        for size in [2, 254, 256, 258, 452, 710, 994, 1200] {
            let mut img = RgbaImage::new(size, size);
            img.put_pixel(size / 2, size / 2, image::Rgba([255; 4]));
            for zoom in [1, 2, 3, 4] {
                let texture = texture(&img, zoom).unwrap();
                assert_eq!(
                    texture.get_pixel(texture.width() / 2, texture.height() / 2)[3],
                    255
                );
            }
        }
    }
    #[test]
    fn shapes_have_transparency_and_alpha() {
        for shape in [
            Shape::Dot,
            Shape::Cross,
            Shape::Ring,
            Shape::Square,
            Shape::Quadrant,
        ] {
            let c = Crosshair {
                shape,
                opacity: 80,
                outline_opacity: 80,
                ..Default::default()
            };
            let img = preset(&c).unwrap();
            assert_eq!(img.get_pixel(0, 0)[3], 0);
            assert!(img.pixels().any(|p| p[3] > 0));
            assert!(img.pixels().all(|p| p[3] <= 80));
        }
    }
    #[test]
    fn fill_and_outline_opacity_are_independent_for_every_shape() {
        for shape in [
            Shape::Dot,
            Shape::Cross,
            Shape::Square,
            Shape::Quadrant,
            Shape::Ring,
        ] {
            let c = Crosshair {
                shape,
                center_dot: true,
                thickness: 6.,
                gap: 24.,
                outline: 3.,
                color: [0, 255, 0],
                outline_color: [255, 0, 255],
                ..Default::default()
            };
            let opaque = preset(&c).unwrap();
            for (fill_alpha, border_alpha) in [(60, 140), (0, 255), (255, 0), (0, 0)] {
                let img = preset(&Crosshair {
                    opacity: fill_alpha,
                    outline_opacity: border_alpha,
                    ..c.clone()
                })
                .unwrap();
                let mid = img.width() / 2;
                assert_eq!(img.get_pixel(mid, mid)[3], fill_alpha, "{shape:?}");
                let mut saw_border = false;
                for (reference, pixel) in opaque.pixels().zip(img.pixels()) {
                    if reference.0 == [255, 0, 255, 255] {
                        saw_border = true;
                        assert_eq!(pixel[3], border_alpha, "{shape:?}");
                    }
                    if reference.0 == [0, 255, 0, 255] {
                        assert_eq!(pixel[3], fill_alpha, "{shape:?}");
                    }
                    assert!(pixel[3] <= fill_alpha.max(border_alpha), "{shape:?}");
                }
                assert!(saw_border, "{shape:?}");
            }
        }
    }
    #[test]
    fn large_center_dot_is_not_clipped() {
        let c = Crosshair {
            shape: Shape::Cross,
            center_dot: true,
            thickness: 64.,
            length: 1.,
            gap: 0.,
            ..Default::default()
        };
        let img = preset(&c).unwrap();
        assert!(img.width() >= 70);
        assert_eq!(img.get_pixel(0, 0)[3], 0);
    }
    #[test]
    fn t_shape_removes_only_the_top_arm() {
        let c = Crosshair {
            gap: 12.,
            length: 8.,
            outline: 0.,
            ..Default::default()
        };
        let cross = preset(&c).unwrap();
        let t = preset(&Crosshair { t_shape: true, ..c }).unwrap();
        let mid = cross.width() / 2;
        for (x, y) in [
            (mid, mid - 8),
            (mid, mid + 8),
            (mid - 8, mid),
            (mid + 8, mid),
        ] {
            assert_eq!(cross.get_pixel(x, y)[3], 255);
            assert_eq!(t.get_pixel(x, y)[3], if y < mid { 0 } else { 255 });
        }
    }
    #[test]
    fn square_is_connected_and_quadrant_has_four_separate_corners() {
        let c = Crosshair {
            shape: Shape::Square,
            gap: 12.,
            thickness: 4.,
            outline: 0.,
            ..Default::default()
        };
        let square = preset(&c).unwrap();
        let quadrant = preset(&Crosshair {
            shape: Shape::Quadrant,
            ..c
        })
        .unwrap();
        let mid = square.width() / 2;
        let qmid = quadrant.width() / 2;
        assert_eq!(square.get_pixel(mid, mid)[3], 0);
        assert_eq!(quadrant.get_pixel(qmid, qmid)[3], 0);
        for (x, y) in [
            (mid - 8, mid - 8),
            (mid + 8, mid - 8),
            (mid - 8, mid + 8),
            (mid + 8, mid + 8),
        ] {
            assert_eq!(square.get_pixel(x, y)[3], 255);
            assert_eq!(quadrant.get_pixel(x + qmid - mid, y + qmid - mid)[3], 255);
        }
        assert_eq!(square.get_pixel(mid, mid - 8)[3], 255);
        assert_eq!(quadrant.get_pixel(qmid, qmid - 8)[3], 0);
    }
    #[test]
    fn quadrant_length_extends_both_legs_without_moving_the_inner_corners() {
        let c = Crosshair {
            shape: Shape::Quadrant,
            gap: 16.,
            thickness: 2.,
            length: 8.,
            outline: 0.,
            ..Default::default()
        };
        let long = preset(&c).unwrap();
        let short = preset(&Crosshair {
            length: 3.,
            ..c.clone()
        })
        .unwrap();
        assert!(long.width() > short.width());
        let outlined = preset(&Crosshair {
            outline: 2.,
            outline_color: [255, 0, 255],
            ..c
        })
        .unwrap();
        let mid = outlined.width() / 2;
        assert_eq!(outlined.get_pixel(mid + 14, mid + 9).0, [0, 255, 0, 255]);
        for sx in [-1, 1] {
            for sy in [-1, 1] {
                let alpha = |img: &RgbaImage, x: i32, y: i32| {
                    let mid = img.width() as i32 / 2;
                    img.get_pixel((mid + sx * x) as u32, (mid + sy * y) as u32)[3]
                };
                assert_eq!(alpha(&long, 9, 9), 255);
                assert_eq!(alpha(&short, 9, 9), 255);
                assert_eq!(alpha(&long, 14, 9), 255);
                assert_eq!(alpha(&long, 9, 14), 255);
                assert_eq!(alpha(&long, 14, 14), 0);
                assert_eq!(alpha(&long, 7, 9), 0);
                assert_eq!(alpha(&short, 12, 9), 0);
                assert_eq!(alpha(&short, 9, 12), 0);
            }
        }
    }
    #[test]
    fn unsupported_options_do_not_change_rendering() {
        for shape in [Shape::Dot, Shape::Square, Shape::Quadrant, Shape::Ring] {
            let c = Crosshair {
                shape,
                ..Default::default()
            };
            let expected = preset(&c).unwrap();
            let mut changed = c;
            if !shape.has_length() {
                changed.length = 100.;
            }
            changed.t_shape = true;
            if shape == Shape::Dot {
                changed.gap = 100.;
                changed.center_dot = true;
            }
            assert_eq!(preset(&changed).unwrap(), expected, "{shape:?}");
        }
    }
    #[test]
    fn center_dot_and_colored_outline_apply_to_every_preset() {
        for shape in [
            Shape::Dot,
            Shape::Cross,
            Shape::Square,
            Shape::Quadrant,
            Shape::Ring,
        ] {
            let c = Crosshair {
                shape,
                gap: 24.,
                thickness: 4.,
                outline: 2.,
                outline_color: [255, 0, 255],
                ..Default::default()
            };
            let img = preset(&c).unwrap();
            assert!(img.pixels().any(|p| p.0 == [255, 0, 255, 255]), "{shape:?}");
            let with_dot = preset(&Crosshair {
                center_dot: true,
                ..c
            })
            .unwrap();
            let mid = with_dot.width() / 2;
            assert_eq!(
                with_dot.get_pixel(mid, mid).0,
                [0, 255, 0, 255],
                "{shape:?}"
            );
            if shape != Shape::Dot {
                assert_eq!(img.get_pixel(mid, mid)[3], 0, "{shape:?}");
            }
        }
    }
    #[test]
    fn ring_gap_controls_inner_diameter_and_thickness_controls_band() {
        let c = Crosshair {
            shape: Shape::Ring,
            gap: 20.,
            thickness: 4.,
            outline: 0.,
            ..Default::default()
        };
        let img = preset(&c).unwrap();
        let mid = img.width() / 2;
        assert_eq!(img.get_pixel(mid + 8, mid)[3], 0);
        assert_eq!(img.get_pixel(mid + 11, mid)[3], 255);
        assert_eq!(img.get_pixel(mid + 15, mid)[3], 0);
        let wider = preset(&Crosshair {
            gap: 30.,
            ..c.clone()
        })
        .unwrap();
        let thicker = preset(&Crosshair { thickness: 8., ..c }).unwrap();
        assert!(wider.width() > img.width());
        assert!(thicker.width() > img.width());
    }
}

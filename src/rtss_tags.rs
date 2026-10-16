use crate::model::{Crosshair, Kind};
use anyhow::{Context, Result, ensure};

pub fn alpha_tags(
    c: &Crosshair,
    zoom: u32,
    path: &str,
    size: (u32, u32),
    tiles: &[crate::render::AlphaTile],
) -> Result<String> {
    c.validate()?;
    ensure!((1..=16).contains(&zoom), "倍率无效");
    ensure!(!path.contains(['\"', '<', '>']), "缓存路径含 RTSS 保留字符");
    let mut result = format!("<LI=\"{path}\"><S=1>");
    let mut sorted: Vec<_> = tiles.iter().collect();
    sorted.sort_by_key(|t| t.alpha);
    let mut alpha = None;
    for (layer, t) in sorted.into_iter().enumerate() {
        let dx = c.x + (t.source_x as i32 + t.width as i32 / 2 - size.0 as i32 / 2) / zoom as i32;
        let dy = c.y + (t.source_y as i32 + t.height as i32 / 2 - size.1 as i32 / 2) / zoom as i32;
        use std::fmt::Write;
        if alpha != Some(t.alpha) {
            write!(result, "<C={:02X}FFFFFF>", t.alpha)?;
            alpha = Some(t.alpha);
        }
        write!(
            result,
            "<P4><L{layer}><E={},{},4><M={dx},{dy},0,0><I={},{},{},{},{},{}>",
            t.width / zoom,
            t.height / zoom,
            t.width / zoom,
            t.height / zoom,
            t.atlas_x,
            t.atlas_y,
            t.width,
            t.height
        )?;
    }
    result.push_str("<C><S>");
    ensure!(result.len() < 32768, "RTSS 输出内容过长");
    Ok(result)
}

pub fn tags(c: &Crosshair, zoom: u32, image: Option<(&str, u32, u32)>) -> Result<String> {
    c.validate()?;
    ensure!((1..=16).contains(&zoom), "倍率无效");
    let margin = format!("<M={},{},0,0>", c.x, c.y);
    if c.kind == Kind::Text {
        let [r, g, b] = c.color;
        return Ok(format!(
            "<P4><L0><S={:.0}><E=-{},-1,4>{margin}<C={:02X}{r:02X}{g:02X}{b:02X}>{}<C><S>",
            c.text_size as f32 / zoom as f32,
            c.text.len(),
            c.opacity,
            c.text
        ));
    }
    let (path, width, height) = image.context("缺少输出图片")?;
    ensure!(
        width > 0
            && height > 0
            && width.is_multiple_of(2 * zoom)
            && height.is_multiple_of(2 * zoom),
        "图片尺寸与倍率不匹配"
    );
    ensure!(!path.contains(['"', '<', '>']), "缓存路径含 RTSS 保留字符");
    // Sticky positioning clamps each layer's natural extent to the client.
    // Keep that extent bounded, and place native-pixel source tiles relative
    // to the screen center. S=1 prevents font line height from enlarging tiles.
    let tile = 128 / (2 * zoom) * (2 * zoom);
    let mut result = format!("<LI=\"{path}\"><S=1><C=FFFFFFFF>");
    let mut layer = 0;
    for sy in (0..height).step_by(tile as usize) {
        let h = tile.min(height - sy);
        for sx in (0..width).step_by(tile as usize) {
            let w = tile.min(width - sx);
            let dx = c.x + (sx as i32 + w as i32 / 2 - width as i32 / 2) / zoom as i32;
            let dy = c.y + (sy as i32 + h as i32 / 2 - height as i32 / 2) / zoom as i32;
            use std::fmt::Write;
            write!(
                result,
                "<P4><L{layer}><E={},{},4><M={dx},{dy},0,0><I={},{},{sx},{sy},{w},{h}>",
                w / zoom,
                h / zoom,
                w / zoom,
                h / zoom
            )?;
            layer += 1;
        }
    }
    result.push_str("<C><S>");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alpha_atlas_rectangles_keep_native_pixel_positions_and_layer_opacity() {
        let img = image::RgbaImage::from_fn(256, 256, |x, y| {
            if (100..110).contains(&x) && (130..150).contains(&y) {
                image::Rgba([255, 80, 160, 128])
            } else {
                image::Rgba([0; 4])
            }
        });
        let c = Crosshair {
            x: 12,
            y: -6,
            ..Default::default()
        };
        for zoom in [1, 2, 3, 4, 16] {
            let source = crate::render::texture(&img, zoom).unwrap();
            let (_, tiles) = crate::render::alpha_texture(&source, zoom).unwrap();
            let tag = alpha_tags(&c, zoom, "test.png", source.dimensions(), &tiles).unwrap();
            assert!(tag.contains("<C=80FFFFFF>"));
            assert!(tag.len() < 32768);
            for t in tiles {
                let dx = c.x
                    + (t.source_x as i32 + t.width as i32 / 2 - source.width() as i32 / 2)
                        / zoom as i32;
                let dy = c.y
                    + (t.source_y as i32 + t.height as i32 / 2 - source.height() as i32 / 2)
                        / zoom as i32;
                assert!(tag.contains(&format!(
                    "<M={dx},{dy},0,0><I={},{},{},{},{},{}>",
                    t.width / zoom,
                    t.height / zoom,
                    t.atlas_x,
                    t.atlas_y,
                    t.width,
                    t.height
                )));
                assert_eq!(
                    dx * zoom as i32 - t.width as i32 / 2,
                    c.x * zoom as i32 + t.source_x as i32 - source.width() as i32 / 2
                );
                assert_eq!(
                    dy * zoom as i32 - t.height as i32 / 2,
                    c.y * zoom as i32 + t.source_y as i32 - source.height() as i32 / 2
                );
            }
        }
    }
    #[test]
    fn tiled_output_covers_every_pixel_once_with_native_scale_and_bounded_layers() {
        for zoom in [1, 2, 3, 4, 16] {
            for (width, height) in [(256_u32, 256_u32), (996, 996), (4096, 1024), (1024, 4096)] {
                let q = 2 * zoom;
                let (width, height) = (width.div_ceil(q) * q, height.div_ceil(q) * q);
                let c = Crosshair {
                    x: 12,
                    y: -6,
                    ..Default::default()
                };
                let t = tags(&c, zoom, Some(("C:/cache.png", width, height))).unwrap();
                assert!(t.len() < 32768);
                let mut coverage = vec![0u8; (width * height) as usize];
                for layer in t.split("<P4>").skip(1) {
                    let params = |prefix| -> Vec<i32> {
                        layer
                            .split(prefix)
                            .nth(1)
                            .unwrap()
                            .split('>')
                            .next()
                            .unwrap()
                            .split(',')
                            .map(|p| p.parse().unwrap())
                            .collect()
                    };
                    let m = params("<M=");
                    let i = params("<I=");
                    let (dw, dh, sx, sy, w, h) = (i[0], i[1], i[2], i[3], i[4], i[5]);
                    assert_eq!(dw * zoom as i32, w);
                    assert_eq!(dh * zoom as i32, h);
                    assert!(w <= 128 && h <= 128);
                    assert_eq!(
                        m[0] * zoom as i32 - w / 2,
                        c.x * zoom as i32 + sx - width as i32 / 2
                    );
                    assert_eq!(
                        m[1] * zoom as i32 - h / 2,
                        c.y * zoom as i32 + sy - height as i32 / 2
                    );
                    for y in sy..sy + h {
                        for x in sx..sx + w {
                            coverage[(y as u32 * width + x as u32) as usize] += 1;
                        }
                    }
                }
                assert!(coverage.iter().all(|&n| n == 1));
            }
        }
    }
    #[test]
    fn explicit_zoom_and_path_generate_compensated_tags_without_io() {
        let c = Crosshair {
            x: 12,
            y: -6,
            ..Default::default()
        };
        for zoom in [1, 2, 4, 16] {
            let t = tags(&c, zoom, Some(("C:/cache with spaces/cross.png", 256, 256))).unwrap();
            let tile = 128 / (2 * zoom) * (2 * zoom);
            let extent = tile / zoom;
            assert!(t.contains(&format!("<I={extent},{extent},0,0,{tile},{tile}>")));
            assert!(t.contains(&format!(
                "<M={},{},0,0>",
                12 + (tile as i32 / 2 - 128) / zoom as i32,
                -6 + (tile as i32 / 2 - 128) / zoom as i32
            )));
            assert!(!t.contains("FNT"));
        }
    }
    #[test]
    fn rejects_unresolved_zoom_reserved_paths_dimensions_and_text() {
        let mut c = Crosshair::default();
        for zoom in [0, 17] {
            assert!(tags(&c, zoom, Some(("x.png", 256, 256))).is_err());
        }
        for path in ["x\".png", "<x>.png"] {
            assert!(tags(&c, 1, Some((path, 256, 256))).is_err());
        }
        assert!(tags(&c, 2, Some(("x.png", 255, 256))).is_err());
        assert!(tags(&c, 1, None).is_err());
        c.kind = Kind::Text;
        c.text = "<I>".into();
        assert!(tags(&c, 1, None).is_err());
        c.text = "+".into();
        assert!(tags(&c, 2, None).unwrap().contains("<S=50>"));
    }
}

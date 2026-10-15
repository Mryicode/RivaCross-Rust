use anyhow::{Context, Result, ensure};
use image::{RgbaImage, imageops::FilterType};
use resvg::{tiny_skia as sk, usvg};

pub fn decode(data: &[u8], ext: &str, scale: f32) -> Result<RgbaImage> {
    ensure!(
        scale.is_finite() && (0.01..=16.).contains(&scale),
        "缩放无效"
    );
    if ext == "svg" {
        let xml = std::str::from_utf8(data).context("SVG 必须使用 UTF-8 编码")?;
        let doc = roxmltree::Document::parse(xml)?;
        for node in doc.descendants().filter(|n| n.is_element()) {
            ensure!(
                !matches!(
                    node.tag_name().name(),
                    "animate" | "animateTransform" | "animateMotion" | "set"
                ),
                "不支持动画 SVG"
            );
            for attr in node.attributes().filter(|a| a.name() == "href") {
                ensure!(
                    attr.value().starts_with('#') || attr.value().starts_with("data:"),
                    "SVG 含外部资源，请先嵌入资源后导入"
                );
            }
        }
        let mut options = usvg::Options::default();
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        if doc.descendants().any(|n| n.has_tag_name("text")) {
            options.fontdb_mut().load_system_fonts();
        }
        let tree = usvg::Tree::from_data(data, &options).context("SVG 解析失败（禁止外部资源）")?;
        let full_w = (tree.size().width() * scale).round().max(1.) as u32;
        let full_h = (tree.size().height() * scale).round().max(1.) as u32;
        let sx = full_w as f32 / tree.size().width();
        let sy = full_h as f32 / tree.size().height();
        let (mut w, mut h) = (full_w, full_h);
        let (mut trim_x, mut trim_y) = (0, 0);
        if dimensions(w, h).is_err() {
            // Large SVG canvases may contain mostly transparent margins. Bound
            // allocation by the drawable layers (including stroke/filter bounds),
            // keeping opposite trims equal so the canvas-center anchor survives.
            let bounds = tree.root().abs_layer_bounding_box();
            let symmetric_trim = |size: u32, start: f32, end: f32| {
                start.floor().min(size as f32 - end.ceil()).max(0.) as u32
            };
            trim_x = symmetric_trim(w, bounds.left() * sx, bounds.right() * sx)
                .min((w - 1) / 2)
                .saturating_sub(2);
            trim_y = symmetric_trim(h, bounds.top() * sy, bounds.bottom() * sy)
                .min((h - 1) / 2)
                .saturating_sub(2);
            w -= 2 * trim_x;
            h -= 2 * trim_y;
        }
        dimensions(w, h)?;
        let mut pix = sk::Pixmap::new(w, h).context("图片内存分配失败")?;
        resvg::render(
            &tree,
            sk::Transform::from_row(sx, 0., 0., sy, -(trim_x as f32), -(trim_y as f32)),
            &mut pix.as_mut(),
        );
        return straight(&pix);
    }
    ensure!(ext == "png", "不支持此图片格式");
    // APNG is outside the first-release scope.
    let mut offset = 8usize;
    while offset.checked_add(12).is_some_and(|end| end <= data.len()) {
        let len = u32::from_be_bytes(data[offset..offset + 4].try_into()?) as usize;
        ensure!(&data[offset + 4..offset + 8] != b"acTL", "不支持动画 PNG");
        offset = offset
            .checked_add(len)
            .and_then(|n| n.checked_add(12))
            .context("PNG 数据无效")?;
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(data), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode()?.to_rgba8();
    let w = (img.width() as f32 * scale).round().max(1.) as u32;
    let h = (img.height() as f32 * scale).round().max(1.) as u32;
    dimensions(w, h)?;
    let filter = if scale > 1. {
        FilterType::Nearest
    } else {
        FilterType::Lanczos3
    };
    Ok(image::imageops::resize(&img, w, h, filter))
}
fn dimensions(w: u32, h: u32) -> Result<()> {
    ensure!(
        w > 0 && h > 0 && w <= 4096 && h <= 4096 && u64::from(w) * u64::from(h) <= 4_194_304,
        "图片目标尺寸过大（最多 4096 边长、4 百万像素）"
    );
    Ok(())
}
pub(crate) fn straight(pix: &sk::Pixmap) -> Result<RgbaImage> {
    let mut data = pix.data().to_vec();
    for p in data.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        for c in &mut p[..3] {
            *c = (u32::from(*c) * 255 + a / 2)
                .checked_div(a)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
    RgbaImage::from_raw(pix.width(), pix.height(), data).context("像素尺寸无效")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redrawn_sniper_is_vector_only_with_individually_named_lines_and_fixed_anchor() {
        let svg = std::str::from_utf8(crate::model::HuntWeapon::SniperCrossbow.svg()).unwrap();
        let doc = roxmltree::Document::parse(svg).unwrap();
        let root = doc.root_element();
        assert_eq!(root.attribute("viewBox"), Some("-200 -200 400 400"));
        assert!(!doc.descendants().any(|n| n.has_tag_name("image")));
        let lines: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("path"))
            .collect();
        assert_eq!(lines.len(), 19);
        for line in &lines {
            assert!(line.attribute("id").is_some());
            assert!(line.children().any(|n| n.has_tag_name("title")));
        }
        let mut ids: Vec<_> = lines.iter().map(|n| n.attribute("id").unwrap()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), lines.len());
        for scale in [1., 1.57, 2.20] {
            let img = decode(svg.as_bytes(), "svg", scale).unwrap();
            let cx = img.width() / 2;
            let cy = img.height() / 2;
            assert!(img.get_pixel(cx, (cy as f32 + 63. * scale).round() as u32)[3] > 0);
            assert!(img.get_pixel(cx, (cy as f32 + 148. * scale).round() as u32)[3] > 0);
            assert_eq!(
                img.get_pixel(cx, (cy as f32 + 30. * scale).round() as u32)[3],
                0
            );
        }
    }
    #[test]
    fn sniper_svg_crops_large_transparent_canvas_before_allocation_without_rescaling() {
        let data = include_bytes!("../assets/reference/sniper-crossbow-original.svg");
        let original = decode(data, "svg", 1.).unwrap();
        assert_eq!(original.dimensions(), (604, 604));
        let normalized = std::str::from_utf8(data).unwrap().replacen(
            "width=\"2400\" height=\"2400\" viewBox=\"-1200 -1200 2400 2400\"",
            "width=\"604\" height=\"604\" viewBox=\"-302 -302 604 604\"",
            1,
        );
        assert_eq!(original, decode(normalized.as_bytes(), "svg", 1.).unwrap());
        for scale in [1.52, 1.56, 1.57, 1.58, 1.60, 2.20] {
            let img = decode(data, "svg", scale).unwrap();
            dimensions(img.width(), img.height()).unwrap();
            assert!(img.pixels().any(|p| p[3] > 0));
        }
        assert!(decode(data, "svg", 16.).is_err());
    }
    #[test]
    fn oversized_svg_with_real_content_still_obeys_pixel_and_edge_limits() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2400" height="2400"><rect width="2400" height="2400" fill="red"/></svg>"#;
        assert!(decode(svg, "svg", 1.).is_err());
    }
    #[test]
    fn refuses_external_and_animated_svg() {
        for svg in [
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><image href="C:/secret.png"/></svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><animate attributeName="x"/></svg>"#,
        ] {
            assert!(decode(svg.as_bytes(), "svg", 1.).is_err());
        }
    }
    #[test]
    fn enlarged_png_keeps_hard_edges() {
        let mut img = RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([0, 255, 0, 255]));
        let mut bytes = std::io::Cursor::new(vec![]);
        img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let enlarged = decode(bytes.get_ref(), "png", 4.).unwrap();
        assert!(enlarged.pixels().all(|p| p[3] == 0 || p[3] == 255));
    }
    #[test]
    fn svg_rerenders_at_size() {
        let svg=br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"><circle cx="5" cy="10" r="3" fill="red"/></svg>"#;
        let a = decode(svg, "svg", 2.).unwrap();
        assert_eq!(a.dimensions(), (20, 40));
        assert_eq!(a.get_pixel(0, 0)[3], 0);
    }
}

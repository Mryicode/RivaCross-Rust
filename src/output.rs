use crate::{
    image_input,
    model::{Crosshair, Kind},
    render, rtss_tags,
    storage::Store,
};
use anyhow::{Context, Result};
use image::RgbaImage;

pub fn raster(c: &Crosshair, store: &Store) -> Result<RgbaImage> {
    c.validate()?;
    if c.kind == Kind::Preset {
        return render::preset(c);
    }
    let name = c.image.as_ref().context("请导入 PNG 或 SVG 图片")?;
    let data = store.read_resource(name)?;
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mut img = image_input::decode(&data, ext, c.image_scale)?;
    render::opacity(&mut img, c.opacity);
    Ok(img)
}
pub fn tags(c: &Crosshair, store: &Store, zoom: u32) -> Result<(String, Option<RgbaImage>)> {
    c.validate()?;
    if c.kind == Kind::Text {
        return Ok((rtss_tags::tags(c, zoom, None)?, None));
    }
    let img = raster(c, store)?;
    let texture = render::texture(&img, zoom)?;
    let path = store.write_cache(&texture)?;
    let path = path.to_str().context("缓存路径无法编码")?;
    Ok((
        rtss_tags::tags(c, zoom, Some((path, texture.width(), texture.height())))?,
        Some(img),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_is_direct_and_anchored() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        let c = Crosshair {
            kind: Kind::Text,
            x: 12,
            y: -6,
            rtss_zoom: 2,
            ..Default::default()
        };
        let (t, p) = tags(&c, &s, c.rtss_zoom).unwrap();
        assert!(t.contains("<E=-1,-1,4><M=12,-6,0,0>"));
        assert!(p.is_none());
        assert!(t.contains("<S=50>"));
        assert!(!t.contains("FNT"));
    }
    #[test]
    fn image_texture_is_one_to_one_without_global_font_override() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        let c = Crosshair {
            length: 3.,
            gap: 0.,
            outline: 0.,
            rtss_zoom: 2,
            ..Default::default()
        };
        let (t, preview) = tags(&c, &s, c.rtss_zoom).unwrap();
        assert_eq!(preview.unwrap().width(), 10);
        assert!(t.contains("<I=128,128,0,0,256,256>"));
        assert!(t.contains("<E=128,128,4>"));
        assert!(!t.contains("FNT"));
    }
}

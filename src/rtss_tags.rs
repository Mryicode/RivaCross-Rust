use crate::model::{Crosshair, Kind};
use anyhow::{Context, Result, ensure};

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
        width > 0 && height > 0 && width.is_multiple_of(zoom) && height.is_multiple_of(zoom),
        "图片尺寸与倍率不匹配"
    );
    ensure!(!path.contains(['"', '<', '>']), "缓存路径含 RTSS 保留字符");
    Ok(format!(
        "<LI=\"{path}\"><P4><L0><S=100><E={},{},4>{margin}<C=FFFFFFFF><I={},{},0,0,{},{}><C><S>",
        width / zoom,
        height / zoom,
        width / zoom,
        height / zoom,
        width,
        height
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_zoom_and_path_generate_compensated_tags_without_io() {
        let c = Crosshair {
            x: 12,
            y: -6,
            ..Default::default()
        };
        for zoom in [1, 2, 4, 16] {
            let t = tags(&c, zoom, Some(("C:/cache with spaces/cross.png", 256, 256))).unwrap();
            let extent = 256 / zoom;
            assert!(t.contains(&format!("<I={extent},{extent},0,0,256,256>")));
            assert!(t.contains("<M=12,-6,0,0>"));
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

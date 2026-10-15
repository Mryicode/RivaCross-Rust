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
    let mut img = if c.kind == Kind::Hunt {
        image_input::decode(c.hunt_weapon.svg(), "svg", c.image_scale)?
    } else {
        let name = c.image.as_ref().context("请导入 PNG 或 SVG 图片")?;
        let data = store.read_resource(name)?;
        let ext = std::path::Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        image_input::decode(&data, ext, c.image_scale)?
    };
    // Crop before opacity so alpha=0 retains the same geometry.
    img = render::crop_centered(&img);
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
    /// Run with scripts/verify-hunt-live.py. Uses only our own RTSS slot and
    /// refuses to start while an editor or a retained RivaCross overlay exists.
    #[test]
    #[ignore = "live RTSS QA; opens an injected window and requires screenshot driver"]
    fn live_centering_window() {
        use eframe::egui;
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let _instance = crate::instance::Instance::acquire()
            .unwrap()
            .expect("close editor first");
        let mapping = crate::rtss::Mapping::open().unwrap();
        assert!(
            mapping.own_text().unwrap().is_none(),
            "existing overlay must be cleared first"
        );
        struct Live {
            mapping: crate::rtss::Mapping,
            store: Store,
            root: std::path::PathBuf,
            id: u64,
            frames: u32,
            ready: Option<serde_json::Value>,
            start: std::time::Instant,
        }
        impl Drop for Live {
            fn drop(&mut self) {
                self.mapping.release().unwrap();
            }
        }
        impl eframe::App for Live {
            fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
                assert!(self.start.elapsed().as_secs() < 180, "live QA timed out");
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(20, 20, 20)))
                    .show(ctx, |_| {});
                let command = std::fs::read(self.root.join("command.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
                if let Some(v) = command {
                    let id = v["id"].as_u64().unwrap();
                    if id != self.id {
                        self.id = id;
                        if v["quit"].as_bool() == Some(true) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            return;
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            v["width"].as_f64().unwrap() as f32,
                            v["height"].as_f64().unwrap() as f32,
                        )));
                        let c = Crosshair {
                            kind: Kind::Hunt,
                            hunt_weapon: crate::model::HuntWeapon::Bow,
                            image_scale: v["scale"].as_f64().unwrap() as f32,
                            x: v["x"].as_i64().unwrap_or(0) as i32,
                            y: v["y"].as_i64().unwrap_or(0) as i32,
                            ..Default::default()
                        };
                        let zoom = v["zoom"].as_u64().unwrap() as u32;
                        let (tag, img) = if v["old"].as_bool() == Some(true) {
                            let img =
                                image_input::decode(c.hunt_weapon.svg(), "svg", c.image_scale)
                                    .unwrap();
                            let texture = render::texture(&img, zoom).unwrap();
                            let path = self.store.write_cache(&texture).unwrap();
                            (
                                format!(
                                    "<LI=\"{}\"><P4><L0><S=100><E={},{},4><M=0,0,0,0><C=FFFFFFFF><I={},{},0,0,{},{}><C><S>",
                                    path.display(),
                                    texture.width() / zoom,
                                    texture.height() / zoom,
                                    texture.width() / zoom,
                                    texture.height() / zoom,
                                    texture.width(),
                                    texture.height()
                                ),
                                Some(img),
                            )
                        } else if v["full"].as_bool() == Some(true) {
                            let img =
                                image_input::decode(c.hunt_weapon.svg(), "svg", c.image_scale)
                                    .unwrap();
                            let texture = render::texture(&img, zoom).unwrap();
                            let path = self.store.write_cache(&texture).unwrap();
                            (
                                rtss_tags::tags(
                                    &c,
                                    zoom,
                                    Some((
                                        path.to_str().unwrap(),
                                        texture.width(),
                                        texture.height(),
                                    )),
                                )
                                .unwrap(),
                                Some(img),
                            )
                        } else {
                            tags(&c, &self.store, zoom).unwrap()
                        };
                        img.unwrap().save(self.root.join("raster.png")).unwrap();
                        let bytes = crate::rtss::encode(&tag).unwrap();
                        let deadline =
                            std::time::Instant::now() + std::time::Duration::from_secs(3);
                        while !self.mapping.write(&bytes, false).unwrap() {
                            assert!(std::time::Instant::now() < deadline);
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        self.frames = 0;
                        self.ready = Some(serde_json::json!({"id":id,"tag":tag}));
                    }
                }
                self.frames += 1;
                if self.frames == 30
                    && let Some(v) = self.ready.take()
                {
                    std::fs::write(
                        self.root.join("ready.json"),
                        serde_json::to_vec(&v).unwrap(),
                    )
                    .unwrap();
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        let root = std::env::var_os("RIVACROSS_LIVE_QA_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap().join("target/hunt-live"));
        std::fs::create_dir_all(&root).unwrap();
        let store = Store::at(root.join("data")).unwrap();
        eframe::run_native(
            "RivaCross live centering QA",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([720., 800.])
                    .with_window_level(egui::WindowLevel::AlwaysOnTop),
                event_loop_builder: Some(Box::new(|b| {
                    b.with_any_thread(true);
                })),
                ..Default::default()
            },
            Box::new(move |_| {
                Ok(Box::new(Live {
                    mapping,
                    store,
                    root,
                    id: 0,
                    frames: 0,
                    ready: None,
                    start: std::time::Instant::now(),
                }))
            }),
        )
        .unwrap();
    }
    #[test]
    fn hunt_and_imported_svg_share_pixels_anchor_scaling_and_layer_alpha() {
        use crate::model::HuntWeapon;
        let d = tempfile::tempdir().unwrap();
        let store = Store::at(d.path().into()).unwrap();
        for weapon in [
            HuntWeapon::Crossbow,
            HuntWeapon::Bow,
            HuntWeapon::SniperCrossbow,
        ] {
            let source = d.path().join("weapon.svg");
            std::fs::write(&source, weapon.svg()).unwrap();
            let imported = store.import(&source).unwrap();
            let scales: &[f32] = if weapon == HuntWeapon::SniperCrossbow {
                &[1.00, 2.20]
            } else {
                &[1.00, 1.52, 1.56, 1.57, 1.58, 1.60, 2.20]
            };
            for &scale in scales {
                let c = Crosshair {
                    kind: Kind::Hunt,
                    hunt_weapon: weapon,
                    image_scale: scale,
                    ..Default::default()
                };
                let opaque = raster(&c, &store).unwrap();
                let image = raster(
                    &Crosshair {
                        kind: Kind::Image,
                        image: Some(imported.clone()),
                        ..c.clone()
                    },
                    &store,
                )
                .unwrap();
                assert_eq!(image, opaque);
                assert!(opaque.pixels().any(|p| p[3] > 0 && p[3] < 255));
                for alpha in [0, 128, 255] {
                    let img = raster(
                        &Crosshair {
                            opacity: alpha,
                            ..c.clone()
                        },
                        &store,
                    )
                    .unwrap();
                    assert_eq!(img.dimensions(), opaque.dimensions());
                    for (a, b) in opaque.pixels().zip(img.pixels()) {
                        assert_eq!(&a.0[..3], &b.0[..3]);
                        assert_eq!(b[3], (u16::from(a[3]) * u16::from(alpha) / 255) as u8);
                    }
                }
                for zoom in [1, 2, 3, 4] {
                    let t = render::texture(&opaque, zoom).unwrap();
                    let tag = rtss_tags::tags(&c, zoom, Some(("test.png", t.width(), t.height())))
                        .unwrap();
                    assert!(tag.contains("<P4><L0>"));
                    assert!(tag.len() < 32768);
                }
            }
        }
    }
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
        assert!(t.contains("<I=64,64,0,0,128,128>"));
        assert!(t.contains("<E=64,64,4><M=-32,-32,0,0>"));
        assert!(!t.contains("FNT"));
    }
}

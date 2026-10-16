use crate::{
    model::{Crosshair, Kind},
    storage::Store,
};
use eframe::egui::{self, Color32};
pub(super) struct Preview {
    texture: Option<egui::TextureHandle>,
    preview_bg: u8,
    pub(super) zoom: f32,
    last_preview: Option<Crosshair>,
}
impl Default for Preview {
    fn default() -> Self {
        Self {
            texture: None,
            preview_bg: 0,
            zoom: 1.,
            last_preview: None,
        }
    }
}
impl Preview {
    pub fn draw(&mut self, ui: &mut egui::Ui, c: &Crosshair, store: &Store) -> Option<String> {
        ui.heading("准星预览");
        ui.horizontal(|ui| {
            for (id, label) in [(0, "深色"), (1, "浅色"), (2, "棋盘格")] {
                ui.selectable_value(&mut self.preview_bg, id, label);
            }
            ui.selectable_value(&mut self.zoom, 1., "实际大小");
            ui.selectable_value(&mut self.zoom, 4., "4× 放大");
        });
        let mut error = None;
        let key = appearance(c);
        if self.last_preview.as_ref() != Some(&key) {
            self.texture = None;
            if c.kind != Kind::Text {
                match crate::output::raster(c, store) {
                    Ok(img) => {
                        let color = egui::ColorImage {
                            size: [img.width() as usize, img.height() as usize],
                            pixels: img.pixels().map(|p| preview_color(p.0)).collect(),
                        };
                        self.texture = Some(ui.ctx().load_texture(
                            "crosshair",
                            color,
                            egui::TextureOptions::NEAREST,
                        ));
                    }
                    Err(e) => error = Some(e.to_string()),
                }
            }
            self.last_preview = Some(key);
        }
        let size = egui::vec2(ui.available_width(), 220.);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let bg = if self.preview_bg == 1 {
            Color32::from_gray(230)
        } else {
            Color32::from_gray(28)
        };
        ui.painter().rect_filled(rect, 6., bg);
        if self.preview_bg == 2 {
            for y in 0..(size.y / 16.).ceil() as usize {
                for x in 0..(size.x / 16.).ceil() as usize {
                    let r = egui::Rect::from_min_size(
                        rect.min + egui::vec2(x as f32 * 16., y as f32 * 16.),
                        egui::vec2(16., 16.),
                    )
                    .intersect(rect);
                    ui.painter().rect_filled(
                        r,
                        0.,
                        Color32::from_gray(if (x + y) % 2 == 0 { 65 } else { 95 }),
                    );
                }
            }
        }
        if let Some(t) = &self.texture {
            let r = egui::Rect::from_center_size(
                rect.center(),
                t.size_vec2() * self.zoom / ui.ctx().pixels_per_point(),
            );
            ui.painter().with_clip_rect(rect).image(
                t.id(),
                r,
                egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                Color32::WHITE,
            );
        }
        if c.kind == Kind::Text {
            let [r, g, b] = c.color;
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &c.text,
                egui::FontId::monospace(16. * c.text_size as f32 / 100.),
                preview_color([r, g, b, c.opacity]),
            );
            ui.label("字符预览仅供参考，实际外观以 RTSS 为准。");
        }
        ui.label("预览居中展示外观；偏移只作用于 RTSS 输出。");
        error
    }
}
// RTSS blends in gamma space. egui's unmultiplied constructor multiplies in
// linear space, which makes translucent white lines look too bright here.
fn preview_color([r, g, b, a]: [u8; 4]) -> Color32 {
    let multiply = |v| (u16::from(v) * u16::from(a) / 255) as u8;
    Color32::from_rgba_premultiplied(multiply(r), multiply(g), multiply(b), a)
}
fn appearance(c: &Crosshair) -> Crosshair {
    let mut c = c.clone();
    c.x = 0;
    c.y = 0;
    c.rtss_zoom = 0;
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_uses_rtss_gamma_space_alpha() {
        assert_eq!(
            preview_color([255, 128, 64, 128]).to_array(),
            [128, 64, 32, 128]
        );
        assert_eq!(preview_color([255, 128, 64, 0]), Color32::TRANSPARENT);
        assert_eq!(
            preview_color([255, 128, 64, 255]).to_array(),
            [255, 128, 64, 255]
        );
    }
    #[test]
    fn offsets_and_compensation_do_not_invalidate_preview_texture() {
        let before = Crosshair::default();
        let mut after = before.clone();
        after.x = 100;
        after.y = -100;
        after.rtss_zoom = 4;
        assert_eq!(appearance(&before), appearance(&after));
        after.opacity = 80;
        assert_ne!(appearance(&before), appearance(&after));
        let mut p = Preview {
            last_preview: Some(appearance(&before)),
            ..Default::default()
        };
        p.preview_bg = 2;
        p.zoom = 4.;
        assert_eq!(p.last_preview, Some(appearance(&before)));
    }
}

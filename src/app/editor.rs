use crate::{
    model::{Config, Crosshair, HuntWeapon, Kind, Scheme, Shape},
    storage::Store,
};
use eframe::egui::{self, Color32};
#[derive(Debug, PartialEq)]
pub(super) struct EditResult {
    pub save: bool,
    pub output: bool,
}
impl EditResult {
    pub fn between(before: &Config, after: &Config) -> Self {
        Self {
            save: before != after,
            output: before.current != after.current || before.visible != after.visible,
        }
    }
}
pub fn controls(ui: &mut egui::Ui, config: &mut Config, store: &Store, error: &mut String) {
    let mut kind = config.current.kind;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut kind, Kind::Preset, "预设");
        ui.selectable_value(&mut kind, Kind::Image, "图片");
        ui.selectable_value(&mut kind, Kind::Text, "字符");
        ui.selectable_value(&mut kind, Kind::Hunt, "Hunt");
    });
    config.switch_kind(kind);
    ui.small("四种模式分别保存设置，切换后恢复各自的参数。");
    ui.separator();
    if kind == Kind::Preset {
        let mut shape = config.current.shape;
        ui.horizontal_wrapped(|ui| {
            for (value, label) in [
                (Shape::Cross, "静态十字"),
                (Shape::Dot, "仅中心点"),
                (Shape::Square, "静态方形"),
                (Shape::Quadrant, "静态四分"),
                (Shape::Ring, "圆形"),
            ] {
                ui.selectable_value(&mut shape, value, label);
            }
        });
        config.switch_shape(shape);
        ui.small("五种预设分别保存设置，切换后恢复各自的参数。");
    }
    if kind == Kind::Hunt {
        let mut weapon = config.current.hunt_weapon;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut weapon, HuntWeapon::Crossbow, "十字弩");
            ui.selectable_value(&mut weapon, HuntWeapon::Bow, "弓");
            ui.selectable_value(&mut weapon, HuntWeapon::SniperCrossbow, "狙击弩");
        });
        config.switch_weapon(weapon);
        ui.small("三种武器分别保存缩放、透明度、偏移和 RTSS 补偿。");
    }
    let c = &mut config.current;
    match c.kind {
        Kind::Preset => {
            ui.label("尺寸（像素）");
            if c.shape.has_length() {
                ui.add(
                    egui::Slider::new(&mut c.length, 1.0..=128.0)
                        .drag_value_speed(0.1)
                        .text("长度"),
                );
            }
            ui.add(
                egui::Slider::new(&mut c.thickness, 1.0..=256.0)
                    .drag_value_speed(0.1)
                    .text("粗细"),
            );
            if c.shape.has_gap() {
                ui.add(
                    egui::Slider::new(&mut c.gap, 0.0..=128.0)
                        .drag_value_speed(0.1)
                        .text("间距"),
                );
            }
            if c.shape.has_center_dot() {
                ui.label("形状");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut c.center_dot, "中心点");
                    if c.shape.has_t_shape() {
                        ui.checkbox(&mut c.t_shape, "T 型（去掉顶部线条）");
                    }
                });
            }
            color_and_opacity(ui, c);
            ui.separator();
            ui.add(
                egui::Slider::new(&mut c.outline, 0.0..=16.0)
                    .drag_value_speed(0.1)
                    .text("描边粗细"),
            );
            ui.horizontal(|ui| {
                ui.label("描边颜色");
                ui.color_edit_button_srgb(&mut c.outline_color);
            });
            ui.add(
                egui::Slider::new(&mut c.outline_opacity, 0..=255)
                    .drag_value_speed(0.1)
                    .text("描边透明度（255 不透明）"),
            );
        }
        Kind::Image => {
            if ui.button("导入 PNG / SVG…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("静态图片", &["png", "svg"])
                    .pick_file()
            {
                match store.import(&path) {
                    Ok(name) => {
                        c.image = Some(name);
                        c.image_scale = 1.;
                    }
                    Err(e) => *error = e.to_string(),
                }
            }
            ui.label(c.image.as_deref().unwrap_or("尚未导入图片"));
            ui.add(
                egui::Slider::new(&mut c.image_scale, 0.01..=16.0)
                    .logarithmic(true)
                    .drag_value_speed(0.01)
                    .text("等比缩放"),
            );
        }
        Kind::Hunt => {
            ui.add(
                egui::Slider::new(&mut c.image_scale, 0.01..=16.0)
                    .logarithmic(true)
                    .drag_value_speed(0.01)
                    .text("等比缩放"),
            );
        }
        Kind::Text => {
            ui.label("单行英文和 ASCII 符号（不支持尖括号）");
            ui.text_edit_singleline(&mut c.text);
            ui.add(
                egui::Slider::new(&mut c.text_size, 10..=1000)
                    .drag_value_speed(0.1)
                    .text("字符大小 %"),
            );
            if let Err(e) = crate::model::validate_text(&c.text) {
                ui.colored_label(Color32::LIGHT_RED, e.to_string());
            }
        }
    }
    if c.kind != Kind::Preset {
        color_and_opacity(ui, c);
    }
    ui.horizontal(|ui| {
        ui.label("RTSS 缩放补偿");
        ui.add(
            egui::DragValue::new(&mut c.rtss_zoom)
                .speed(0.1)
                .range(0..=16),
        );
        ui.small("0 自动读取全局倍率；应用专属倍率不同时手动填写。");
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("水平偏移");
        ui.add(
            egui::DragValue::new(&mut c.x)
                .speed(0.1)
                .range(-8192..=8192),
        );
        ui.label("垂直偏移");
        ui.add(
            egui::DragValue::new(&mut c.y)
                .speed(0.1)
                .range(-8192..=8192),
        );
        if ui.button("重置位置").clicked() {
            c.x = 0;
            c.y = 0;
        }
    });
}
fn color_and_opacity(ui: &mut egui::Ui, c: &mut Crosshair) {
    if matches!(c.kind, Kind::Preset | Kind::Text) {
        ui.horizontal(|ui| {
            ui.label("颜色");
            ui.color_edit_button_srgb(&mut c.color);
        });
    }
    ui.add(
        egui::Slider::new(&mut c.opacity, 0..=255)
            .drag_value_speed(0.1)
            .text("透明度（255 不透明）"),
    );
}
pub fn schemes(
    ui: &mut egui::Ui,
    config: &mut Config,
    name: &mut String,
    error: &mut String,
) -> bool {
    let mut save_now = false;
    ui.horizontal(|ui| {
        ui.label("方案名称");
        ui.text_edit_singleline(name);
        if ui.button("保存方案").clicked() {
            let name = name.trim();
            if name.is_empty() || name.len() > 128 {
                *error = "请输入有效方案名称（最多 128 字节）".into();
            } else if let Err(e) = config.current.validate() {
                *error = e.to_string();
            } else if let Some(s) = config.schemes.iter_mut().find(|s| s.name == name) {
                s.crosshair = config.current.clone();
            } else if config.schemes.len() < 100 {
                config.schemes.push(Scheme {
                    name: name.into(),
                    crosshair: config.current.clone(),
                });
            } else {
                *error = "最多保存 100 个方案".into();
            }
        }
    });
    let mut delete = None;
    let mut selected = None;
    for (i, s) in config.schemes.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(&s.name);
            if ui.button("使用").clicked() {
                selected = Some(s.crosshair.clone());
            }
            if ui.button("删除").clicked() {
                delete = Some(i);
            }
        });
    }
    if let Some(crosshair) = selected {
        config.apply_crosshair(crosshair);
    }
    if let Some(i) = delete {
        config.schemes.remove(i);
    }
    ui.horizontal(|ui| {
        let reset_label = if config.current.kind == Kind::Preset {
            "恢复当前预设默认设置"
        } else if config.current.kind == Kind::Hunt {
            "恢复当前武器默认设置"
        } else {
            "恢复当前模式默认设置"
        };
        if ui.button(reset_label).clicked() {
            config.reset_current();
        }
        if ui.button("立即保存设置").clicked() {
            save_now = true;
        }
    });
    save_now
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hotkey_and_scheme_management_save_without_submitting() {
        let before = Config::default();
        let mut after = before.clone();
        after.hotkey = "Ctrl+Alt+F8".into();
        assert_eq!(
            EditResult::between(&before, &after),
            EditResult {
                save: true,
                output: false
            }
        );
        after = before.clone();
        after.schemes.push(Scheme {
            name: "saved".into(),
            crosshair: after.current.clone(),
        });
        assert_eq!(
            EditResult::between(&before, &after),
            EditResult {
                save: true,
                output: false
            }
        );
        let saved = after.clone();
        after.schemes.clear();
        assert_eq!(
            EditResult::between(&saved, &after),
            EditResult {
                save: true,
                output: false
            }
        );
    }
    #[test]
    fn output_changes_save_and_submit() {
        let before = Config::default();
        for field in [
            "offset",
            "zoom",
            "visible",
            "appearance",
            "outline_opacity",
            "mode",
            "scheme",
        ] {
            let mut after = before.clone();
            match field {
                "offset" => after.current.x = 12,
                "zoom" => after.current.rtss_zoom = 4,
                "visible" => after.visible = false,
                "appearance" => after.current.opacity = 80,
                "outline_opacity" => after.current.outline_opacity = 60,
                "mode" => after.switch_kind(Kind::Text),
                "scheme" => {
                    let mut c = after.current.clone();
                    c.gap = 20.;
                    after.apply_crosshair(c);
                }
                _ => unreachable!(),
            }
            assert_eq!(
                EditResult::between(&before, &after),
                EditResult {
                    save: true,
                    output: true
                },
                "{field}"
            );
        }
        assert_eq!(
            EditResult::between(&before, &before),
            EditResult {
                save: false,
                output: false
            }
        );
    }
}

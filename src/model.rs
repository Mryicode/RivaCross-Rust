use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Preset,
    Image,
    Text,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Dot,
    Cross,
    // Retained for decoding version 1 configurations and saved schemes.
    CrossDot,
    Ring,
    Square,
    Quadrant,
}
impl Shape {
    pub fn has_length(self) -> bool {
        matches!(self, Self::Cross | Self::CrossDot | Self::Quadrant)
    }
    pub fn has_gap(self) -> bool {
        self != Self::Dot
    }
    pub fn has_center_dot(self) -> bool {
        self != Self::Dot
    }
    pub fn has_t_shape(self) -> bool {
        matches!(self, Self::Cross | Self::CrossDot)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crosshair {
    pub kind: Kind,
    pub shape: Shape,
    pub color: [u8; 3],
    pub opacity: u8,
    // Version 1 dot/ring diameter; new presets use thickness and gap.
    pub size: f32,
    pub thickness: f32,
    pub length: f32,
    pub gap: f32,
    pub outline: f32,
    pub outline_color: [u8; 3],
    pub outline_opacity: u8,
    pub center_dot: bool,
    pub t_shape: bool,
    pub x: i32,
    pub y: i32,
    pub image: Option<String>,
    pub image_scale: f32,
    pub text: String,
    pub text_size: u32,
    pub rtss_zoom: u32,
}
impl Default for Crosshair {
    fn default() -> Self {
        Self {
            kind: Kind::Preset,
            shape: Shape::Cross,
            color: [0, 255, 0],
            opacity: 255,
            size: 8.,
            thickness: 2.,
            length: 10.,
            gap: 4.,
            outline: 1.,
            outline_color: [0, 0, 0],
            outline_opacity: 255,
            center_dot: false,
            t_shape: false,
            x: 0,
            y: 0,
            image: None,
            image_scale: 1.,
            text: "+".into(),
            text_size: 100,
            rtss_zoom: 0,
        }
    }
}
impl Crosshair {
    pub fn validate(&self) -> Result<()> {
        for (v, min, max) in [
            (self.size, 1., 256.),
            (self.thickness, 1., 256.),
            (self.length, 1., 256.),
            (self.gap, 0., 256.),
            (self.outline, 0., 16.),
            (self.image_scale, 0.01, 16.),
        ] {
            ensure!(
                v.is_finite() && (min..=max).contains(&v),
                "参数超出有效范围"
            );
        }
        ensure!(
            self.x.abs_diff(0) <= 8192 && self.y.abs_diff(0) <= 8192,
            "偏移超出有效范围"
        );
        ensure!((10..=1000).contains(&self.text_size), "字符大小无效");
        ensure!(self.rtss_zoom <= 16, "RTSS 缩放补偿无效");
        if self.kind == Kind::Text {
            validate_text(&self.text)?;
        }
        if let Some(name) = &self.image {
            ensure!(
                !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..",
                "图片资源名无效"
            );
        }
        Ok(())
    }
}
pub fn validate_text(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty() && s.len() <= 128,
        "请输入 1–128 个英文字符或符号"
    );
    ensure!(
        s.bytes()
            .all(|b| (32..=126).contains(&b) && b != b'<' && b != b'>'),
        "仅支持单行可打印 ASCII；尖括号为 RTSS 标签保留字符"
    );
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scheme {
    pub name: String,
    pub crosshair: Crosshair,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModeSettings {
    pub preset: Crosshair,
    pub image: Crosshair,
    pub text: Crosshair,
}
impl ModeSettings {
    fn from_legacy(current: &Crosshair) -> Self {
        let mut preset = current.clone();
        preset.kind = Kind::Preset;
        let mut image = current.clone();
        image.kind = Kind::Image;
        let mut text = current.clone();
        text.kind = Kind::Text;
        // Older configs could contain invalid text while another mode was active.
        if current.kind != Kind::Text && validate_text(&text.text).is_err() {
            text.text = Crosshair::default().text;
        }
        Self {
            preset,
            image,
            text,
        }
    }
    fn get_mut(&mut self, kind: Kind) -> &mut Crosshair {
        match kind {
            Kind::Preset => &mut self.preset,
            Kind::Image => &mut self.image,
            Kind::Text => &mut self.text,
        }
    }
    fn validate(&self) -> Result<()> {
        for (c, kind) in [
            (&self.preset, Kind::Preset),
            (&self.image, Kind::Image),
            (&self.text, Kind::Text),
        ] {
            ensure!(c.kind == kind, "模式设置类型不匹配");
            c.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PresetSettings {
    pub cross: Crosshair,
    pub dot: Crosshair,
    pub square: Crosshair,
    pub quadrant: Crosshair,
    pub ring: Crosshair,
}
impl PresetSettings {
    fn from_legacy(current: &Crosshair) -> Self {
        let default = |shape| Crosshair {
            shape,
            ..Default::default()
        };
        let mut settings = Self {
            cross: default(Shape::Cross),
            dot: default(Shape::Dot),
            square: default(Shape::Square),
            quadrant: default(Shape::Quadrant),
            ring: default(Shape::Ring),
        };
        let mut preset = current.clone();
        preset.kind = Kind::Preset;
        if preset.shape == Shape::CrossDot {
            preset.shape = Shape::Cross;
            preset.center_dot = true;
        }
        let shape = preset.shape;
        *settings.get_mut(shape) = preset;
        settings
    }
    fn get_mut(&mut self, shape: Shape) -> &mut Crosshair {
        match shape {
            Shape::Cross | Shape::CrossDot => &mut self.cross,
            Shape::Dot => &mut self.dot,
            Shape::Square => &mut self.square,
            Shape::Quadrant => &mut self.quadrant,
            Shape::Ring => &mut self.ring,
        }
    }
    fn validate(&self) -> Result<()> {
        for (c, shape) in [
            (&self.cross, Shape::Cross),
            (&self.dot, Shape::Dot),
            (&self.square, Shape::Square),
            (&self.quadrant, Shape::Quadrant),
            (&self.ring, Shape::Ring),
        ] {
            ensure!(
                c.kind == Kind::Preset && c.shape == shape,
                "预设设置类型不匹配"
            );
            c.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub current: Crosshair,
    // None is only used when reading a config saved before independent settings.
    #[serde(default)]
    pub mode_settings: Option<ModeSettings>,
    #[serde(default)]
    pub preset_settings: Option<PresetSettings>,
    pub visible: bool,
    pub hotkey: String,
    pub schemes: Vec<Scheme>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 2,
            current: Crosshair::default(),
            mode_settings: Some(ModeSettings::from_legacy(&Crosshair::default())),
            preset_settings: Some(PresetSettings::from_legacy(&Crosshair::default())),
            visible: true,
            hotkey: "Ctrl+Alt+F9".into(),
            schemes: vec![],
        }
    }
}
impl Config {
    pub fn migrate_presets(&mut self) {
        if self.version != 1 {
            return;
        }
        fn migrate(c: &mut Crosshair) {
            match c.shape {
                Shape::Dot => c.thickness = c.size,
                Shape::CrossDot => {
                    c.shape = Shape::Cross;
                    c.center_dot = true;
                }
                Shape::Ring => c.gap = (c.size - c.thickness).max(0.),
                _ => {}
            }
        }
        migrate(&mut self.current);
        if let Some(settings) = &mut self.mode_settings {
            migrate(&mut settings.preset);
            migrate(&mut settings.image);
            migrate(&mut settings.text);
        }
        if let Some(settings) = &mut self.preset_settings {
            for c in [
                &mut settings.cross,
                &mut settings.dot,
                &mut settings.square,
                &mut settings.quadrant,
                &mut settings.ring,
            ] {
                migrate(c);
            }
        }
        for scheme in &mut self.schemes {
            migrate(&mut scheme.crosshair);
        }
        self.version = 2;
    }
    pub fn remember_current(&mut self) {
        let settings = self
            .mode_settings
            .get_or_insert_with(|| ModeSettings::from_legacy(&self.current));
        *settings.get_mut(self.current.kind) = self.current.clone();
        let presets = self
            .preset_settings
            .get_or_insert_with(|| PresetSettings::from_legacy(&settings.preset));
        if self.current.kind == Kind::Preset {
            *presets.get_mut(self.current.shape) = self.current.clone();
        }
    }
    pub fn switch_shape(&mut self, shape: Shape) {
        if self.current.kind != Kind::Preset || self.current.shape == shape {
            return;
        }
        self.remember_current();
        self.current = self
            .preset_settings
            .as_mut()
            .unwrap()
            .get_mut(shape)
            .clone();
        self.remember_current();
    }
    pub fn switch_kind(&mut self, kind: Kind) {
        if self.current.kind == kind {
            return;
        }
        self.remember_current();
        self.current = self.mode_settings.as_mut().unwrap().get_mut(kind).clone();
    }
    pub fn apply_crosshair(&mut self, crosshair: Crosshair) {
        self.remember_current();
        self.current = crosshair;
        self.remember_current();
    }
    pub fn reset_current(&mut self) {
        self.apply_crosshair(Crosshair {
            kind: self.current.kind,
            shape: if self.current.kind == Kind::Preset {
                self.current.shape
            } else {
                Shape::Cross
            },
            ..Default::default()
        });
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(matches!(self.version, 1 | 2), "不支持此配置版本");
        self.current.validate()?;
        if let Some(settings) = &self.mode_settings {
            settings.validate()?;
        }
        if let Some(settings) = &self.preset_settings {
            settings.validate()?;
        }
        ensure!(self.schemes.len() <= 100, "方案过多");
        for s in &self.schemes {
            ensure!(
                !s.name.trim().is_empty() && s.name.len() <= 128,
                "方案名称无效"
            );
            s.crosshair.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preset_shapes_restore_complete_settings_without_inheriting_edits() {
        let mut c = Config::default();
        let mut expected = Vec::new();
        for (index, shape) in [
            Shape::Cross,
            Shape::Dot,
            Shape::Square,
            Shape::Quadrant,
            Shape::Ring,
        ]
        .into_iter()
        .enumerate()
        {
            c.switch_shape(shape);
            assert_eq!(
                c.current,
                Crosshair {
                    shape,
                    ..Default::default()
                }
            );
            c.current.length = 12. + index as f32;
            c.current.thickness = 2. + index as f32;
            c.current.gap = 4. + index as f32;
            c.current.center_dot = index % 2 == 0;
            c.current.t_shape = index == 0;
            c.current.outline = index as f32;
            c.current.outline_color = [index as u8, 30, 100];
            c.current.outline_opacity = 40 + index as u8;
            c.current.color = [100, index as u8, 50];
            c.current.opacity = 100 + index as u8;
            c.current.x = index as i32;
            c.current.y = -(index as i32);
            c.current.rtss_zoom = index as u32;
            expected.push(c.current.clone());
        }
        for preset in expected.iter().rev() {
            c.switch_shape(preset.shape);
            assert_eq!(&c.current, preset);
        }
        c.switch_kind(Kind::Image);
        c.current.image_scale = 3.;
        c.switch_kind(Kind::Text);
        c.current.text = "@".into();
        c.switch_kind(Kind::Preset);
        assert_eq!(c.current, expected[0]);
        for preset in &expected {
            c.switch_shape(preset.shape);
            assert_eq!(&c.current, preset);
        }
    }
    #[test]
    fn scheme_and_reset_only_change_the_selected_preset_shape() {
        let mut c = Config::default();
        c.current.length = 24.;
        c.current.t_shape = true;
        let cross = c.current.clone();
        c.switch_shape(Shape::Ring);
        c.current.gap = 30.;
        let ring = c.current.clone();
        c.apply_crosshair(Crosshair {
            shape: Shape::Quadrant,
            length: 40.,
            outline_color: [200, 20, 80],
            ..Default::default()
        });
        let quadrant = c.current.clone();
        c.switch_shape(Shape::Cross);
        assert_eq!(c.current, cross);
        c.switch_shape(Shape::Quadrant);
        assert_eq!(c.current, quadrant);
        c.reset_current();
        assert_eq!(
            c.current,
            Crosshair {
                shape: Shape::Quadrant,
                ..Default::default()
            }
        );
        c.switch_shape(Shape::Ring);
        assert_eq!(c.current, ring);
        c.switch_shape(Shape::Cross);
        assert_eq!(c.current, cross);
    }
    #[test]
    fn validates_inactive_preset_values_and_shape_identity() {
        let mut c = Config::default();
        c.preset_settings.as_mut().unwrap().quadrant.length = f32::NAN;
        assert!(c.validate().is_err());
        c.preset_settings.as_mut().unwrap().quadrant.length = 10.;
        c.preset_settings.as_mut().unwrap().quadrant.shape = Shape::Cross;
        assert!(c.validate().is_err());
        c.preset_settings.as_mut().unwrap().quadrant.shape = Shape::Quadrant;
        c.preset_settings.as_mut().unwrap().quadrant.kind = Kind::Image;
        assert!(c.validate().is_err());
    }
    #[test]
    fn preset_controls_match_supported_parameters() {
        for (shape, length, gap, dot, t) in [
            (Shape::Cross, true, true, true, true),
            (Shape::Dot, false, false, false, false),
            (Shape::Square, false, true, true, false),
            (Shape::Quadrant, true, true, true, false),
            (Shape::Ring, false, true, true, false),
        ] {
            assert_eq!(shape.has_length(), length);
            assert_eq!(shape.has_gap(), gap);
            assert_eq!(shape.has_center_dot(), dot);
            assert_eq!(shape.has_t_shape(), t);
        }
    }
    #[test]
    fn modes_restore_all_settings_without_inheriting_edits() {
        let mut c = Config::default();
        c.current.color = [255, 0, 0];
        c.current.opacity = 80;
        c.current.x = 15;
        c.current.y = -12;
        c.current.rtss_zoom = 2;
        c.current.shape = Shape::Ring;
        c.current.size = 25.;
        let preset = c.current.clone();
        c.switch_kind(Kind::Image);
        assert_eq!(c.current.opacity, 255);
        assert_eq!(c.current.x, 0);
        c.current.image = Some("test.png".into());
        c.current.image_scale = 3.;
        c.current.opacity = 120;
        c.current.x = -8;
        c.current.rtss_zoom = 4;
        let image = c.current.clone();
        c.switch_kind(Kind::Text);
        assert_eq!(c.current.color, [0, 255, 0]);
        c.current.text = "X".into();
        c.current.text_size = 200;
        c.current.color = [0, 0, 255];
        c.current.y = 32;
        let text = c.current.clone();
        for (kind, expected) in [
            (Kind::Preset, preset),
            (Kind::Image, image),
            (Kind::Text, text),
        ] {
            c.switch_kind(kind);
            assert_eq!(c.current, expected);
        }
    }
    #[test]
    fn applying_scheme_and_reset_only_change_target_mode() {
        let mut c = Config::default();
        c.current.x = 50;
        let preset = c.current.clone();
        c.apply_crosshair(Crosshair {
            kind: Kind::Text,
            text: "!".into(),
            x: 20,
            ..Default::default()
        });
        c.reset_current();
        assert_eq!(c.current.kind, Kind::Text);
        assert_eq!(c.current.text, "+");
        assert_eq!(c.current.x, 0);
        c.switch_kind(Kind::Preset);
        assert_eq!(c.current, preset);
    }
    #[test]
    fn validates_inactive_modes_and_their_types() {
        let mut c = Config::default();
        c.mode_settings.as_mut().unwrap().image.image = Some("../bad.png".into());
        assert!(c.validate().is_err());
        c.mode_settings.as_mut().unwrap().image.image = None;
        c.mode_settings.as_mut().unwrap().text.kind = Kind::Preset;
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_injection_unicode_and_lines() {
        for s in ["<P4>", "中", "😀", "a\nb", ""] {
            assert!(validate_text(s).is_err());
        }
        assert!(validate_text("+ . !@#$%^&*()[]{}\\").is_ok());
    }
    #[test]
    fn rejects_nan_and_resource_traversal() {
        let mut c = Crosshair {
            size: f32::NAN,
            ..Default::default()
        };
        assert!(c.validate().is_err());
        c.size = 8.;
        c.image = Some("../a.png".into());
        assert!(c.validate().is_err());
    }
}

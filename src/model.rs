use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Preset,
    Image,
    Text,
    Hunt,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum HuntWeapon {
    Crossbow,
    Bow,
    SniperCrossbow,
}
impl HuntWeapon {
    pub fn label(self) -> &'static str {
        match self {
            Self::Crossbow => "十字弩",
            Self::Bow => "弓",
            Self::SniperCrossbow => "狙击弩",
        }
    }
    pub fn svg(self) -> &'static [u8] {
        match self {
            Self::Crossbow => include_bytes!("../assets/crossbow.svg"),
            Self::Bow => include_bytes!("../assets/bow.svg"),
            Self::SniperCrossbow => include_bytes!("../assets/sniper-crossbow.svg"),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Dot,
    Cross,
    Ring,
    Square,
    Quadrant,
}
impl Shape {
    pub fn has_length(self) -> bool {
        matches!(self, Self::Cross | Self::Quadrant)
    }
    pub fn has_gap(self) -> bool {
        self != Self::Dot
    }
    pub fn has_center_dot(self) -> bool {
        self != Self::Dot
    }
    pub fn has_t_shape(self) -> bool {
        self == Self::Cross
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crosshair {
    pub kind: Kind,
    pub hunt_weapon: HuntWeapon,
    pub shape: Shape,
    pub color: [u8; 3],
    pub opacity: u8,
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
            hunt_weapon: HuntWeapon::Crossbow,
            shape: Shape::Cross,
            color: [0, 255, 0],
            opacity: 255,
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
    pub hunt: Crosshair,
}
impl Default for ModeSettings {
    fn default() -> Self {
        Self {
            preset: Crosshair::default(),
            image: Crosshair {
                kind: Kind::Image,
                ..Default::default()
            },
            hunt: Crosshair {
                kind: Kind::Hunt,
                ..Default::default()
            },
            text: Crosshair {
                kind: Kind::Text,
                ..Default::default()
            },
        }
    }
}
impl ModeSettings {
    fn get_mut(&mut self, kind: Kind) -> &mut Crosshair {
        match kind {
            Kind::Preset => &mut self.preset,
            Kind::Image => &mut self.image,
            Kind::Text => &mut self.text,
            Kind::Hunt => &mut self.hunt,
        }
    }
    fn validate(&self) -> Result<()> {
        for (c, kind) in [
            (&self.preset, Kind::Preset),
            (&self.image, Kind::Image),
            (&self.text, Kind::Text),
            (&self.hunt, Kind::Hunt),
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
impl Default for PresetSettings {
    fn default() -> Self {
        let default = |shape| Crosshair {
            shape,
            ..Default::default()
        };
        Self {
            cross: default(Shape::Cross),
            dot: default(Shape::Dot),
            square: default(Shape::Square),
            quadrant: default(Shape::Quadrant),
            ring: default(Shape::Ring),
        }
    }
}
impl PresetSettings {
    fn get_mut(&mut self, shape: Shape) -> &mut Crosshair {
        match shape {
            Shape::Cross => &mut self.cross,
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
pub struct HuntSettings {
    pub crossbow: Crosshair,
    pub bow: Crosshair,
    // Only this newly added weapon defaults in existing complete version 3 files.
    #[serde(default = "default_sniper_crossbow")]
    pub sniper_crossbow: Crosshair,
}
fn default_sniper_crossbow() -> Crosshair {
    Crosshair {
        kind: Kind::Hunt,
        hunt_weapon: HuntWeapon::SniperCrossbow,
        ..Default::default()
    }
}
impl Default for HuntSettings {
    fn default() -> Self {
        let weapon = |hunt_weapon| Crosshair {
            kind: Kind::Hunt,
            hunt_weapon,
            ..Default::default()
        };
        Self {
            crossbow: weapon(HuntWeapon::Crossbow),
            bow: weapon(HuntWeapon::Bow),
            sniper_crossbow: default_sniper_crossbow(),
        }
    }
}
impl HuntSettings {
    fn get_mut(&mut self, weapon: HuntWeapon) -> &mut Crosshair {
        match weapon {
            HuntWeapon::Crossbow => &mut self.crossbow,
            HuntWeapon::Bow => &mut self.bow,
            HuntWeapon::SniperCrossbow => &mut self.sniper_crossbow,
        }
    }
    fn validate(&self) -> Result<()> {
        for (c, weapon) in [
            (&self.crossbow, HuntWeapon::Crossbow),
            (&self.bow, HuntWeapon::Bow),
            (&self.sniper_crossbow, HuntWeapon::SniperCrossbow),
        ] {
            ensure!(
                c.kind == Kind::Hunt && c.hunt_weapon == weapon,
                "Hunt 武器设置类型不匹配"
            );
            c.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub current: Crosshair,
    pub mode_settings: ModeSettings,
    pub preset_settings: PresetSettings,
    pub hunt_settings: HuntSettings,
    pub visible: bool,
    pub hotkey: String,
    pub schemes: Vec<Scheme>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 3,
            current: Crosshair::default(),
            mode_settings: ModeSettings::default(),
            preset_settings: PresetSettings::default(),
            hunt_settings: HuntSettings::default(),
            visible: true,
            hotkey: "Ctrl+Alt+F9".into(),
            schemes: vec![],
        }
    }
}
impl Config {
    pub fn remember_current(&mut self) {
        *self.mode_settings.get_mut(self.current.kind) = self.current.clone();
        if self.current.kind == Kind::Hunt {
            *self.hunt_settings.get_mut(self.current.hunt_weapon) = self.current.clone();
        }
        if self.current.kind == Kind::Preset {
            *self.preset_settings.get_mut(self.current.shape) = self.current.clone();
        }
    }
    pub fn switch_weapon(&mut self, weapon: HuntWeapon) {
        if self.current.kind != Kind::Hunt || self.current.hunt_weapon == weapon {
            return;
        }
        self.remember_current();
        self.current = self.hunt_settings.get_mut(weapon).clone();
        self.remember_current();
    }
    pub fn switch_shape(&mut self, shape: Shape) {
        if self.current.kind != Kind::Preset || self.current.shape == shape {
            return;
        }
        self.remember_current();
        self.current = self.preset_settings.get_mut(shape).clone();
        self.remember_current();
    }
    pub fn switch_kind(&mut self, kind: Kind) {
        if self.current.kind == kind {
            return;
        }
        self.remember_current();
        self.current = self.mode_settings.get_mut(kind).clone();
    }
    pub fn apply_crosshair(&mut self, crosshair: Crosshair) {
        self.remember_current();
        self.current = crosshair;
        self.remember_current();
    }
    pub fn reset_current(&mut self) {
        self.apply_crosshair(Crosshair {
            kind: self.current.kind,
            hunt_weapon: self.current.hunt_weapon,
            shape: if self.current.kind == Kind::Preset {
                self.current.shape
            } else {
                Shape::Cross
            },
            ..Default::default()
        });
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 3,
            "不支持此配置版本；需要版本 3，请重置或手动处理旧配置"
        );
        self.current.validate()?;
        self.mode_settings.validate()?;
        self.preset_settings.validate()?;
        self.hunt_settings.validate()?;
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
    fn all_hunt_weapons_keep_independent_settings_and_sniper_reset_is_isolated() {
        let mut c = Config::default();
        c.switch_kind(Kind::Hunt);
        let mut saved = Vec::new();
        for (index, weapon) in [
            HuntWeapon::Crossbow,
            HuntWeapon::Bow,
            HuntWeapon::SniperCrossbow,
        ]
        .into_iter()
        .enumerate()
        {
            c.switch_weapon(weapon);
            c.current.image_scale = 1. + index as f32 * 0.5;
            c.current.opacity = 80 + index as u8 * 50;
            c.current.x = index as i32 * 10;
            c.current.y = -(index as i32) * 5;
            c.current.rtss_zoom = index as u32 + 1;
            saved.push(c.current.clone());
        }
        c.switch_kind(Kind::Text);
        c.switch_kind(Kind::Hunt);
        assert_eq!(c.current, saved[2]);
        c.reset_current();
        assert_eq!(c.current, default_sniper_crossbow());
        for expected in &saved {
            c.apply_crosshair(expected.clone());
            assert_eq!(&c.current, expected);
        }
        for expected in &saved {
            c.switch_weapon(expected.hunt_weapon);
            assert_eq!(&c.current, expected);
        }
        c.hunt_settings.sniper_crossbow.image_scale = f32::NAN;
        assert!(c.validate().is_err());
        c.hunt_settings.sniper_crossbow = default_sniper_crossbow();
        c.hunt_settings.sniper_crossbow.hunt_weapon = HuntWeapon::Bow;
        assert!(c.validate().is_err());
    }
    #[test]
    fn hunt_weapons_restore_and_reset_only_the_selected_weapon() {
        let mut c = Config::default();
        c.current.x = 42;
        let preset = c.current.clone();
        c.switch_kind(Kind::Hunt);
        assert_eq!(c.current.hunt_weapon, HuntWeapon::Crossbow);
        c.current.image_scale = 1.57;
        c.current.opacity = 128;
        c.current.x = -20;
        c.current.y = 15;
        c.current.rtss_zoom = 3;
        let crossbow = c.current.clone();
        c.switch_weapon(HuntWeapon::Bow);
        assert_eq!(c.current.image_scale, 1.);
        c.current.image_scale = 2.20;
        c.current.opacity = 80;
        c.current.x = 12;
        c.current.rtss_zoom = 4;
        let bow = c.current.clone();
        c.switch_kind(Kind::Image);
        c.switch_kind(Kind::Hunt);
        assert_eq!(c.current, bow);
        c.switch_weapon(HuntWeapon::Crossbow);
        assert_eq!(c.current, crossbow);
        c.apply_crosshair(bow.clone());
        c.reset_current();
        assert_eq!(
            c.current,
            Crosshair {
                kind: Kind::Hunt,
                hunt_weapon: HuntWeapon::Bow,
                ..Default::default()
            }
        );
        c.switch_weapon(HuntWeapon::Crossbow);
        assert_eq!(c.current, crossbow);
        c.switch_kind(Kind::Preset);
        assert_eq!(c.current, preset);
        c.apply_crosshair(bow.clone());
        assert_eq!(c.current, bow);
        c.validate().unwrap();
    }
    #[test]
    fn inactive_hunt_values_and_weapon_identity_are_validated() {
        let mut c = Config::default();
        c.hunt_settings.bow.image_scale = f32::NAN;
        assert!(c.validate().is_err());
        c.hunt_settings.bow.image_scale = 1.;
        c.hunt_settings.bow.hunt_weapon = HuntWeapon::Crossbow;
        assert!(c.validate().is_err());
        c.hunt_settings = HuntSettings::default();
        c.mode_settings.hunt.kind = Kind::Image;
        assert!(c.validate().is_err());
    }
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
        c.preset_settings.quadrant.length = f32::NAN;
        assert!(c.validate().is_err());
        c.preset_settings.quadrant.length = 10.;
        c.preset_settings.quadrant.shape = Shape::Cross;
        assert!(c.validate().is_err());
        c.preset_settings.quadrant.shape = Shape::Quadrant;
        c.preset_settings.quadrant.kind = Kind::Image;
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
        c.current.gap = 25.;
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
        c.mode_settings.image.image = Some("../bad.png".into());
        assert!(c.validate().is_err());
        c.mode_settings.image.image = None;
        c.mode_settings.text.kind = Kind::Preset;
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
            thickness: f32::NAN,
            ..Default::default()
        };
        assert!(c.validate().is_err());
        c.thickness = 8.;
        c.image = Some("../a.png".into());
        assert!(c.validate().is_err());
    }
}

use crate::model::Config;
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Store {
    root: PathBuf,
}
impl Store {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn read_resource(&self, name: &str) -> Result<Vec<u8>> {
        ensure!(
            !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..",
            "资源名无效"
        );
        fs::read(self.root.join("resources").join(name)).context("源图片丢失，请重新导入")
    }
    pub fn write_cache(&self, texture: &image::RgbaImage) -> Result<PathBuf> {
        let path = self.root.join("cache").join(format!("{}.png", unique()));
        texture.save(&path)?;
        Ok(path)
    }
    pub fn record_exit_error(&self, error: &str) -> Result<()> {
        fs::write(self.root.join("last-exit-error.txt"), error)?;
        Ok(())
    }

    pub fn prune_cache(&self, age: std::time::Duration) -> Result<()> {
        let mut files = Vec::new();
        for entry in fs::read_dir(self.root.join("cache"))? {
            let entry = entry?;
            let info = entry.metadata()?;
            if info.is_file() && entry.path().extension().is_some_and(|e| e == "png") {
                files.push((entry.path(), info.modified()?));
            }
        }
        files.sort_by_key(|(_, time)| *time);
        // Keep the latest image even when old: it must remain reloadable after RTSS restarts.
        files.pop();
        for (path, time) in files {
            if time.elapsed().is_ok_and(|elapsed| elapsed > age) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
    pub fn open() -> Result<Self> {
        let root =
            PathBuf::from(std::env::var_os("LOCALAPPDATA").context("无法定位 LOCALAPPDATA")?)
                .join("RivaCross");
        Self::at(root)
    }
    pub fn at(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(root.join("resources"))?;
        fs::create_dir_all(root.join("cache"))?;
        Ok(Self { root })
    }
    pub fn load(&self) -> Result<Config> {
        let path = self.root.join("config.json");
        if !path.exists() {
            return Ok(Config::default());
        }
        let data = fs::read(&path)?;
        ensure!(data.len() <= 1024 * 1024, "配置文件过大");
        let mut data: serde_json::Value =
            serde_json::from_slice(&data).context("配置损坏；可重置当前设置后保存")?;
        inherit_legacy_outline_opacity(&mut data);
        let mut c: Config =
            serde_json::from_value(data).context("配置损坏；可重置当前设置后保存")?;
        c.validate()?;
        c.migrate_presets();
        c.remember_current();
        Ok(c)
    }
    pub fn save(&self, c: &Config) -> Result<()> {
        let mut c = c.clone();
        c.migrate_presets();
        c.remember_current();
        c.validate()?;
        let tmp = self.root.join("config.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&c)?)?;
        let dest = self.root.join("config.json");
        // MoveFileEx replaces atomically on Windows, preserving the previous file on failure.
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{
                MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            },
            core::PCWSTR,
        };
        let a: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            MoveFileExW(
                PCWSTR(a.as_ptr()),
                PCWSTR(b.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }?;
        Ok(())
    }
    pub fn import(&self, path: &Path) -> Result<String> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(ext == "png" || ext == "svg", "仅支持静态 PNG 和 SVG");
        ensure!(
            fs::metadata(path)?.len() <= 32 * 1024 * 1024,
            "图片不能超过 32 MiB"
        );
        let data = fs::read(path)?;
        crate::image_input::decode(&data, &ext, 1.)?;
        let name = format!("{}.{ext}", unique());
        fs::write(self.root.join("resources").join(&name), data)?;
        Ok(name)
    }
}
// Old configs used one alpha for both fill and outline. Apply the inherited
// value to every stored crosshair, including inactive presets and schemes.
fn inherit_legacy_outline_opacity(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            if object.contains_key("kind") && !object.contains_key("outline_opacity") {
                let opacity = object.get("opacity").cloned().unwrap_or(255.into());
                object.insert("outline_opacity".into(), opacity);
            }
            for child in object.values_mut() {
                inherit_legacy_outline_opacity(child);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                inherit_legacy_outline_opacity(child);
            }
        }
        _ => {}
    }
}
pub fn unique() -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_retains_latest_for_reconnection() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        let path = s.root.join("cache/current.png");
        fs::write(&path, b"png").unwrap();
        s.prune_cache(std::time::Duration::ZERO).unwrap();
        assert!(path.exists());
    }
    #[test]
    fn unicode_import_is_copied_and_survives_source_removal() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().join("app")).unwrap();
        let source = d.path().join("中文 空格.png");
        crate::render::preset(&Default::default())
            .unwrap()
            .save(&source)
            .unwrap();
        let name = s.import(&source).unwrap();
        fs::remove_file(source).unwrap();
        assert!(s.root.join("resources").join(name).exists());
    }
    #[test]
    fn legacy_outline_opacity_inherits_alpha_in_presets_and_schemes() {
        use crate::model::{Kind, Scheme, Shape};
        let d = tempfile::tempdir().unwrap();
        let store = Store::at(d.path().into()).unwrap();
        let mut config = Config::default();
        config.current.opacity = 70;
        config.schemes.push(Scheme {
            name: "legacy".into(),
            crosshair: config.current.clone(),
        });
        config.switch_shape(Shape::Ring);
        config.current.opacity = 120;
        config.switch_kind(Kind::Text);
        store.save(&config).unwrap();
        let path = store.root.join("config.json");
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        fn strip(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(object) => {
                    object.remove("outline_opacity");
                    for child in object.values_mut() {
                        strip(child);
                    }
                }
                serde_json::Value::Array(array) => {
                    for child in array {
                        strip(child);
                    }
                }
                _ => {}
            }
        }
        strip(&mut json);
        fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let mut loaded = store.load().unwrap();
        assert_eq!(loaded.schemes[0].crosshair.outline_opacity, 70);
        loaded.switch_kind(Kind::Preset);
        assert_eq!(loaded.current.outline_opacity, 120);
        loaded.switch_shape(Shape::Cross);
        assert_eq!(loaded.current.outline_opacity, 70);
        loaded.current.outline_opacity = 180;
        store.save(&loaded).unwrap();
        assert_eq!(store.load().unwrap().current.outline_opacity, 180);
    }
    #[test]
    fn roundtrip_and_corruption() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().to_owned()).unwrap();
        let c = Config::default();
        s.save(&c).unwrap();
        assert_eq!(s.load().unwrap().current, c.current);
        fs::write(s.root.join("config.json"), b"broken").unwrap();
        assert!(s.load().is_err());
    }
    #[test]
    fn independent_modes_survive_save_and_restart() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        let mut c = Config::default();
        c.current.x = 17;
        c.current.color = [255, 0, 0];
        c.current.shape = crate::model::Shape::Quadrant;
        c.current.center_dot = true;
        c.current.t_shape = true;
        c.current.outline_color = [40, 80, 120];
        let preset = c.current.clone();
        c.switch_kind(crate::model::Kind::Image);
        c.current.image = Some("resource.svg".into());
        c.current.image_scale = 2.5;
        c.current.opacity = 100;
        let image = c.current.clone();
        c.switch_kind(crate::model::Kind::Text);
        c.current.text = "*".into();
        c.current.text_size = 150;
        c.current.rtss_zoom = 3;
        let text = c.current.clone();
        s.save(&c).unwrap();
        let mut loaded = s.load().unwrap();
        assert_eq!(loaded.current, text);
        loaded.switch_kind(crate::model::Kind::Preset);
        assert_eq!(loaded.current, preset);
        loaded.switch_kind(crate::model::Kind::Image);
        assert_eq!(loaded.current, image);
        loaded.switch_kind(crate::model::Kind::Text);
        assert_eq!(loaded.current, text);
    }
    #[test]
    fn every_preset_survives_restart_while_another_mode_is_active() {
        use crate::model::{Kind, Shape};
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
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
            c.current.length = 20. + index as f32;
            c.current.gap = 12. + index as f32;
            c.current.thickness = 3. + index as f32;
            c.current.color = [index as u8, 120, 60];
            c.current.outline_color = [220, index as u8, 20];
            c.current.outline_opacity = 30 + index as u8;
            c.current.center_dot = index % 2 == 0;
            c.current.t_shape = index == 0;
            expected.push(c.current.clone());
        }
        c.switch_kind(Kind::Image);
        c.current.image_scale = 4.;
        let image = c.current.clone();
        s.save(&c).unwrap();
        let mut loaded = s.load().unwrap();
        assert_eq!(loaded.current, image);
        loaded.switch_kind(Kind::Preset);
        assert_eq!(loaded.current, *expected.last().unwrap());
        for preset in expected {
            loaded.switch_shape(preset.shape);
            assert_eq!(loaded.current, preset);
        }
    }
    #[test]
    fn old_config_seeds_only_the_last_preset_and_keeps_other_shapes_default() {
        use crate::model::{Crosshair, Kind, Shape};
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        for active_kind in [Kind::Preset, Kind::Image] {
            let mut c = Config::default();
            c.current.shape = Shape::Quadrant;
            c.current.length = 37.;
            c.current.thickness = 7.;
            c.current.outline_color = [120, 90, 60];
            let quadrant = c.current.clone();
            c.switch_kind(active_kind);
            let mut old = serde_json::to_value(c).unwrap();
            old.as_object_mut().unwrap().remove("preset_settings");
            fs::write(
                s.root.join("config.json"),
                serde_json::to_vec(&old).unwrap(),
            )
            .unwrap();
            let mut loaded = s.load().unwrap();
            loaded.switch_kind(Kind::Preset);
            assert_eq!(loaded.current, quadrant);
            for shape in [Shape::Cross, Shape::Dot, Shape::Square, Shape::Ring] {
                loaded.switch_shape(shape);
                assert_eq!(
                    loaded.current,
                    Crosshair {
                        shape,
                        ..Default::default()
                    }
                );
            }
            loaded.switch_shape(Shape::Quadrant);
            assert_eq!(loaded.current, quadrant);
            s.save(&loaded).unwrap();
            assert_eq!(s.load().unwrap().current, quadrant);
        }
    }
    #[test]
    fn legacy_config_preserves_shared_and_mode_specific_values() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        let mut c = Config::default();
        c.current.kind = crate::model::Kind::Image;
        c.current.opacity = 95;
        c.current.x = 22;
        c.current.image = Some("old.png".into());
        c.current.image_scale = 2.;
        c.current.text = "@".into();
        c.current.text_size = 175;
        let expected = c.current.clone();
        let mut legacy = serde_json::to_value(&c).unwrap();
        legacy.as_object_mut().unwrap().remove("mode_settings");
        fs::write(
            s.root.join("config.json"),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        let mut loaded = s.load().unwrap();
        assert_eq!(loaded.current, expected);
        loaded.switch_kind(crate::model::Kind::Text);
        assert_eq!(loaded.current.text, "@");
        assert_eq!(loaded.current.text_size, 175);
        assert_eq!(loaded.current.opacity, 95);
        loaded.current.x = -10;
        loaded.switch_kind(crate::model::Kind::Image);
        assert_eq!(loaded.current, expected);
        s.save(&loaded).unwrap();
        assert!(s.load().unwrap().mode_settings.is_some());
    }
    #[test]
    fn legacy_presets_and_schemes_migrate_once_with_black_outline() {
        use crate::model::{Crosshair, Scheme, Shape};
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        let mut c = Config {
            version: 1,
            ..Default::default()
        };
        c.current.shape = Shape::CrossDot;
        c.mode_settings.as_mut().unwrap().preset.shape = Shape::Ring;
        c.mode_settings.as_mut().unwrap().preset.size = 30.;
        c.schemes.push(Scheme {
            name: "旧点".into(),
            crosshair: Crosshair {
                shape: Shape::Dot,
                size: 12.,
                ..Default::default()
            },
        });
        c.schemes.push(Scheme {
            name: "旧圆环".into(),
            crosshair: c.mode_settings.as_ref().unwrap().preset.clone(),
        });
        let mut value = serde_json::to_value(c).unwrap();
        fn remove_new_fields(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    for field in ["center_dot", "t_shape", "outline_color"] {
                        map.remove(field);
                    }
                    for child in map.values_mut() {
                        remove_new_fields(child);
                    }
                }
                serde_json::Value::Array(values) => {
                    for child in values {
                        remove_new_fields(child);
                    }
                }
                _ => {}
            }
        }
        remove_new_fields(&mut value);
        fs::write(
            s.root.join("config.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let loaded = s.load().unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.current.shape, Shape::Cross);
        assert!(loaded.current.center_dot);
        assert!(!loaded.current.t_shape);
        assert_eq!(loaded.current.outline_color, [0, 0, 0]);
        assert_eq!(loaded.schemes[0].crosshair.thickness, 12.);
        assert_eq!(loaded.schemes[1].crosshair.gap, 28.);
        s.save(&loaded).unwrap();
        let reloaded = s.load().unwrap();
        assert_eq!(loaded.current, reloaded.current);
        assert_eq!(loaded.schemes[1].crosshair, reloaded.schemes[1].crosshair);
    }
}

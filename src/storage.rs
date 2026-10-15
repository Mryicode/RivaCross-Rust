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
        let value: serde_json::Value = serde_json::from_slice(&data).context("配置损坏")?;
        ensure!(
            value.get("version").and_then(|v| v.as_u64()) == Some(3),
            "不支持此配置版本；需要版本 3，请重置或手动处理旧配置"
        );
        let mut c: Config =
            serde_json::from_value(value).context("配置损坏；可重置当前设置后保存")?;
        c.validate()?;
        c.remember_current();
        Ok(c)
    }
    pub fn save(&self, c: &Config) -> Result<()> {
        let mut c = c.clone();
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
    fn existing_v3_adds_only_sniper_defaults_and_preserves_all_saved_parameters() {
        use crate::model::{HuntWeapon, Kind, Scheme};
        let d = tempfile::tempdir().unwrap();
        let store = Store::at(d.path().into()).unwrap();
        let mut c = Config::default();
        c.switch_kind(Kind::Hunt);
        c.current.image_scale = 1.57;
        c.current.x = 12;
        c.switch_weapon(HuntWeapon::Bow);
        c.current.image_scale = 2.20;
        c.current.opacity = 128;
        c.remember_current();
        let mut old = serde_json::to_value(&c).unwrap();
        old["hunt_settings"]
            .as_object_mut()
            .unwrap()
            .remove("sniper_crossbow");
        fs::write(
            store.root.join("config.json"),
            serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        let mut loaded = store.load().unwrap();
        assert_eq!(loaded, c);
        loaded.switch_weapon(HuntWeapon::SniperCrossbow);
        assert_eq!(loaded.current.image_scale, 1.);
        loaded.current.image_scale = 2.20;
        loaded.current.opacity = 128;
        loaded.current.x = -20;
        loaded.current.rtss_zoom = 4;
        let sniper = loaded.current.clone();
        loaded.schemes.push(Scheme {
            name: "狙击弩".into(),
            crosshair: sniper.clone(),
        });
        loaded.switch_kind(Kind::Preset);
        store.save(&loaded).unwrap();
        let mut restarted = store.load().unwrap();
        restarted.switch_kind(Kind::Hunt);
        assert_eq!(restarted.current, sniper);
        restarted.reset_current();
        restarted.apply_crosshair(restarted.schemes[0].crosshair.clone());
        assert_eq!(restarted.current, sniper);
    }
    #[test]
    fn hunt_weapons_and_schemes_survive_restart_from_another_mode() {
        use crate::model::{HuntWeapon, Kind, Scheme};
        let d = tempfile::tempdir().unwrap();
        let store = Store::at(d.path().into()).unwrap();
        let mut c = Config::default();
        c.switch_kind(Kind::Hunt);
        c.current.image_scale = 1.57;
        c.current.opacity = 128;
        c.current.x = -14;
        c.current.y = 9;
        c.current.rtss_zoom = 3;
        let crossbow = c.current.clone();
        c.switch_weapon(HuntWeapon::Bow);
        c.current.image_scale = 2.20;
        c.current.opacity = 200;
        c.current.rtss_zoom = 4;
        let bow = c.current.clone();
        c.schemes.push(Scheme {
            name: "弓".into(),
            crosshair: bow.clone(),
        });
        c.switch_kind(Kind::Text);
        store.save(&c).unwrap();
        let mut loaded = store.load().unwrap();
        assert_eq!(loaded, c);
        loaded.switch_kind(Kind::Hunt);
        assert_eq!(loaded.current, bow);
        loaded.switch_weapon(HuntWeapon::Crossbow);
        assert_eq!(loaded.current, crossbow);
        loaded.apply_crosshair(loaded.schemes[0].crosshair.clone());
        assert_eq!(loaded.current, bow);
    }
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
    fn roundtrip_and_corruption() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().to_owned()).unwrap();
        let c = Config::default();
        s.save(&c).unwrap();
        assert_eq!(s.load().unwrap(), c);
        fs::write(s.root.join("config.json"), b"broken").unwrap();
        assert!(s.load().is_err());
    }
    #[test]
    fn incomplete_configs_are_rejected_without_filling_defaults() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        let complete = serde_json::to_value(Config::default()).unwrap();
        for (pointer, field) in [
            ("", "version"),
            ("", "current"),
            ("", "mode_settings"),
            ("", "preset_settings"),
            ("", "hunt_settings"),
            ("/mode_settings", "hunt"),
            ("/current", "hunt_weapon"),
            ("/hunt_settings", "bow"),
            ("/hunt_settings/crossbow", "opacity"),
            ("/current", "center_dot"),
            ("/current", "t_shape"),
            ("/current", "outline_color"),
            ("/current", "outline_opacity"),
            ("/mode_settings/text", "outline_opacity"),
            ("/preset_settings/ring", "outline_opacity"),
        ] {
            let mut incomplete = complete.clone();
            incomplete
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            fs::write(
                s.root.join("config.json"),
                serde_json::to_vec(&incomplete).unwrap(),
            )
            .unwrap();
            assert!(s.load().is_err(), "{pointer}/{field}");
        }
        for field in ["mode_settings", "preset_settings"] {
            let mut incomplete = complete.clone();
            incomplete[field] = serde_json::Value::Null;
            fs::write(
                s.root.join("config.json"),
                serde_json::to_vec(&incomplete).unwrap(),
            )
            .unwrap();
            assert!(s.load().is_err(), "{field}");
        }
    }
    #[test]
    fn unsupported_versions_and_removed_shapes_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        let current = Config::default();
        s.save(&current).unwrap();
        for version in [1, 2, 4] {
            let mut unsupported = current.clone();
            unsupported.version = version;
            assert!(s.save(&unsupported).is_err());
            assert_eq!(s.load().unwrap(), current);
        }
        let complete = serde_json::to_value(&current).unwrap();
        for (pointer, value) in [
            ("/version", serde_json::json!(1)),
            ("/version", serde_json::json!(2)),
            ("/version", serde_json::json!(4)),
            ("/current/hunt_weapon", serde_json::json!("Sniper")),
            ("/current/shape", serde_json::json!("CrossDot")),
        ] {
            let mut unsupported = complete.clone();
            *unsupported.pointer_mut(pointer).unwrap() = value;
            fs::write(
                s.root.join("config.json"),
                serde_json::to_vec(&unsupported).unwrap(),
            )
            .unwrap();
            assert!(s.load().is_err(), "{pointer}");
        }
    }
    #[test]
    fn independent_outline_opacity_survives_restart_in_presets_and_schemes() {
        use crate::model::{Kind, Scheme, Shape};
        let d = tempfile::tempdir().unwrap();
        let s = Store::at(d.path().into()).unwrap();
        let mut c = Config::default();
        c.current.opacity = 70;
        c.current.outline_opacity = 180;
        c.schemes.push(Scheme {
            name: "独立描边".into(),
            crosshair: c.current.clone(),
        });
        c.switch_shape(Shape::Ring);
        c.current.opacity = 120;
        c.current.outline_opacity = 30;
        c.switch_kind(Kind::Text);
        s.save(&c).unwrap();
        let mut loaded = s.load().unwrap();
        assert_eq!(loaded, c);
        loaded.switch_kind(Kind::Preset);
        assert_eq!(loaded.current.outline_opacity, 30);
        loaded.switch_shape(Shape::Cross);
        assert_eq!(loaded.current.outline_opacity, 180);
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
}

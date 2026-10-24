//! Tiny, data-only appearance export. Imports are bounded and validated in the UI.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Skin {
    format: String,
    name: String,
    accent: String,
    background: String,
    surface: String,
    opacity: u8,
    panel_opacity: u8,
    blur: u8,
    radius: u8,
}
impl Skin {
    fn validate(&self) -> Result<(), String> {
        if self.format != "open-hub-skin/1"
            || self.name.trim().is_empty()
            || self.name.chars().count() > 40
            || self.name.chars().any(char::is_control)
        {
            return Err("无效的皮肤格式或名称".into());
        }
        for color in [&self.accent, &self.background, &self.surface] {
            if color.len() != 7
                || !color.starts_with('#')
                || !color[1..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("无效的皮肤颜色".into());
            }
        }
        if self.opacity > 100
            || !(20..=100).contains(&self.panel_opacity)
            || self.blur > 24
            || self.radius > 24
        {
            return Err("皮肤透明度、磨砂或圆角超出范围".into());
        }
        Ok(())
    }
}
#[tauri::command]
pub fn export_ui_skin(path: String, skin: Skin) -> Result<(), String> {
    skin.validate()?;
    let path = std::path::Path::new(&path);
    if !matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("json" | "openhubskin")
    ) {
        return Err("皮肤文件需要 .json 或 .openhubskin 扩展名".into());
    }
    let bytes = serde_json::to_vec_pretty(&skin).map_err(|e| e.to_string())?;
    if bytes.len() > 8192 {
        return Err("皮肤文件不能超过 8 KB".into());
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn skin() -> serde_json::Value {
        serde_json::json!({"format":"open-hub-skin/1","name":"薄荷","accent":"#70e6cf","background":"#0d1520","surface":"#172434","opacity":72,"panelOpacity":80,"blur":12,"radius":12})
    }
    #[test]
    fn appearance_skin_roundtrip_is_tiny_and_validated() {
        let s: Skin = serde_json::from_value(skin()).unwrap();
        s.validate().unwrap();
        let bytes = serde_json::to_vec(&s).unwrap();
        assert!(bytes.len() < 512);
        serde_json::from_slice::<Skin>(&bytes)
            .unwrap()
            .validate()
            .unwrap();
    }
    #[test]
    fn appearance_rejects_unknown_payloads_and_bad_colors() {
        let mut v = skin();
        v["css"] = serde_json::json!("url(https://example.com)");
        assert!(serde_json::from_value::<Skin>(v).is_err());
        let mut v = skin();
        v["accent"] = serde_json::json!("red; background:url(x)");
        assert!(serde_json::from_value::<Skin>(v)
            .unwrap()
            .validate()
            .is_err());
        let mut v = skin();
        v["opacity"] = serde_json::json!(101);
        assert!(serde_json::from_value::<Skin>(v)
            .unwrap()
            .validate()
            .is_err());
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;
    #[test]
    fn appearance_exports_only_valid_tiny_skin_file() {
        let root =
            std::env::temp_dir().join(format!("open-hub-skin-{}", crate::profiles::new_id()));
        std::fs::create_dir_all(&root).unwrap();
        let skin:Skin=serde_json::from_str(r##"{"format":"open-hub-skin/1","name":"测试","accent":"#70e6cf","background":"#0d1520","surface":"#172434","opacity":0,"panelOpacity":60,"blur":0,"radius":12}"##).unwrap();
        let path = root.join("theme.json");
        export_ui_skin(path.to_string_lossy().into_owned(), skin.clone()).unwrap();
        let data = std::fs::read(&path).unwrap();
        assert!(data.len() < 512);
        serde_json::from_slice::<Skin>(&data)
            .unwrap()
            .validate()
            .unwrap();
        assert!(export_ui_skin(root.join("bad.exe").to_string_lossy().into_owned(), skin).is_err());
        assert!(!root.join("bad.exe").exists());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

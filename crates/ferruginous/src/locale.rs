use std::collections::BTreeMap;

pub struct LocaleManager {
    translations: BTreeMap<String, BTreeMap<String, String>>,
}

impl LocaleManager {
    pub fn new() -> Self {
        let mut mgr = Self { translations: BTreeMap::new() };
        mgr.load_embedded();
        mgr.load_external();
        mgr
    }

    fn load_embedded(&mut self) {
        let en_raw = include_str!("../assets/locales/en.json");
        let ja_raw = include_str!("../assets/locales/ja.json");

        if let Ok(en_map) = serde_json::from_str::<BTreeMap<String, String>>(en_raw) {
            self.translations.insert("en".to_string(), en_map);
        }
        if let Ok(ja_map) = serde_json::from_str::<BTreeMap<String, String>>(ja_raw) {
            self.translations.insert("ja".to_string(), ja_map);
        }
    }

    fn load_external(&mut self) {
        if let Ok(exe_path) = std::env::current_exe()
            && let Some(exe_dir) = exe_path.parent()
        {
            let locales_dir = exe_dir.join("locales");
            if locales_dir.is_dir()
                && let Ok(entries) = std::fs::read_dir(locales_dir)
            {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && path.extension().and_then(|s| s.to_str()) == Some("json")
                        && let Some(lang_code) = path.file_stem().and_then(|s| s.to_str())
                        && let Ok(content) = std::fs::read_to_string(&path)
                        && let Ok(map) = serde_json::from_str::<BTreeMap<String, String>>(&content)
                    {
                        log::info!("Dynamically loaded external locale: {lang_code}");
                        self.translations.insert(lang_code.to_string(), map);
                    }
                }
            }
        }
    }

    pub fn tr(&self, lang: &str, key: &str) -> String {
        if let Some(lang_map) = self.translations.get(lang)
            && let Some(val) = lang_map.get(key)
        {
            return val.clone();
        }
        // Fallback to English
        if let Some(en_map) = self.translations.get("en")
            && let Some(val) = en_map.get(key)
        {
            return val.clone();
        }
        // If not found anywhere, return the key
        key.to_string()
    }

    pub fn available_languages(&self) -> Vec<String> {
        let mut langs: Vec<String> = self.translations.keys().cloned().collect();
        langs.sort();
        langs
    }
}

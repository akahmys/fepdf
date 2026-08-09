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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_locales_expose_the_same_key_set() {
        // A key present in one locale but not the other silently falls back to
        // English at runtime, which reads as a half-translated UI.
        let mgr = LocaleManager::new();
        let en = mgr.translations.get("en").expect("en locale must be embedded");
        let ja = mgr.translations.get("ja").expect("ja locale must be embedded");

        let missing_in_ja: Vec<&String> = en.keys().filter(|k| !ja.contains_key(*k)).collect();
        let missing_in_en: Vec<&String> = ja.keys().filter(|k| !en.contains_key(*k)).collect();

        assert!(missing_in_ja.is_empty(), "keys absent from ja.json: {missing_in_ja:?}");
        assert!(missing_in_en.is_empty(), "keys absent from en.json: {missing_in_en:?}");
    }

    #[test]
    fn tr_prefers_the_requested_language() {
        let mgr = LocaleManager::new();
        let en = mgr.tr("en", "cmd_redaction_studio");
        let ja = mgr.tr("ja", "cmd_redaction_studio");
        assert_eq!(en, "Redaction Studio");
        assert_ne!(en, ja, "ja should not merely echo the English string");
    }

    #[test]
    fn tr_falls_back_to_english_for_an_unknown_language() {
        let mgr = LocaleManager::new();
        assert_eq!(mgr.tr("de", "cmd_redaction_studio"), mgr.tr("en", "cmd_redaction_studio"));
    }

    #[test]
    fn tr_returns_the_key_when_nothing_matches() {
        let mgr = LocaleManager::new();
        assert_eq!(mgr.tr("en", "no_such_key_exists"), "no_such_key_exists");
    }
}

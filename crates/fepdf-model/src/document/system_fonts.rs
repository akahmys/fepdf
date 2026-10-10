//! The fonts this machine has, found where each platform keeps them, for a page whose
//! fonts are not embedded.

use super::{Document, fallback_fonts};
use crate::font::FallbackFontType;
use std::collections::BTreeMap;
use std::sync::Arc;

impl Document {
    #[cfg(target_os = "macos")]
    pub(super) fn load_mac_fallbacks(
        fonts: &mut BTreeMap<FallbackFontType, Arc<Vec<u8>>>,
        missing_types: &[FallbackFontType],
    ) {
        let mac_paths = [
            (
                crate::font::FallbackFontType::JapaneseSerif,
                "/System/Library/Fonts/ヒラギノ明朝 ProN.ttc",
            ),
            (
                crate::font::FallbackFontType::JapaneseSans,
                "/System/Library/Fonts/ヒラギノ角ゴ Interface.ttc",
            ),
            (crate::font::FallbackFontType::Serif, "/System/Library/Fonts/Times.ttc"),
            (crate::font::FallbackFontType::SansSerif, "/System/Library/Fonts/Helvetica.ttc"),
            (crate::font::FallbackFontType::Monospace, "/System/Library/Fonts/Courier.dfont"),
        ];
        for (ftype, path) in mac_paths {
            if missing_types.contains(&ftype)
                && let Ok(data) = std::fs::read(path)
            {
                fonts.insert(ftype, Arc::new(data));
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub(super) fn load_windows_fallbacks(
        fonts: &mut BTreeMap<FallbackFontType, Arc<Vec<u8>>>,
        missing_types: &[FallbackFontType],
    ) {
        let win_paths = [
            (crate::font::FallbackFontType::JapaneseSerif, "C:\\Windows\\Fonts\\msmincho.ttc"),
            (crate::font::FallbackFontType::JapaneseSans, "C:\\Windows\\Fonts\\msgothic.ttc"),
            (crate::font::FallbackFontType::Serif, "C:\\Windows\\Fonts\\times.ttf"),
            (crate::font::FallbackFontType::SansSerif, "C:\\Windows\\Fonts\\arial.ttf"),
            (crate::font::FallbackFontType::Monospace, "C:\\Windows\\Fonts\\cour.ttf"),
        ];
        for (ftype, path) in win_paths {
            if missing_types.contains(&ftype)
                && let Ok(data) = std::fs::read(path)
            {
                fonts.insert(ftype, Arc::new(data));
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(super) fn load_linux_fallbacks(
        fonts: &mut BTreeMap<FallbackFontType, Arc<Vec<u8>>>,
        missing_types: &[FallbackFontType],
    ) {
        // Each type takes the first of its paths that is there. `fonts-japanese-*` are the
        // links Debian's Japanese font packages make; Noto CJK is what an Ubuntu desktop
        // ships, and with only the links named, a Japanese page found no font at all.
        let linux_paths = [
            (
                crate::font::FallbackFontType::JapaneseSerif,
                "/usr/share/fonts/truetype/fonts-japanese-mincho.ttf",
            ),
            (
                crate::font::FallbackFontType::JapaneseSerif,
                "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
            ),
            (
                crate::font::FallbackFontType::JapaneseSans,
                "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
            ),
            (
                crate::font::FallbackFontType::JapaneseSans,
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            ),
            (
                crate::font::FallbackFontType::Serif,
                "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
            ),
            (
                crate::font::FallbackFontType::SansSerif,
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ),
            (
                crate::font::FallbackFontType::Monospace,
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            ),
        ];
        for (ftype, path) in linux_paths {
            if missing_types.contains(&ftype)
                && !fonts.contains_key(&ftype)
                && let Ok(data) = std::fs::read(path)
            {
                fonts.insert(ftype, Arc::new(data));
            }
        }
    }

    pub(super) fn load_platform_fallback_fonts(
        fonts: &mut BTreeMap<FallbackFontType, Arc<Vec<u8>>>,
        missing_types: &[FallbackFontType],
    ) {
        #[cfg(target_os = "macos")]
        Self::load_mac_fallbacks(fonts, missing_types);

        #[cfg(target_os = "windows")]
        Self::load_windows_fallbacks(fonts, missing_types);

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Self::load_linux_fallbacks(fonts, missing_types);
    }

    /// Loads the fallback faces into this document.
    ///
    /// The assembly itself is [`crate::document::fallback_fonts`], because three callers
    /// wanted it and each had written its own: this one, `VelloBackend::load_system_fonts`
    /// with no platform fallback under it, and `fepdf-cli`'s `host_cjk_fallbacks` with a
    /// hand-written path list that named macOS and Debian and no Windows at all.
    pub fn load_system_fonts(&mut self) {
        self.system_fonts = Arc::new(fallback_fonts());
    }
}

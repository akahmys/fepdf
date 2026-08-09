//! Adobe Glyph List (AGL) Mapping.
//!
//! (ISO 32000-2:2020 Clause 9.10.3)

/// Maps a glyph name to its corresponding Unicode string.
pub fn lookup(name: &str) -> Option<String> {
    if let Some(s) = lookup_pattern(name) {
        return Some(s);
    }
    lookup_agl(name)
}

fn lookup_pattern(name: &str) -> Option<String> {
    if name.starts_with("uni") && name.len() >= 7 {
        if let Ok(val) = u32::from_str_radix(&name[3..7], 16)
            && let Some(c) = std::char::from_u32(val)
        {
            return Some(c.to_string());
        }
    } else if name.starts_with('u')
        && name.len() >= 5
        && let Ok(val) = u32::from_str_radix(&name[1..], 16)
        && let Some(c) = std::char::from_u32(val)
    {
        return Some(c.to_string());
    }
    None
}

fn lookup_agl(name: &str) -> Option<String> {
    if let Some(s) = lookup_agl_standard(name) {
        return Some(s);
    }
    lookup_agl_extended(name)
}

fn lookup_agl_standard(name: &str) -> Option<String> {
    match name {
        "space" => Some("\u{0020}".to_string()),
        "exclam" => Some("\u{0021}".to_string()),
        "quotedbl" => Some("\u{0022}".to_string()),
        "numbersign" => Some("\u{0023}".to_string()),
        "dollar" => Some("\u{0024}".to_string()),
        "percent" => Some("\u{0025}".to_string()),
        "ampersand" => Some("\u{0026}".to_string()),
        "quoteright" => Some("\u{0027}".to_string()),
        "parenleft" => Some("\u{0028}".to_string()),
        "parenright" => Some("\u{0029}".to_string()),
        "asterisk" => Some("\u{002A}".to_string()),
        "plus" => Some("\u{002B}".to_string()),
        "comma" => Some("\u{002C}".to_string()),
        "hyphen" => Some("\u{002D}".to_string()),
        "period" => Some("\u{002E}".to_string()),
        "slash" => Some("\u{002F}".to_string()),
        "zero" => Some("\u{0030}".to_string()),
        "one" => Some("\u{0031}".to_string()),
        "two" => Some("\u{0032}".to_string()),
        "three" => Some("\u{0033}".to_string()),
        "four" => Some("\u{0034}".to_string()),
        "five" => Some("\u{0035}".to_string()),
        "six" => Some("\u{0036}".to_string()),
        "seven" => Some("\u{0037}".to_string()),
        "eight" => Some("\u{0038}".to_string()),
        "nine" => Some("\u{0039}".to_string()),
        "colon" => Some("\u{003A}".to_string()),
        "semicolon" => Some("\u{003B}".to_string()),
        "less" => Some("\u{003C}".to_string()),
        "equal" => Some("\u{003D}".to_string()),
        "greater" => Some("\u{003E}".to_string()),
        "question" => Some("\u{003F}".to_string()),
        "at" => Some("\u{0040}".to_string()),
        "bracketleft" => Some("\u{005B}".to_string()),
        "backslash" => Some("\u{005C}".to_string()),
        "bracketright" => Some("\u{005D}".to_string()),
        "asciicircum" => Some("\u{005E}".to_string()),
        "underscore" => Some("\u{005F}".to_string()),
        "quoteleft" => Some("\u{0060}".to_string()),
        "braceleft" => Some("\u{007B}".to_string()),
        "bar" => Some("\u{007C}".to_string()),
        "braceright" => Some("\u{007D}".to_string()),
        "asciitilde" => Some("\u{007E}".to_string()),
        _ => None,
    }
}

fn lookup_agl_extended(name: &str) -> Option<String> {
    match name {
        "bullet" => Some("\u{2022}".to_string()),
        "dagger" => Some("\u{2020}".to_string()),
        "daggerdbl" => Some("\u{2021}".to_string()),
        "ellipsis" => Some("\u{2026}".to_string()),
        "emdash" => Some("\u{2014}".to_string()),
        "endash" => Some("\u{2013}".to_string()),
        "florin" => Some("\u{0192}".to_string()),
        "fraction" => Some("\u{2044}".to_string()),
        "guilsinglleft" => Some("\u{2039}".to_string()),
        "guilsinglright" => Some("\u{203A}".to_string()),
        "minus" => Some("\u{2212}".to_string()),
        "quotesinglbase" => Some("\u{201A}".to_string()),
        "quotedblbase" => Some("\u{201E}".to_string()),
        "quotedblleft" => Some("\u{201C}".to_string()),
        "quotedblright" => Some("\u{201D}".to_string()),
        "quotehook" => Some("\u{02BB}".to_string()),
        "trademark" => Some("\u{2122}".to_string()),
        "euro" => Some("\u{20AC}".to_string()),
        _ => {
            if name.len() == 1 {
                Some(name.to_string())
            } else {
                None
            }
        }
    }
}

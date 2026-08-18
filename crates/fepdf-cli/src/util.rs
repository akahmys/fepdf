use anyhow::{Context, Result};

/// Parses a page range string (e.g. "1-5", "1,3-5", "all") into 0-indexed page indices.
pub fn parse_page_range(range_str: &str, max_pages: usize) -> Result<Vec<usize>> {
    let mut pages = Vec::new();
    for part in range_str.split(',') {
        if part.trim() == "all" {
            return Ok((0..max_pages).collect());
        }
        if part.contains('-') {
            let bounds: Vec<&str> = part.split('-').collect();
            if bounds.len() == 2 {
                let start: usize = bounds[0].trim().parse::<usize>()?.saturating_sub(1);
                let end: usize = bounds[1].trim().parse::<usize>()?;
                for i in start..end.min(max_pages) {
                    pages.push(i);
                }
            }
        } else {
            let p: usize = part.trim().parse::<usize>()?.saturating_sub(1);
            if p < max_pages {
                pages.push(p);
            }
        }
    }
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

/// Parses a unicode character string (e.g. "A" or "U+6C38").
pub fn parse_unicode(s: &str) -> Result<char> {
    if s.starts_with("U+") || s.starts_with("u+") {
        let hex = &s[2..];
        let val = u32::from_str_radix(hex, 16).with_context(|| "Invalid hex code")?;
        std::char::from_u32(val)
            .ok_or_else(|| anyhow::anyhow!("Invalid unicode scalar: U+{val:04X}"))
    } else if let Some(c) = s.chars().next() {
        Ok(c)
    } else {
        anyhow::bail!(
            "Invalid unicode input. Use single char or U+XXXX format (e.g. 'A' or 'U+6C38')"
        )
    }
}

/// Formats an `Option<T>` into a displayable string or an em dash if absent.
pub fn opt<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "—".to_string(), |v| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unicode_input() {
        assert_eq!(parse_unicode("A").unwrap(), 'A');
        assert_eq!(parse_unicode("U+6C38").unwrap(), '永');
    }

    #[test]
    fn test_parse_page_range() {
        assert_eq!(parse_page_range("all", 5).unwrap(), vec![0, 1, 2, 3, 4]);
        assert_eq!(parse_page_range("1,3,5", 5).unwrap(), vec![0, 2, 4]);
        assert_eq!(parse_page_range("1-3", 5).unwrap(), vec![0, 1, 2]);
    }
}

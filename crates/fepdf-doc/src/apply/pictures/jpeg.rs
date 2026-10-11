//! A JPEG, read for what its markers say and carried as it is (ITU T.81, Annex B).
//!
//! **Only the markers before the scan are read.** The frame header gives the size, the
//! depth and the number of components; JFIF's `APP0` and EXIF's `APP1` give the
//! resolution and the orientation; `APP2` carries an ICC profile in pieces; and Adobe's
//! `APP14` says a four-component image holds its samples inverted, as Adobe's CMYK
//! JPEGs do.

use super::{Picture, Samples, Space, Unreadable, per_inch};

/// What the markers said.
#[derive(Default)]
struct Markers {
    frame: Option<(u8, u8, u16, u16, u8)>,
    jfif_dpi: Option<(f64, f64)>,
    exif: Exif,
    icc: Vec<(u8, Vec<u8>)>,
    adobe: bool,
}

/// What EXIF's first directory said.
#[derive(Default)]
struct Exif {
    orientation: Option<u16>,
    resolution: Option<(f64, f64)>,
    unit: Option<u16>,
}

/// The picture a JPEG holds.
///
/// # Errors
/// When it has no frame, a coding `/DCTDecode` does not read (lossless, hierarchical or
/// arithmetic), a depth other than eight, or a number of components other than 1, 3 or 4.
pub(super) fn read(bytes: &[u8]) -> Result<Picture, Unreadable> {
    let markers = markers(bytes)?;
    let (kind, bits, height, width, components) = markers.frame.ok_or("it has no frame header")?;
    if !matches!(kind, 0xC0..=0xC2) {
        return Err(format!(
            "it is coded as SOF{}, which PDF's DCTDecode does not read",
            kind - 0xC0
        )
        .into());
    }
    if bits != 8 {
        return Err(format!("its samples are {bits} bits, and DCTDecode reads eight").into());
    }
    let space = match components {
        1 => Space::Gray,
        3 => Space::Rgb,
        4 => Space::Cmyk,
        n => return Err(format!("it has {n} components").into()),
    };
    if width == 0 || height == 0 {
        return Err("it has no size".into());
    }
    let exif_dpi = markers.exif.resolution.and_then(|(x, y)| {
        let unit = markers.exif.unit.unwrap_or(2);
        Some((per_inch(x, unit, (2, 3))?, per_inch(y, unit, (2, 3))?))
    });
    Ok(Picture {
        width: u32::from(width),
        height: u32::from(height),
        space,
        bits: 8,
        samples: Samples::Dct(bytes.to_vec()),
        inverted: markers.adobe && space == Space::Cmyk,
        alpha: None,
        icc: profile(markers.icc),
        dpi: markers.jfif_dpi.or(exif_dpi).unwrap_or((72.0, 72.0)),
        orientation: markers.exif.orientation.filter(|o| (1..=8).contains(o)).unwrap_or(1),
    })
}

/// The markers from the start of the file to the first scan.
fn markers(bytes: &[u8]) -> Result<Markers, Unreadable> {
    let mut found = Markers::default();
    let mut at = 2;
    loop {
        // Fill bytes before a marker are allowed (B.1.1.2).
        while bytes.get(at) == Some(&0xFF) && bytes.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        let (Some(&0xFF), Some(&marker)) = (bytes.get(at), bytes.get(at + 1)) else {
            return Err("its markers end before its picture begins".into());
        };
        // RSTn, SOI and TEM stand alone; SOS starts the picture, and the rest is data.
        if matches!(marker, 0xD0..=0xD8 | 0x01) {
            at += 2;
            continue;
        }
        if marker == 0xDA {
            return Ok(found);
        }
        let length = match bytes.get(at + 2..at + 4) {
            Some(&[high, low]) => usize::from(u16::from_be_bytes([high, low])),
            _ => 0,
        };
        if length < 2 {
            return Err("a marker's length is cut off".into());
        }
        let segment = bytes.get(at + 4..at + 2 + length).ok_or("a marker runs past the end")?;
        take(&mut found, marker, segment);
        at += 2 + length;
    }
}

/// What one marker segment adds.
fn take(found: &mut Markers, marker: u8, segment: &[u8]) {
    match marker {
        // SOFn, which are not DHT (C4), JPG (C8) or DAC (CC).
        0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
            if let [bits, h0, h1, w0, w1, components, ..] = *segment {
                let (h, w) = (u16::from_be_bytes([h0, h1]), u16::from_be_bytes([w0, w1]));
                found.frame.get_or_insert((marker, bits, h, w, components));
            }
        }
        0xE0 => found.jfif_dpi = found.jfif_dpi.or_else(|| jfif(segment)),
        0xE1 => {
            if let Some(tiff) = segment.strip_prefix(b"Exif\0\0") {
                found.exif = exif(tiff);
            }
        }
        0xE2 => {
            if let Some([sequence, _count, data @ ..]) = segment.strip_prefix(b"ICC_PROFILE\0") {
                found.icc.push((*sequence, data.to_vec()));
            }
        }
        0xEE => found.adobe |= segment.starts_with(b"Adobe"),
        _ => {}
    }
}

/// JFIF's density, where its unit is inches (1) or centimetres (2); 0 is an aspect
/// ratio, not a resolution.
fn jfif(segment: &[u8]) -> Option<(f64, f64)> {
    let [b'J', b'F', b'I', b'F', 0, _, _, unit, x0, x1, y0, y1, ..] = *segment else {
        return None;
    };
    let (x, y) = (f64::from(u16::from_be_bytes([x0, x1])), f64::from(u16::from_be_bytes([y0, y1])));
    let unit = u16::from(unit);
    Some((per_inch(x, unit, (1, 2))?, per_inch(y, unit, (1, 2))?))
}

/// An ICC profile from its `APP2` pieces, in the order their sequence numbers give
/// (ICC.1 Annex B.4). None where a piece is missing.
fn profile(mut pieces: Vec<(u8, Vec<u8>)>) -> Option<Vec<u8>> {
    pieces.sort_by_key(|(sequence, _)| *sequence);
    let whole =
        pieces.iter().enumerate().all(|(nth, (sequence, _))| usize::from(*sequence) == nth + 1);
    (whole && !pieces.is_empty()).then(|| pieces.into_iter().flat_map(|(_, data)| data).collect())
}

/// Orientation and resolution from EXIF's first image directory (EXIF 2.3, 4.6.2), whose
/// layout is TIFF's.
fn exif(tiff: &[u8]) -> Exif {
    let mut found = Exif::default();
    let big = match tiff.get(..2) {
        Some(b"MM") => true,
        Some(b"II") => false,
        _ => return found,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let &[a, b] = tiff.get(at..at + 2)? else { return None };
        Some(if big { u16::from_be_bytes([a, b]) } else { u16::from_le_bytes([a, b]) })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let &[a, b, c, d] = tiff.get(at..at + 4)? else { return None };
        Some(if big { u32::from_be_bytes([a, b, c, d]) } else { u32::from_le_bytes([a, b, c, d]) })
    };
    let rational = |at: usize| -> Option<f64> {
        let offset = usize::try_from(u32_at(at)?).ok()?;
        let (n, d) = (u32_at(offset)?, u32_at(offset + 4)?);
        (d != 0).then(|| f64::from(n) / f64::from(d))
    };
    let Some(ifd) = u32_at(4).and_then(|o| usize::try_from(o).ok()) else { return found };
    let count = u16_at(ifd).unwrap_or(0);
    let (mut x, mut y) = (None, None);
    for entry in 0..usize::from(count) {
        let at = ifd + 2 + entry * 12;
        let value = at + 8;
        match u16_at(at) {
            Some(0x0112) => found.orientation = u16_at(value),
            Some(0x011A) => x = rational(value),
            Some(0x011B) => y = rational(value),
            Some(0x0128) => found.unit = u16_at(value),
            _ => {}
        }
    }
    found.resolution = x.zip(y);
    found
}

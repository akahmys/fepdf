//! PDF Content Stream Serializer (Desublimation).
//!
//! This module converts high-level `Command` IR back into physical PDF operators.

use crate::object::sublimation::{Command, IrObject, TextArrayItem};
use kurbo::{Affine, Point};

/// Serializes a sequence of commands into a valid PDF content stream.
pub fn serialize_commands(cmds: &[Command]) -> Vec<u8> {
    let mut buffer = Vec::new();
    for cmd in cmds {
        serialize_command(cmd, &mut buffer);
    }
    buffer
}

fn serialize_command(cmd: &Command, buf: &mut Vec<u8>) {
    // RR-15 Limit: Dispatcher - Serializes high-level command IR via a single exhaustive flat match loop
    match cmd {
        Command::PushState => buf.extend_from_slice(b"q\n"),
        Command::PopState => buf.extend_from_slice(b"Q\n"),
        Command::Transform(affine) => {
            write_affine(affine, buf);
            buf.extend_from_slice(b" cm\n");
        }
        Command::MoveTo(p) => {
            write_point(p, buf);
            buf.extend_from_slice(b" m\n");
        }
        Command::LineTo(p) => {
            write_point(p, buf);
            buf.extend_from_slice(b" l\n");
        }
        Command::CurveTo(p1, p2, p3) => {
            write_point(p1, buf);
            buf.push(b' ');
            write_point(p2, buf);
            buf.push(b' ');
            write_point(p3, buf);
            buf.extend_from_slice(b" c\n");
        }
        Command::ClosePath => buf.extend_from_slice(b"h\n"),
        // Six places, as every other number here is written. `{}` was used instead,
        // which prints an `f64` in full: `re` operands came out as
        // `0.12000000000000455` and `12.600000000000001`.
        //
        // The digits are not noise from nowhere. `re` gives x, y, width and height;
        // the parser adds to get x1 and y1, and this subtracts to get them back, and
        // add-then-subtract on binary floating point does not return the input. Six
        // places is far finer than any rendering distinguishes, and it makes the round
        // trip a fixed point: the value written parses back to the same rectangle.
        Command::Rect(rect) => {
            buf.extend_from_slice(
                format!(
                    "{:.6} {:.6} {:.6} {:.6} re\n",
                    rect.x0,
                    rect.y0,
                    rect.width(),
                    rect.height()
                )
                .as_bytes(),
            );
        }
        Command::Fill(winding) => match winding {
            crate::graphics::WindingRule::NonZero => buf.extend_from_slice(b"f\n"),
            crate::graphics::WindingRule::EvenOdd => buf.extend_from_slice(b"f*\n"),
        },
        Command::Stroke(_) => buf.extend_from_slice(b"S\n"),
        Command::FillStroke(winding, _) => match winding {
            crate::graphics::WindingRule::NonZero => buf.extend_from_slice(b"B\n"),
            crate::graphics::WindingRule::EvenOdd => buf.extend_from_slice(b"B*\n"),
        },
        // `W` and `W*` alone. The painting operator that ends the path — `n`, `f`, `S`
        // — is a separate command and follows on its own.
        //
        // Writing `W n` here made the mapping asymmetric: the parser turns bare `W`
        // into `Clip` and leaves the following operator as its own command, so `W n`
        // came back as `W n n` and grew by one on every pass. Worse, `W f` came back as
        // `W n f`, where the `n` ends the path before the `f` can fill it — the fill
        // was silently lost, and `W* S` lost its stroke the same way.
        Command::Clip(winding) => match winding {
            crate::graphics::WindingRule::NonZero => buf.extend_from_slice(b"W\n"),
            crate::graphics::WindingRule::EvenOdd => buf.extend_from_slice(b"W*\n"),
        },
        Command::BeginText => buf.extend_from_slice(b"BT\n"),
        Command::EndText => buf.extend_from_slice(b"ET\n"),
        Command::SetFont { font, size } => {
            buf.extend_from_slice(format!("/{font} {size:.6} Tf\n").as_bytes());
        }
        Command::SetFillColor(color) => match color {
            crate::graphics::Color::Gray(g) => {
                buf.extend_from_slice(format!("{g:.6} g\n").as_bytes());
            }
            crate::graphics::Color::Rgb(r, g, b) => {
                buf.extend_from_slice(format!("{r:.6} {g:.6} {b:.6} rg\n").as_bytes());
            }
            crate::graphics::Color::Cmyk(c, m, y, k) => {
                buf.extend_from_slice(format!("{c:.6} {m:.6} {y:.6} {k:.6} k\n").as_bytes());
            }
            crate::graphics::Color::Lab(l, a, b) => {
                // Keep High-Fidelity color space (do not downgrade to RGB)
                buf.extend_from_slice(format!("{l:.6} {a:.6} {b:.6} scn\n").as_bytes());
            }
        },
        Command::SetStrokeColor(color) => match color {
            crate::graphics::Color::Gray(g) => {
                buf.extend_from_slice(format!("{g:.6} G\n").as_bytes());
            }
            crate::graphics::Color::Rgb(r, g, b) => {
                buf.extend_from_slice(format!("{r:.6} {g:.6} {b:.6} RG\n").as_bytes());
            }
            crate::graphics::Color::Cmyk(c, m, y, k) => {
                buf.extend_from_slice(format!("{c:.6} {m:.6} {y:.6} {k:.6} K\n").as_bytes());
            }
            crate::graphics::Color::Lab(l, a, b) => {
                // Keep High-Fidelity color space (do not downgrade to RGB)
                buf.extend_from_slice(format!("{l:.6} {a:.6} {b:.6} SCN\n").as_bytes());
            }
        },
        Command::ShowText(bytes) => {
            buf.push(b'<');
            for &b in bytes {
                buf.extend_from_slice(format!("{b:02x}").as_bytes());
            }
            buf.extend_from_slice(b"> Tj\n");
        }
        Command::ShowTextArray(items) => {
            buf.push(b'[');
            for item in items {
                match item {
                    TextArrayItem::Text(b) => {
                        buf.push(b'<');
                        for &byte in b {
                            buf.extend_from_slice(format!("{byte:02x}").as_bytes());
                        }
                        buf.push(b'>');
                    }
                    TextArrayItem::Offset(o) => {
                        buf.extend_from_slice(format!(" {o:.6}").as_bytes());
                    }
                }
            }
            buf.extend_from_slice(b"] TJ\n");
        }
        Command::MoveText(p) => {
            buf.extend_from_slice(format!("{:.6} {:.6} Td\n", p.x, p.y).as_bytes());
        }
        Command::SetTextMatrix(affine) => {
            write_affine(affine, buf);
            buf.extend_from_slice(b" Tm\n");
        }
        Command::SetCharSpacing(s) => buf.extend_from_slice(format!("{s:.6} Tc\n").as_bytes()),
        Command::SetWordSpacing(s) => buf.extend_from_slice(format!("{s:.6} Tw\n").as_bytes()),
        Command::SetHorizontalScaling(s) => {
            buf.extend_from_slice(format!("{s:.6} Tz\n").as_bytes());
        }
        Command::SetTextRenderMode(m) => {
            buf.extend_from_slice(format!("{} Tr\n", *m as i32).as_bytes());
        }
        Command::SetTextRise(s) => buf.extend_from_slice(format!("{s:.6} Ts\n").as_bytes()),
        Command::SetTextLeading(s) => buf.extend_from_slice(format!("{s:.6} TL\n").as_bytes()),
        Command::MoveToNextLine => buf.extend_from_slice(b"T*\n"),
        Command::DrawXObject(name) => buf.extend_from_slice(format!("/{name} Do\n").as_bytes()),
        Command::SetLineWidth(w) => buf.extend_from_slice(format!("{w:.6} w\n").as_bytes()),
        Command::SetLineCap(cap) => {
            buf.extend_from_slice(format!("{} J\n", *cap as i32).as_bytes());
        }
        Command::SetLineJoin(join) => {
            buf.extend_from_slice(format!("{} j\n", *join as i32).as_bytes());
        }
        Command::SetMiterLimit(m) => buf.extend_from_slice(format!("{m:.6} M\n").as_bytes()),
        Command::SetDashPattern(dash, phase) => {
            buf.push(b'[');
            for (i, d) in dash.iter().enumerate() {
                if i > 0 {
                    buf.push(b' ');
                }
                buf.extend_from_slice(format!("{d:.6}").as_bytes());
            }
            buf.extend_from_slice(format!("] {phase:.6} d\n").as_bytes());
        }
        Command::DrawInlineImage { width, height, format, data } => {
            write_inline_image(*width, *height, *format, data, buf);
        }
        Command::RawOperator { name, operands } => {
            for op in operands {
                write_ir_object(op, buf);
                buf.push(b' ');
            }
            buf.extend_from_slice(name.as_bytes());
            buf.push(b'\n');
        }
        Command::SetFillColorSpace(name) => {
            buf.extend_from_slice(format!("/{name} cs\n").as_bytes());
        }
        Command::SetStrokeColorSpace(name) => {
            buf.extend_from_slice(format!("/{name} CS\n").as_bytes());
        }
        Command::BeginMarkedContent { tag, properties } => {
            if let Some(props) = properties {
                buf.extend_from_slice(format!("/{} ", tag.0).as_bytes());
                write_ir_object(props, buf);
                buf.extend_from_slice(b" BDC\n");
            } else {
                buf.extend_from_slice(format!("/{} BMC\n", tag.0).as_bytes());
            }
        }
        Command::EndMarkedContent => {
            buf.extend_from_slice(b"EMC\n");
        }
        Command::Type3SetMetrics { wx, wy, bbox } => {
            if let Some(r) = bbox {
                buf.extend_from_slice(
                    format!(
                        "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} d1\n",
                        wx, wy, r.x0, r.y0, r.x1, r.y1
                    )
                    .as_bytes(),
                );
            } else {
                buf.extend_from_slice(format!("{wx:.6} {wy:.6} d0\n").as_bytes());
            }
        }
        _ => {} // Other commands like SetWritingMode are internal and don't map to PDF operators
    }
}

fn write_inline_image(
    width: u32,
    height: u32,
    format: crate::graphics::PixelFormat,
    data: &[u8],
    buf: &mut Vec<u8>,
) {
    buf.extend_from_slice(b"BI\n");
    buf.extend_from_slice(format!("  /W {width}\n").as_bytes());
    buf.extend_from_slice(format!("  /H {height}\n").as_bytes());
    let cs = match format {
        crate::graphics::PixelFormat::Gray8 => "/G",
        crate::graphics::PixelFormat::Rgb8 => "/RGB",
        crate::graphics::PixelFormat::Rgba8 => "/RGB",
        crate::graphics::PixelFormat::Cmyk8 => "/CMYK",
        crate::graphics::PixelFormat::MonoMask | crate::graphics::PixelFormat::MonoMaskInverted => {
            "/G"
        }
    };
    buf.extend_from_slice(format!("  /CS {cs}\n").as_bytes());
    buf.extend_from_slice(b"  /BPC 8\n");
    buf.extend_from_slice(b"ID\n");
    buf.extend_from_slice(data);
    buf.extend_from_slice(b"\nEI\n");
}

fn write_point(p: &Point, buf: &mut Vec<u8>) {
    buf.extend_from_slice(format!("{:.6} {:.6}", p.x, p.y).as_bytes());
}

fn write_affine(a: &Affine, buf: &mut Vec<u8>) {
    let c = a.as_coeffs();
    buf.extend_from_slice(
        format!("{:.6} {:.6} {:.6} {:.6} {:.6} {:.6}", c[0], c[1], c[2], c[3], c[4], c[5])
            .as_bytes(),
    );
}

fn write_ir_object(obj: &IrObject, buf: &mut Vec<u8>) {
    match obj {
        IrObject::Boolean(b) => buf.extend_from_slice(if *b { b"true" } else { b"false" }),
        IrObject::Integer(i) => buf.extend_from_slice(i.to_string().as_bytes()),
        IrObject::Real(f) => buf.extend_from_slice(format!("{f:.6}").as_bytes()),
        IrObject::String(b) => {
            buf.push(b'(');
            buf.extend_from_slice(&escape_pdf_string(b));
            buf.push(b')');
        }
        IrObject::Hex(b) => {
            buf.push(b'<');
            for &byte in b {
                buf.extend_from_slice(format!("{byte:02x}").as_bytes());
            }
            buf.push(b'>');
        }
        IrObject::Name(n) => buf.extend_from_slice(format!("/{n}").as_bytes()),
        IrObject::Null => buf.extend_from_slice(b"null"),
        IrObject::Array(arr) => {
            buf.push(b'[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    buf.push(b' ');
                }
                write_ir_object(item, buf);
            }
            buf.push(b']');
        }
        IrObject::Dictionary(dict) => {
            buf.extend_from_slice(b"<< ");
            for (key, val) in dict {
                buf.extend_from_slice(format!("/{key} ").as_bytes());
                write_ir_object(val, buf);
                buf.push(b' ');
            }
            buf.extend_from_slice(b">>");
        }
    }
}

fn escape_pdf_string(data: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(data.len());
    for &b in data {
        match b {
            b'(' => escaped.extend_from_slice(b"\\("),
            b')' => escaped.extend_from_slice(b"\\)"),
            b'\\' => escaped.extend_from_slice(b"\\\\"),
            _ => escaped.push(b),
        }
    }
    escaped
}

/// Serializes an image back into a compressed PDF stream.
pub fn serialize_image(
    _width: u32,
    _height: u32,
    _format: crate::graphics::PixelFormat,
    data: &[u8],
) -> crate::error::PdfResult<(Vec<u8>, Vec<String>)> {
    // For now, use FlateDecode (lossless) as the default.
    // In a full implementation, we would check the format and potentially use DCTDecode for JPEG.
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    use std::io::Write;
    encoder.write_all(data)?;
    let compressed = encoder.finish()?;

    Ok((compressed, vec!["FlateDecode".to_string()]))
}

#[cfg(test)]
mod clipping {
    //! `W` and `W*` set a clip; they do not end the path. Emitting the `n` for them
    //! made the round trip both lossy and non-idempotent.

    use super::*;
    use crate::object::sublimation::parser::Sublimator;
    use std::collections::BTreeMap;

    fn round_trip(source: &str) -> String {
        let fonts = BTreeMap::new();
        let mut sublimator = Sublimator::new(&fonts);
        let out = serialize_commands(&sublimator.sublimate(source.as_bytes()));
        String::from_utf8_lossy(&out).replace('\n', " ").trim().to_string()
    }

    /// The operators, in order, with the operands dropped.
    ///
    /// These tests are about which operators survive and in what order, not about how
    /// numbers are spelled. Asserting on the whole string made them fail when `re`
    /// operands moved from `{}` to `{:.6}` — a change that had nothing to do with what
    /// they were checking.
    fn operators(source: &str) -> Vec<String> {
        round_trip(source)
            .split_whitespace()
            .filter(|t| t.parse::<f64>().is_err())
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn clipping_then_painting_keeps_the_paint() {
        // The severe case. `W f` used to come back as `W n f`, where the `n` ends the
        // path before the `f` can fill it — the fill was gone, silently.
        assert_eq!(operators("q 10 10 100 100 re W f Q"), ["q", "re", "W", "f", "Q"]);
        assert_eq!(operators("q 10 10 100 100 re W* S Q"), ["q", "re", "W*", "S", "Q"]);
    }

    #[test]
    fn clipping_without_painting_does_not_grow() {
        // `W n` came back as `W n n`, and gained one more `n` on every pass:
        // samples/sample.pdf grew by exactly 52 bytes each time, 26 clips at two bytes.
        assert_eq!(operators("q 10 10 100 100 re W n Q"), ["q", "re", "W", "n", "Q"]);
    }

    #[test]
    fn the_round_trip_is_a_fixed_point() {
        // Idempotence is the property that was lost, and the one worth asserting:
        // whatever the first pass produces, the second must reproduce exactly.
        for source in [
            "q 10 10 100 100 re W n Q",
            "q 10 10 100 100 re W f Q",
            "q 10 10 100 100 re W* S Q",
            "q 10 10 100 100 re f Q",
            "q 1 2 3 4 re W n BT /F1 12 Tf ET Q",
        ] {
            let once = round_trip(source);
            let twice = round_trip(&once);
            assert_eq!(once, twice, "not a fixed point: {source}");
        }
    }

    #[test]
    fn painting_without_clipping_is_untouched() {
        assert_eq!(operators("q 10 10 100 100 re f Q"), ["q", "re", "f", "Q"]);
        assert_eq!(operators("q 10 10 100 100 re S Q"), ["q", "re", "S", "Q"]);
    }
}

use anyhow::Result;

pub fn handle_credits() -> Result<()> {
    println!("\n--- [ OPEN SOURCE CREDITS ] ---");
    println!("fepdf is powered by the following excellent libraries:\n");

    let credits = [
        ("pdf-writer", "Apache-2.0", "Efficient PDF object serialization"),
        ("flate2", "MIT / Apache-2.0", "Deflate/Zlib compression"),
        ("vello", "Apache-2.0 / MIT", "High-performance vector graphics"),
        ("kurbo", "Apache-2.0 / MIT", "Vector geometry primitives"),
        ("skrifa / read-fonts", "Apache-2.0 / MIT", "Modern font parsing and glyph scaling"),
        ("image", "MIT / Apache-2.0", "Raster image processing"),
        ("anyhow / thiserror", "MIT / Apache-2.0", "Structured error handling"),
        ("serde", "MIT / Apache-2.0", "Universal serialization framework"),
        ("tokio", "MIT", "Asynchronous runtime"),
    ];

    println!("{:<25} | {:<20} | {:<30}", "Crate", "License", "Purpose");
    println!("{:-<25}-+-{:-<20}-+-{:-<30}", "", "", "");
    for (name, license, purpose) in credits {
        println!("{name:<25} | {license:<20} | {purpose:<30}");
    }

    println!("\nFull license texts are available in the repository's NOTICE file.");
    println!("Thank you to the Rust community for these amazing tools.");
    Ok(())
}

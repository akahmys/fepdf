//! Basic PDF Info Example using Ferruginous SDK
//! Demonstrates initializing the SDK and parsing PDF document metadata.

use anyhow::Result;
use ferruginous_sdk::Document;

fn main() -> Result<()> {
    println!("--- Ferruginous SDK Example: PDF Document Inspection ---");

    // Minimal valid PDF 2.0 header and trailer buffer for demonstration
    let demo_pdf_bytes = b"%PDF-2.0\n%\xE2\xE3\xCF\xD3\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\nxref\n0 4\n0000000000 65535 f \n0000000015 00000 n \n0000000068 00000 n \n0000000125 00000 n \ntrailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n193\n%%EOF\n";

    let doc = Document::from_bytes(demo_pdf_bytes)?;

    println!("✅ Successfully ingested PDF document!");
    println!("  - Page Count: {}", doc.page_count());
    println!("  - PDF Version: {}", doc.version_string());

    Ok(())
}

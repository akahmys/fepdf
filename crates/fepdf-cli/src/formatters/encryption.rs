use super::{render_decisions_markdown, render_decisions_text};
use crate::util::opt;

pub fn conformance_label(c: fepdf::Conformance) -> &'static str {
    match c {
        fepdf::Conformance::Implemented => "implemented",
        fepdf::Conformance::NonConformant => "NON-CONFORMANT",
        fepdf::Conformance::Unsupported => "UNSUPPORTED",
    }
}

pub fn render_encryption_text(r: &fepdf::EncryptionReport, input: &std::path::Path) {
    println!("fepdf encryption: {}", input.display());
    render_payload_text(r);
    if !r.encrypted {
        // A wrapper carries no `/Encrypt` and is emphatically protected, so the
        // absence of one does not settle the question on its own.
        if r.payload.is_some() {
            println!("\n  the wrapper itself carries no /Encrypt, as 7.6.7 intends");
        } else {
            println!("\n  no /Encrypt — the document is not protected");
        }
        render_decisions_text(&r.decisions);
        return;
    }

    println!("\n--- [ HANDLER (7.6.4) ] ---");
    println!("  /Filter            {}", r.handler.as_deref().unwrap_or("(absent)"));
    println!("  /V                 {}", opt(r.version));
    println!("  /R                 {}", opt(r.revision));
    println!("  key length         {}", r.key_bits.map_or("—".into(), |b| format!("{b} bits")));
    println!("  stream cipher      {}", r.cipher.as_deref().unwrap_or("—"));
    println!("  /EncryptMetadata   {}", r.encrypt_metadata);
    println!("  unlocked           {}", if r.unlocked { "yes" } else { "NO" });
    if let Some(access) = &r.access {
        // /P restricts user access only (7.6.4.1), so which one opened it is the
        // difference between the permissions applying and not applying at all.
        println!("  access             {access}");
    }

    if !r.crypt_filters.is_empty() {
        println!("\n--- [ CRYPT FILTERS (7.6.5) ] ---");
        for f in &r.crypt_filters {
            let mut used = Vec::new();
            if f.for_streams {
                used.push("streams");
            }
            if f.for_strings {
                used.push("strings");
            }
            let used = if used.is_empty() { "unused".to_string() } else { used.join(", ") };
            println!("  /{:<12} /CFM {:<8} {}", f.name, f.method, used);
        }
    }

    render_encryption_permissions(r);

    println!("\n--- [ WHAT THIS ENGINE DOES WITH IT ] ---");
    println!("  {}", conformance_label(r.conformance));
    println!("  {}", r.conformance_note);

    render_decisions_text(&r.decisions);
}

pub fn render_encryption_permissions(r: &fepdf::EncryptionReport) {
    println!("\n--- [ PERMISSIONS (7.6.4.2, Table 22) ] ---");
    let Some(bits) = r.permission_bits else {
        println!("  /P is absent or unreadable");
        return;
    };
    // The sign is the point: these are 32 bits, and the hex is how the file writes
    // them. Reading them back as a positive integer is what ADR-0009 is about.
    println!("  /P {bits} (0x{:08X})", bits.cast_unsigned());
    for p in &r.permissions {
        println!("    bit {:>2}  {}  {}", p.bit, if p.granted { "yes" } else { "no " }, p.meaning);
    }
}

pub fn render_encryption_markdown(r: &fepdf::EncryptionReport, input: &std::path::Path) {
    println!("# Encryption: {}", input.display());
    if !r.encrypted {
        println!("\nNo `/Encrypt` — the document is not protected.");
        render_decisions_markdown(&r.decisions);
        return;
    }
    println!("\n| Property | Value |");
    println!("| :--- | :--- |");
    println!("| `/Filter` | {} |", r.handler.as_deref().unwrap_or("—"));
    println!("| `/V` / `/R` | {} / {} |", opt(r.version), opt(r.revision));
    println!("| Key length | {} |", r.key_bits.map_or("—".into(), |b| format!("{b} bits")));
    println!("| Stream cipher | {} |", r.cipher.as_deref().unwrap_or("—"));
    println!("| `/EncryptMetadata` | {} |", r.encrypt_metadata);
    println!("| Unlocked | {} |", if r.unlocked { "yes" } else { "no" });
    println!("| Conformance | **{}** |", conformance_label(r.conformance));
    println!("\n{}\n", r.conformance_note);

    if !r.permissions.is_empty() {
        println!("| Bit | Granted | Permits |");
        println!("| ---: | :---: | :--- |");
        for p in &r.permissions {
            println!("| {} | {} | {} |", p.bit, if p.granted { "yes" } else { "no" }, p.meaning);
        }
    }
    render_decisions_markdown(&r.decisions);
}

/// Reports an encrypted payload, when the file is an unencrypted wrapper (7.6.7).
///
/// Nothing here decrypts anything, and cannot: the payload is protected by a handler
/// this standard does not define. Naming the filter is the service the clause exists to
/// provide — a reader without it can still tell the user what they need.
pub fn render_payload_text(r: &fepdf::EncryptionReport) {
    let Some(p) = &r.payload else { return };
    println!("\n--- [ ENCRYPTED PAYLOAD (7.6.7) ] ---");
    println!("  this file is an unencrypted wrapper; its content is embedded and encrypted");
    println!(
        "  required filter   /{}{}",
        p.filter,
        match &p.filter_version {
            Some(v) => format!(" version {v}"),
            None => String::new(),
        }
    );
    if let Some(name) = &p.file_name {
        println!("  payload file      {name}");
    }
    if let Some(desc) = &p.description {
        println!("  producer says     {desc}");
    }
    println!("  this engine does not implement that filter, so the payload stays sealed");
    for condition in &p.conditions_met {
        println!("    met      {condition}");
    }
    for condition in &p.conditions_unmet {
        println!("    NOT MET  {condition}");
    }
}

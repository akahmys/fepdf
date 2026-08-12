//! Example demonstrating how to inspect the CIDToGIDMap of a PDF font.

use fepdf_model::font::FontResource;
use fepdf_sdk::PdfDocument;

/// Main function for running the CID map inspection example.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path_str = args.get(1).map_or("samples/bokutokitan.pdf", |s| s.as_str());

    let data = std::fs::read(path_str)?;
    let doc = PdfDocument::open(data.into())?;

    let page = doc.get_page(0)?;
    let resources_h = page.resources_handle();
    let resources = doc.inner().arena().get_dict(resources_h).unwrap();
    let fonts_obj =
        resources.get(&doc.inner().arena().name("Font")).unwrap().resolve(doc.inner().arena());
    let fonts_h = fonts_obj.as_dict_handle().unwrap();
    let font_dict = doc.inner().arena().get_dict(fonts_h).unwrap();

    for (name, obj) in font_dict {
        let name_str = doc.inner().arena().get_name(name).unwrap().as_str().to_string();

        let obj_resolved = obj.resolve(doc.inner().arena());
        let Some(obj_h) = obj_resolved.as_dict_handle() else {
            continue;
        };
        let dict = doc.inner().arena().get_dict(obj_h).unwrap();
        let font_res = FontResource::load(&dict, doc.inner()).unwrap();

        println!("Font: {} ({})", name_str, font_res.base_font.as_str());
        if let Some(ref map) = font_res.cid_to_gid_map {
            println!("  CIDToGIDMap length: {}", map.len());
            let mut count = 0;
            for (&cid, &gid) in map {
                if gid != 0 {
                    if count < 10 {
                        println!("    CID {cid} -> GID {gid}");
                    }
                    count += 1;
                }
            }
            if count > 10 {
                println!("    ... and {} more mappings", count - 10);
            }
        } else {
            println!("  No CIDToGIDMap (Identity or missing)");
        }
    }

    Ok(())
}

//! Example for inspecting decoded character sequences on PDF page 2.

use fepdf_model::font::FontResource;
use fepdf_sdk::PdfDocument;

/// Main function for running the page 2 decoding inspection example.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger to capture resolve_gid candidate scores
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let path_str = "samples/unicode_16.pdf";
    let data = std::fs::read(path_str)?;
    let doc = PdfDocument::open(data.into())?;

    let page = doc.get_page(1)?; // Index 1 is page 2
    let arena = doc.inner().arena();

    // Find the fonts defined in the resources
    let res_h = page.resources_handle();
    let res_dict = arena.get_dict(res_h).unwrap();
    let font_dict_obj = res_dict.get(&arena.name("Font")).unwrap().resolve(arena);
    let font_dict_h = font_dict_obj.as_dict_handle().unwrap();
    let font_dict = arena.get_dict(font_dict_h).unwrap();

    // Map of font name -> FontResource
    let mut fonts = std::collections::BTreeMap::new();
    for (k, v) in font_dict {
        let name = arena.get_name(k).unwrap().as_str().to_string();
        let font_obj = v.resolve(arena);
        if let Some(font_obj_h) = font_obj.as_dict_handle() {
            let dict = arena.get_dict(font_obj_h).unwrap();
            let mut font_res = FontResource::load(&dict, doc.inner()).unwrap();

            if font_res.subtype.as_str() == "Type0"
                && let Some(desc_fonts_obj) = dict.get(&arena.name("DescendantFonts"))
                && let fepdf_model::Object::Array(ah) = desc_fonts_obj.resolve(arena)
                && let Some(arr) = arena.get_array(ah)
                && let Some(desc_font) = arr.first()
                && let fepdf_model::Object::Dictionary(dfh) = desc_font.resolve(arena)
                && let Some(df_dict) = arena.get_dict(dfh)
            {
                let mut desc_res = FontResource::load(&df_dict, doc.inner()).unwrap();
                desc_res.encoding.clone_from(&font_res.encoding);
                desc_res.wmode = font_res.wmode;
                if desc_res.to_unicode.is_none() {
                    desc_res.to_unicode.clone_from(&font_res.to_unicode);
                }
                desc_res.build_unified_map();
                font_res = desc_res;
            }
            fonts.insert(name, font_res);
        }
    }

    let handles = page.contents_handles();
    for &stream_h in &handles {
        let sublimated = arena.get_sublimated_data(stream_h).unwrap();
        if let fepdf_model::object::SublimatedData::Commands { items: ref cmds, .. } = *sublimated {
            let mut current_font = None;
            for cmd in cmds {
                match cmd {
                    fepdf_model::object::sublimation::Command::SetFont { font, .. } => {
                        current_font = fonts.get(font);
                    }
                    fepdf_model::object::sublimation::Command::ShowTextArray(items) => {
                        if let Some(font) = current_font {
                            for item in items {
                                if let fepdf_model::object::sublimation::TextArrayItem::Text(
                                    bytes,
                                ) = item
                                {
                                    let mut i = 0;
                                    while i < bytes.len() {
                                        let (consumed, u_opt) = font.decode_next(&bytes[i..]);
                                        if consumed == 0 {
                                            break;
                                        }
                                        let code = &bytes[i..i + consumed];
                                        let cid = font.to_cid(code);
                                        let u_char = u_opt.as_ref().and_then(|s| s.chars().next());

                                        // Let's print out the decoding step and character
                                        println!(
                                            "[DECODE] CID {cid} -> Unicode {u_opt:?} (char_code={code:?})"
                                        );

                                        // Call resolve_gid, which will emit log messages
                                        let _resolved = font.resolve_gid(cid, u_char, None);

                                        i += consumed;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

#![allow(missing_docs)]
use fepdf_model::Object;
use fepdf_sdk::PdfDocument;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path_str = args.get(1).map_or("samples/bokutokitan.pdf", |s| s.as_str());

    let data = std::fs::read(path_str)?;
    let doc = PdfDocument::open(data.into())?;

    let page = doc.get_page(0)?;
    println!("Page 0 media box: {:?}", page.media_box());
    let rh = page.resources_handle();
    println!("Page 0 resources: {rh:?}");

    let arena = doc.inner().arena();
    if let Some(res_dict) = arena.get_dict(rh) {
        for (k, v) in res_dict {
            if let Some(name) = arena.get_name(k) {
                println!("Resource {}: {:?}", name.as_str(), v);
                if name.as_str() == "XObject"
                    && let Some(xh) = v.resolve(arena).as_dict_handle()
                    && let Some(x_dict) = arena.get_dict(xh)
                {
                    for (xk, xv) in x_dict {
                        if let Some(xname) = arena.get_name(xk) {
                            println!("  XObject {}: {:?}", xname.as_str(), xv);
                            if xname.as_str() == "Im1" {
                                println!("    Im1 Entry in Resources: {xv:?}");
                                let resolved = xv.resolve(arena);
                                if let Object::Stream(sh, data) = resolved {
                                    if let Some(s_dict) = arena.get_dict(sh) {
                                        println!("    Im1 Stream Dictionary:");
                                        for (sk, sv) in s_dict {
                                            if let Some(sk_str) = arena.get_name_str(sk) {
                                                println!(
                                                    "      {}: {:?}",
                                                    sk_str,
                                                    sv.resolve(arena)
                                                );
                                            }
                                        }
                                    }
                                    if let Ok(bytes) = arena.get_stream_bytes(&data) {
                                        println!(
                                            "    Im1 Header: {:02x?}",
                                            &bytes[..16.min(bytes.len())]
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for contents_h in page.contents_handles() {
        if let Some(obj) = arena.get_object(contents_h) {
            let decoded = doc.inner().decode_stream(&obj).unwrap();
            println!("Stream length: {}", decoded.len());
            println!("{}", String::from_utf8_lossy(&decoded));
        }
    }

    Ok(())
}

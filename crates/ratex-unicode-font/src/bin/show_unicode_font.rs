use std::sync::Arc;

use ratex_unicode_font::{load_unicode_font_arc, set_unicode_font, unicode_font_face_index};
use ttf_parser::{name_id, Face, PlatformId};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path_arg = args
        .iter()
        .position(|arg| arg == "--path")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let (bytes, face_index, source) = load_input_font(path_arg);
    println!("source: {}", source);
    print_face_summary(&bytes, face_index);
}

fn load_input_font(path_arg: Option<String>) -> (Arc<Vec<u8>>, u32, String) {
    if let Some(spec) = path_arg {
        if !set_unicode_font(&spec) {
            eprintln!("Failed to load font from spec: {}", spec);
            std::process::exit(2);
        }
        let Some(bytes) = load_unicode_font_arc() else {
            eprintln!("Failed to load font bytes");
            std::process::exit(2);
        };
        let face_index = unicode_font_face_index().unwrap_or(0);
        let source = format!("path:{}", spec);
        (bytes, face_index, source)
    } else {
        let Some(bytes) = load_unicode_font_arc() else {
            eprintln!("No Unicode fallback font found.");
            std::process::exit(1);
        };
        let face_index = unicode_font_face_index().unwrap_or(0);
        let source = format!("discovered#{}", face_index);
        (bytes, face_index, source)
    }
}

fn print_face_summary(bytes: &[u8], face_index: u32) {
    let faces_in_file = ttf_parser::fonts_in_collection(bytes).unwrap_or(1);
    println!("face: {}/{}", face_index, faces_in_file);

    match Face::parse(bytes, face_index) {
        Ok(face) => {
            println!("bytes: {} glyphs: {}", bytes.len(), face.number_of_glyphs());
            print_family_line(&face);
            print_name_line(&face, name_id::POST_SCRIPT_NAME, "post_script_name");
            print_name_line(&face, name_id::FULL_NAME, "full_name");

            let localized_family_names = localized_names(&face, name_id::FAMILY);
            if !localized_family_names.is_empty() {
                println!("family_names: {}", localized_family_names.join(", "));
            }
            if faces_in_file > 1 {
                print_all_family_names(bytes, faces_in_file);
            }
        }
        Err(err) => {
            println!("parse_error: {}", err);
        }
    }
}

fn print_family_line(face: &Face<'_>) {
    let family = english_name(face, name_id::FAMILY);
    let subfamily = english_name(face, name_id::SUBFAMILY);
    let weight = face.weight().to_number();

    match (family, subfamily) {
        (Some(family), Some(subfamily)) => {
            println!(
                "family: {} subfamily: {} weight: {}",
                family, subfamily, weight
            );
        }
        (Some(family), None) => {
            println!("family: {} weight: {}", family, weight);
        }
        (None, Some(subfamily)) => {
            println!("subfamily: {} weight: {}", subfamily, weight);
        }
        (None, None) => {
            println!("weight: {}", weight);
        }
    }
}

fn print_name_line(face: &Face<'_>, name_id: u16, label: &str) {
    if let Some(value) = english_name(face, name_id) {
        println!("{}: {}", label, value);
    }
}

fn english_name(face: &Face<'_>, name_id: u16) -> Option<String> {
    face.names()
        .into_iter()
        .find(|name| {
            name.name_id == name_id
                && name.is_unicode()
                && matches!(
                    name.platform_id,
                    PlatformId::Unicode | PlatformId::Windows
                )
        })
        .and_then(|name| name.to_string())
}

fn localized_names(face: &Face<'_>, name_id: u16) -> Vec<String> {
    let mut names = Vec::new();
    for name in face.names() {
        if name.name_id != name_id {
            continue;
        }
        let Some(value) = name.to_string() else {
            continue;
        };
        if !names.contains(&value) {
            names.push(value);
        }
    }
    names
}

fn print_all_family_names(bytes: &[u8], faces_in_file: u32) {
    println!("all_family_names:");
    for index in 0..faces_in_file {
        let names = Face::parse(bytes, index)
            .ok()
            .map(|face| localized_names(&face, name_id::FAMILY))
            .unwrap_or_default();

        if names.is_empty() {
            println!("  #{}: <none>", index);
        } else {
            println!("  #{}: {}", index, names.join(", "));
        }
    }
}

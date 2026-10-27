use std::time::Instant;

fn main() {
    let dir = std::path::Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let dict_path = dir.join("german.dict");

    let t = Instant::now();
    let data = std::fs::read(&dict_path).expect("read");
    let read_ms = t.elapsed().as_secs_f64() * 1e3;
    drop(data);

    let t = Instant::now();
    let words = og_tagger::morfologik::load_fsa_word_list(&dict_path, &dir.join("german.info"))
        .expect("word list");
    let walk_ms = t.elapsed().as_secs_f64() * 1e3;

    let t = Instant::now();
    let dict =
        og_tagger::morfologik::MorfologikDictionary::load(&dict_path, &dir.join("german.info"))
            .expect("load");
    let load_ms = t.elapsed().as_secs_f64() * 1e3;

    println!(
        "read_ms={read_ms:.0} walk_first_field_ms={walk_ms:.0} unique_first_fields={} full_load_ms={load_ms:.0} entries={}",
        words.len(),
        dict.len()
    );
}

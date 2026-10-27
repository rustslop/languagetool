use og_tagger::morfologik::MorfologikDictionary;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: probe_dict_lookup <lang> <dict-stem> [words...]");
        return;
    }
    let lang = &args[0];
    let stem = &args[1];
    let dir = std::path::PathBuf::from(format!(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{lang}/src/main/resources/org/languagetool/resource/{lang}"
    ));
    let Some(dict) = MorfologikDictionary::load(
        &dir.join(format!("{stem}.dict")),
        &dir.join(format!("{stem}.info")),
    ) else {
        println!("LOAD FAILED {}", dir.join(format!("{stem}.dict")).display());
        return;
    };
    let probe = args[2..].iter().map(String::as_str).collect::<Vec<_>>();
    let words: Vec<&str> = if probe.is_empty() {
        vec!["the", "and", "jeg", "er"]
    } else {
        probe
    };
    println!(
        "{lang}/{stem}: non_empty={} len={}",
        !dict.is_empty(),
        dict.len()
    );
    for w in words {
        let readings: Vec<String> = dict
            .lookup(w)
            .iter()
            .map(|(l, t)| format!("{l}/{t}"))
            .collect();
        println!("  {w} => [{}]", readings.join(", "));
    }
}

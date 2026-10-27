use og_xml::compiler::XmlCompiler;

fn main() {
    for code in [
        "en", "de", "fr", "es", "pt", "nl", "pl", "ru", "ca", "br", "da", "el", "gl", "ro", "it",
        "ast", "ar", "be", "sv",
    ] {
        let path = format!(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{code}/src/main/resources/org/languagetool/rules/{code}/grammar.xml"
        );
        let Ok(xml) = std::fs::read_to_string(&path) else {
            continue;
        };
        let rule_set = match XmlCompiler::new().compile_file(&xml) {
            Ok(rs) => rs,
            Err(_) => continue,
        };
        let mut dead = 0usize;
        let mut dead_ids: Vec<String> = Vec::new();
        let mut live = 0usize;
        for r in &rule_set.rules {
            if let Some(re) = &r.regexp {
                if re.regex.is_none() && re.fancy_regex.is_none() {
                    dead += 1;
                    dead_ids.push(format!("{}{}", r.id, r.sub_id.as_deref().unwrap_or("")));
                } else {
                    live += 1;
                }
            }
        }
        if dead > 0 {
            println!("{code}: live={live} dead={dead}: {}", dead_ids.join(","));
        }
    }
}

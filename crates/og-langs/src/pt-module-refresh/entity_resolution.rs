use regex::Regex;
use std::path::{Path, PathBuf};

pub(super) fn expand_lt_entities(xml: &str) -> String {
    match crate::engine::find_lt_resource_dir() {
        Some(root) => expand_parameter_entities(xml, &root, 0),
        None => xml.to_string(),
    }
}

fn expand_parameter_entities(xml: &str, root: &Path, depth: usize) -> String {
    if depth > 8 {
        return xml.to_string();
    }
    let decl = Regex::new(r#"<!ENTITY\s+%\s+(\S+)\s+SYSTEM\s+"([^"]+)"\s*>"#).unwrap();
    let mut expanded = xml.to_string();
    let mut changed = false;
    for cap in decl.captures_iter(xml) {
        let system_id = cap.get(2).map_or("", |m| m.as_str());
        if !system_id.ends_with(".ent") {
            continue;
        }
        let Some(content) = read_entity(root, system_id) else {
            continue;
        };
        let nested = expand_parameter_entities(&content, root, depth + 1);
        let marker = format!("%{};", &cap[1]);
        if expanded.contains(&marker) {
            expanded = expanded.replace(&marker, &nested);
            changed = true;
        }
    }
    if changed { expanded } else { xml.to_string() }
}

fn read_entity(root: &Path, system_id: &str) -> Option<String> {
    let relative = path_from_lt_resource_folder(system_id);
    let mut modules: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    modules.sort_unstable();
    for module in modules {
        let candidate = module
            .join("src/main/resources/org/languagetool/resource")
            .join(&relative);
        if candidate.is_file() {
            return std::fs::read_to_string(candidate).ok();
        }
    }
    None
}

fn path_from_lt_resource_folder(system_id: &str) -> String {
    system_id
        .rfind("/resource/")
        .map_or(system_id, |idx| &system_id[idx + "/resource/".len()..])
        .replace("../", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt_pt_xml() -> String {
        std::fs::read_to_string(
            crate::pt::filters::pt_rules_dir()
                .expect("pt rules dir")
                .join("pt-PT/grammar.xml"),
        )
        .expect("pt-PT grammar.xml")
    }

    #[test]
    fn expands_every_external_parameter_entity_of_the_nested_variant_file() {
        let xml = pt_pt_xml();
        let expanded = expand_lt_entities(&xml);
        let decl = Regex::new(r#"<!ENTITY\s+%\s+(\S+)\s+SYSTEM\s+"([^"]+)"\s*>"#).unwrap();
        let external: Vec<&str> = decl
            .captures_iter(&xml)
            .map(|c| c.get(1).unwrap().as_str())
            .collect();
        assert_eq!(
            external.len(),
            10,
            "pt-PT/grammar.xml declares 10 .ent files"
        );
        for name in external {
            assert!(
                !expanded.contains(&format!("%{name};")),
                "%{name}; must be replaced by the entity file content"
            );
        }
        for probe in [
            "<!ENTITY hifen \"(?:[-",
            "<!ENTITY expressao_invariavel_com_DE \"",
            "<!ENTITY expressoes_de_tempo_simples \"",
            "<!ENTITY art_detecao_paronimos \"",
        ] {
            assert!(
                expanded.contains(probe),
                "the expanded doctype must define the entity family {probe:?}"
            );
        }
    }

    #[test]
    fn expanded_doctype_declares_no_duplicate_entity_names() {
        let expanded = expand_lt_entities(&pt_pt_xml());
        let double_q = Regex::new(r#"<!ENTITY\s+(\w+)\s+""#).unwrap();
        let single_q = Regex::new(r#"<!ENTITY\s+(\w+)\s+'"#).unwrap();
        let mut names: Vec<&str> = double_q
            .captures_iter(&expanded)
            .chain(single_q.captures_iter(&expanded))
            .map(|c| c.get(1).unwrap().as_str())
            .collect();
        names.sort_unstable();
        let duplicates: Vec<&str> = names
            .windows(2)
            .filter(|w| w[0] == w[1])
            .map(|w| w[0])
            .collect();
        assert!(
            duplicates.is_empty(),
            "Java SAX keeps the first duplicate declaration while the shared expander keeps the \
             last, so duplicates would diverge: {duplicates:?}"
        );
    }

    #[test]
    fn path_normalization_matches_the_java_resolver() {
        assert_eq!(
            path_from_lt_resource_folder("../../resource/pt/entities/messages.ent"),
            "pt/entities/messages.ent"
        );
        assert_eq!(
            path_from_lt_resource_folder("pt/entities/messages.ent"),
            "pt/entities/messages.ent"
        );
        assert_eq!(
            path_from_lt_resource_folder("../resource/x/a.ent"),
            "x/a.ent"
        );
    }
}

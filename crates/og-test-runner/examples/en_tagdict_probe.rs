use og_tagger::opennlp::{GisModel, OpenNlpPosTagger, parse_tag_dictionary};

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    );
    let with = OpenNlpPosTagger::new(
        GisModel::from_file(&dir.join("en-pos-maxent.model")).expect("pos model"),
        parse_tag_dictionary(
            &std::fs::read_to_string(dir.join("en-pos-maxent.tagdict")).expect("tagdict"),
            &GisModel::from_file(&dir.join("en-pos-maxent.model")).expect("pos model"),
        ),
    );
    let without = OpenNlpPosTagger::new(
        GisModel::from_file(&dir.join("en-pos-maxent.model")).expect("pos model"),
        std::collections::HashMap::new(),
    );
    for text in std::env::args().skip(1) {
        let words: Vec<&str> = text.split_whitespace().collect();
        let t1: Vec<&str> = with.tag(&words).iter().map(|&i| with.outcome(i)).collect();
        let t2: Vec<&str> = without
            .tag(&words)
            .iter()
            .map(|&i| without.outcome(i))
            .collect();
        println!("with tagdict:    {t1:?}");
        println!("without tagdict: {t2:?}");
    }
}

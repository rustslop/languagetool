use og_test_runner::runner::TestRunner;

fn main() {
    let code = std::env::args().nth(1).unwrap_or_else(|| "en".into());
    let text = std::env::args().nth(2).expect("text");
    let runner = TestRunner::for_language(&code);
    let sentence = runner.debug_sentence(&text);
    for t in sentence.tokens() {
        if t.is_whitespace() {
            continue;
        }
        let readings: Vec<String> = t
            .readings()
            .iter()
            .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
            .collect();
        println!(
            "{:?} [{}] => {}",
            t.token().token(),
            t.chunks().join(","),
            readings.join(", ")
        );
    }
}

use og_test_runner::runner::TestRunner;

fn main() {
    let code = std::env::args().nth(1).unwrap_or_else(|| "en".into());
    let text = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "The quick brown fox.".into());
    let runner = TestRunner::for_language(&code);
    let sentence = runner.debug_sentence(&text);
    for t in sentence.tokens() {
        if t.is_whitespace() {
            continue;
        }
        let readings: Vec<String> = t
            .readings()
            .iter()
            .map(|r| {
                let lemma = r.lemma().unwrap_or("?");
                format!(
                    "{}[{}] {:02x?}",
                    lemma,
                    r.pos_tags().join("|"),
                    lemma.as_bytes()
                )
            })
            .collect();
        println!("{:?} => {}", t.token().token(), readings.join(" ; "));
    }
}

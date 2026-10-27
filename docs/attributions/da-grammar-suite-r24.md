# da grammar.xml example suite — bucketed attribution (round 24)

Generated on the tree of commit `ee9513f` (2026-10-03). The r23-recorded baseline (22 of 233
failing, stable since r9) was **a test-runner wiring bug, not engine behavior**: `da` was
missing from `runner.rs`'s `generic_dict_stem` table (the same class as the sk bug from
`0a1e307`), so the suite tagged Danish text with `EnglishTagger` over the da resource dir and
every word came back with empty-postag fallback readings.

## Headline

- `cargo test -p og-test-runner --release test_real_danish_grammar_examples -- --ignored`
  → **`da: 232/233 passed (99.6%) — no_match: 1`** (was 211/233 with 22 no-match).
- Evidence the dict itself is fine: `target/release/examples/probe_dict_lookup da danish jeg
  hende kanoen store Jens slidbane under unde` → `jeg/sub:ube:sin:neu:nom + jeg/pron:sin:nom`,
  `hende/hun/pron:sin:akk`, `Jens/pro:nom`, `under` 4 readings incl. `unde/ver:præ:akt`.
- r23 trace reading (all 22 failures were `postag=... got=[word:]` empty readings) is fully
  explained by the wrong tagger; nothing engine-side was broken.

## The one survivor

`Ordgentagelse` on `Nu er min min hals meget træt.` — the pron sub-rule needs a `pron:.*`
reading on `min`, but `min` (like `Nu`, `træt`) is absent from the fetched 469,885-entry
`danish.dict` (probe returns `min => []`). Same older-dict data limitation class as es/pt
(commit `370c7e6`); re-downloading is out of scope.

## Danish.java natives (wired this round, `ee9513f`)

`Danish.java:81-93` — the custom `GenericUnpairedBracketsRule` symbol list
(`[ ( { " ”` / `] ) } " ”` — Java pairs the Danish close-quote with itself and excludes `'`)
and `UppercaseSentenceStartRule` (da uses plain `SRXSentenceTokenizer`; the code comment's
`DanishSentenceTokenizer` class does not exist). `CommaWhitespaceRule`, `DoublePunctuationRule`,
`MultipleWhitespaceRule` were already installed by the generic for_code path; the speller rides
`HUNSPELL_RULE`. Native rules are outside the example-suite scope, so the suite number does not
move.

## Reproduction

```
cargo test -p og-test-runner --release test_real_danish_grammar_examples -- --ignored --nocapture
cargo run --release -p og-test-runner --example nomatch_trace -- da
cargo run --release -p og-test-runner --example probe_dict_lookup -- da danish jeg min
```

Artifacts this round: `/tmp/test_failures_da.txt` (1 line), `/tmp/nomatch_trace_da_r24.txt`.

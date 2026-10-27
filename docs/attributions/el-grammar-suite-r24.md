# el grammar.xml example suite — bucketed attribution (round 24)

Generated on the tree of commit `43412f7` (2026-10-03).

## Headline

- `cargo test -p og-test-runner --release test_real_greek_grammar_examples -- --ignored`
  → `el: 115/118 passed (97.5%) — no_match: 3, fp: 0, marker: 0`.
- Split of the 3: all no-match, all readings-driven, all data-blocked (below).

## The trio is data-blocked, not engine-side

`nomatch_trace el` (0 matched-anyway, 0 untraced):

| rule | deepest failure |
|---|---|
| GREEK_ART_MASC_MISSING_N | elem#1 tok=ανταγωνισμό: `postag="N:MASC:SG:ACC" got=[:]` |
| GREEK_PREP_ART_MASC_MISSING_N | elem#1 tok=ανταγωνισμό: `postag="N:MASC:SG:ACC" got=[:]` |
| GREEK_MORE_ADJ_COMP | elem#1 tok=ισχυρότερα: `postag="ADJ:COMP:.*" got=[:]` |

The fetched `greek.dict` is a **239-byte stub** (17 entries; `probe_dict_lookup el greek
ανταγωνισμό ισχυρότερα` returns `[]` for both), so the generic tagger produces only
empty-postag fallback readings for ordinary Greek words. The gap's named lever "GreekTagger
dict coverage" reduces to this missing data file; no code change can move the trio.

## Greek.java natives (shared ones wired this round, `43412f7`)

`Greek.java:73-94` — wired: `GenericUnpairedBracketsRule` with the `EL_UNPAIRED_BRACKETS` id
and the Greek symbol lists (`[ ( { “ " «` / `] ) } ” " »`), `LongSentenceRule(50)`,
`UppercaseSentenceStartRule`. `CommaWhitespace`/`DoublePunctuation`/`MultipleWhitespace` ride
the generic for_code path; the speller rides `MORFOLOGIK_RULE_EL_GR` (same missing-dict
limitation).

## Remaining residue (named for the next round)

1. Six Greek-specific rule classes: `GreekWordRepeatBeginningRule`, `WordRepeatRule`,
   `ReplaceHomonymsRule`, `GreekSpecificCaseRule`, `NumeralStressRule`, `GreekRedundancyRule`.
2. `GreekWordTokenizer` = a JFlex spec (`GreekWordTokenizerImpl`) + `joinEMailsAndUrls` —
   needs the same port treatment as the es/eo tokenizers.
3. Latent synth arm (`GreekSynthesizer` via `createDefaultSynthesizer`) — cheap once wanted,
   the seek-dictionary variant used for sk/gl/ro.
4. The `(EL|el).*` SRX check from the gap notes.

## Reproduction

```
cargo test -p og-test-runner --release test_real_greek_grammar_examples -- --ignored --nocapture
cargo run --release -p og-test-runner --example nomatch_trace -- el
cargo run --release -p og-test-runner --example probe_dict_lookup -- el greek ανταγωνισμό
```

Artifacts this round: `/tmp/test_failures_el.txt` (3 lines), `/tmp/nomatch_trace_el_r24.txt`.

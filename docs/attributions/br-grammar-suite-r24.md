# br grammar.xml example suite — bucketed attribution (round 24)

Generated on the r23 fresh battery artifacts (tree of `6a9229d`, 2026-10-03); no br-specific
code changed this round.

## Headline

- Suite: `cargo test -p og-test-runner --release test_real_breton_grammar_examples -- --ignored`
  → `br: 1634/1834 passed (89.1%)` — 200 failing lines in `/tmp/test_failures_br.txt`:
  **193 no-match, 5 fp, 2 marker**.
- `nomatch_trace br`: 193 traced, matched-anyway 5, untraced 17 (171 bucketed).

## no-match buckets (top of the aggregate)

The mass is *surface-regex* failures on Breton mutation families — the deepest element
expects a mutated/preferred form the pipeline never produces:

- 53 `surface!="^(?:2|[ae]|aba|da|[dtz]aou|div|dre|endra|gwall|na|ne|pa|pe|ra|re|seul)$"` +
  49 `^(?:am|em|he|ma|nav|o|[pbf]eder|[pbf]evar|[tdz]eir|[tdz]ri|va)$` + 48 `^(?:ho|ez|az)$`
  + 42 `^(?:[aeu]r)$` + 36 `da` — the KEMM_DRE_* mutation rule family (soft/mixed/hard
  mutation prefixes, KLANV_PE_GLANVOCH is the top rule at 15 fails).
- 32 `^(?:(estonet|klañv|laouen|nec'het|plijet|skuizh|...)...$` + 32
  `^(?:aon|bec'h|diegi|...)$` — adjective/noun stem classes behind EO_BET_E_OA_BET-class rules.
- 26 `^(?:[au]n)$`, 22 `postag="L [eo]" got=[:SENT_START]`, 17 `^(?:on|out|eo|omp|oc'h|int|eur)$`
  — the O_DA / verb-paradigm families; the single postag bucket confirms the readings side is
  thin because the breton tagger has **no compiled dict** (data-blocked, unchanged since r22:
  `MorfologikBretonTagger` ships no `breton.dict` in the LT repo).
- 15 `backref-no=Some(0) mismatch` — a small genuine matcher-signal bucket worth a look once
  the data side moves.

## fp (5) and marker (2)

DIV x2 (duplication), PEOGWIR, PA, KEMM_MESKET_RANNIG_E_O — individually inspectable, too
small to bucket.

## Next levers (in order)

1. The mutation families above are surface-driven, so they move with the tagger/tokenizer
   data, not with matcher work — confirm against Java's `BretonTagger`/`BretonWordTokenizer`
   what supplies the mutated readings (Java's tagger is rule+data based; without the compiled
   dict our readings stay empty).
2. `Breton.java` natives (unwired).
3. The 15 backref mismatches as the one code-side signal.

## Reproduction

```
cargo test -p og-test-runner --release test_real_breton_grammar_examples -- --ignored --nocapture
cargo run --release -p og-test-runner --example nomatch_trace -- br
```

Artifacts this round: `/tmp/test_failures_br.txt` (200 lines), `/tmp/nomatch_trace_br_r24.txt`.

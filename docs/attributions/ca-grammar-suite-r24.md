# ca grammar.xml example suite — bucketed attribution (round 24)

Generated on the r23 fresh battery artifacts (tree of `6a9229d`, 2026-10-03); no ca-specific
code changed this round. This discharges the re-planned "committed nomatch_trace ca split".

## Headline

- Suite: `cargo test -p og-test-runner --release test_real_catalan_grammar_examples -- --ignored`
  → `ca: 14410/25598 passed (56.3%)` — 11,188 failing lines in `/tmp/test_failures_ca.txt`:
  **10,207 no-match, 884 fp, 97 marker**.
- `nomatch_trace ca` (full run this round, ~30 min): 10,207 traced, matched-anyway 589,
  untraced 995 (8,623 bucketed).

## The no-match mass is the missing tagger dictionary (data-blocked, confirmed fresh)

The ca resource dir has **no compiled dict at all** (`ls resource/ca/*.dict` → nothing;
`probe_dict_lookup ca catalan ...` → LOAD FAILED). The generic tagger therefore runs on
`added.txt` (178 KB manual data) + the 984 KB `disambiguation.xml` (ca disambig ground truth
is 10-failure stable), so ordinary words carry only empty-postag readings. The aggregate
deepest-cause buckets are exactly that signature:

- 1729+715 `postag="P0.{6}|PP3CN000|... "` got=[:SENT_START] / got=[:] — clitic-pronoun
  postags on untagged tokens.
- 1314+523+268+267+240+236+196 `postag="V.[MNG].*"` / `V.*` / `V.N.*` / `V.[SI].*` /
  `V.{6}[VBZ34567]` / `VA.*` families, all `got=[:]`.
- 866+743+563+258 `DA.*` / `SPS00|LOC_PREP` / `D.*` / `D[^R].*|PP...A..` — determiner/prep
  postags.
- 508+317+252 `inflected-lemma!="haver"` / `sembilar|parèixer` / the anar-family and the
  noise-noun lemma lists.
- Per-rule samples confirm the shape end to end (`ACABAR_AMB` elem#0 tok=acabar
  `postag="V.*" got=[:]`, `ADRECA_DIRECCIO` tok=adreça `postag="N.*" got=[:]`).

Everything above moves only when a compiled ca tagger dictionary exists (out of scope:
re-downloading model/dict data is excluded; `370c7e6` records the same class for es/pt).

## Code-side buckets that survive the data block

- 691 `chunk!="GV"` — verb-chunk-dependent rules; the ca chunker path is exercised nowhere
  else at this scale; worth one lang_rule_trace once readings exist.
- 632+626+320+299+291+284+272+... surface families (`de|d'`, `a`, `.+[aeiï]`, `el|l`,
  `du|duu|endú`, `no|com`) — mostly the same missing-readings story expressed through
  element surfaces, but a few (apostrophe elisions `d'`, `n'|'n`) may be tokenizer-level.
- matched-anyway 589 / untraced 995 — same caveats as the pt/fr docs (sub_id siblings,
  rules whose id never appears in the compiled set).

## fp clusters (884)

Top: MES1 75, CONFUSIONS_ACCENT 30, DARRERA 20, SON 18, ES 18, COMPLEXES 15, QUANT_QUAN 14,
COMMA_LOCUTION 13. These match on *correct* examples — with readings this thin the usual cause
is a pattern matching on the empty-fallback reading; re-bucket after the dict exists before
touching the matcher.

## Next levers

1. Compiled ca tagger dictionary (data-blocked; blocks ~8.6k of the 10.2k no-match mass).
2. `chunk!="GV"` family (691) — the only large non-readings bucket.
3. The 2-3 largest fp clusters above (CatalanMultitopoDisambiguator / lemma SimpleReplace /
   WordCoherency natives are still unwired per the gap notes).

## Reproduction

```
cargo test -p og-test-runner --release test_real_catalan_grammar_examples -- --ignored --nocapture
cargo run --release -p og-test-runner --example nomatch_trace -- ca     # ~30 min
cargo run --release -p og-test-runner --example probe_dict_lookup -- ca catalan menjar
```

Artifacts this round: `/tmp/test_failures_ca.txt` (11,188 lines), `/tmp/nomatch_trace_ca_r24.txt`.

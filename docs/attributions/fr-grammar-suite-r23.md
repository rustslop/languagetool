# fr grammar.xml example suite — bucketed attribution (round 23)

Generated on the tree of commit `844c946` (2026-10-03). All numbers below were
produced by the commands listed in *Reproduction* run in this round. This is the
first committed fr bucketing (the r22 claim of "nomatch_trace bucketing for
nl/pt/fr" was only substantiated for nl).

## Headline

- Suite: `cargo test -p og-test-runner --release test_real_french_grammar_examples -- --ignored`
  → `fr: 14301/15293 passed (93.5%) — no_match: 738, fp: 249, marker: 5` (992 failing lines in
  `/tmp/test_failures_fr.txt`).
- Trajectory: 1000 (r21) → 994 (r22, IsEnglishWordFilter inertness fixed ON_ONT-class FPs) →
  992 (this round: min≥2 parse-time expansion, commit `414925d`, fixed the remaining ON_ONT
  example and POINTS_SUSPENSIONS_SPACE).
- Per-class split of the 992: 738 no-match, 249 fp, 5 marker.
- fr disambig ground truth: 0 failures (`cargo run --release -p og-test-runner --example
  disambig_examples -- fr` → "13 untouched + 4 ambiguous examples, 0 failures").

## no-match bucketing

`cargo run --release -p og-test-runner --example nomatch_trace -- fr`:
738 no-match failures traced, matched-anyway: 123, untraced: 28.

- matched-anyway (123): a sibling sub_id matches in isolation — mostly legitimate under
  (id, sub_id) grouping (fr has 4 phraserefs, all outside rulegroups, so this is sub-rule
  mismatch, not the sibling disease pt had); verify individually.
- untraced (28): no trace events; inspect by hand.

Top rules by failing examples:

| rule | fails | class |
|---|---|---|
| ACCORD_SUJET_VERBE | 34 | no-match |
| ETRE_VPPA_OU_ADJ | 27 | no-match |
| ACCORD_R_PERS_VERBE | 24 | no-match |
| VERB_PRONOUN | 23 | no-match |
| AGREEMENT_POSTPONED_ADJ | 22 | fp |
| AUX_AVOIR | 18 | no-match |
| A_ACCENT | 16+13 | mixed |
| C_EST_COMPLIQUER | 14 | no-match |

Aggregate deepest-failure causes (top of 50 distinct) and reading:

- 443 `surface!="^(?:[«\"]|\")$"` + 66 `surface!="^(?:[«\"\(\[]|\")$"` — ACCORD_SUJET_VERBE-class
  rules whose pattern begins with an opening-quote element: the traced sentence position never
  carries `«`/`"` as its own token. Compare Java tokenization of `«Il …»` before touching the
  matcher.
- 309 `inflected-lemma!="ne"` + 76 `surface!="^(?:ne|n')$"` + 74 `surface!="-ce"` — the
  ne-clitic family: element never sees `ne`/`-ce` where expected (tokenizer or readings).
- 221 `surface!="et"`, 122 `surface!="a"`, 105 `inflected-lemma!="de"`, 91 `inflected-lemma!="que"`
  — plain-surface elements that never line up: mostly examples where an earlier element already
  diverged (the trace reports the deepest failure, not the root cause).
- 183 `postag="A" got=[:]`, 140 `postag="R pers suj 1 s" got=[:SENT_START]`, 130 `R pers suj 2 p`,
  113 `R pers suj 3 . s`, 107 `A inte`, 122 `P`, 71 `D.*` (all `got=[:SENT_START]`) — readings
  parity: the French tagger's clitic/adj/det postags are absent or differently shaped at the
  traced position. This is the named next lever: per-reading diff vs Java
  (FrenchTaggerTest pins the dict format — `passagère passager/J f s + N f s` per commit
  `370c7e6`).
- 181+115 `inflected-lemma!="(être|sembler|paraître|…)$"` — copula lemma sets.

## Standing divergences / limitations

- `working`-lemma None-lemma readings; regexp_match/case_conversion/setpos attrs on disambig
  `<match>` (0 live fr rules); french_synth.dict absent (accepted-excluded).
- fr was an IsEnglishWordFilter user; the filter is inert outside en since commit `931a6c1`
  (Java-faithful per module classpaths).

## Reproduction

```
cargo test -p og-test-runner --release test_real_french_grammar_examples -- --ignored --nocapture
cargo run --release -p og-test-runner --example nomatch_trace -- fr
cargo run --release -p og-test-runner --example disambig_examples -- fr
```

Artifacts this round: `/tmp/test_failures_fr.txt` (992 lines), `/tmp/nomatch_trace_fr_r23.txt`.

# pt grammar.xml example suite — bucketed attribution (round 25, lang-pt round 2)

Generated 2026-10-03 on the tree of this round's pt module changes (uncommitted at writing
time). Supersedes `docs/attributions/pt-grammar-suite-r23.md` (which documents the pre-port
480-failure set at commit `844c946`). All numbers below were produced in this round by the
commands in *Reproduction*.

## Headline

| configuration | suite result | no_match | fp | marker |
|---|---|---|---|---|
| HEAD baseline (dict md5 `5031f647…`, tracked fetch) | 5174/5492 (94.2%) | 250 | 63 | 5 |
| + portuguese.dict swapped to fetch md5 `8d7f0590…` | 5218/5492 (95.0%) | 219 | 49 | 6 |
| + `PortugueseTagger` on the engine path (in-module pipeline; the state after the integrator wiring below) | **5222/5492 (95.1%)** | 214 | 50 | 6 |

### Refresh (round 26, 2026-10-06, by the wave integrator)

The rows above are the round-25 state and are stale: the round-26 module work (disambiguation
chain completion of commit `13fd76b`, wired by `34f9ec9`) moved the suite further. Current
state, re-measured this round on the committed tree
(`cargo test -p og-langs --release --lib pt::example_suite::portuguese_grammar_examples_with_pt_pipeline
-- --ignored --test-threads=1 --nocapture`):

| configuration | suite result | no_match | fp | marker |
|---|---|---|---|---|
| round 26, grammar.xml module pipeline | **5282/5492 (96.2%)** | 181 | 23 | 6 |

Failure artifact: `/tmp/test_failures_pt_module.txt` (210 lines, down from 270). The style.xml
residue of the same round is documented in `docs/attributions/pt-style-suite-r26.md`.

Suite command: `cargo test -p og-test-runner --release test_real_portuguese_grammar_examples
-- --ignored --nocapture`. The in-module measurement: `cargo test -p og-langs --release --lib
pt::example_suite -- --ignored --nocapture`.

## Changes this round (all inside `crates/og-langs/src/pt/`)

1. **portuguese.dict selection** (data, mirrors the es decision in commit `06d26b1`): two
   snapshots exist on disk — `/tmp/dicts-pt/portuguese.dict` (583,785 B, 908,819 sequences,
   md5 `5031f64736f47d1022f5d2e17b7b4bdb`) and `/tmp/ptdict/portuguese.dict` (635,094 B,
   872,717 sequences, md5 `8d7f05909a5ba6d1e99ebb131d8f12d7`). Coverage is non-monotone
   (`resolver` exists only in the first, `vinte`/`quatro`/`google` in neither), so the choice
   was made empirically at suite scale: the second scores +44 examples and is now installed in
   `languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/resource/pt/`.
   Dict bytes are untracked fetch data (same policy as es).
2. **`pt/tagger.rs` — faithful port of `PortugueseTagger.java`** (es `tagger.rs` shape):
   hyphen-aware `isMixedCase` (per-part `StringTools.isMixedCase`), `’`→`'` normalization for
   words longer than one char with the `containsTypewriterApostrophe` chunk set from the
   pre-replacement word, BaseTagger case fallback, `tagNumberExpressions` (ordinals →
   `NC..000`+`AO0..0` pairs, degrees and percents → `NCMP000`), `tagMenteAdverbs` (`X-mente` →
   `RG` when the lowercased stem has a `V.P..SF.|A[QO].[FC][SN].` reading), `tagPrefixedVerbs`
   (`(soto-)(...+)` → stem verb tags, `ignoreSpelling` on the ATR), manual
   additions/removals, and `shared_arc()`. Pinned by 9 unit tests incl. the PortugueseTaggerTest
   ordinal/percent/degree/prefix values.
3. **`pt/word_tokenizer.rs`**: `shared_portuguese_word_tokenizer` now gates hyphen splitting on
   `PortugueseTagger::is_word_tagged` (Java `PortugueseWordTokenizer.wordsToAdd` calls
   `tagger.tag(...).get(0).isTagged()`), instead of a bare GenericTagger dict lookup. Runner
   suite byte-stable at 5218 with this change.
4. **`pt/test_support.rs` + `pt/example_suite.rs`**: the fr/es-style in-module measurement
   harness (`PortuguesePipeline`: shared pt tokenizer + `PortugueseTagger` +
   `XmlDisambiguator`+`hybrid_disambiguator("pt",…)` + `srx_splitter("pt")`), plus a pt-specific
   SRX pin test (`Av.` / `n.` abbreviation no-breaks) mirroring `es/mod.rs:317`.

PortugueseTagger's own effect on the suite (module vs runner failure sets): +5 no-match fixed
(`DE_MANEIRA_ADVERBIO_MODO` ×2 and `CONCORDANCIA_SER_PLURAL`, `SAIR_AS_RUAS`,
`SUBST_DE_QUE_TER_DIREITO_A_QUE` — all `-mente` adverbs the dict lacks, now `RG`→`RM` through
disambiguation.xml's `WORKAROUND_MISSING_ADVERBS_POS`), −1 new fp
(`IRREGULAR_PAST_PARTICIPLES` 'Depois de ter discretamente limpado o carro…' — see filters
below; the unwired `RegularIrregularParticipleFilter` lets the raw pattern fire now that
`discretamente` is tagged).

## Residue attribution (270 module failures: 214 no_match / 50 fp / 6 marker)

### A. Older pt tagger dict — accepted-excluded (~230 failures)

Per the r23 doc's *Standing data limitations* and the model byte-identity exclusion. Three
dict manifestations, each evidenced by direct dictionary probes
(`cargo run --release -p og-test-runner --example probe_dict_lookup -- pt portuguese <words…>`):

- **Clitic-chain and contraction tags absent.** Current grammar.xml keys on `V.+:PP.+`,
  `SPS00:DA…`, `PP…:PP…` chains; the dict gives `das → de+as/SPS00+*`, `ao → a+o/SPS00+*`,
  `qui-lo → quis+lo/V+PP`, and has no `deixe-me`/`distribuí-lo-ei`/`bebemo-lo`/`dão-no` at
  all. Java pins (`PortugueseTaggerTest.java:87-89,114,168`) expect
  `das/[de:o]SPS00:DA0FP0`, `distribuí-lo-ei/[distribuir]VMIF1S0:PP3MSA00`,
  `Deixe-me/[deixar]VMM03S0:PP1CSO00`. The es-style `+`→`:` tag normalization does **not**
  apply here: the es dict carried full chained tags (`del → de+el/SP+DA`), the pt snapshot's
  clitic part is literally `*`, so there is nothing faithful to normalize to. Families:
  `COLOCACAO_PRONOMINAL_COM_ATRATOR_SIMPLES`(40) + `_COMPLEXO`(15),
  `ACENTUAÇÃO_VOGAL_ÊNCLISE`(4), `COLOCACAO_PRONOMINAL_EM_O_SENDO`(2),
  `REFLEXIVE_VERB_SE_AGREEMENT`(3), `AUXILIARY_VERB_INFINITIVE`(10, incl. both marker cases),
  `HOMONYM_CERRA_12` / `EASILY_CONFUSED_RARE_WORDS` / `GENERAL_VERB_AGREEMENT_ERRORS` marker
  cases (match stops before the clitic), `RIU-ME`, `FALARAM-SE_DE`, `ESQUECEU-SE_QUE`, …
- **Missing basic vocabulary.** `vinte`, `quatro`, `google`, `destacadamente` have no entry in
  either snapshot → numeral (`Z.+`) and RG-dependent rules stall with `got=[:]`:
  `CONCORDANCIA_DAR_HORAS`(6), `IMPERSONAL_FAZER_TIME_EXPRESSION`(4),
  `NON_IMPERSONAL_VERBS`, `URL_INICIAL`, `DUAS_MILHOES`, `AS_MILHARES_DE_PESSOAS`, …
- **Stale readings.** `vaca → boi/NCFS000` (correct lemma, feminine tag),
  `cara → cara/NCFS000, caro/AQ0FS0` (no masculine/plural readings) — the CONCORDANCIA /
  GENERAL agreement families see the wrong gender/number readings:
  `GENERAL_GENDER_AGREEMENT_ERRORS`(12), `GENERAL_NUMBER`(11), `GENERAL_GENDER_NUMBER`(7),
  `GENERAL_VERB`(7), `CONCORDANCIA_DETERMINANTES_MASCULINOS`(10) (`postag="D..MP.+"
  got=[cara:NCFP000; caro:AQ0FP0]` in `nomatch_trace`).

nomatch_trace over the current set (219 no-match lines): matched-anyway 14, untraced 1 — same
classification as r23 (Java also fails most matched-anyway cases; verify before counting as
engine bugs).

### B. Unwired pt filter classes in the shared og-rules dispatch (~26 recoverable)

`crates/og-rules/src/pattern_rule.rs` dispatches `pt.DateCheckFilter`, `pt.FutureDateFilter`,
`pt.NewYearDateFilter`, `pt.YMDNewYearDateFilter`, but **not** these five classes used by 33
filter elements in pt grammar.xml (counts = occurrences):

| filter class | usages | pt residue family (module failures) |
|---|---|---|
| `org.languagetool.rules.pt.ConfusionCheckFilter` | 3 | `DIACRITICS`(13; the fp mass — e.g. the `nos|são` subrule's correct examples 'Uma pedra nos servia de assento.' / 'Você nos incomoda…' match the raw pattern, Java's filter nulls them because form `de` is not in `confusion_pairs.txt`(+variant)) |
| `org.languagetool.rules.pt.BrazilianToponymFilter` | 2 | `SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES`(7) + `_PARENTESES`(1) |
| `org.languagetool.rules.pt.RegularIrregularParticipleFilter` | 2 | `IRREGULAR_PAST_PARTICIPLES`(4, incl. the 1 new fp above — with the filter wired the fp disappears) |
| `org.languagetool.rules.pt.RomanNumeralFilter` | 1 | `ROMAN_NUMBERS_CHECKER`(1) |
| `org.languagetool.rules.pt.PortugueseProclisisFilter` / `PortugueseEnclisisFilter` | 3+3 | `COLOCACAO_*` suggestions — blocked on data regardless (patterns need `V.+:PP.+` first; the filter also synthesizes via the absent pt synth dict) |

Java sources: `languagetool/languagetool-language-modules/pt/src/main/java/org/languagetool/rules/pt/`
(ConfusionCheckFilter 111 lines incl. `ConfusionPairsDataLoader` over `/pt/confusion_pairs.txt`
+ variant file; BrazilianToponymFilter 49 lines incl. `BrazilianToponymMap`;
RegularIrregularParticipleFilter 110 lines; RomanNumeralFilter 41 lines).

### C. Everything else (small families)

`FRAGMENT_TWO_ARTICLES`(6), `PT_MULTITOKEN_SPELLING_TWO`(5) (multitoken speller data),
`OUVE_PROBLEMAS`(5), `CONFUSÃO_À_HÁ`(5), `SERIAR_VERB`(4), `FUTURE_CONJUGATION_ERROR`(4),
`CONC_NUM_GEN_O_SUAS`(4), `MELHOR_EDUCADO`(3), singles — all trace to missing readings of the
same dict classes (A) via `nomatch_trace` deepest-cause stalls.

## Integrator wiring (outside this round's boundary)

1. `pub mod pt;` in `crates/og-langs/src/lib.rs` (if not already present).
2. Engine tagger arm, `crates/og-langs/src/engine.rs` (~line 803, before the generic
   `GenericTagger::from_resource_dir` fallback): consume
   `crate::pt::tagger::PortugueseTagger::shared_arc()` for `short == "pt"` (it implements both
   `og_tagger::Tagger` and `og_core::checker::Tagger`, same shape as es `SpanishTagger`).
   Expected effect: the runner suite moves 5218 → ~5222 (measured in-module).
3. Runner tagger arm, `crates/og-test-runner/src/runner.rs` (~line 200, next to the es arm):
   `AnyTagger::Portuguese(Arc<PortugueseTagger>)` with
   `Tagger::tag(t.as_ref(), tokens)` — mirrors `AnyTagger::Spanish`.
4. The five unwired filter classes (table above) in the shared filter dispatch.
5. pt message for `PunctuationMarkAtParagraphEnd2Rule` (gap's polish note; ru overrides at
   `engine.rs:1111-1113`, pt does not): use the pt-PT bundle strings
   (`MessagesBundle_pt_PT.properties:289-290`) — desc "Falta de pontuação final no final do
   parágrafo", message "Adicione pontuação final ao fim do parágrafo.".
6. Promote this file to `docs/attributions/pt-grammar-suite-r25.md` (or fold its content into
   the r25 attribution round doc); the r23 doc is stale (documents the pre-port 480 set).

## Reproduction

```
cargo test -p og-test-runner --release test_real_portuguese_grammar_examples -- --ignored --nocapture
cargo test -p og-langs --release --lib pt::example_suite -- --ignored --nocapture
cargo run --release -p og-test-runner --example nomatch_trace -- pt
cargo run --release -p og-test-runner --example probe_dict_lookup -- pt portuguese das deixe-me vinte vaca
md5sum languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/resource/pt/portuguese.dict
```

Artifacts this round: `/tmp/test_failures_pt.txt` (runner, 274 lines),
`/tmp/test_failures_pt_module.txt` (module, 270 lines), `/tmp/test_failures_pt_8d7f0.txt`
(runner under the swapped dict before the tagger), `/tmp/nomatch_trace_pt_r25.txt`.

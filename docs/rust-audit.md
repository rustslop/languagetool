# OpenGrammar-RS Audit Report

**Date:** 2026-05-27 (updated)
**Codebase:** `opengrammar-rs/` — 11 crates, ~60 source files, ~21,500 lines of Rust
**Development span:** May 26–27, 2026 (18 commits)
**Target:** LanguageTool Java engine feature parity for `/v2/check`

---

## Executive Summary

The workspace skeleton is complete — all 11 planned crates exist with proper Cargo workspace configuration. The English pipeline is functional: API server responds, grammar.xml loads and pattern rules execute, POS tagging and disambiguation work for English, and an XML example test runner can validate rules against embedded examples.

**What works end-to-end:** POST `/v2/check` with English text returns LT-shaped JSON with real grammar rule matches.

**What is not done:** 36 languages have zero implementation, spellchecking is minimal, no SRX sentence splitting, no statistical POS tagger, no compatibility harness, and many Java LT features are stubs.

**Changes since initial audit (2026-05-27):**
- Fixed XML parser to extract `description`, `sub_id`, `url`, `short`, `correction`, `type` attributes
- Propagated `sub_id`, `url`, `short_message`, `issue_type` to RuleMatch output
- Wired `picky`/`level` parameters into rule filtering
- Fixed API response: `language.name`, `RuleMatch.type`, proper `code`/`longCode` split
- Expanded sentence splitter abbreviations from 37 to 200+ with word boundary matching
- Unified Tagger traits (direct `og_core::checker::Tagger` impl on `EnglishTagger` with chunking)
- Honored `default_on` flag in rule filtering

---

## Workspace Overview

| Crate | Lines | Status | Role |
|-------|-------|--------|------|
| og-core | ~1,550 | Functional skeleton | Core types: Language, Token, AnalyzedSentence, Rule, RuleMatch, Checker |
| og-api | ~390 | Working | Axum HTTP server: /v2/check, /v2/languages, /v2/version |
| og-xml | ~4,050 | Mostly complete | XML parser/compiler for grammar.xml, disambiguation.xml |
| og-rules | ~3,550 | Functional | Pattern matching engine, native rules, text-level rules |
| og-tokenizer | ~2,350 | Partial | Sentence splitting and word tokenization |
| og-tagger | ~4,500 | Partial (English only) | POS tagging, lemmatization, disambiguation, chunking |
| og-spell | 421 | Stub | Dictionary lookup, brute-force Levenshtein suggestions |
| og-languagemodel | 26 | Stub | Empty struct, returns 0.0 |
| og-langs | ~1,200 | English only | Language engine factory, resource loading, native rule ports |
| og-test-runner | ~1,950 | Partial | XML example test runner, no golden tests |
| og-compat | 444 | Partial types only | Comparison logic exists, no harness to run Java LT |

---

## Phase Completion Matrix

| Phase | Plan Requirement | Status | % | Blockers |
|-------|-----------------|--------|---|----------|
| 0 — Source audit | Compatibility matrix, rule port lists, test lists | Started | 40% | See compatibility matrix below; JAVA_RULE_PORT_LIST partially documented |
| 1 — API shell | Axum server, /v2/check, LT-shaped JSON | Done | 95% | `motherTongue` still dead; missing `isPremium`/`sourceFile`/`group` on RuleMatchRule |
| 2 — Core text model | All planned types | Done | 80% | Missing: whitespace-before on tokens, token type enum, AnalyzedDocument type, Rule lifecycle methods |
| 3 — Tokenizer | Sentence splitting, word tokenization | Partial | 65% | SRX loading still missing; 200+ abbreviations now cover most common cases; no URL/email detection |
| 4 — XML parser | grammar.xml, style.xml, compilation | Done | 85% | Fixed: description/sub_id/url/short/correction/type now parsed. Missing: `<unify>`, `<phrases>`, style.xml |
| 5 — Pattern rule engine | Token matcher, regex, skip/min/max, suggestions | Functional | 80% | Fixed: default_on honored, sub_id/url/short_message/issue_type propagated. Missing: `<unify>`, many filters still stubs |
| 6 — Spellchecker | Dictionary, suggestions, ranking | Stub | 30% | Missing: Hunspell parsing, compound handling, frequency ranking, phonetic matching |
| 7 — POS/morphology | Tagger dicts, lemma/POS readings | Partial | 65% | Fixed: Tagger traits unified. Missing: statistical tagger, auto-loading external dicts |
| 8 — Disambiguation | XML disambiguation, POS-sensitive rules | Partial | 65% | Missing: unify action is stub, no feature parsing, only English |
| 9 — Native rule ports | Java Rule classes → Rust | Active | 55% | 21 rules ported (4 generic + 17 English). Style freq, regional replace, Compound, Dash, WordCoherency + more |
| 10 — Language modules | Per-language resources, tests | English only | 5% | 1 of 37 languages. No de/fr/es/pt/pl/nl modules |
| 11 — Test suite parity | Golden tests, cross-language regression | Stub | 5% | No golden fixtures, no compatibility harness, no CI |
| 12 — Performance | Rule indexing, parallelism, zero-copy | Partial | 20% | Has rayon + first-token index. No compiled regex cache, no dictionary optimization |

---

## Detailed Findings by Crate

### og-core

**All 17 planned types exist:**

| Type | Status | Gaps |
|------|--------|------|
| `Language` | Present (47 variants) | Missing: country info, CJK flags, tokenizer/tagger references |
| `Document` | Present | Missing: annotations, coordinate mapping |
| `Sentence` | Present | Missing: whitespace tracking |
| `Token` | Present | Missing: type enum (WORD/WHITESPACE/PUNCT), `whitespace_before` |
| `AnalyzedToken` | Present | Missing: `isSentStart/End`, `isImmunized`, chunk tags inline |
| `AnalyzedTokenReadings` | Present | Missing: `isSentStart/End`, `getDismissed`, lazy POS lookup |
| `AnalyzedSentence` | Present | Missing: `getAnalyzedTokensWithWhitespace`, language reference |
| `Rule` (trait) | Present | Missing: `reset()`, `activate/deactivate`, config support, priority |
| `TextLevelRule` (trait) | Present | Signature uses `&str` + `&[AnalyzedSentence]`, not `AnalyzedDocument` |
| `RuleMatch` | Present | Missing: `type` field, line/column, inverted suggestions, immunization |
| `SuggestedReplacement` | Present | Missing: type, priority, custom info |
| `Category` | Present | Missing: priority, severity, hierarchy, hidden/remote flags |
| `IssueType` | Present (21 variants) | Comprehensive |
| `Checker` | Present | Missing: caching, auto-detection, max text length, thread pool |
| `CheckRequest` | Present | `level`, `picky`, `mother_tongue` are dead fields |
| `SentenceRange` | Present | Adequate |

**Key architectural issues:**
- Two incompatible `Tagger` traits: `og-core::checker::Tagger` takes `&mut [AnalyzedTokenReadings]`, `og-tagger::Tagger` takes `&[&str]`
- No `AnalyzedDocument` type exists (plan mentions it for `TextLevelRule`)
- No `RuleContext` parameter (plan specifies it in trait signatures)

### og-api

| Feature | Status |
|---------|--------|
| `POST/GET /v2/check` | Working |
| `GET /v2/languages` | Working |
| `GET /v2/version` | Working |
| CORS | Enabled |
| Response: `software`, `warnings`, `language`, `matches`, `sentenceRanges`, `extendedSentenceRanges` | All present |

**Parameter support (all 10 planned):**

| Parameter | Parsed | Acted On |
|-----------|--------|----------|
| `text` | Yes | Yes |
| `language` | Yes | Yes |
| `motherTongue` | Yes | **No** (dead) |
| `enabledRules` | Yes | Yes |
| `disabledRules` | Yes | Yes |
| `enabledCategories` | Yes | Yes |
| `disabledCategories` | Yes | Yes |
| `level` | Yes | Yes (picky mode when "picky") |
| `picky` | Yes | Yes (enables default-off rules) |

**Response shape:**
- `language.name`, `language.detected.detectedBy`, `language.detected.confidence` — all present
- `RuleMatch.type` field — present (serialized as `"type"`)
- `code`/`longCode` split in languages — `code` returns short code (e.g., "en"), `longCode` returns full code (e.g., "en-US")

**Remaining response gaps:**
- `RuleMatchRule` missing `isPremium`, `sourceFile`, `group`
- `extendedSentenceRanges` always empty
- `motherTongue` parameter still dead

### og-xml

**XML compatibility levels:**

| Level | Scope | Status |
|-------|-------|--------|
| 1 | Basic `<rule>`, `<pattern>`, `<token>`, `<message>`, `<suggestion>` | Complete |
| 2 | `regexp`, `case_sensitive`, exceptions, antipatterns | Complete |
| 3 | `rulegroups`, examples, categories, default enable/disable | Mostly done (example corrections never populated) |
| 4 | POS constraints, inflected, lemmas, skip/min/max | Mostly done |
| 5 | Backreferences, markers, suggestion transforms | Mostly done |
| 6 | Disambiguation rules | Mostly done (unify is stub) |
| 7 | Filter hooks | Partially (XML parsed, no runtime execution) |

**Attributes/elements parsed from `<token>`:**
- `regexp`, `postag`, `postag_regexp`, `negate`, `negate_pos`, `case_sensitive`, `inflected`, `min`, `max`, `skip`, `spacebefore`, `chunk`, `chunk_re` — all parsed and compiled

**Critical gaps:**
- No `<unify>` element support in grammar rules
- No `<phrases>` / `<while>` / `<tests>` support
- Entity expansion is simplistic (no external entities, no parameter entities)
- style.xml never loaded

**Fixed in this session:**
- `<rule description="...">` now parsed and propagated
- `<rule sub_id="...">` now parsed and propagated to RuleMatch
- `<url>...</url>` content now extracted and propagated
- `<short>...</short>` content now extracted and propagated as `short_message`
- `<example correction="...">` text now captured and split on `|`
- `<category type="...">` / `<rulegroup type="...">` / `<rule type="...">` now parsed
- Marker-based implicit Incorrect detection for examples now works

### og-rules

**Pattern matching engine features:**

| Feature | Status |
|---------|--------|
| Literal text matching | Working |
| Regex matching (anchored) | Working |
| `inflected="yes"` (lemma fallback) | Working |
| `negate` / `negate_pos` | Working |
| POS tag constraints | Working |
| Chunk constraints | Working |
| `spacebefore` | Working |
| Exceptions (with scope: next/previous/current) | Working |
| Skip tokens | Working (including unlimited skip) |
| Min/max repetition | Working (greedy) |
| Optional tokens (min=0) | Working |
| OR groups / AND groups | Working |
| Antipatterns (with overlap immunization) | Working |
| Markers (error span) | Working |
| Backreferences in patterns | Working |
| Suggestion generation (text + match refs) | Working |
| Case conversion (startupper/alllower/etc.) | Working |
| Regexp match/replace in suggestions | Working |
| First-token word index | Working |
| Rayon parallelism | Working |

**Filter implementation:**

| Filter | Status |
|--------|--------|
| `FutureDateFilter` | Working (hardcoded 2014 date) |
| `DateCheckFilter` | Working (Zeller's congruence) |
| `NewYearDateFilter` / `YMDNewYearDateFilter` | Working (hardcoded 2014) |
| `EnglishSuppressMisspelledSuggestionsFilter` | Stub (reads args) |
| All other filters (10+) | Stub (return true) |

**Native rules (4):**

| Rule | Category | Tests |
|------|----------|-------|
| `WordRepeatRule` | MISC/Redundancy | 5 |
| `DoublePunctuationRule` | PUNCTUATION | 3 |
| `UppercaseSentenceStartRule` | CASING/Capitalization | 5 |
| `CommaWhitespaceRule` | TYPOGRAPHY/Whitespace | 4 |

**Text-level rules (6):**

| Rule | Category | Tests |
|------|----------|-------|
| `MultipleWhitespaceRule` | TYPOGRAPHY | 3 |
| `SentenceWhitespaceRule` | TYPOGRAPHY | 3 |
| `GenericUnpairedBracketsRule` | PUNCTUATION | 4 |
| `GenericUnpairedQuotesRule` | PUNCTUATION | 3 |
| `LongSentenceRule` | STYLE | 1 |
| `WordRepeatBeginningRule` | STYLE | 2 |

**Gaps:**
- Date filters use hardcoded 2014 date
- 5 dead helper functions in text_level_rules.rs
- No `<unify>`, no `<and>`/`<or>` inside exceptions
- Many Java filters are still stubs

**Fixed in this session:**
- `default_on` now checked before matching (rules with `default_on=false` skipped unless picky or explicitly enabled)
- `sub_id` propagated to RuleMatch
- `url` propagated to RuleMatch
- `short_message` propagated to RuleMatch
- `issue_type` propagated to RuleMatch (fallback chain: rule → category → group → "grammar")

### og-tokenizer

**Sentence splitter:** Working with 200+ hardcoded abbreviations, punctuation/whitespace heuristics, quotation/parenthesis skipping, end-of-text handling, word boundary matching.

**Word tokenizer:** Working with contraction splitting (20+ contraction stems, 6 suffix patterns, 3 prefix patterns), character-class tokenization, byte-level offset preservation.

**Abbreviation coverage (200+ vs LT's hundreds):**

| Category | OG Count | LT Equivalent |
|----------|----------|---------------|
| Titles | 15+ | 15+ |
| Business | 10+ | 10+ |
| Latin | 15+ | 15+ |
| Months | 12 | 12 |
| Days | 7 | 7 |
| Academic degrees | 10+ | 10+ |
| Publication abbreviations | 10+ | 30+ |
| State/city abbreviations | 20+ | 50+ |
| Word-stem abbreviations | 100+ | 200+ |
| URL patterns | 0 | regex-based |

**Critical gaps:**
- No SRX file loading — LT's sentence splitting is driven by ~50 SRX regex rules
- No URL/email detection as atomic tokens
- No hyphenated word handling (always splits on hyphens)
- No numbered list detection
- No non-breaking space / Unicode ellipsis handling

### og-tagger

**EnglishTagger capabilities:**

| Feature | Status |
|---------|--------|
| Built-in POS dictionary | ~2,000 words across all Penn Treebank tags |
| External FSA dictionary loading (`dict_decoded.txt`) | Working |
| `added.txt` loading | Working |
| `uncountable.txt` / `partlycountable.txt` loading | Working |
| Heuristic unknown-word tagging | Working (15+ suffix rules) |
| Irregular verb lemmatization | Working (~200 forms) |
| Irregular adjective lemmatization | Working (better/best, worse/worst, etc.) |
| Morphological lemma guessing | Working (NNS, VBZ, VBD, VBG, JJR, JJS suffix stripping) |
| VBP expansion from VB | Working |
| Modal-noun disambiguation | Working |
| Penn Treebank + LT extension tags | Full coverage (CC..WRB + NN:U, ORD, SENT_START, etc.) |

**XmlDisambiguator:**
- Loads real LT English `disambiguation.xml` (17,228 lines, >100 rules)
- Supports: text/regex matching, POS constraints, or/and groups, antipatterns, markers
- Actions: setPos, replace, remove, add, filter, filterAll, ignoreSpelling
- Unify action is a stub

**Chunker:** Rule-based, produces NP/VP/PP/ADVP/ADJP/SBAR/PRT tags

**Critical gaps:**
- No statistical POS tagger (LT uses trigram HMM)
- No auto-loading of external dictionaries at `EnglishTagger::new()`
- No morphological synthesis
- Only English supported
- Unification not implemented

**Fixed in this session:**
- Tagger traits unified — `EnglishTagger` now directly implements `og_core::checker::Tagger` with full chunking logic (POS heuristic selection + IOB chunk tag assignment). Bridge adapter eliminated.

### og-spell

| Feature | Status |
|---------|--------|
| Dictionary (HashSet) | Working — plain word lists, case-insensitive |
| `from_file()` / `from_words()` | Working |
| Ignore words | Working |
| `SpellingCheckRule` (implements Rule trait) | Working |
| Suggestions | Brute-force Levenshtein, distance ≤ 3 |

**Gaps:**
- No Hunspell `.dic`/`.aff` parsing
- No compound word handling
- No word frequency ranking
- No phonetic matching (Soundex, Metaphone)
- O(N*M) suggestion performance — scans entire dictionary per word
- No language-specific spelling rules
- URL heuristic is fragile

### og-languagemodel

**Status: Stub.** Empty `LanguageModel` struct with `get_probability()` returning `0.0`. No data storage, no file loading, no probability computation. 0% of planned functionality.

### og-langs

**English engine (`LanguageEngine::english()`):**
- Loads grammar.xml, disambiguation.xml, dict_decoded.txt, added.txt, uncountable.txt, partlycountable.txt, spelling dictionaries
- Wires: EnglishTagger → XmlDisambiguator → chunking → PatternRuleEngine
- 2 native rules: `AvsAnRule`, `SimpleReplaceRule`
- 1 spelling rule: `SpellingCheckRule`

**Gaps:**
- Only English. 0 of 34+ other languages have any module code
- `style.xml` never loaded
- No abbreviation loading
- No `grammar_custom.xml` support
- No generic `create_engine(language_code)` factory
- `LanguageRegistry` is a stub (stores Language enum values, cannot create engines)
- External directory dependency (not self-contained)
- 4 generic native rules from og-rules not wired into engine

### og-test-runner

**Working:**
- XML example test runner: parses grammar.xml, runs correct/incorrect/triggers_error examples
- Marker extraction and offset verification
- Full tokenization pipeline with 28 hand-coded disambiguation rules
- Can load and run real English grammar.xml (4000+ rules)

**Gaps:**
- No XML schema validation
- No rule ID uniqueness checks
- `GoldenFixture` defined but never used (dead code)
- No golden test files exist
- Hardcoded absolute paths
- No CLI binary
- No multi-file test orchestration

### og-compat

**Working:**
- `JavaMatch` / `JavaCheckResult` types with JSON deserialization
- `compare_results()` and `compare_json_results()` fuzzy matching functions
- `ComparisonStatus` enum: Pass, OffsetMismatch, LengthMismatch, ReplacementMismatch, etc.
- `CompatReport` with pass/fail counts and text formatting
- 8 unit tests

**Gaps:**
- Cannot run Java LT (no subprocess, no HTTP client)
- Cannot run Rust OG (depends on og-api but never uses it)
- No end-to-end harness (run both → compare → report)
- `og-api` and `chrono` dependencies are unused
- `MessageMismatch` and `CategoryMismatch` declared but never checked

---

## Test Coverage Summary

| Crate | Test Functions | Coverage Quality |
|-------|---------------|------------------|
| og-core | 0 | No unit tests |
| og-api | 6 (integration) | Basic happy path only, no match-producing inputs |
| og-xml | ~15 | Parser, compiler, disambig parser |
| og-rules | ~33 | Pattern matching, native rules, text-level rules |
| og-tokenizer | ~130 | Very thorough sentence + word tokenizer tests |
| og-tagger | ~20 | English tagger, disambiguator, chunker |
| og-spell | ~4 | Basic detection, suggestions |
| og-languagemodel | 0 | No tests |
| og-langs | ~18 | Engine, avsan, simple_replace |
| og-test-runner | ~20 | XML examples, real grammar.xml regression |
| og-compat | 8 | Comparison logic |
| **Total** | **~254** | |

**Test infrastructure gaps:**
- No golden test files or fixture directories
- No CI/CD pipeline
- No clippy/rustfmt configuration
- No test data files (all inline)
- Many tests use hardcoded absolute paths

---

## Dependency Architecture

```
og-core (leaf, no internal deps)
  ↑
og-xml, og-tokenizer, og-tagger, og-spell, og-languagemodel (mid-level)
  ↑
og-rules, og-langs (composition)
  ↑
og-api, og-test-runner, og-compat (top-level integration)
```

**Key dependencies:** serde, regex, quick-xml, axum, tokio, rayon, phf (tagger only)
**Edition:** Rust 2024, MSRV 1.95
**License:** LGPL-2.1 (declared, no LICENSE file)

---

## Top 10 Blocking Issues for LT Parity

1. **No SRX sentence splitting** — 200+ abbreviations now cover most cases, but SRX regex rules would ensure full parity. Still the #1 source of edge-case sentence-boundary mismatches.

2. **No statistical POS tagger** — LT uses trigram HMM disambiguation before XML rules. OG relies entirely on dictionary + XML disambiguation, producing more ambiguous output.

3. **36 languages with zero implementation** — Only English has a module (1 of 37). The registry cannot create engines for other languages.

4. **Hunspell dictionary format not supported** — Can only load plain word lists, not real `.dic`/`.aff` dictionaries with morphological rules.

5. **No compatibility harness** — og-compat cannot invoke Java LT or Rust OG. No automated way to compare outputs.

6. **16 English native Java rules unported** — 21 of 37 Java English rule classes have Rust equivalents. Remaining: RepeatedWordsRule, regional spellers, MultitokenSpeller, confusion/ngram rules, false-friend rules, UnitConversion, WrongWordInContext. Full list in Java Rule Port section below.

7. **No Phase 0 deliverables complete** — Compatibility matrix started but JAVA_RULE_PORT_LIST and TEST_PORT_LIST need more detail.

8. ~~Date filters hardcoded to 2014~~ — Fixed. Now uses `current_date()` with civil_from_days algorithm.

9. **No `<unify>` support** — Many grammar rules use `<unify>` for variable binding across pattern tokens; this is a stub.

10. **Spellchecker is minimal** — No Hunspell parsing, no compound words, no frequency ranking, O(N*M) suggestion performance.

---

## Phase 0: Java LanguageTool Feature Compatibility Matrix

### Language Modules (37 total, 1 implemented)

| Language | Code | Module Dir | grammar.xml Rules | Status |
|----------|------|-----------|-------------------|--------|
| English | en | en/ | 1,772 | Partially working |
| German | de | de/ | — | Not started |
| French | fr | fr/ | — | Not started |
| Spanish | es | es/ | — | Not started |
| Portuguese | pt | pt/ | — | Not started |
| Dutch | nl | nl/ | — | Not started |
| Polish | pl | pl/ | — | Not started |
| Russian | ru | ru/ | — | Not started |
| Italian | it | it/ | — | Not started |
| Japanese | ja | ja/ | — | Not started |
| Chinese | zh | zh/ | — | Not started |
| Arabic | ar | ar/ | — | Not started |
| + 25 more | — | — | — | Not started |

### English Java Rules Port Status (37 total)

| Java Rule Class | Rust Equivalent | Status |
|----------------|-----------------|--------|
| AvsAnRule | `og-langs::en::AvsAnRule` | Ported |
| SimpleReplaceRule | `og-langs::en::SimpleReplaceRule` | Ported |
| EnglishWordRepeatRule | `og-rules::native_rules::WordRepeatRule` (generic) | Partial |
| EnglishRepeatedWordsRule | — | Not started (picky mode, needs synonyms.txt + synthesizer) |
| CompoundRule | `og-langs::en::CompoundRule` | Ported |
| ConsistentApostrophesRule | `og-langs::en::ConsistentApostrophesRule` | Ported |
| EnglishDashRule | `og-langs::en::DashRule` | Ported |
| EnglishSpecificCaseRule | `og-langs::en::SpecificCaseRule` | Ported |
| EnglishUnpairedBracketsRule | `og-rules::text_level_rules::GenericUnpairedBracketsRule` (generic) | Partial |
| EnglishUnpairedQuotesRule | `og-rules::text_level_rules::GenericUnpairedQuotesRule` (generic) | Partial |
| WordCoherencyRule | `og-langs::en::WordCoherencyRule` | Ported |
| MorfologikAmericanSpellerRule | `og-spell::SpellingCheckRule` (generic) | Partial |
| MorfologikBritishSpellerRule | — | Not started |
| + 6 regional spellers | — | Not started |
| EnglishMultitokenSpeller | — | Not started |
| ContractionSpellingRule | `og-langs::en::ContractionSpellingRule` | Ported |
| EnglishDiacriticsRule | `og-langs::en::DiacriticsRule` | Ported |
| LongSentenceRule | `og-rules::text_level_rules::LongSentenceRule` (generic) | Ported |
| StyleTooOftenUsedAdjectiveRule | `og-langs::en::StyleFrequencyRule::adjective()` | Ported |
| StyleTooOftenUsedNounRule | `og-langs::en::StyleFrequencyRule::noun()` | Ported |
| StyleTooOftenUsedVerbRule | `og-langs::en::StyleFrequencyRule::verb()` | Ported |
| EnglishPlainEnglishRule | `og-langs::en::PlainEnglishRule` | Ported |
| EnglishRedundancyRule | `og-langs::en::SimpleReplaceRule::english_redundancies()` | Ported |
| UnitConversionRule | — | Not started |
| EnglishConfusionProbabilityRule | — | Not started |
| EnglishNgramProbabilityRule | — | Not started (needs og-languagemodel) |
| UpperCaseNgramRule | — | Not started (needs og-languagemodel) |
| EnglishWrongWordInContextRule | — | Not started |
| EnglishForGermansFalseFriendRule | — | Not started |
| EnglishForFrenchFalseFriendRule | — | Not started |
| EnglishForDutchmenFalseFriendRule | — | Not started |
| EnglishForSpaniardsFalseFriendRule | — | Not started |
| EnglishForL2SpeakersFalseFriendRule | — | Not started |
| AmericanReplaceRule | `og-langs::en::SimpleReplaceRule::american_replace()` | Ported |
| BritishReplaceRule | `og-langs::en::SimpleReplaceRule::british_replace()` | Ported |
| NewZealandReplaceRule | `og-langs::en::SimpleReplaceRule::new_zealand_replace()` | Ported |
| SimpleReplaceProfanityRule | `og-langs::en::SimpleReplaceRule::english_profanity()` | Ported |

### Core Pipeline Feature Parity

| Feature | Java LT | Rust OG | Status |
|---------|---------|---------|--------|
| Sentence splitting | SRX regex rules (50+ patterns) | Hardcoded 200+ abbreviations + heuristics | ~60% |
| Word tokenization | SRX + language-specific rules | Contraction splitting + character classes | ~70% |
| POS tagging | Morfologik dictionary + HMM | Dictionary + heuristic suffix rules | ~50% |
| Disambiguation | XML rules (761 rules, 1062 actions) | XML rules (loaded + executed) | ~65% |
| Chunking | OpenNLP IOB chunker | Rule-based IOB chunker | ~70% |
| Spellchecking | Morfologik + Hunspell + SymSpell | Morfologik 259K dict + Levenshtein | ~50% |
| Pattern rules | Full XML grammar.xml (1,772 rules) | XML loaded + most features work | ~80% |
| Native rules | 37 Java classes | 4 generic + 11 English (15 total) | ~40% |
| Language model | N-gram probabilities | Stub (returns 0.0) | ~0% |
| API | 8 endpoints | 3 endpoints | ~40% |

### API Endpoint Parity

| Endpoint | Java LT | Rust OG | Status |
|----------|---------|---------|--------|
| `POST/GET /v2/check` | Full | Working (missing some fields) | ~85% |
| `GET /v2/languages` | Full | Working | ~90% |
| `GET /v2/version` | — | Working | Done |
| `GET /v2/maxtextlength` | Yes | Missing | Not started |
| `GET /v2/configinfo` | Yes | Missing | Not started |
| `GET /v2/info` | Yes | Missing | Not started |
| `GET/POST/DELETE /v2/words` | Yes | Missing | Not started |
| `GET /v2/users/me` | Yes | Missing | Not started |

---

## Audit Update (Session 2 — 2026-05-27)

### New rules ported (3 additional English native rules):

| Rule | Type | Data File | Status |
|------|------|-----------|--------|
| ContractionSpellingRule | Sentence-level | contractions.txt (179 entries) | Ported |
| ConsistentApostrophesRule | Text-level | None | Ported |
| SpecificCaseRule | Sentence-level | specific_case.txt (5,539 entries) | Ported |

### Rules wired into English engine (now 12 total):

Sentence-level rules: AvsAnRule, SimpleReplaceRule, WordRepeatRule, DoublePunctuationRule, UppercaseSentenceStartRule, CommaWhitespaceRule, ContractionSpellingRule, SpecificCaseRule, SpellingCheckRule

Text-level rules: MultipleWhitespaceRule, SentenceWhitespaceRule, GenericUnpairedBracketsRule, GenericUnpairedQuotesRule, LongSentenceRule, WordRepeatBeginningRule, ConsistentApostrophesRule

### Other fixes:
- Date filters now use actual current date (not hardcoded 2014)
- English native rule port count: 5 of 37 (up from 2)

---

## Audit Update (Session 2 continued — 2026-05-27)

### Additional rules ported (3 more):

| Rule | Type | Data File | Status |
|------|------|-----------|--------|
| SpecificCaseRule | Sentence-level | specific_case.txt (5,539 entries) | Ported |
| DiacriticsRule | Sentence-level | diacritics.txt (1,415 entries) | Ported |

### Filters implemented (3 with actual logic):

| Filter | Status |
|--------|--------|
| DateRangeChecker | Implemented (checks date range validity) |
| RegexAntiPatternFilter | Implemented (rejects matches overlapping antipattern regex) |
| ApostropheTypeFilter | Implemented (checks typographic vs typewriter apostrophe) |

### API improvements:
- Added `/v2/info` and `/v2/maxtextlength` endpoints
- Enforced max text length (100,000 chars) on `/v2/check`

### Core type improvements:
- `is_sentence_start()`, `is_sentence_end()` on AnalyzedTokenReadings
- `all_pos_tags()`, `all_lemmas()` helper methods

### Spellchecker improvements:
- Case preservation in suggestions (capitalized, ALL CAPS)
- Better URL/email/digit word skipping

### English engine now has 16 rules total:
- 10 sentence-level: AvsAnRule, SimpleReplaceRule, WordRepeatRule, DoublePunctuationRule, UppercaseSentenceStartRule, CommaWhitespaceRule, ContractionSpellingRule, SpecificCaseRule, DiacriticsRule, SpellingCheckRule
- 7 text-level: MultipleWhitespaceRule, SentenceWhitespaceRule, GenericUnpairedBracketsRule, GenericUnpairedQuotesRule, LongSentenceRule, WordRepeatBeginningRule, ConsistentApostrophesRule

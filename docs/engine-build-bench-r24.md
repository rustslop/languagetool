# Engine build/lookup bench — round 24

One command, fresh process per language (cold numbers are only honest in a fresh process;
`shared_dictionary` is a Weak cache and `shared_for_code` leaks a static engine):

```sh
cargo build --release -p og-test-runner --example bench_engine_build
for L in de nl sk eo en; do target/release/examples/bench_engine_build $L; done          # cold/warm build + checks
for L in de nl sk eo en; do target/release/examples/bench_engine_build --phases $L; done # per-phase attribution
OG_ENGINE_PROFILE=1 target/release/examples/bench_engine_build de                        # de stage laps (engine.rs german())
```

The example waits for `MemAvailable >= 3500 MiB` (2500 in `--phases`) before starting: this
7.7 GiB no-swap box is shared with a concurrent session whose battery runs make big-language
benches marginal otherwise. Peak RSS is `VmHWM`. Numbers below were taken Oct 3 2026 on that
shared box; wall-clock varies roughly ±40% with concurrent load, RSS is stable.

## What was measured before any fix (same session, pre-b0467b3 binary)

| lang | cold build | warm rebuild | peak RSS |
|------|-----------|--------------|----------|
| nl   | 31.8–40.5 s | 1.5–1.6 s | 2.26–2.53 GiB |
| sk   | 5.9–7.5 s  | 14–25 ms    | 550 MiB |
| eo   | 72–319 ms  | 48–154 ms   | 98–113 MiB |
| en   | 9.4–12.0 s | 3.8–5.9 s   | 2.2 GiB |
| de   | 13.8–17.1 s| 2.4 s       | 2.6 GiB (3 engines alive OOM-killed under concurrent load) |

## Root cause found and fixed (b0467b3)

`LanguageEngine::for_code` gates its speller block on `!dict.is_empty()`
(crates/og-langs/src/engine.rs:874). `Dictionary::is_empty` routed through
`MorfologikView::len` → `MorfologikDictionary::len` → `materialized()`, so every cold build
of every `for_code` language fully materialized its morfologik FSA into interned hash maps —
for nl that is the 13,416,746-byte `dutch.dict`. The fix answers emptiness structurally
(`Fsa::has_sequence`: V5/C1 root node has an arc, C2 root != 0), preferring already-
materialized entries when the OnceLock is set so the two answers can never disagree.
Lookups were already seek-based (`lookup_uncached` + per-word cache) — the lazy-parse
contract of the `.info` format is intact; only the spurious full materialization is gone.

| lang | cold build after | peak RSS after | suite parity (this session) |
|------|-----------------|----------------|------------------------------|
| nl   | 977 ms (32–40x) | 268 MiB build / 535 MiB total | nl 3212/3212 |
| sk   | 56 ms (~120x)   | 90 MiB         | sk 315/315 |
| en   | 4.65 s (~2x)    | 2.2 GiB        | (en suite unchanged by construction; en path also passes a shared dict) |
| da   | –               | –              | da 22-failure artifact count stable |
| el   | –               | –              | el 3-failure artifact count stable |
| eo   | 80 ms (unchanged — rule-based tagger, no morfologik dict) | 98 MiB | eo suite unchanged |

Post-fix phase attribution (`--phases`): nl = dict load 40 ms + tagger 128 ms + grammar 273 ms
+ style 154 ms + disambig 222 ms; sk = dict 8 ms + synth 7 ms + grammar 16 ms;
eo = tagger 11 ms + grammar 58 ms + disambig 16 ms. Nothing left in any `for_code`
cold path exceeds ~300 ms.

## Sized residuals (not applied this round)

1. **de cold 13.8 s: `speller_dict+support+speller_rules = 11.1 s`** (OG_ENGINE_PROFILE lap).
   The hunspell aff+dic expansion of ~500k words. Cached per process since ac353bd, so
   production (`shared_for_code`, og-api) pays it once; Java pays an equivalent one-time
   hunspell init. No further action proposed.
2. **de warm rebuild 2.4 s = `tagger+disambig 1.5 s + xml_rule_loads 0.8 s`**: `german()`
   re-runs `XmlDisambiguator::from_xml` on every call (engine.rs:426-432) and recompiles
   grammar.xml/style.xml (~4716+512 rules). A `OnceLock` around the compiled de
   disambiguator mirrors the speller cache from ac353bd and is a ~2-line change — but it
   edits engine.rs, which a concurrent session is actively modifying, so it is deferred.
3. **en cold 4.65 s / warm 3.8 s**: EnglishTagger 870 ms + spelling dict 428 ms +
   grammar compile 1.26 s + disambig compile 0.9 s (both re-run per build) + per-engine
   owned word set. Same sharing treatment as (2) applies when engine.rs is next open.
4. **en/de steady check ~30 ms/sentence**: the ngram rules dominate (identical in both);
   not a build-cost item.

`de_build_profile` (the r20 example) is superseded by this bench; it stays for history.

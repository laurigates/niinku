# niinku

A living, regenerated word dictionary for colloquial and slang Finnish, plus a
HeliBoard mechanism to consume it without manual re-imports.

> *niinku* — the puhekieli filler-and-discourse marker. Used here as a stand-in
> for everything `main_fi` doesn't cover.

## Status

MVP. The pipeline crate's pure functions (merge, denylist, min-count,
Voikko-based kirjakieli filter, log-scoring, `.combined` header + body
emit) are implemented and unit-tested. The `assemble` CLI is wired
end-to-end with the curated seed list and the OpenSubtitles, Urbaani,
and Mastodon adapters;
`ingest mastodon` pulls fresh Finnish posts from public hashtag
streams; `compile` shells out to `dicttool_aosp.jar` to produce a
HeliBoard-loadable `.dict`. Only the Suomi24 adapter remains stubbed.

## Quick start

Prerequisites:

- A stable Rust toolchain (the workspace targets `edition = 2021`).
- `just`.
- `libvoikko` and the Finnish morphology dictionary:
  - macOS: `brew install libvoikko`
  - Debian/Ubuntu: `sudo apt-get install libvoikko-dev voikko-fi`
- Java (any JDK 11+) for the `.dict` compile step.

```
just test          # cargo test --workspace
just lint          # fmt --check + clippy -D warnings
just ci            # lint + test
just build         # cargo build --release --workspace
just download      # fetch OpenSubtitles Finnish frequency list into data/cached/
just download-jar  # fetch dicttool_aosp.jar into tools/
just generate      # download corpus + jar, assemble, compile → data/out/puhekieli_fi.dict
just assemble      # run Stage B with custom flags (see `niinku assemble --help`)
just compile       # run dicttool on data/out/niinku.combined
just ingest-mastodon # pull Finnish hashtag posts → data/cached/mastodon-fi.txt
just ingest        # pass through to the CLI (e.g. `just ingest mastodon --tags suomi`)
```

The `.dict` file ships at `data/out/puhekieli_fi.dict`. To use it:

1. Transfer to an Android device that has HeliBoard installed.
2. Settings → Languages → Finnish → "Add dictionary from file" → select the `.dict`.

The `dictionary=puhekieli:fi` header field declares this as an
*additional* (non-`main`) Finnish dictionary, so HeliBoard loads it
alongside its built-in `main_fi` rather than replacing it.

## Background

HeliBoard's Finnish dictionaries (`main_fi`, AOSP-format) are kirjakieli only —
derived from written-Finnish frequency corpora. Finnish IM and social writing
is puhekieli (`mä`, `oon`, `tää`, `niinku`, `sun`) and fast-moving internet
slang. These forms are absent from any loaded dictionary, so word suggestion
and — more importantly — glide typing fail for them. HeliBoard supports
multiple dictionaries per locale, so the fix is additive: ship a separate
puhekieli dictionary alongside the kirjakieli one.

Slang evolves quickly, so the dictionary must be regenerated on a cadence
rather than authored once.

## Goals

- An automated, iterable pipeline that produces a HeliBoard-compatible
  `.dict` file for colloquial Finnish from one or more sources.
- Iteration without re-engineering: adding a source or tuning the filter
  should be a small, local change.
- A HeliBoard path (feature request, fork as fallback) to load dictionaries
  from a URL, cache them, and optionally auto-update.

## Non-goals

- Replacing the kirjakieli dictionary. The output supplements it.
- Shipping on F-Droid as part of HeliBoard proper (separate concern; not
  blocking).
- Perfect coverage. A curated, well-ranked subset beats an exhaustive noisy
  one.

---

## Workstream A — Dictionary generation pipeline (Rust)

Core implementation in Rust: fast, single static binary, TDD-friendly, matches
the maintainer's stack. `dicttool_aosp.jar` (Java) is invoked as a subprocess
for the final compile step only.

### Architecture — two stages

Heavy corpus crunching is separated from light scheduled regeneration so CI
runs stay cheap.

**Stage A — ingest (heavy, cached or manual).** Each source is processed by
an adapter into a normalized per-source frequency table (`token`, `count`).
Large corpora (Suomi24, OpenSubtitles) are crunched rarely; their tables are
cached via `actions/cache` keyed on corpus version, or committed compressed.
Not re-run per build.

**Stage B — assemble (light, scheduled).** Live sources are fetched fresh,
merged with cached tables, filtered, scored, and emitted. This is the
cron-driven job.

### Sources (ranked by CI-friendliness)

- **Curated seed list** (`data/curated-fi.txt`) — hand-vetted puhekieli
  forms committed to the repo: pronoun/verb contractions (`mä`, `oon`),
  reduced `-ks` question forms (`oliks`, `saaks`), and clitic chains
  (`miksköhän`, `saakohan`). Unlike the fetched corpora it is always
  present, so these forms are guaranteed in the output; assemble folds
  every curated token into the allowlist so they survive the Voikko
  filter even when libvoikko accepts the form. Grow it via PR.
- **Urbaani Sanakirja** — curated slang headwords. Extract headwords only,
  not definitions.
- **Mastodon public timeline** — Finnish-language posts via API, no auth.
  Freshest slang signal.
- **OpenSubtitles Finnish frequency** — conversational register, openly
  available.
- **Suomi24 corpus (Kielipankki)** — strongest puhekieli mass; access has
  academic licensing. A *derived frequency table* is redistributable even
  where the source text is not — consume it, publish only counts.

Each source is an adapter behind a common trait, so sources can be added or
dropped independently.

### Filter — the core logic

The discriminator is **frequency × morphological rejection**:

- A token that is **frequent in a colloquial corpus** but **rejected by
  libvoikko** is almost certainly puhekieli or slang — keep it.
- A token Voikko **accepts** is kirjakieli, already covered by `main_fi` —
  drop it.
- Use `voikko-rs` to keep the whole pipeline in one language.

The frequency floor removes typos and one-off noise. A repo-managed
`denylist.txt` / `allowlist.txt` handles the residual, applied at assembly
and evolved via PR.

### Scoring

Map log-frequency to HeliBoard's `f` value (0–255). Tier the output:

- puhekieli function words (pronouns, verb contractions) — high, `f ≈ 200+`
- slang content words — lower, `f ≈ 100–170`, so they surface without
  drowning real words

`bigram=` next-word data is a later quality improvement; the format supports
it.

### Output

- Emit a `.combined` wordlist (plain text, one ` word=X,f=N` line per entry).
- Compile to `.dict` via `dicttool_aosp.jar makedict`.
- Verify the exact `.combined` header fields and the dictionary-type prefix
  expected for an additional (non-`main`) dictionary against the HeliBoard
  wiki before finalizing — this is an open detail (see Open questions).

### CI/CD — GitHub Actions

- Scheduled `cron` (monthly suggested) runs Stage B.
- The job does **not** auto-publish. It opens a PR containing the regenerated
  wordlist plus a diff of new-vs-removed entries. A human merges.
- On merge, the compiled `.dict` is published as a GitHub Release artifact at
  a stable URL (consumed by Workstream B).

### Repo layout

```
/crates
  /pipeline      # merge, filter, score, emit — core, fully tested
  /sources       # one adapter module per source
/data
  /cached        # committed/cached frequency tables (Stage A output)
  curated-fi.txt # hand-vetted puhekieli seed list (always-on source)
  allowlist.txt
  denylist.txt
/.github/workflows
  ingest.yml     # Stage A, manual / version-triggered
  assemble.yml   # Stage B, cron + PR
  ci.yml         # fmt, clippy, test
/tools
  dicttool_aosp.jar   # not yet vendored
```

TDD throughout: the filter and scoring logic are pure functions over
frequency tables and must be unit-tested.

---

## Workstream B — HeliBoard URL dictionaries

Today a user-added dictionary is a one-off file import; HeliBoard does not
refresh it. With a monthly-regenerated dictionary this means a manual
re-import every cycle.

### Step 1 — feature request upstream

Open an issue on HeliBoard proposing: load a dictionary from a URL, cache it
locally, and optionally auto-update — either on a cadence or when a version
change is detected. Frame it generically (any locale benefits), with this
project as the motivating use case. Cheapest path if accepted.

### Step 2 — fork as fallback

If upstream declines, fork. Scope:

- Settings entry: add a dictionary by URL per locale.
- Cache the fetched `.dict` in app storage.
- Update trigger: cadence (configurable) or version detection — compare a
  version field (HTTP `ETag`/`Last-Modified`, or a version in the `.combined`
  header) before downloading.
- Offline-safe: keep the last good cached copy on fetch failure.

Keep the fork minimal and rebaseable on upstream to ease an eventual merge.

The Release artifact URL from Workstream A is the input here, so the two
workstreams meet at a stable, versioned URL.

---

## MVP

Prove the pipeline end to end before breadth:

1. One corpus (OpenSubtitles Finnish) + Urbaani Sanakirja headwords.
2. Voikko-rejection filter + manual `denylist.txt`.
3. `.combined` → `.dict` compile.
4. Monthly cron → PR → Release artifact.
5. Manual import into HeliBoard to validate suggestions and glide typing.

Then: add Mastodon, the new-vs-removed diff in the PR, bigram data, and begin
Workstream B.

## Privacy & licensing

- Discard raw post/corpus text immediately after counting. Persist only
  aggregate frequencies. This sidesteps most licensing friction and is the
  correct default.
- Headword lists and derived frequency tables are generally redistributable
  even where source text is not; definitions and full corpus text are not.
  Keep the distinction explicit per source.

## Open questions

- Exact `.combined` header and dictionary-type prefix for an additional
  Finnish dictionary loaded alongside `main_fi` — verify against the
  HeliBoard wiki.
- Suomi24 / Kielipankki access terms and what specifically may be
  redistributed.
- Mastodon Finnish-language detection: rely on the post `language` field, or
  run detection locally.
- Whether puhekieli and internet slang should be one dictionary or two (lets
  the user enable them independently).
- Update cadence — monthly is a starting assumption; slang half-life may
  argue for shorter.

## License

MIT.

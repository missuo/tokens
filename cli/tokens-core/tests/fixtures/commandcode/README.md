# Command Code transcript compatibility

The parser supports native **v1, v2, and v3** transcripts. v1/v2 flat records
retain per-turn text estimation; v3 wrapped messages use recorded usage when
available. v3 was introduced in stable Command Code 1.0.0, immediately after
0.52.5's v2 format. CLI package versions and transcript schema versions are
different: `version:3` is not Command Code 3.x.

| Released writer | Fixture | Parser expectation |
| --- | --- | --- |
| 1.0.0, 1.20.0 | `<version>.jsonl` | Recorded usage/model/cost; estimate only messages lacking usage |
| 1.50.0, 1.52.0, 1.53.0 | `<version>.jsonl` | Same, with `cacheWriteTokens1h` already included in total cache writes |
| 0.17.18 | `0.17.18.jsonl` | Native v1: estimated tokens, config model fallback, original session ID |
| 0.50.0, 0.52.5 | `<version>.jsonl` | Native v2: same behavior; metadata.version = 2 |
| 0.52.5 migrated by 1.0.0 | `0.52.5-migrated-by-1.0.0.jsonl` | Content/model retained; absent historical usage is estimated |

These are sampled release **writer/reader compatibility tests**, not live API
runs for every version or a guarantee about future releases. Only 1.52.0 was
also exercised with a live model request; its captured usage is tested in the
report-level test in `src/lib.rs`. That print-mode run left an empty transcript,
so the live request did not verify persistence.

## Fixture provenance

Synthetic messages were passed to functions extracted from the published npm
bundles. No personal transcripts are included. Historical packages were
unpacked, not installed or started as CLIs. Sources are the versioned npm
archives at:

`https://registry.npmjs.org/command-code/-/command-code-<version>.tgz`

For v3, the executed functions were `createSessionStoreV3`,
`createSessionRecorder`, and `toSessionUsage`. Only their dependencies were
injected: a fixed clock and IDs, filesystem writes to the fixture directory,
an empty initial session context, and a fixed cost estimator ($0.25 or $0).
The writer/recorder performed message wrapping, usage association, model-change
serialization, and flushing. Model names are synthetic test inputs, not claims
about historical model availability. `cacheWriteTokens1h:100` was supplied to
all v3 writers; older writers omit it and newer writers preserve it as a subset
of `cacheWriteTokens:200`, not an additional token bucket.

Legacy fixtures execute `SessionManager.saveMessages` (0.17.18) or
`SessionManager.writeMessages` (0.50.0/0.52.5), with synthetic state and an
already-created output directory. The migrated fixture executes 1.0.0's
migration functions on the 0.52.5 fixture, with a synthetic sidecar model.
Migration does not recover token usage that the original writer never stored.
Tokens reads both native legacy and migrated transcripts without migrating or
rewriting them. The report test includes native v1/v2 and v3 files together to
verify historical estimates are not dropped and are priced using config.
Unknown version markers and malformed v3 records are not reinterpreted as
legacy messages.

The 1.0.0 and 1.20.0 embedded cost estimators differ from 1.50.0 onward: the
older implementations do not subtract cache buckets before pricing input.
These fixtures deliberately do not validate historical API billing or those
estimators; they test serialization and preservation of supplied cost. The
parser preserves existing embedded estimates rather than retroactively
recalculating historical costs.

Bundle SHA-256 (`dist/cli.mjs`, except `dist/index.mjs` for 0.17.18):

| Version | SHA-256 |
| --- | --- |
| 0.17.18 | `33bde5a79bd7d0faf45aa4a72802e82735df560d4dc4710ce36045ef5c90fcc4` |
| 0.50.0 | `076b958907fe0763c7cc91337d5503e917c7af908a35a40943b31ae9d1510cc6` |
| 0.52.5 | `27f87ce11f4f29e3284307b7e1bb2afcad6c49ed7d6ee2f7b155a28b9d2e0802` |
| 1.0.0 | `5f15c81c2547606f2ec51cbaca992ff49323cd35f9d379e1f2ff680cc9a132e4` |
| 1.20.0 | `aa5f3e9e32072499018c76c0c71a1162f91e1efb059546e042f381708a508051` |
| 1.50.0 | `429e94e9c9a2548526a352ac551cf7ca74f58d3068009c8a01b96d8dd1a7c11f` |
| 1.52.0 | `9cc35f147b3b845a48e0d1f5b93d663e4fabc30db4f8a0e2ccd0ee578bacf395` |
| 1.53.0 | `b4f5398190f2912d90cecb3c558017f9e0c2c97d55ccf6b66848eed910642859` |

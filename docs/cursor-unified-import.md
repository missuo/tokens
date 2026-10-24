# Unified Cursor usage import

After an account is merged into the unified Grok billing system, the legacy
`cursor.com/api/dashboard/get-filtered-usage-events` endpoint can still return
historical usage successfully without returning post-merge usage. Reauthorizing
that same legacy endpoint does not move the newer billing events into it.

`tokens cursor import` reads the official CSV exported from
[Grok Usage](https://grok.com/dashboard/usage). It requires no credentials or
network access and does not submit data. This is an explicit import workflow;
`tokens cursor sync` continues to use the legacy endpoint.

## Import

1. Export the date range you need from the unified dashboard.
2. Determine the last legacy event's timestamp from your Cursor cache. Use that
   timestamp, including its timezone, as `--after`. Only newer events are imported.
   The date below is an example, not an automatically detected migration date.
3. Preview, then repeat without `--dry-run` to write the supplementary cache:

```sh
tokens cursor import personal-unified-usage.csv \
  --email you@example.com \
  --after '2026-10-14T12:00:00Z' \
  --output-dir "$HOME/.config/tokens/cursor-cache" \
  --dry-run
```

Then inspect the combined usage before submitting:

```sh
tokens submit --client cursor --dry-run
tokens submit --client cursor
```

The importer rejects legacy caches in the output directory that contain events
after the selected cutoff. This is a conservative check across all accounts in
that directory. If another account has newer legacy usage, use a separate
directory and configure it with `TOKENS_EXTRA_DIRS=cursor:/absolute/path` when
submitting, after verifying that the selected account's legacy usage ends at
the chosen cutoff. Do not scan two copies of the same import.

## Data handling

- Only rows with `Product=cursor` and `Charge Category=usage` are imported.
  Other Grok products are excluded. All Cursor usage rows must match the
  expected email and a single user principal; team exports are unsupported.
- The four token dimensions are preserved separately: prompt, completion,
  cached prompt, and cache write. Missing dimensions are zero. Unknown
  dimensions, invalid counts, unsupported currencies, and malformed rows fail
  the import before changing the usage cache.
- `Pricing Contracted` in USD becomes provider-reported **actual billed cost**.
  Included usage can therefore have zero cost. This differs from the legacy
  API's metered `tokenUsage.totalCents`; a combined total mixes those cost bases.
  The export does not supply enough information to reconstruct metered cost.
- The export has billing item IDs but no conversation IDs. Tokens uses its
  existing per-account/day fallback for sessions. Session counts do not
  represent individual conversations.
- Imports are saved as `usage.unified-<SHA256-of-principal>.json`, separate from
  `usage.json`. The file records its source, principal, cutoff, and cost basis,
  without retaining the email or non-Cursor rows. On Unix it is written with
  mode `0600`.
- Repeated or overlapping exports for the same principal merge by Billing Item
  ID, preserving earlier imports. Identical duplicates are ignored. Conflicting
  versions of an ID or a changed cutoff fail without replacing the existing
  usage file. Keep the original CSVs for audit and any manual reconciliation.

The importer cannot match billing IDs to legacy conversation IDs. Use a cutoff
that cleanly separates the two sources, and keep one supplementary file per
principal across scanned directories. If a future sync starts returning these
same post-merge events, remove the corresponding supplementary file from the
scanned directories before submitting, or the two sources can double-count.
Previously created manual supplementary files must also be removed before
switching to this importer. Legacy account logout/purge does not manage these
independent unified imports; remove their files explicitly when needed.

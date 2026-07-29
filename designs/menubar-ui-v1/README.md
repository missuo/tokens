# Tokens Menu Bar — UI designs v1

**Status:** Implemented (Minimal Mono UI landed in Tasks 1–5)

## Final visual language

**FINAL · 06 Minimal Mono v2** is locked for this version:

- Mono / Swiss receipt type (`IBM Plex Mono` feel)
- Breakdown as 4-up cards (from 04, mono accents)
- Sections separated by **spacing only** (no hairlines)
- **Cost · 14 days** bar chart (Y = $, X = date)

Shot: `full-final.png` / `full-06.png` / `full-06-light.png`

## Interaction frames

| ID | Name | Spec |
|----|------|------|
| **IX-A** | Day chart hover | Active bar full opacity + outline; siblings ~28%; vertical guide; tooltip date / cost / tokens |
| **IX-B** | Long list scroll | Fixed viewport; top/bottom fades; 3px mono thumb; mid-scroll state; “SCROLL · n / total” |
| **IX-C** | Settings panel | 420-wide mono form: Display segmented, Interval select, Full Rescan, CLI path, Recheck / Done |
| **IX-D** | Settings in context | Sheet over dimmed main panel (opened from footer **Settings…**) |

Shots: `full-ix-hover.png`, `full-ix-scroll.png`, `full-ix-settings.png`, `full-ix-settings-ctx.png`

## Preview

```bash
python3 -m http.server 4311 --directory designs
# http://localhost:4311/menubar-ui-v1/
```

Tabs: **Final** · **Interactions** · **Archive 01–05** · **All**

## Archive

01 Native Glass · 02 Terminal · 03 Dense Dashboard · 04 Brand Neon · 05 Minimal Mono

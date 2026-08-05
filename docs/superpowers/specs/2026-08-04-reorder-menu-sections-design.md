# Reorder Menu Sections Design

**Date:** 2026-08-04

## Goal

Reorder the menu bar report sections so cost information appears before client usage and project usage is the final business section.

## Final Order

1. TOTAL
2. BREAKDOWN
3. COST
4. CLIENT
5. MODEL
6. PROJECT
7. Optional error banner
8. Fixed footer

PROJECT is the final normal data section. A transient error banner may still appear below it, and the footer remains fixed outside the scrolling report content.

## Approach

Use the existing section components and change only their composition order in the report body. Do not alter section internals, report data, pagination, formatting, panel sizing, or footer behavior.

Update the existing design and implementation documentation so the documented order matches the application.

## Data and Error Handling

The change does not modify data loading or transformation. Each section continues receiving the same report data as before. Existing empty-state, pagination, and error-banner behavior remains unchanged.

## Testing

- Run `make build-release` to build the repository CLI and release menu bar application together.
- Run `TOKENS_CLI="$PWD/cli/target/release/tokens" swift test` so the Swift suite explicitly exercises the repository-built CLI.
- Launch the release menu bar application and visually confirm the final order.
- Confirm COST rendering, PROJECT pagination, nested model pagination, scrolling, error-banner placement, and fixed-footer behavior remain intact.

Final verification requires the current XCTest suite to pass against the repository's integrated behavior.

## Non-Goals

- Making section order configurable.
- Refactoring section components.
- Changing report schemas or aggregation.
- Moving PROJECT below the fixed footer.
- Moving PROJECT below the optional error banner.

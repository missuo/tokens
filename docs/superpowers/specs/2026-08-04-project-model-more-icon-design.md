# Project Model More Icon Design

## Goal

Make the nested model pagination control inside each Project visually distinct from the Project section's own `More` button.

## Scope

- Replace only the nested Project model list's visible `More` text with a small downward chevron icon.
- Keep the Project section-level `More` text button unchanged.
- Preserve the existing pagination count, click behavior, layout hierarchy, and state reset behavior.
- Preserve an explicit accessibility label and remaining-item hint so the icon-only control remains understandable to assistive technology.

## Interaction and Visual Design

The nested control remains centered below the currently visible models and uses a downward chevron to communicate expansion. Its plain, secondary styling should match the existing compact menu-bar visual language without adding a background, border, or menu affordance.

The section-level Project `More` control remains text. This creates a clear distinction between “show more models in this project” and “show more projects.”

## Implementation Boundary

Use a dedicated icon pagination control for nested Project models rather than changing the shared text pagination control used by top-level sections. No data model, service, page-size, or persistence changes are required.

## Verification

- Build and run the existing Swift test suite.
- Verify the nested Project control renders as a downward chevron when more models exist.
- Verify clicking it reveals the next model page.
- Verify the Project section-level control still renders as `More` and reveals more projects.
- Verify the icon-only control retains its model-specific accessibility label and remaining-count hint.

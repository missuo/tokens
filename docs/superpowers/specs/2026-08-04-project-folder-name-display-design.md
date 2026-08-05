# Project Folder Name Display Design

## Goal

Keep project rows compact and readable by showing only the final folder name instead of a full project path. When the folder name does not fit, truncate it and reveal the complete folder name on hover.

## Scope

This change applies only to project names in the PROJECT section of the menu bar panel. It does not change project aggregation, sorting, costs, token counts, nested model rows, or the report schema.

## Display Rules

1. For an attributed project, derive the visible name from the final non-empty component of its project path.
2. Do not display the full project path in the row or hover text.
3. If the final folder name exceeds the available row width, keep it on one line and truncate the overflowing portion with an ellipsis.
4. Hovering the project name shows the complete, untruncated final folder name using the platform's standard help tooltip.
5. Preserve enough layout priority for the cost and token summary so project names cannot push those values out of the row.
6. For an unattributed project, continue to display `Unattributed` and do not expose workspace or path information.
7. If a non-nil project path has no usable final component, fall back to the existing display name; if that is also empty, use `Unattributed`. A nil project path is always treated as unattributed.

## Data Flow

The menu bar already receives both a project key and a display name. The presentation layer will derive the final folder component from the project key for attributed projects, then apply the fallback rules above. No report format or CLI contract change is required.

## Interaction and Accessibility

The hover tooltip contains only the full folder name. The accessible label should also expose the complete folder name together with the row's existing usage summary, without revealing the full path.

## Error and Edge-Case Handling

- Ignore trailing path separators when selecting the final folder component.
- Treat `/` as the path separator used by the existing macOS report data.
- Treat empty, root-only, or otherwise unusable project keys as missing.
- Keep duplicate final folder names unchanged; distinguishing same-named projects by exposing paths is outside this change.

## Testing

Add focused tests for the name-selection logic:

- standard absolute path returns its final folder name;
- trailing separator still returns the final folder name;
- a long folder name remains available in full for tooltip and accessibility use;
- missing or unusable project keys use the existing display-name fallback;
- unattributed projects remain `Unattributed` and never expose path data.

Run the existing menu bar test suite and verify the built menu bar app manually: short names remain unchanged, long names truncate without displacing metrics, and hover reveals the full folder name.

## Out of Scope

- Showing a full project path anywhere in the panel;
- adding custom tooltip styling;
- changing panel width or pagination;
- changing project identity, grouping, or sorting;
- disambiguating projects with identical final folder names.

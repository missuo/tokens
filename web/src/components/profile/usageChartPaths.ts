import type {
  CubicValueBoundary,
  CubicValueSegment,
} from "./usageChartGeometry";

/** The plot rectangle inside the chart's viewBox. */
export interface ChartFrame {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Path coordinates are emitted with two decimals. The viewBox is a few hundred
 * units wide and stretched to the page, so 0.01 units is far below a pixel —
 * while full float precision made every coordinate ~18 characters long, and a
 * profile with a long history and many models rendered megabytes of `d` data.
 */
function round(value: number): number {
  return Math.round(value * 100) / 100;
}

export function xForIndex(
  index: number,
  pointCount: number,
  frame: ChartFrame,
): number {
  if (pointCount <= 1) return frame.left + frame.width / 2;
  return frame.left + (index / (pointCount - 1)) * frame.width;
}

export function yForValue(
  value: number,
  maximum: number,
  frame: ChartFrame,
): number {
  const safeMaximum = maximum > 0 ? maximum : 1;
  const finiteValue = Number.isFinite(value) ? Math.max(0, value) : 0;
  return frame.top + frame.height - (finiteValue / safeMaximum) * frame.height;
}

function hasThickness(segment: CubicValueSegment | undefined): boolean {
  return (
    !!segment &&
    (segment.from > 0 ||
      segment.control1 > 0 ||
      segment.control2 > 0 ||
      segment.to > 0)
  );
}

/**
 * Maximal runs of consecutive segments where the layer has any thickness, as
 * inclusive [first, last] segment indices.
 *
 * Outside these runs a layer's upper and lower boundaries coincide, so its
 * area is empty and its top edge is the edge of a layer beneath it. Drawing
 * those stretches anyway is what made path size grow with
 * (models × days) instead of with the days each model was actually used.
 */
export function thicknessRuns(
  thickness: CubicValueBoundary,
): Array<[number, number]> {
  const runs: Array<[number, number]> = [];
  let start = -1;
  thickness.segments.forEach((segment, index) => {
    if (hasThickness(segment)) {
      if (start < 0) start = index;
    } else if (start >= 0) {
      runs.push([start, index - 1]);
      start = -1;
    }
  });
  if (start >= 0) runs.push([start, thickness.segments.length - 1]);
  return runs;
}

function forwardCubic(
  segment: CubicValueSegment,
  pointCount: number,
  maximum: number,
  frame: ChartFrame,
): string {
  const fromX = xForIndex(segment.index, pointCount, frame);
  const toX = xForIndex(segment.index + 1, pointCount, frame);
  const third = (toX - fromX) / 3;
  return `C ${round(fromX + third)} ${round(yForValue(segment.control1, maximum, frame))} ${round(toX - third)} ${round(yForValue(segment.control2, maximum, frame))} ${round(toX)} ${round(yForValue(segment.to, maximum, frame))}`;
}

function reverseCubic(
  segment: CubicValueSegment,
  pointCount: number,
  maximum: number,
  frame: ChartFrame,
): string {
  const fromX = xForIndex(segment.index, pointCount, frame);
  const toX = xForIndex(segment.index + 1, pointCount, frame);
  const third = (toX - fromX) / 3;
  return `C ${round(toX - third)} ${round(yForValue(segment.control2, maximum, frame))} ${round(fromX + third)} ${round(yForValue(segment.control1, maximum, frame))} ${round(fromX)} ${round(yForValue(segment.from, maximum, frame))}`;
}

/** The layer's top edge, drawn only where the layer has thickness. */
export function layerLinePath(
  upper: CubicValueBoundary,
  thickness: CubicValueBoundary,
  maximum: number,
  frame: ChartFrame,
): string {
  const pointCount = upper.values.length;
  if (pointCount === 0) return "";
  if (pointCount === 1) {
    if (!((thickness.values[0] ?? 0) > 0)) return "";
    const x = xForIndex(0, 1, frame);
    const y = round(yForValue(upper.values[0] ?? 0, maximum, frame));
    return `M ${round(x - 4)} ${y} L ${round(x + 4)} ${y}`;
  }

  return thicknessRuns(thickness)
    .map(([first, last]) => {
      const head = upper.segments[first];
      const parts = [
        `M ${round(xForIndex(first, pointCount, frame))} ${round(yForValue(head?.from ?? 0, maximum, frame))}`,
      ];
      for (let index = first; index <= last; index += 1) {
        const segment = upper.segments[index];
        if (segment) parts.push(forwardCubic(segment, pointCount, maximum, frame));
      }
      return parts.join(" ");
    })
    .join(" ");
}

/** The filled band between `lower` and `upper`, one closed subpath per run. */
export function layerAreaPath(
  lower: CubicValueBoundary,
  upper: CubicValueBoundary,
  thickness: CubicValueBoundary,
  maximum: number,
  frame: ChartFrame,
): string {
  const pointCount = upper.values.length;
  if (pointCount === 0) return "";
  if (pointCount === 1) {
    if (!((thickness.values[0] ?? 0) > 0)) return "";
    const x = xForIndex(0, 1, frame);
    const lowerY = round(yForValue(lower.values[0] ?? 0, maximum, frame));
    const upperY = round(yForValue(upper.values[0] ?? 0, maximum, frame));
    return `M ${round(x - 4)} ${lowerY} L ${round(x - 4)} ${upperY} L ${round(x + 4)} ${upperY} L ${round(x + 4)} ${lowerY} Z`;
  }

  return thicknessRuns(thickness)
    .map(([first, last]) => {
      const parts = [
        `M ${round(xForIndex(first, pointCount, frame))} ${round(yForValue(upper.segments[first]?.from ?? 0, maximum, frame))}`,
      ];
      for (let index = first; index <= last; index += 1) {
        const segment = upper.segments[index];
        if (segment) parts.push(forwardCubic(segment, pointCount, maximum, frame));
      }
      parts.push(
        `L ${round(xForIndex(last + 1, pointCount, frame))} ${round(yForValue(lower.segments[last]?.to ?? 0, maximum, frame))}`,
      );
      for (let index = last; index >= first; index -= 1) {
        const segment = lower.segments[index];
        if (segment) parts.push(reverseCubic(segment, pointCount, maximum, frame));
      }
      parts.push("Z");
      return parts.join(" ");
    })
    .join(" ");
}

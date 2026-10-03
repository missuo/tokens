/// <reference types="bun-types" />
import { describe, expect, test } from "bun:test";
import { createNonCrossingStackGeometry } from "./usageChartGeometry";
import {
  layerAreaPath,
  layerLinePath,
  thicknessRuns,
  type ChartFrame,
} from "./usageChartPaths";

const FRAME: ChartFrame = { left: 0, top: 8, width: 848, height: 218 };

function stack(series: number[][]) {
  return createNonCrossingStackGeometry(series);
}

function subpathCount(path: string): number {
  return (path.match(/M /g) ?? []).length;
}

describe("thicknessRuns", () => {
  test("splits on segments where the layer is empty", () => {
    const { layers } = stack([[0, 0, 3, 4, 0, 0, 0, 2, 0]]);
    // Segments touching a positive value count as part of the run.
    expect(thicknessRuns(layers[0]!.thickness)).toEqual([
      [1, 3],
      [6, 7],
    ]);
  });

  test("an all-zero layer has no runs", () => {
    const { layers } = stack([[0, 0, 0, 0]]);
    expect(thicknessRuns(layers[0]!.thickness)).toEqual([]);
  });
});

describe("layer paths", () => {
  test("empty layers emit no path at all", () => {
    const { layers, maximum } = stack([
      [1, 2, 3, 2],
      [0, 0, 0, 0],
    ]);
    const empty = layers[1]!;
    expect(
      layerAreaPath(empty.lower, empty.upper, empty.thickness, maximum, FRAME),
    ).toBe("");
    expect(layerLinePath(empty.upper, empty.thickness, maximum, FRAME)).toBe("");
  });

  test("a continuous layer is one closed band spanning the plot", () => {
    const { layers, maximum } = stack([[1, 2, 3, 2, 1]]);
    const layer = layers[0]!;
    const area = layerAreaPath(
      layer.lower,
      layer.upper,
      layer.thickness,
      maximum,
      FRAME,
    );
    expect(subpathCount(area)).toBe(1);
    expect(area.startsWith("M 0 ")).toBe(true);
    expect(area).toContain("L 848 ");
    expect(area.endsWith("Z")).toBe(true);
    // Upper and lower boundaries: one cubic per segment each way.
    expect((area.match(/C /g) ?? []).length).toBe(8);
  });

  test("each run of activity becomes its own subpath", () => {
    const { layers, maximum } = stack([[0, 0, 3, 4, 0, 0, 0, 2, 0]]);
    const layer = layers[0]!;
    const area = layerAreaPath(
      layer.lower,
      layer.upper,
      layer.thickness,
      maximum,
      FRAME,
    );
    const line = layerLinePath(layer.upper, layer.thickness, maximum, FRAME);
    expect(subpathCount(area)).toBe(2);
    expect((area.match(/Z/g) ?? []).length).toBe(2);
    expect(subpathCount(line)).toBe(2);
    expect((line.match(/C /g) ?? []).length).toBe(5);
  });

  test("coordinates carry at most two decimals", () => {
    const { layers, maximum } = stack([
      Array.from({ length: 366 }, (_, i) => (i * 7919) % 13),
    ]);
    const layer = layers[0]!;
    const area = layerAreaPath(
      layer.lower,
      layer.upper,
      layer.thickness,
      maximum,
      FRAME,
    );
    const numbers = area.match(/-?\d+(\.\d+)?/g) ?? [];
    expect(numbers.length).toBeGreaterThan(0);
    for (const value of numbers) {
      expect(value.split(".")[1]?.length ?? 0).toBeLessThanOrEqual(2);
    }
  });

  test("a single-point layer keeps its 8-unit stub", () => {
    const { layers, maximum } = stack([[5]]);
    const layer = layers[0]!;
    expect(layerLinePath(layer.upper, layer.thickness, maximum, FRAME)).toBe(
      "M 420 8 L 428 8",
    );
  });
});

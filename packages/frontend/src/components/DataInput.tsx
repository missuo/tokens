"use client";

import { useState, useCallback } from "react";
import type { TokenContributionData } from "@/lib/types";
import { isValidContributionData } from "@/lib/utils";

interface DataInputProps {
  onDataLoaded: (data: TokenContributionData) => void;
}

export function DataInput({ onDataLoaded }: DataInputProps) {
  const [rawJson, setRawJson] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const parseJson = useCallback(() => {
    setError(null);
    if (!rawJson.trim()) {
      setError("Please enter JSON data");
      return;
    }
    try {
      const parsed = JSON.parse(rawJson);
      if (!isValidContributionData(parsed)) {
        setError("Invalid data format. Expected TokenContributionData structure with meta, summary, years, and contributions.");
        return;
      }
      onDataLoaded(parsed);
    } catch (err) {
      setError(`Invalid JSON: ${err instanceof Error ? err.message : "Unknown error"}`);
    }
  }, [rawJson, onDataLoaded]);

  const loadSampleData = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const response = await fetch("/sample-data.json");
      if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
      const data = await response.json();
      if (!isValidContributionData(data)) throw new Error("Sample data has invalid format");
      setRawJson(JSON.stringify(data, null, 2));
      onDataLoaded(data);
    } catch (err) {
      setError(`Failed to load sample data: ${err instanceof Error ? err.message : "Unknown error"}`);
    } finally {
      setIsLoading(false);
    }
  }, [onDataLoaded]);

  return (
    <div className="w-full">
      <div className="mb-6">
        <h2 className="text-2xl font-bold tracking-tight text-foreground sm:text-[1.75rem]">
          Load Token Usage Data
        </h2>
        <p className="mt-1 text-sm text-muted">
          Paste JSON from{" "}
          <code className="rounded bg-surface-secondary px-1.5 py-0.5 font-mono text-[0.85em] text-foreground">
            tokens graph
          </code>{" "}
          command, or load sample data.
        </p>
      </div>

      <div className="mb-4">
        <textarea
          value={rawJson}
          onChange={(e) => {
            setRawJson(e.target.value);
            setError(null);
          }}
          onKeyDown={(e) => {
            if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
              e.preventDefault();
              parseJson();
            }
          }}
          placeholder='{"meta": {...}, "summary": {...}, "contributions": [...]}'
          className={`min-h-[220px] w-full resize-y rounded-lg border bg-surface-secondary px-3 py-3 font-mono text-sm text-foreground outline-none transition focus:ring-2 ${
            error
              ? "border-danger/40 focus:border-danger focus:ring-danger/25"
              : "border-line focus:border-accent focus:ring-accent/25"
          }`}
        />
        <p className="mt-2 text-sm text-muted">Tip: Press Ctrl+Enter (Cmd+Enter on Mac) to parse</p>
      </div>

      {error && (
        <div className="mb-4 rounded-lg border border-danger/40 bg-danger/10 px-4 py-3 text-sm text-danger">
          {error}
        </div>
      )}

      <div className="flex flex-wrap gap-3">
        <button
          onClick={parseJson}
          disabled={isLoading || !rawJson.trim()}
          className="inline-flex h-10 items-center justify-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-foreground transition hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-60"
        >
          Parse JSON
        </button>

        <button
          onClick={loadSampleData}
          disabled={isLoading}
          className="inline-flex h-10 items-center justify-center gap-1.5 rounded-lg border border-line bg-surface px-4 text-sm font-medium text-foreground transition hover:border-foreground/20 hover:bg-surface-secondary disabled:cursor-not-allowed disabled:opacity-60"
        >
          {isLoading ? (
            <>
              <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" className="h-4 w-4 animate-spin">
                <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" className="opacity-25" />
                <path fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" className="opacity-75" />
              </svg>
              Loading...
            </>
          ) : (
            "Load Sample Data"
          )}
        </button>
      </div>

      <div className="mt-8 rounded-xl border border-line bg-surface p-4 sm:p-6">
        <h3 className="text-sm font-semibold text-foreground">How to get your data</h3>
        <ol className="mt-3 list-decimal space-y-2 pl-5 text-sm text-muted">
          <li className="leading-relaxed">
            Install tokens:{" "}
            <code className="rounded bg-surface-secondary px-1.5 py-0.5 font-mono text-[0.85em] text-foreground">
              brew install tokens
            </code>
          </li>
          <li className="leading-relaxed">
            Run the graph command:{" "}
            <code className="rounded bg-surface-secondary px-1.5 py-0.5 font-mono text-[0.85em] text-foreground">
              tokens graph
            </code>
          </li>
          <li className="leading-relaxed">Copy the JSON output and paste it above</li>
        </ol>
      </div>
    </div>
  );
}

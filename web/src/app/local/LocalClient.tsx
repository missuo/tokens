"use client";

import { useState } from "react";
import type { TokenContributionData } from "@/lib/types";
import { DataInput } from "@/components/DataInput";
import { GraphContainer } from "@/components/GraphContainer";
import { Panel, PageHeader } from "@/components/ui/primitives";

export default function LocalClient() {
  const [data, setData] = useState<TokenContributionData | null>(null);

  return (
    <div className="min-h-screen bg-background px-4 py-8 sm:px-6">
      <div className="mx-auto max-w-[820px]">
        <PageHeader
          title="Local Viewer"
          subtitle="View your token usage data locally without submitting"
          className="mb-8"
        />

        {!data ? (
          <DataInput onDataLoaded={setData} />
        ) : (
          <div className="flex flex-col gap-6">
            <Panel className="p-4 sm:p-6">
              <div className="flex flex-wrap items-center gap-x-3 gap-y-2 text-sm">
                <span className="text-muted-foreground">Data loaded:</span>
                <span className="font-mono tabular-nums font-semibold text-foreground">
                  {data.meta.dateRange.start} - {data.meta.dateRange.end}
                </span>
                <span className="text-border">|</span>
                <span className="font-mono tabular-nums font-semibold text-primary">
                  ${data.summary.totalCost.toFixed(2)} total
                </span>
                <span className="text-border">|</span>
                <span className="text-muted-foreground">
                  <span className="font-mono tabular-nums">{data.summary.activeDays}</span> active days
                </span>
                <button
                  onClick={() => setData(null)}
                  className="ml-auto inline-flex h-10 items-center justify-center gap-1.5 rounded-lg border border-border bg-card px-4 text-sm font-medium text-foreground transition hover:border-foreground/20 hover:bg-muted"
                >
                  Load Different Data
                </button>
              </div>
            </Panel>
            <GraphContainer data={data} />
          </div>
        )}
      </div>
    </div>
  );
}

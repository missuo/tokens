"use client";

import type { DailyContribution, GraphColorPalette, ClientType } from "@/lib/types";
import { formatCurrency, formatTokenCount, groupClientsByType, sortClientsByCost } from "@/lib/utils";
import { formatContributionDateFull } from "@/lib/date-utils";
import { SOURCE_DISPLAY_NAMES, SOURCE_COLORS } from "@/lib/constants";
import { SourceLogo } from "./SourceLogo";

interface BreakdownPanelProps {
  day: DailyContribution | null;
  onClose: () => void;
  palette: GraphColorPalette;
}

export function BreakdownPanel({ day, onClose, palette }: BreakdownPanelProps) {
  if (!day) return null;

  const groupedClients = groupClientsByType(day.clients);
  const sortedClientTypes = Array.from(groupedClients.keys()).sort();

  return (
    <div role="region" aria-label="Day breakdown" className="mt-8 overflow-hidden rounded-2xl border border-line bg-surface shadow-sm transition-shadow hover:shadow-md">
      <div className="flex items-center justify-between gap-3 border-b border-line px-6 py-4 max-[480px]:items-start max-[480px]:p-4">
        <h3 className="min-w-0 flex-1 text-base font-semibold text-foreground">{formatContributionDateFull(day)} — Detailed Breakdown</h3>
        <button
          onClick={onClose}
          aria-label="Close breakdown panel"
          className="flex h-11 w-11 flex-none items-center justify-center rounded-full text-muted transition hover:scale-110 hover:bg-[var(--surface-tertiary)]"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      <div className="p-6">
        {day.clients.length === 0 ? (
          <p className="py-8 text-center text-sm font-medium text-muted">No activity on this day</p>
        ) : (
          <div className="flex flex-col gap-6">
            {sortedClientTypes.map((clientType) => {
              const clients = sortClientsByCost(groupedClients.get(clientType) || []);
              const clientTotalCost = clients.reduce((sum, c) => sum + c.cost, 0);
              return <ClientSection key={clientType} clientType={clientType} clients={clients} totalCost={clientTotalCost} palette={palette} />;
            })}
          </div>
        )}

        {day.clients.length > 0 && (
          <div className="mt-6 flex flex-wrap gap-6 border-t border-line pt-6 text-sm text-muted">
            <div className="font-medium">
              Total: <span className="font-mono text-base font-bold text-foreground tabular-nums">{formatCurrency(day.totals.cost)}</span>
            </div>
            <div className="font-medium">
              across{" "}
              <span className="font-semibold text-foreground">
                {sortedClientTypes.length} client{sortedClientTypes.length !== 1 ? "s" : ""}
              </span>
            </div>
            <div className="font-medium">
              <span className="font-semibold text-foreground">
                {(() => {
                  const allModels = new Set<string>();
                  for (const c of day.clients) {
                    if (c.models) {
                      for (const modelId of Object.keys(c.models)) allModels.add(modelId);
                    } else if (c.modelId) {
                      allModels.add(c.modelId);
                    }
                  }
                  const count = allModels.size;
                  return `${count} model${count !== 1 ? "s" : ""}`;
                })()}
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

interface ClientSectionProps {
  clientType: ClientType;
  clients: DailyContribution["clients"];
  totalCost: number;
  palette: GraphColorPalette;
}

function ClientSection({ clientType, clients, totalCost, palette }: ClientSectionProps) {
  const clientColor = SOURCE_COLORS[clientType] || palette.grade3;

  const modelEntries: Array<{ modelId: string; cost: number; messages: number; tokens: { input: number; output: number; cacheRead: number; cacheWrite: number; reasoning: number } }> = [];
  for (const client_contrib of clients) {
    if (client_contrib.models && Object.keys(client_contrib.models).length > 0) {
      for (const [modelId, data] of Object.entries(client_contrib.models)) {
        modelEntries.push({
          modelId,
          cost: data.cost || 0,
          messages: data.messages || 0,
          tokens: {
            input: data.input || 0,
            output: data.output || 0,
            cacheRead: data.cacheRead || 0,
            cacheWrite: data.cacheWrite || 0,
            reasoning: data.reasoning || 0,
          },
        });
      }
    } else if (client_contrib.modelId) {
      modelEntries.push({
        modelId: client_contrib.modelId,
        cost: client_contrib.cost,
        messages: client_contrib.messages,
        tokens: client_contrib.tokens,
      });
    }
  }

  const sortedModels = modelEntries.sort((a, b) => b.cost - a.cost);

  return (
    <div>
      <div className="mb-3 flex items-center gap-3">
        <span className="inline-flex items-center gap-2 rounded-full px-3 py-1.5 text-xs font-semibold transition hover:scale-105" style={{ backgroundColor: `${clientColor}20`, color: clientColor }}>
          <SourceLogo sourceId={clientType} height={14} />
          {SOURCE_DISPLAY_NAMES[clientType] || clientType}
        </span>
        <span className="text-sm font-bold text-foreground">{formatCurrency(totalCost)}</span>
      </div>

      <div className="ml-5 flex flex-col gap-3">
        {sortedModels.map((model, index) => (
          <ModelRow key={`${model.modelId}-${index}`} model={model} isLast={index === sortedModels.length - 1} />
        ))}
      </div>
    </div>
  );
}

interface ModelRowProps {
  model: { modelId: string; cost: number; messages: number; tokens: { input: number; output: number; cacheRead: number; cacheWrite: number; reasoning: number } };
  isLast: boolean;
}

function ModelRow({ model, isLast }: ModelRowProps) {
  const { modelId, tokens, cost, messages } = model;

  return (
    <div className="relative">
      <div className="absolute top-0 left-0 h-full w-4 text-muted">
        <span className="absolute top-3 left-0 w-3 border-t border-current opacity-20" />
        {!isLast && <span className="absolute top-0 left-0 h-full border-l border-current opacity-20" />}
      </div>

      <div className="ml-6">
        <div className="flex min-w-0 flex-wrap items-center gap-3">
          <span className="min-w-0 flex-1 truncate font-mono text-sm font-semibold text-foreground">{modelId}</span>
          <span className="flex-none font-mono text-sm font-bold text-accent tabular-nums">
            {formatCurrency(cost)}
          </span>
        </div>

        <div className="mt-2 grid grid-cols-2 gap-x-4 gap-y-2 text-xs max-[400px]:grid-cols-1 sm:grid-cols-3 md:grid-cols-5">
          {tokens.input > 0 && <TokenBadge label="Input" value={tokens.input} />}
          {tokens.output > 0 && <TokenBadge label="Output" value={tokens.output} />}
          {tokens.cacheRead > 0 && <TokenBadge label="Cache Read" value={tokens.cacheRead} />}
          {tokens.cacheWrite > 0 && <TokenBadge label="Cache Write" value={tokens.cacheWrite} />}
          {tokens.reasoning > 0 && <TokenBadge label="Reasoning" value={tokens.reasoning} />}
        </div>

        <div className="mt-2 font-mono text-xs font-medium text-muted tabular-nums">
          {messages.toLocaleString()} message{messages !== 1 ? "s" : ""}
        </div>
      </div>
    </div>
  );
}

function TokenBadge({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex min-w-0 items-center gap-1.5">
      <span className="font-medium text-muted">{label}:</span>
      <span className="font-mono font-semibold text-foreground tabular-nums">{formatTokenCount(value)}</span>
    </div>
  );
}

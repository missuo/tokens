"use client";

import { useCallback, useId, useState } from "react";
import { Button } from "@/components/ui/button";
import { BREW_INSTALL_COMMAND } from "@/lib/constants";
import type { TokenContributionData } from "@/lib/types";
import { isValidContributionData } from "@/lib/utils";
import { useT } from "@/components/layout/LocaleProvider";
import { richText } from "@/lib/i18n/rich";

interface DataInputProps {
  onDataLoaded: (data: TokenContributionData) => void;
}

/** Inline code for a command inside a translated sentence. */
function Command({ children }: { children: React.ReactNode }) {
  return (
    <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.85em] text-foreground">
      {children}
    </code>
  );
}

export function DataInput({ onDataLoaded }: DataInputProps) {
  const t = useT();
  const textareaId = useId();
  const hintId = useId();
  const errorId = useId();
  const [rawJson, setRawJson] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const parseJson = useCallback(() => {
    setError(null);
    if (!rawJson.trim()) {
      setError(t("settings.local.errors.empty"));
      return;
    }
    try {
      const parsed = JSON.parse(rawJson);
      if (!isValidContributionData(parsed)) {
        setError(t("settings.local.errors.invalidFormat"));
        return;
      }
      onDataLoaded(parsed);
    } catch (err) {
      setError(
        t("settings.local.errors.invalidJson", {
          detail: err instanceof Error ? err.message : t("settings.local.errors.unknown"),
        }),
      );
    }
  }, [rawJson, onDataLoaded, t]);

  const loadSampleData = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const response = await fetch("/sample-data.json");
      if (!response.ok) {
        throw new Error(t("settings.local.errors.http", { status: response.status }));
      }
      const data = await response.json();
      if (!isValidContributionData(data)) {
        throw new Error(t("settings.local.errors.sampleInvalid"));
      }
      setRawJson(JSON.stringify(data, null, 2));
      onDataLoaded(data);
    } catch (err) {
      setError(
        t("settings.local.errors.sampleFailed", {
          detail: err instanceof Error ? err.message : t("settings.local.errors.unknown"),
        }),
      );
    } finally {
      setIsLoading(false);
    }
  }, [onDataLoaded, t]);

  return (
    <div className="w-full">
      <div className="mb-6">
        <h2 className="text-2xl font-bold tracking-tight text-foreground sm:text-[1.75rem]">
          {t("settings.local.input.title")}
        </h2>
        <p className="mt-1 text-sm text-muted-foreground">
          {richText(t("settings.local.input.intro"), {
            command: <Command>tokens graph</Command>,
          })}
        </p>
      </div>

      <div className="mb-4">
        <label htmlFor={textareaId} className="mb-2 block text-sm font-medium text-foreground">
          {t("settings.local.input.jsonLabel")}
        </label>
        <textarea
          id={textareaId}
          aria-describedby={error ? `${errorId} ${hintId}` : hintId}
          aria-invalid={error ? true : undefined}
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
          className={`min-h-[220px] w-full resize-y rounded-lg border bg-muted px-3 py-3 font-mono text-sm text-foreground outline-none transition focus:ring-2 ${
            error
              ? "border-danger/40 focus:border-danger focus:ring-danger/25"
              : "border-border focus:border-primary focus:ring-primary/25"
          }`}
        />
        <p id={hintId} className="mt-2 text-sm text-muted-foreground">
          {t("settings.local.input.hint")}
        </p>
      </div>

      {error && (
        <div
          id={errorId}
          role="alert"
          className="mb-4 rounded-lg border border-danger/40 bg-danger/10 px-4 py-3 text-sm text-danger"
        >
          {error}
        </div>
      )}

      <div className="flex flex-wrap gap-3">
        <Button
          type="button"
          className="h-10"
          onClick={parseJson}
          disabled={isLoading || !rawJson.trim()}
        >
          {t("settings.local.input.parse")}
        </Button>

        <Button
          type="button"
          variant="outline"
          className="h-10"
          onClick={loadSampleData}
          disabled={isLoading}
        >
          {isLoading ? (
            <>
              <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" className="h-4 w-4 animate-spin">
                <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" className="opacity-25" />
                <path fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" className="opacity-75" />
              </svg>
              {t("common.loading")}
            </>
          ) : (
            t("settings.local.input.loadSample")
          )}
        </Button>
      </div>

      <div className="mt-8 rounded-xl border border-border bg-card p-4 sm:p-6">
        <h3 className="text-sm font-semibold text-foreground">{t("settings.local.input.howToTitle")}</h3>
        <ol className="mt-3 list-decimal space-y-2 pl-5 text-sm text-muted-foreground">
          <li className="leading-relaxed">
            {richText(t("settings.local.input.howToInstall"), {
              command: <Command>{BREW_INSTALL_COMMAND}</Command>,
            })}
          </li>
          <li className="leading-relaxed">
            {richText(t("settings.local.input.howToRun"), {
              command: <Command>tokens graph</Command>,
            })}
          </li>
          <li className="leading-relaxed">{t("settings.local.input.howToPaste")}</li>
        </ol>
      </div>
    </div>
  );
}

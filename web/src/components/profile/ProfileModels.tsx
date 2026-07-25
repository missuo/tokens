"use client";

import { formatCurrency, formatNumber } from "@/lib/utils";
import {
  ListBody,
  ListCaption,
  ListCard,
  ListCell,
  ListHead,
  ListHeaderCell,
  ListPrimaryCell,
  ListRow,
  ListTable,
  NumericValue,
} from "./listStyles";
import { tw } from "@/lib/tw";
import { cn } from "@/lib/utils";
import { ModelIcon } from "./ModelIcon";
import type { ModelUsage } from "./types";

export interface ProfileModelsProps {
  models: string[];
  modelUsage?: ModelUsage[];
  className?: string;
}

const ModelIdentity = tw("span", "flex min-w-0 items-center gap-2");

const ModelName = tw(
  "span",
  "overflow-hidden text-ellipsis whitespace-nowrap text-foreground max-[639px]:whitespace-normal max-[639px]:[overflow-wrap:anywhere]"
);

// ListCard already carries the border and colours; these only add the bits
// that differed from it.
const ModelsFallback = ({
  className,
  ...props
}: React.ComponentPropsWithoutRef<"div">) => (
  <ListCard {...props} className={cn("p-3", className)} />
);

const ModelsFallbackList = tw(
  "ul",
  "m-0 flex list-none flex-wrap gap-1.5 p-0"
);

const ModelTag = tw(
  "li",
  "inline-flex min-w-0 items-center gap-1.5 rounded-lg border bg-muted px-2 py-1.5 text-xs leading-tight text-foreground"
);

export function ProfileModels({
  models,
  modelUsage,
  className,
}: ProfileModelsProps) {
  const filteredUsage = (modelUsage ?? [])
    .filter((usage) => usage.model !== "<synthetic>")
    .sort((a, b) => b.cost - a.cost);
  const filteredModels = Array.from(
    new Set(models.filter((model) => model !== "<synthetic>")),
  );

  if (filteredUsage.length > 0) {
    return (
      <ListCard className={className}>
        <ListTable>
          <ListCaption>Model usage</ListCaption>
          <ListHead>
            <tr>
              <ListHeaderCell $width="52%">Model</ListHeaderCell>
              <ListHeaderCell $width="18%" $align="right">
                Tokens
              </ListHeaderCell>
              <ListHeaderCell $width="17%" $align="right">
                Cost
              </ListHeaderCell>
              <ListHeaderCell $width="13%" $align="right">
                Share
              </ListHeaderCell>
            </tr>
          </ListHead>
          <ListBody>
            {filteredUsage.map((usage) => (
              <ListRow key={usage.model}>
                <ListPrimaryCell scope="row">
                  <ModelIdentity>
                    <ModelIcon model={usage.model} size={14} />
                    <ModelName>{usage.model}</ModelName>
                  </ModelIdentity>
                </ListPrimaryCell>
                <ListCell data-label="Tokens" $align="right">
                  <NumericValue title={usage.tokens.toLocaleString("en-US")}>
                    {formatNumber(usage.tokens)}
                  </NumericValue>
                </ListCell>
                <ListCell data-label="Cost" $align="right">
                  <NumericValue $accent>
                    {formatCurrency(usage.cost)}
                  </NumericValue>
                </ListCell>
                <ListCell data-label="Share" $align="right">
                  <NumericValue>{usage.percentage.toFixed(1)}%</NumericValue>
                </ListCell>
              </ListRow>
            ))}
          </ListBody>
        </ListTable>
      </ListCard>
    );
  }

  if (filteredModels.length === 0) return null;

  return (
    <ModelsFallback className={className}>
      <ModelsFallbackList aria-label="Models used">
        {filteredModels.map((model) => (
          <ModelTag key={model}>
            <ModelIcon model={model} size={13} />
            <ModelName>{model}</ModelName>
          </ModelTag>
        ))}
      </ModelsFallbackList>
    </ModelsFallback>
  );
}

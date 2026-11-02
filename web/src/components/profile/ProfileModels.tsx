"use client";

import { formatCurrency, formatNumber } from "@/lib/utils";
import { useLocale, useT } from "@/components/layout/LocaleProvider";
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
import { formatWholePercents } from "@/lib/format";
import { getSpeedTierRows } from "@/lib/speedUsage";
import { ModelIcon } from "./ModelIcon";
import { SpeedTierLabel } from "./SpeedTier";
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

// Mirrors the day-detail tier list in ProfileContributionGraph so the same
// breakdown reads the same way in both places.
const SpeedTierList = tw("div", "mt-1 grid gap-0.5 pl-[1.375rem]");

const SpeedTierLine = tw(
  "div",
  "flex min-w-0 items-start gap-1.5 text-[0.6875rem] font-normal leading-snug text-muted-foreground"
);

/**
 * Speed tiers for one model over the scoped period.
 *
 * Nested inside the model cell rather than given its own columns: the table's
 * "Share" column is a share of total *cost*, while a tier's share is of its
 * model's *tokens*. Putting the two in one column would invite reading one as
 * the other. Messages are omitted because the period-scoped `modelUsage` has
 * no message total to measure the Unknown remainder against.
 */
function ModelSpeedTiers({ usage }: { usage: ModelUsage }) {
  const t = useT();
  const locale = useLocale();
  const tiers = getSpeedTierRows(usage.speedUsage, usage.tokens, 0);
  if (tiers.length === 0) return null;
  const percents = formatWholePercents(
    tiers.map((tier) => (usage.tokens > 0 ? (tier.tokens / usage.tokens) * 100 : 0)),
    locale,
  );

  return (
    <SpeedTierList>
      {tiers.map((tier, index) => (
        <SpeedTierLine key={tier.speed}>
          <SpeedTierLabel
            tier={tier}
            className={cn("flex-none", tier.speed !== "unknown" && "text-foreground")}
          />
          <span className="min-w-0">
            {t("profile.models.tierLine", {
              tokens: t("profile.units.token", { count: tier.tokens }),
              share: percents[index],
            })}
          </span>
        </SpeedTierLine>
      ))}
    </SpeedTierList>
  );
}

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
  const t = useT();
  const locale = useLocale();
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
          <ListCaption>{t("profile.models.caption")}</ListCaption>
          <ListHead>
            <tr>
              <ListHeaderCell $width="52%">{t("profile.models.columnModel")}</ListHeaderCell>
              <ListHeaderCell $width="18%" $align="right">
                {t("profile.models.columnTokens")}
              </ListHeaderCell>
              <ListHeaderCell $width="17%" $align="right">
                {t("profile.models.columnCost")}
              </ListHeaderCell>
              <ListHeaderCell $width="13%" $align="right">
                {t("profile.models.columnShare")}
              </ListHeaderCell>
            </tr>
          </ListHead>
          <ListBody>
            {filteredUsage.map((usage) => {
              const hasTiers = getSpeedTierRows(usage.speedUsage, usage.tokens, 0).length > 0;
              return (
              // Tier lines make the model cell taller, so the numeric cells
              // top-align with the model name instead of floating to the
              // middle of the grown row.
              <ListRow key={usage.model} className={cn(hasTiers && "[&>*]:align-top")}>
                <ListPrimaryCell scope="row">
                  <ModelIdentity>
                    <ModelIcon model={usage.model} size={14} />
                    <ModelName>{usage.model}</ModelName>
                  </ModelIdentity>
                  <ModelSpeedTiers usage={usage} />
                </ListPrimaryCell>
                <ListCell data-label={t("profile.models.columnTokens")} $align="right">
                  <NumericValue title={usage.tokens.toLocaleString(locale)}>
                    {formatNumber(usage.tokens)}
                  </NumericValue>
                </ListCell>
                <ListCell data-label={t("profile.models.columnCost")} $align="right">
                  <NumericValue $accent>
                    {formatCurrency(usage.cost)}
                  </NumericValue>
                </ListCell>
                <ListCell data-label={t("profile.models.columnShare")} $align="right">
                  <NumericValue>{usage.percentage.toFixed(1)}%</NumericValue>
                </ListCell>
              </ListRow>
              );
            })}
          </ListBody>
        </ListTable>
      </ListCard>
    );
  }

  if (filteredModels.length === 0) return null;

  return (
    <ModelsFallback className={className}>
      <ModelsFallbackList aria-label={t("profile.models.fallbackAria")}>
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

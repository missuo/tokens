import React from "react";

/** Standard elevated card surface used across the app. */
export function Panel({
  className = "",
  as: Tag = "div",
  ...props
}: React.HTMLAttributes<HTMLElement> & { as?: React.ElementType }) {
  return <Tag className={`rounded-xl border bg-card ${className}`} {...props} />;
}

interface StatTileProps {
  label: string;
  value: React.ReactNode;
  sub?: React.ReactNode;
  accent?: boolean;
  title?: string;
  icon?: React.ReactNode;
}

/** Compact metric tile: small uppercase label + large tabular-mono value. */
export function StatTile({ label, value, sub, accent, title, icon }: StatTileProps) {
  return (
    <div className="flex flex-col rounded-xl border bg-card px-4 py-3.5">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        {icon}
        <p className="text-[11px] font-semibold tracking-wider uppercase">{label}</p>
      </div>
      <p
        title={title}
        className={`mt-1 font-mono text-[1.6rem] leading-tight font-semibold tracking-tight tabular-nums max-[400px]:text-2xl ${
          accent ? "text-accent" : "text-foreground"
        }`}
      >
        {value}
      </p>
      {sub != null && <p className="mt-0.5 text-xs text-muted-foreground">{sub}</p>}
    </div>
  );
}

/** Responsive grid for StatTiles — wraps cleanly down to 2-up on phones. */
export function StatGrid({
  children,
  cols = 3,
  className = "",
}: {
  children: React.ReactNode;
  cols?: 2 | 3 | 4;
  className?: string;
}) {
  const colClass =
    cols === 4
      ? "grid-cols-2 lg:grid-cols-4"
      : cols === 3
        ? "grid-cols-2 sm:grid-cols-3"
        : "grid-cols-2";
  return <div className={`grid gap-3 ${colClass} ${className}`}>{children}</div>;
}

interface PageHeaderProps {
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}

/** Title + optional subtitle on the left, optional actions on the right. */
export function PageHeader({ title, subtitle, actions, className = "" }: PageHeaderProps) {
  return (
    <div className={`flex flex-wrap items-start justify-between gap-4 ${className}`}>
      <div className="min-w-0">
        <h1 className="text-2xl font-bold tracking-tight text-foreground sm:text-[1.75rem]">{title}</h1>
        {subtitle != null && <p className="mt-1 text-sm text-muted-foreground">{subtitle}</p>}
      </div>
      {actions != null && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  );
}

import React from "react";

/** Standard elevated card surface used across the app. */
export function Panel({
  className = "",
  as: Tag = "div",
  ...props
}: React.HTMLAttributes<HTMLElement> & { as?: React.ElementType }) {
  return <Tag className={`rounded-xl border bg-card ${className}`} {...props} />;
}

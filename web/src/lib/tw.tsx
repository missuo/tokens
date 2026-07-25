import { cn } from "@/lib/utils";

/**
 * A named element with baked-in classes.
 *
 * This is the shape the codebase already had under styled-components — a
 * `const Row = styled.div\`...\`` used as `<Row>` — so views can move off that
 * dependency without their markup being rewritten at the same time. The
 * difference is that the styles are Tailwind classes resolved at build time
 * rather than a runtime stylesheet.
 *
 * Every DOM prop is forwarded — including `ref`, which React 19 passes to
 * function components like any other prop — and a `className` from the call
 * site is merged after the base, so callers can still override utilities.
 *
 *   const Label = tw("span", "text-xs text-muted-foreground");
 *   <Label className="uppercase">Tokens</Label>
 *
 * For anything conditional — a variant, a runtime colour — write a normal
 * component instead. Squeezing that through here produces class strings that
 * Tailwind cannot see at build time.
 */
export function tw<T extends keyof React.JSX.IntrinsicElements>(
  Tag: T,
  base: string
) {
  const Component = ({
    className,
    ...props
  }: React.ComponentPropsWithRef<T>) => {
    const Element = Tag as React.ElementType;
    return <Element className={cn(base, className)} {...props} />;
  };
  Component.displayName = `tw(${String(Tag)})`;
  return Component;
}

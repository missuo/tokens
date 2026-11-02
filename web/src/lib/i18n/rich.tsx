import { Fragment, type ReactNode } from "react";

const SLOT = /^\{(\w+)\}$/;

/**
 * Interleave translated text with React nodes.
 *
 * A sentence holding links or emphasis cannot be a plain template string: the
 * markup has to be *inside* the sentence so a translator can move it. Here the
 * message carries `{vps}` / `{neon}` slots and the caller supplies the anchors,
 * which lets "Sponsored by {vps} and {neon}" become 由 {vps} 与 {neon} 赞助
 * without the caller knowing where the words fall.
 *
 * A slot with no node renders literally, matching how `formatMessage` treats an
 * unknown placeholder: visible `{vps}` is a better bug than a dropped link.
 */
export function richText(
  message: string,
  nodes: Record<string, ReactNode>,
): ReactNode {
  return message.split(/(\{\w+\})/g).map((part, index) => {
    const name = SLOT.exec(part)?.[1];
    const node = name === undefined ? undefined : nodes[name];

    return <Fragment key={index}>{node ?? part}</Fragment>;
  });
}

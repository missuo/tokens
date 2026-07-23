"use client";

import styled from "styled-components";

const VERIFIED_TOOLTIP =
  "Verified — earned by adding two or more social links (website included) to your GitHub profile.";

export interface VerifiedBadgeProps {
  size?: number;
  className?: string;
}

/**
 * Blue check shown next to avatars of users with enough linked GitHub
 * social accounts. Hover reveals how to earn it.
 */
export function VerifiedBadge({ size = 14, className }: VerifiedBadgeProps) {
  const glyph = Math.max(8, Math.round(size * 0.62));
  return (
    <Badge
      className={className}
      $size={size}
      role="img"
      aria-label={VERIFIED_TOOLTIP}
      data-tooltip={VERIFIED_TOOLTIP}
    >
      <svg
        aria-hidden="true"
        width={glyph}
        height={glyph}
        viewBox="0 0 20 20"
        fill="none"
      >
        <path
          d="M4.5 10.5l3.5 3.5 7.5-8"
          stroke="#fff"
          strokeWidth="2.6"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </Badge>
  );
}

const Badge = styled.span<{ $size: number }>`
  position: relative;
  display: inline-flex;
  width: ${({ $size }) => $size}px;
  height: ${({ $size }) => $size}px;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  background: #1d9bf0;
  box-shadow: 0 0 0 2px var(--service-surface, var(--background, #fff));
  cursor: default;

  &::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: calc(100% + 8px);
    left: 50%;
    transform: translateX(-50%);
    width: max-content;
    max-width: 240px;
    background-color: #111b2c;
    color: #e5e5e5;
    border-radius: 8px;
    padding: 8px 12px;
    font-size: 12px;
    font-weight: 500;
    line-height: 1.45;
    letter-spacing: 0;
    white-space: normal;
    text-align: center;
    box-shadow:
      0 8px 30px rgba(0, 0, 0, 0.4),
      0 0 0 1px rgba(255, 255, 255, 0.06);
    z-index: 1000;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.15s ease;
  }

  &:hover::after {
    opacity: 1;
  }

  @media (prefers-reduced-motion: reduce) {
    &::after {
      transition: none;
    }
  }
`;

"use client";

import React from "react";

interface SkeletonProps {
  w?: string;
  h?: string;
  rounded?: string;
  mb?: string;
  ml?: string;
  className?: string;
}

/** A single shimmer bar. Sizes are passed as inline styles for flexibility. */
export function Skeleton({ w, h, rounded = "0.25rem", mb, ml, className }: SkeletonProps) {
  return (
    <div
      className={`animate-pulse bg-line ${className ?? ""}`}
      style={{
        width: w,
        height: h,
        borderRadius: rounded,
        marginBottom: mb,
        marginLeft: ml,
      }}
    />
  );
}

export function LeaderboardSkeleton() {
  return (
    <div className="space-y-4">
      <div className="mb-10 grid grid-cols-2 gap-4 md:grid-cols-4">
        {[...Array(4)].map((_, i) => (
          <div key={i} className="rounded-xl border border-line bg-surface p-4">
            <Skeleton h="1rem" w="5rem" mb="0.5rem" />
            <Skeleton h="2rem" w="6rem" />
          </div>
        ))}
      </div>

      <div className="overflow-hidden rounded-2xl border border-line bg-surface">
        <div className="border-b border-line bg-surface-secondary px-6 py-3">
          <div className="flex gap-6">
            <Skeleton h="1rem" w="3rem" />
            <Skeleton h="1rem" w="6rem" />
            <Skeleton h="1rem" w="4rem" ml="auto" />
            <Skeleton h="1rem" w="4rem" />
          </div>
        </div>
        {[...Array(10)].map((_, i) => (
          <div key={i} className="border-b border-line px-6 py-4 last:border-b-0">
            <div className="flex items-center gap-6">
              <Skeleton h="1.5rem" w="2rem" />
              <div className="flex items-center gap-3">
                <Skeleton h="2.5rem" w="2.5rem" rounded="9999px" />
                <div>
                  <Skeleton h="1rem" w="8rem" mb="0.25rem" />
                  <Skeleton h="0.75rem" w="5rem" />
                </div>
              </div>
              <Skeleton h="1.25rem" w="4rem" ml="auto" />
              <Skeleton h="1.25rem" w="3.5rem" />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export function ProfileSkeleton() {
  return (
    <div className="space-y-8">
      <div className="mb-8 flex items-start gap-6">
        <Skeleton w="6rem" h="6rem" rounded="1rem" />
        <div className="flex-1">
          <div className="mb-2 flex items-center gap-3">
            <Skeleton h="2rem" w="12rem" />
            <Skeleton h="1.5rem" w="3rem" rounded="0.5rem" />
          </div>
          <Skeleton h="1rem" w="6rem" mb="0.75rem" />
          <div className="flex gap-2">
            <Skeleton h="1.5rem" w="4rem" rounded="0.5rem" />
            <Skeleton h="1.5rem" w="5rem" rounded="0.5rem" />
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
        {[...Array(4)].map((_, i) => (
          <div key={i} className="rounded-xl border border-line bg-surface p-4">
            <Skeleton h="1rem" w="5rem" mb="0.5rem" />
            <Skeleton h="2rem" w="6rem" />
          </div>
        ))}
      </div>

      <div className="rounded-2xl border border-line bg-surface p-6">
        <Skeleton h="1.5rem" w="6rem" mb="1rem" />
        <Skeleton h="10rem" w="100%" rounded="0.5rem" />
      </div>
    </div>
  );
}

export function StatCardSkeleton() {
  return (
    <div className="rounded-xl border border-line bg-surface p-4">
      <Skeleton h="1rem" w="5rem" mb="0.5rem" />
      <Skeleton h="2rem" w="6rem" />
    </div>
  );
}

export function TableRowSkeleton() {
  return (
    <div className="border-b border-line px-6 py-4">
      <div className="flex items-center gap-6">
        <Skeleton h="1.5rem" w="2rem" />
        <div className="flex items-center gap-3">
          <Skeleton h="2.5rem" w="2.5rem" rounded="9999px" />
          <div>
            <Skeleton h="1rem" w="8rem" mb="0.25rem" />
            <Skeleton h="0.75rem" w="5rem" />
          </div>
        </div>
        <Skeleton h="1.25rem" w="4rem" ml="auto" />
        <Skeleton h="1.25rem" w="3.5rem" />
      </div>
    </div>
  );
}

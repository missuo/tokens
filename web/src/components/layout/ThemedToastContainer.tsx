"use client";

import { useTheme } from "next-themes";
import { ToastContainer } from "react-toastify";
import { useT } from "@/components/layout/LocaleProvider";

export function ThemedToastContainer() {
  const { resolvedTheme } = useTheme();
  const t = useT();

  return (
    <ToastContainer
      position="top-right"
      theme={resolvedTheme === "light" ? "light" : "dark"}
      // The container is a labelled region; without this it announces the
      // library's English default ("Notifications Alt+T") on a Chinese page.
      aria-label={t("common.notifications")}
    />
  );
}

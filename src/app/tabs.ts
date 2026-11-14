import type { MessageKey } from "../shared/i18n";

export type TabId = "connect" | "profiles" | "apps" | "settings" | "logs";

export interface Tab {
  id: TabId;
  labelKey: MessageKey;
}

export const tabs: readonly Tab[] = [
  { id: "connect", labelKey: "nav.connect" },
  { id: "profiles", labelKey: "nav.profiles" },
  { id: "apps", labelKey: "nav.apps" },
  { id: "settings", labelKey: "nav.settings" },
  { id: "logs", labelKey: "nav.logs" },
] as const;

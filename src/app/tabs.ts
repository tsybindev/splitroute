export type TabId = "connect" | "profiles" | "apps" | "settings" | "logs";

export interface Tab {
  id: TabId;
  label: string;
}

export const tabs: readonly Tab[] = [
  { id: "connect", label: "Главная" },
  { id: "profiles", label: "Локации" },
  { id: "apps", label: "Приложения" },
  { id: "settings", label: "Настройки" },
  { id: "logs", label: "Логи" },
] as const;

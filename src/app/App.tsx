import { useEffect } from "react";
import { tabs, type TabId } from "./tabs";
import { ConnectScreen } from "../features/connect/ConnectScreen";
import { ProfilesScreen } from "../features/profiles/ProfilesScreen";
import { AppsScreen } from "../features/apps/AppsScreen";
import { SettingsScreen } from "../features/settings/SettingsScreen";
import { LogsScreen } from "../features/logs/LogsScreen";
import { t, useLanguage } from "../shared/i18n";
import { useAppStore } from "../shared/state/appStore";
import { StatusBadge } from "../shared/ui/StatusBadge";

const screens: Record<TabId, () => React.JSX.Element> = {
  connect: ConnectScreen,
  profiles: ProfilesScreen,
  apps: AppsScreen,
  settings: SettingsScreen,
  logs: LogsScreen,
};

export function App() {
  // Подписка на язык: смена языка перерисовывает всё дерево без перезапуска.
  useLanguage();
  const active = useAppStore((state) => state.activeTab);
  const setActive = useAppStore((state) => state.setActiveTab);
  const Screen = screens[active];

  useEffect(() => {
    const store = useAppStore.getState();
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    // Слушатели событий ставятся до инициализации: автоподключение при старте
    // (B2) может завершиться раньше, чем UI успеет прочитать состояние.
    const start = async () => {
      try {
        const stop = await store.subscribe();
        if (cancelled) {
          stop();
          return;
        }
        unlisten = stop;
      } catch {
        // Без событий окно работает: состояние читается командами.
      }
      if (!cancelled) await store.initialize();
    };
    void start();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return (
    <div className="flex h-full flex-col bg-neutral-950 text-neutral-100">
      <header className="flex items-center justify-between border-b border-neutral-800 px-6 py-4">
        <h1 className="text-lg font-semibold tracking-tight">{t("app.name")}</h1>
        <StatusBadge />
      </header>

      <nav className="flex gap-1 border-b border-neutral-800 px-4 py-2">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => setActive(tab.id)}
            className={`rounded-md px-3 py-1.5 text-sm transition-colors ${
              active === tab.id
                ? "bg-neutral-800 text-neutral-100"
                : "text-neutral-400 hover:text-neutral-200"
            }`}
          >
            {t(tab.labelKey)}
          </button>
        ))}
      </nav>

      <main className="flex-1 overflow-auto p-6">
        <Screen />
      </main>
    </div>
  );
}

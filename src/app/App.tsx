import { useState } from "react";
import { tabs, type TabId } from "./tabs";
import { ConnectScreen } from "../features/connect/ConnectScreen";
import { ProfilesScreen } from "../features/profiles/ProfilesScreen";
import { AppsScreen } from "../features/apps/AppsScreen";
import { SettingsScreen } from "../features/settings/SettingsScreen";
import { LogsScreen } from "../features/logs/LogsScreen";

const screens: Record<TabId, () => React.JSX.Element> = {
  connect: ConnectScreen,
  profiles: ProfilesScreen,
  apps: AppsScreen,
  settings: SettingsScreen,
  logs: LogsScreen,
};

export function App() {
  const [active, setActive] = useState<TabId>("connect");
  const Screen = screens[active];

  return (
    <div className="flex h-full flex-col bg-neutral-950 text-neutral-100">
      <header className="flex items-center justify-between border-b border-neutral-800 px-6 py-4">
        <h1 className="text-lg font-semibold tracking-tight">SplitRoute</h1>
        <span className="text-sm text-neutral-400">Не подключено</span>
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
            {tab.label}
          </button>
        ))}
      </nav>

      <main className="flex-1 overflow-auto p-6">
        <Screen />
      </main>
    </div>
  );
}

import type { ConnectionState } from "../api/types";
import { t } from "../i18n";
import { useAppStore } from "../state/appStore";
import { Badge, Dot } from "./Badge";

const labels = {
  disconnected: "app.status.disconnected",
  connecting: "app.status.connecting",
  connected: "app.status.connected",
  disconnecting: "app.status.disconnecting",
  error: "app.status.error",
} as const;

const tones = {
  disconnected: "neutral",
  connecting: "info",
  connected: "success",
  disconnecting: "warn",
  error: "danger",
} as const;

export function statusTone(connection: ConnectionState) {
  return tones[connection];
}

export function StatusBadge() {
  const connection = useAppStore((state) => state.connection);
  return (
    <Badge tone={tones[connection]}>
      <Dot tone={tones[connection]} />
      <span className="ml-1.5">{t(labels[connection])}</span>
    </Badge>
  );
}

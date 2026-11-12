import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConnectScreen } from "./ConnectScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeProfile, makeSettings, runningStatus, stoppedStatus } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  connect: vi.fn(),
  disconnect: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

describe("ConnectScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
  });

  it("показывает пустое состояние без локации и блокирует подключение", () => {
    useAppStore.setState({ settings: makeSettings(), helperStatus: stoppedStatus });
    render(<ConnectScreen />);

    expect(screen.getByText("Локация не выбрана")).toBeInTheDocument();
    expect(screen.getByText(/Подписка не добавлена/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Подключить" })).toBeDisabled();
  });

  it("подключается и показывает статус и локацию", async () => {
    const user = userEvent.setup();
    apiMock.connect.mockResolvedValue(runningStatus);
    useAppStore.setState({
      settings: makeSettings(),
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: stoppedStatus,
      connection: "disconnected",
    });

    render(<ConnectScreen />);
    expect(screen.getByText("Тестовая локация A")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Подключить" }));

    await waitFor(() => expect(apiMock.connect).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.getByText("Подключено")).toBeInTheDocument(),
    );
  });
});

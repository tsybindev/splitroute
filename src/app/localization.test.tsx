import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { setLanguage } from "../shared/i18n";
import { resetAppStore } from "../shared/state/appStore";
import { makeSettings, stoppedStatus } from "../test/fixtures";

const apiMock = vi.hoisted(() => ({
  settingsGet: vi.fn(),
  settingsUpdate: vi.fn(),
  rulesList: vi.fn(),
  profilesList: vi.fn(),
  helperStatus: vi.fn(),
  autoconnectStatus: vi.fn(),
  logsList: vi.fn(),
  appsInstalled: vi.fn(),
  appsRunning: vi.fn(),
}));

vi.mock("../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  onAutoconnectFailed: vi.fn(() => Promise.resolve(() => {})),
  onSubscriptionUpdated: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

function expectNoRussianText() {
  expect(document.body.textContent ?? "").not.toMatch(/[А-Яа-яЁё]/);
}

describe("EN-локализация экранов", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
    setLanguage("en");
    const settings = makeSettings();
    settings.preferences.language = "en";
    apiMock.settingsGet.mockResolvedValue(settings);
    apiMock.settingsUpdate.mockImplementation(async (next) => next);
    apiMock.rulesList.mockResolvedValue([]);
    apiMock.profilesList.mockResolvedValue({
      subscriptions: [],
      profiles: [],
      selected_profile_id: null,
    });
    apiMock.helperStatus.mockResolvedValue(stoppedStatus);
    apiMock.autoconnectStatus.mockResolvedValue(null);
    apiMock.logsList.mockResolvedValue([]);
    apiMock.appsInstalled.mockResolvedValue([]);
    apiMock.appsRunning.mockResolvedValue([]);
  });

  it("не оставляет русского текста ни на одном экране", async () => {
    const user = userEvent.setup();
    render(<App />);

    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: "Connection" }),
      ).toBeInTheDocument(),
    );
    expectNoRussianText();

    await user.click(screen.getByRole("button", { name: "Locations" }));
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: "Locations" }),
      ).toBeInTheDocument(),
    );
    expectNoRussianText();

    await user.click(screen.getByRole("button", { name: "Applications" }));
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: "Applications" }),
      ).toBeInTheDocument(),
    );
    await waitFor(() => expect(apiMock.appsInstalled).toHaveBeenCalled());
    expectNoRussianText();

    await user.click(screen.getByRole("tab", { name: "Running" }));
    await waitFor(() => expect(apiMock.appsRunning).toHaveBeenCalled());
    await waitFor(() =>
      expect(
        screen.getByText("No running processes found"),
      ).toBeInTheDocument(),
    );
    expectNoRussianText();

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Save settings" }),
      ).toBeInTheDocument(),
    );
    expectNoRussianText();

    await user.click(screen.getByRole("button", { name: "Logs" }));
    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "Logs" })).toBeInTheDocument(),
    );
    expectNoRussianText();
  });
});

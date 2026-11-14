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
  logsList: vi.fn(),
  appsInstalled: vi.fn(),
}));

vi.mock("../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
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
      profiles: [],
      selected_profile_id: null,
      fetched_at: null,
    });
    apiMock.helperStatus.mockResolvedValue(stoppedStatus);
    apiMock.logsList.mockResolvedValue([]);
    apiMock.appsInstalled.mockResolvedValue([]);
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

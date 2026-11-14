import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import { AppIcon, clearIconCache } from "./AppIcon";
import { makeInstalledApp } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  appIcon: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

/**
 * Подмена IntersectionObserver: в jsdom его нет, а именно на нём держится
 * ленивая загрузка. Тест сам решает, когда строка «появилась во вьюпорте».
 */
class FakeIntersectionObserver {
  static instances: FakeIntersectionObserver[] = [];

  targets: Element[] = [];
  disconnected = false;
  options: IntersectionObserverInit | undefined;

  private readonly callback: IntersectionObserverCallback;

  constructor(
    callback: IntersectionObserverCallback,
    options?: IntersectionObserverInit,
  ) {
    this.callback = callback;
    this.options = options;
    FakeIntersectionObserver.instances.push(this);
  }

  observe(target: Element) {
    this.targets.push(target);
  }

  unobserve(target: Element) {
    this.targets = this.targets.filter((item) => item !== target);
  }

  disconnect() {
    this.disconnected = true;
    this.targets = [];
  }

  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }

  /** Имитация пересечения с вьюпортом. */
  emit(isIntersecting: boolean) {
    const entries = this.targets.map(
      (target) => ({ target, isIntersecting }) as IntersectionObserverEntry,
    );
    act(() => {
      this.callback(entries, this as unknown as IntersectionObserver);
    });
  }
}

function stubIntersectionObserver() {
  FakeIntersectionObserver.instances = [];
  vi.stubGlobal("IntersectionObserver", FakeIntersectionObserver);
}

function onlyObserver(): FakeIntersectionObserver {
  expect(FakeIntersectionObserver.instances).toHaveLength(1);
  return FakeIntersectionObserver.instances[0];
}

describe("AppIcon", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearIconCache();
    apiMock.appIcon.mockResolvedValue(null);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("показывает иконку из data URL", async () => {
    apiMock.appIcon.mockResolvedValue("data:image/svg+xml;base64,PHN2Zy8+");
    const app = makeInstalledApp({ path: "/Applications/Firefox.app" });
    const { container } = render(<AppIcon app={app} />);

    await waitFor(() =>
      expect(container.querySelector("img")).toHaveAttribute(
        "src",
        "data:image/svg+xml;base64,PHN2Zy8+",
      ),
    );
    expect(apiMock.appIcon).toHaveBeenCalledWith(app);
  });

  it("показывает первую букву имени и «?» для пустого имени", async () => {
    const { container } = render(
      <div>
        <AppIcon app={makeInstalledApp({ name: "zoom" })} />
        <AppIcon
          app={makeInstalledApp({ name: "   ", path: "/Applications/Empty.app" })}
        />
      </div>,
    );

    expect(screen.getByText("Z")).toBeInTheDocument();
    expect(screen.getByText("?")).toBeInTheDocument();
    expect(container.querySelector("img")).toBeNull();
  });

  it("лениво грузит иконку только после появления во вьюпорте", async () => {
    stubIntersectionObserver();
    const app = makeInstalledApp({ path: "/Applications/Slack.app" });
    render(<AppIcon app={app} />);

    const observer = onlyObserver();
    expect(observer.options?.rootMargin).toBe("200px");
    expect(observer.targets).toHaveLength(1);
    expect(apiMock.appIcon).not.toHaveBeenCalled();

    observer.emit(false);
    expect(apiMock.appIcon).not.toHaveBeenCalled();

    observer.emit(true);
    await waitFor(() => expect(apiMock.appIcon).toHaveBeenCalledWith(app));

    // Повторное пересечение не порождает второго запроса.
    observer.emit(true);
    expect(apiMock.appIcon).toHaveBeenCalledTimes(1);
    expect(observer.disconnected).toBe(true);
  });

  it("кэширует отсутствие иконки, в том числе после ошибки", async () => {
    apiMock.appIcon.mockRejectedValueOnce({ code: "UNKNOWN", message: "нет" });
    const app = makeInstalledApp({ path: "/Applications/Broken.app" });

    const first = render(<AppIcon app={app} />);
    await waitFor(() => expect(apiMock.appIcon).toHaveBeenCalledTimes(1));
    expect(screen.getByText("G")).toBeInTheDocument();
    first.unmount();

    // Негативный результат закэширован — второй запрос не нужен.
    apiMock.appIcon.mockResolvedValue("data:image/png;base64,AAAA");
    render(<AppIcon app={app} />);
    await Promise.resolve();
    expect(apiMock.appIcon).toHaveBeenCalledTimes(1);
    expect(screen.getByText("G")).toBeInTheDocument();
  });

  it("не обновляет состояние, если размонтирован до ответа", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    let resolveIcon: (value: string | null) => void = () => {};
    apiMock.appIcon.mockReturnValue(
      new Promise<string | null>((resolve) => {
        resolveIcon = resolve;
      }),
    );

    const { unmount } = render(
      <AppIcon app={makeInstalledApp({ path: "/Applications/Late.app" })} />,
    );
    unmount();

    await act(async () => {
      resolveIcon("data:image/png;base64,BBBB");
    });

    expect(errorSpy).not.toHaveBeenCalled();
    errorSpy.mockRestore();
  });
});

import { useState } from "react";

import { useSentinelApi } from "./components/ApiProvider";
import { BrandMark } from "./components/BrandMark";
import { ScrollIndicator } from "./components/ScrollIndicator";
import { ThemeToggle } from "./components/ThemeToggle";
import { NativeProtectionScanProvider } from "./lib/nativeProtectionScan";
import { useTheme } from "./lib/theme";
import { useScanAction } from "./lib/useScanAction";
import { CleanupPage } from "./pages/CleanupPage";
import { EventsPage } from "./pages/EventsPage";
import { HomePage } from "./pages/HomePage";

type PageId = "home" | "events" | "cleanup";

export function App() {
  const api = useSentinelApi();
  const protection = useScanAction(api.getNativeProtectionStatus);
  const { theme, toggleTheme } = useTheme();
  const [page, setPage] = useState<PageId>("home");

  return (
    <NativeProtectionScanProvider controller={protection}>
      <div className="app-shell">
        <a className="skip-link" href="#main-content">
          跳到主要内容
        </a>

        <aside className="side-rail">
          <div className="rail-brand">
            <BrandMark className="rail-brand__mark" />
            <div>
              <p className="rail-brand__name">SentinelLite</p>
              <p className="rail-brand__tagline">本机安全体检</p>
            </div>
          </div>

          <nav className="rail-nav" aria-label="主要导航">
            <button
              type="button"
              className={page === "home" ? "rail-nav__item is-active" : "rail-nav__item"}
              aria-current={page === "home" ? "page" : undefined}
              onClick={() => setPage("home")}
            >
              <span className="rail-nav__glyph" aria-hidden="true" />
              安全首页
            </button>
            <button
              type="button"
              className={page === "events" ? "rail-nav__item is-active" : "rail-nav__item"}
              aria-current={page === "events" ? "page" : undefined}
              onClick={() => setPage("events")}
            >
              <span className="rail-nav__glyph rail-nav__glyph--events" aria-hidden="true" />
              进程事件
            </button>
            <button
              type="button"
              className={page === "cleanup" ? "rail-nav__item is-active" : "rail-nav__item"}
              aria-current={page === "cleanup" ? "page" : undefined}
              onClick={() => setPage("cleanup")}
            >
              <span className="rail-nav__glyph rail-nav__glyph--square" aria-hidden="true" />
              软件体检
            </button>
          </nav>

          <div className="rail-footer">
            <p>v0.1.0 · 数据留在本机</p>
          </div>
        </aside>

        <div className="workspace">
          <div className="workspace-topbar">
            <ThemeToggle theme={theme} onToggle={toggleTheme} />
          </div>

          <main id="main-content" className="main-content">
            {page === "home" ? <HomePage onOpenCleanup={() => setPage("cleanup")} /> : null}
            {page === "events" ? <EventsPage /> : null}
            <div hidden={page !== "cleanup"}>
              <CleanupPage />
            </div>
          </main>
        </div>

        <ScrollIndicator />
      </div>
    </NativeProtectionScanProvider>
  );
}

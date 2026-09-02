import { useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { TaskCenter } from "./components/TaskCenter";
import { BackupPage } from "./pages/BackupPage";
import { DuplicatesPage } from "./pages/DuplicatesPage";
import { FilesPage } from "./pages/FilesPage";
import { HistoryPage } from "./pages/HistoryPage";
import { OrganizerPage } from "./pages/OrganizerPage";
import { RecentPage } from "./pages/RecentPage";
import { RestorePage } from "./pages/RestorePage";
import { RulesPage } from "./pages/RulesPage";
import { SettingsPage, type ThemeMode } from "./pages/SettingsPage";
import { pageMeta, type WorkspacePage } from "./types/workspace";

function isEditableTarget(target: EventTarget | null) {
  const element = target instanceof HTMLElement ? target : null;
  if (!element) return false;
  if (element instanceof HTMLTextAreaElement || element.isContentEditable) return true;
  if (element instanceof HTMLSelectElement) return false;
  if (element instanceof HTMLInputElement) {
    return !["checkbox", "radio", "button", "submit", "reset", "file"].includes(element.type);
  }
  return false;
}

function initialTheme(): ThemeMode {
  const value = window.localStorage.getItem("workspace-theme");
  return value === "light" || value === "dark" ? value : "system";
}

export default function App() {
  const [page, setPage] = useState<WorkspacePage>("recent");
  const [theme, setTheme] = useState<ThemeMode>(initialTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    window.localStorage.setItem("workspace-theme", theme);
  }, [theme]);

  useEffect(() => {
    function handleWorkspaceShortcut(event: KeyboardEvent) {
      const key = event.key.toLowerCase();
      const editable = isEditableTarget(event.target);

      if (event.ctrlKey && !event.altKey && key === "f" && !editable) {
        const search = document.querySelector<HTMLInputElement>("[data-workspace-search]");
        if (search) {
          event.preventDefault();
          search.focus();
          search.select();
        }
        return;
      }

      if (event.ctrlKey && !event.altKey && key === "a" && !editable) {
        const selectableList = document.querySelector("[data-selectable-list]");
        if (selectableList) {
          event.preventDefault();
          window.dispatchEvent(new Event("workspace-select-all"));
        }
        return;
      }

      if (event.key === "Escape") {
        window.dispatchEvent(new Event("workspace-escape"));
      }
    }

    window.addEventListener("keydown", handleWorkspaceShortcut);
    return () => window.removeEventListener("keydown", handleWorkspaceShortcut);
  }, []);

  function renderPage() {
    switch (page) {
      case "recent": return <RecentPage onNavigate={setPage} />;
      case "files": return <FilesPage />;
      case "organizer": return <OrganizerPage />;
      case "backup": return <BackupPage />;
      case "history": return <HistoryPage />;
      case "settings": return <SettingsPage theme={theme} onThemeChange={setTheme} />;
      case "duplicates": return <DuplicatesPage />;
      case "restore": return <RestorePage />;
      case "rules": return <RulesPage />;
    }
  }

  const meta = pageMeta[page];
  return (
    <div className="app-container">
      <Sidebar page={page} onNavigate={setPage} />
      <div className="app-main">
        <header className="workspace-header">
          <div className="workspace-title-block">
            <span className="workspace-index">{String(Object.keys(pageMeta).indexOf(page) + 1).padStart(2, "0")}</span>
            <div><span className="breadcrumb">归栈 / {meta.description}</span><h2>{meta.label}</h2></div>
          </div>
          <div className="header-context"><span>本机存放</span><span className="header-separator" /><span>改动前先预览</span></div>
        </header>
        <main className="main-content">{renderPage()}</main>
        <TaskCenter />
      </div>
    </div>
  );
}

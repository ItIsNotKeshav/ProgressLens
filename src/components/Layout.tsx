import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { useState, useEffect, useRef, useCallback } from "react";
import { api } from "../api";
import { RefreshCw, Link as LinkIcon, Settings2, X, Sun, Moon } from "lucide-react";
import type { FieldConfig } from "../types";
import { listen } from "@tauri-apps/api/event";

const NAV_ITEMS = [
  {
    path: "/dashboard",
    label: "Dashboard",
    icon: (
      <svg viewBox="0 0 20 20" fill="currentColor" className="w-4 h-4">
        <path d="M10.707 2.293a1 1 0 0 0-1.414 0l-7 7a1 1 0 0 0 1.414 1.414L4 10.414V17a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-2h2v2a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-6.586l.293.293a1 1 0 0 0 1.414-1.414l-7-7Z" />
      </svg>
    ),
  },
  {
    path: "/students",
    label: "Students",
    icon: (
      <svg viewBox="0 0 20 20" fill="currentColor" className="w-4 h-4">
        <path d="M7 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM14.5 9a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5ZM1.615 16.428a1.224 1.224 0 0 1-.569-1.175 6.002 6.002 0 0 1 11.908 0c.058.467-.172.92-.57 1.174A9.953 9.953 0 0 1 7 17a9.953 9.953 0 0 1-5.385-1.572ZM14.5 16h-.106c.07-.297.088-.611.048-.933a7.47 7.47 0 0 0-1.588-3.755 4.502 4.502 0 0 1 5.874 2.636.818.818 0 0 1-.36.98A7.465 7.465 0 0 1 14.5 16Z" />
      </svg>
    ),
  },
  {
    path: "/diff",
    label: "Diff",
    icon: (
      <svg viewBox="0 0 20 20" fill="currentColor" className="w-4 h-4">
        <path
          fillRule="evenodd"
          d="M10 3a.75.75 0 0 1 .75.75v1.69l1.22-1.22a.75.75 0 1 1 1.06 1.06L10.53 7.78a.75.75 0 0 1-1.06 0L7 5.28a.75.75 0 0 1 1.06-1.06l1.19 1.19V3.75A.75.75 0 0 1 10 3ZM3.75 9a.75.75 0 0 0 0 1.5h5.69l-1.22 1.22a.75.75 0 1 0 1.06 1.06l2.5-2.5a.75.75 0 0 0 0-1.06l-2.5-2.5a.75.75 0 1 0-1.06 1.06L9.44 9H3.75ZM16 9a.75.75 0 0 0-.75-.75h-1.69l1.22-1.22a.75.75 0 1 0-1.06-1.06l-2.5 2.5a.75.75 0 0 0 0 1.06l2.5 2.5a.75.75 0 1 0 1.06-1.06l-1.19-1.19h1.66A.75.75 0 0 0 16 9ZM10 13a.75.75 0 0 1 .75.75v1.69l-1.22-1.22a.75.75 0 0 0-1.06 1.06l2.5 2.5a.75.75 0 0 0 1.06 0l2.5-2.5a.75.75 0 1 0-1.06-1.06l-1.19 1.19V13.75A.75.75 0 0 0 10 13Z"
          clipRule="evenodd"
        />
      </svg>
    ),
  },
  {
    path: "/report",
    label: "Report",
    icon: (
      <svg viewBox="0 0 20 20" fill="currentColor" className="w-4 h-4">
        <path
          fillRule="evenodd"
          d="M15.621 4.379a3 3 0 0 0-4.242 0l-7 7a3 3 0 0 0 4.241 4.243h.001l.497-.5a.75.75 0 0 1 1.064 1.057l-.498.501-.002.002a4.5 4.5 0 0 1-6.364-6.364l7-7a4.5 4.5 0 0 1 6.368 6.36l-3.455 3.553A2.625 2.625 0 1 1 9.52 9.52l3.45-3.451a.75.75 0 1 1 1.061 1.06l-3.45 3.451a1.125 1.125 0 0 0 1.587 1.595l3.454-3.553a3 3 0 0 0 0-4.243Z"
          clipRule="evenodd"
        />
      </svg>
    ),
  },
];

function SyncStatusDot({ secsSinceSync }: { secsSinceSync: number | null }) {
  if (secsSinceSync === null) {
    return (
      <span className="relative flex h-2.5 w-2.5" title="Not synced yet">
        <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-ink-600" />
      </span>
    );
  }

  let color: string;
  let label: string;
  let pulseColor: string;

  if (secsSinceSync <= 120) {
    color = "bg-emerald-500";
    pulseColor = "bg-emerald-400";
    label = "Synced just now";
  } else if (secsSinceSync <= 600) {
    color = "bg-amber-500";
    pulseColor = "bg-amber-400";
    const mins = Math.floor(secsSinceSync / 60);
    label = `Synced ${mins}m ago`;
  } else {
    color = "bg-red-500";
    pulseColor = "bg-red-400";
    const mins = Math.floor(secsSinceSync / 60);
    label = `Last sync ${mins}m ago`;
  }

  return (
    <span className="relative flex h-2.5 w-2.5" title={label}>
      {secsSinceSync <= 120 && (
        <span className={`animate-ping absolute inline-flex h-full w-full rounded-full ${pulseColor} opacity-75`} />
      )}
      <span className={`relative inline-flex rounded-full h-2.5 w-2.5 ${color}`} />
    </span>
  );
}

function formatSyncLabel(secs: number | null): string {
  if (secs === null) return "Not synced";
  if (secs <= 120) return "Synced";
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  return `${hrs}h ago`;
}

export default function Layout() {
  const navigate = useNavigate();
  const [sheetUrl, setSheetUrl] = useState("");
  const [syncing, setSyncing] = useState(false);
  const [showInput, setShowInput] = useState(false);
  const [configs, setConfigs] = useState<FieldConfig[] | null>(null);
  const [sheetLabel, setSheetLabel] = useState("");
  const [isLight, setIsLight] = useState(() => document.documentElement.classList.contains('light'));

  // ─── Sync status state ─────────────────────────────────────────────────
  const [syncSecs, setSyncSecs] = useState<number | null>(null);
  const [autoSyncToast, setAutoSyncToast] = useState(false);
  const [forceSyncing, setForceSyncing] = useState(false);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);



  useEffect(() => {
    if (localStorage.getItem('theme') === 'light') {
      document.documentElement.classList.add('light');
      setIsLight(true);
    }
  }, []);

  // Poll sync status every 10 seconds
  useEffect(() => {
    const poll = async () => {
      try {
        const secs = await api.getSyncStatus();
        setSyncSecs(secs);
      } catch { /* ignore */ }
    };
    poll();
    const interval = setInterval(poll, 10000);
    return () => clearInterval(interval);
  }, []);

  // Listen for sync:updated events from backend
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listen("sync:updated", () => {
      // Refresh sync status immediately
      api.getSyncStatus().then(setSyncSecs).catch(() => {});
      setSyncSecs(0); // optimistic: set to 0

      // Show auto-sync toast
      setAutoSyncToast(true);
      if (toastTimer.current) clearTimeout(toastTimer.current);
      toastTimer.current = setTimeout(() => setAutoSyncToast(false), 3000);

      // Trigger a page-level refresh by dispatching a custom event
      window.dispatchEvent(new CustomEvent("progresslens:refresh"));
    }).then(fn => { unlisten = fn; });

    return () => { unlisten?.(); };
  }, []);

  const toggleTheme = () => {
    const next = !isLight;
    document.documentElement.classList.toggle('light', next);
    setIsLight(next);
    localStorage.setItem('theme', next ? 'light' : 'dark');
  };

  const handleForceSync = useCallback(async () => {
    if (forceSyncing) return;
    try {
      setForceSyncing(true);
      await api.forceSync();
      setSyncSecs(0);
    } catch (e) {
      console.error("Force sync failed:", e);
    } finally {
      setForceSyncing(false);
    }
  }, [forceSyncing]);



  const handleSync = async () => {
    if (!sheetUrl) {
      alert("Please enter a Google Sheet URL.");
      return;
    }
    
    try {
      setSyncing(true);
      if (!configs) {
        const preview = await api.previewSheet(sheetUrl);
        setConfigs(preview);
      } else {
        const res = await api.syncFromSheet(sheetUrl, configs, sheetLabel || undefined);
        alert(`Success! Imported ${res.students_upserted} students from "${res.source_label}"`);
        setConfigs(null);
        setShowInput(false);
        setSheetUrl("");
        setSheetLabel("");
        navigate(`/field-setup?sheet_id=${res.sheet_id}`);
      }
    } catch (e) {
      alert(`Operation failed: ${e}`);
      if (configs) setConfigs(null); // allow trying preview again
    } finally {
      setSyncing(false);
    }
  };

  const updateConfig = (idx: number, updates: Partial<FieldConfig>) => {
    if (!configs) return;
    const newConfigs = [...configs];
    newConfigs[idx] = { ...newConfigs[idx], ...updates };
    setConfigs(newConfigs);
  };

  return (
    <div className="flex h-screen overflow-hidden bg-ink-950">
      {/* Sidebar */}
      <aside className="w-56 shrink-0 flex flex-col border-r border-ink-800/60 bg-ink-950 z-20">
        {/* Logo */}
        <div className="px-5 py-5 border-b border-ink-800/60">
          <div className="flex items-center gap-2.5">
            <div className="w-7 h-7 rounded-lg bg-amber-500 flex items-center justify-center">
              <svg viewBox="0 0 16 16" fill="none" className="w-4 h-4">
                <path
                  d="M2 12 L2 4 L8 4 L8 8 L14 8 L14 12"
                  stroke="#1a1713"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </div>
            <div>
              <p className="font-display text-sm font-bold text-ink-50 leading-tight">
                ProgressLens
              </p>
              <p className="text-[10px] text-ink-500 leading-tight">
                Student Tracker
              </p>
            </div>
          </div>
        </div>

        {/* Navigation */}
        <nav className="flex-1 px-3 py-4 flex flex-col gap-0.5">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.path}
              to={item.path}
              className={({ isActive }) =>
                ["nav-link", isActive ? "active" : ""].join(" ").trim()
              }
            >
              {item.icon}
              {item.label}
            </NavLink>
          ))}

        </nav>

        {/* Footer Sync Action */}
        <div className="p-4 border-t border-ink-800/60 flex flex-col gap-3">
          {/* ── Sync Status Bar ─────────────────────────────────────── */}
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2 min-w-0">
              <SyncStatusDot secsSinceSync={syncSecs} />
              <span className="text-[10px] text-ink-500 font-mono truncate">
                {formatSyncLabel(syncSecs)}
              </span>
            </div>
            <div className="flex items-center gap-1">
              <button
                onClick={handleForceSync}
                disabled={forceSyncing}
                title="Force sync now"
                className="p-1.5 hover:text-amber-400 hover:bg-ink-800 rounded-md transition-colors text-ink-500 disabled:opacity-40 disabled:cursor-not-allowed"
              >
                <RefreshCw className={`w-3.5 h-3.5 ${forceSyncing ? 'animate-spin' : ''}`} />
              </button>
            </div>
          </div>

          {showInput ? (
            <div className="flex flex-col gap-2">
              <input 
                autoFocus
                type="text" 
                placeholder="Paste Sheet URL..." 
                className="w-full bg-ink-900 border border-ink-800 rounded-md text-xs py-1.5 px-2 outline-none focus:border-amber-500/50 text-ink-100 placeholder:text-ink-600"
                value={sheetUrl}
                onChange={(e) => setSheetUrl(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleSync();
                  if (e.key === "Escape") setShowInput(false);
                }}
              />
              <div className="flex gap-2">
                <button 
                  onClick={() => setShowInput(false)}
                  className="flex-1 py-1.5 rounded-md text-[10px] font-medium tracking-wide border border-ink-800 text-ink-400 hover:text-ink-200 hover:bg-ink-800 transition-colors"
                >
                  Cancel
                </button>
                <button 
                  onClick={handleSync}
                  disabled={syncing}
                  className="flex-1 py-1.5 rounded-md text-[10px] uppercase font-bold tracking-wide bg-amber-500 text-ink-950 hover:bg-amber-400 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex justify-center items-center gap-1"
                >
                  {syncing ? (
                    <RefreshCw className="w-3 h-3 animate-spin mx-auto" />
                  ) : (
                    "Load Data"
                  )}
                </button>
              </div>
            </div>
          ) : (
            <button 
              onClick={() => setShowInput(true)}
              className="w-full flex items-center justify-center gap-2 py-2 rounded-lg bg-ink-900 border border-ink-800/80 text-ink-300 hover:text-amber-400 hover:border-amber-500/30 transition-all text-sm font-medium"
            >
              <LinkIcon className="w-4 h-4" />
              <span>Link Spreadsheet</span>
            </button>
          )}

          <div className="flex items-center justify-between text-[10px] text-ink-600 font-mono">
            <button 
              onClick={toggleTheme} 
              className="p-1.5 hover:text-ink-300 hover:bg-ink-800 rounded-md transition-colors"
              title="Toggle theme"
            >
              {isLight ? <Moon className="w-3.5 h-3.5" /> : <Sun className="w-3.5 h-3.5" />}
            </button>
            <span>v0.1.0</span>
          </div>
        </div>
      </aside>

      {/* Main content */}
      <main className="flex-1 overflow-y-auto relative bg-ink-950">
        {/* Auto-sync toast */}
        <div
          className={`fixed top-4 right-4 z-[100] flex items-center gap-2 px-4 py-2.5 rounded-lg bg-emerald-500/15 border border-emerald-500/30 text-emerald-400 text-xs font-medium backdrop-blur-sm shadow-lg transition-all duration-300 ${
            autoSyncToast ? 'opacity-100 translate-y-0' : 'opacity-0 -translate-y-2 pointer-events-none'
          }`}
        >
          <span className="relative flex h-2 w-2">
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" />
            <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500" />
          </span>
          Auto-synced just now
        </div>



        {configs ? (
          <div className="absolute inset-0 z-50 bg-ink-950/95 backdrop-blur overflow-y-auto p-8 flex flex-col items-center">
            <div className="max-w-4xl w-full">
              <div className="flex items-center justify-between mb-8">
                <div>
                  <h1 className="text-3xl font-display font-bold text-ink-50 flex items-center gap-3">
                    <Settings2 className="w-8 h-8 text-amber-500" />
                    Field Setup
                  </h1>
                  <p className="mt-2 text-ink-400">Configure how columns are displayed and formatted before confirming the import.</p>
                </div>
                <button onClick={() => setConfigs(null)} className="p-2 hover:bg-ink-800 rounded-lg text-ink-400 hover:text-ink-200 transition-colors">
                  <X className="w-6 h-6" />
                </button>
              </div>

              <div className="bg-ink-900 border border-ink-800/80 rounded-xl overflow-hidden shadow-2xl">
                {/* Sheet Name */}
                <div className="px-6 py-5 border-b border-ink-800/60 bg-ink-950/50">
                  <label className="block text-xs font-semibold text-ink-500 mb-2 uppercase tracking-widest">Sheet Name</label>
                  <input
                    type="text"
                    placeholder="e.g. Semester 4 Tracker"
                    className="w-full bg-ink-950 border border-ink-800 rounded-lg px-4 py-2.5 text-sm text-ink-100 outline-none focus:border-amber-500/50 placeholder:text-ink-600"
                    value={sheetLabel}
                    onChange={(e) => setSheetLabel(e.target.value)}
                  />
                </div>

                <table className="w-full text-left">
                  <thead className="bg-ink-950 border-b border-ink-800 text-xs uppercase tracking-widest text-ink-500 font-semibold">
                    <tr>
                      <th className="px-6 py-4 w-1/3">Original Column Name</th>
                      <th className="px-6 py-4 w-1/3">Display Label</th>
                      <th className="px-6 py-4 w-1/4">Data Type</th>
                      <th className="px-6 py-4 text-center">Visible</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-ink-800/40">
                    {configs.map((cfg, idx) => (
                      <tr key={cfg.sheet_key} className="hover:bg-ink-800/20 transition-colors">
                        <td className="px-6 py-4 text-sm text-ink-400 font-mono truncate max-w-xs" title={cfg.sheet_key}>
                          {cfg.sheet_key}
                        </td>
                        <td className="px-6 py-4">
                          <input 
                            type="text"
                            className="w-full bg-ink-950 border border-ink-800 rounded px-3 py-1.5 text-sm text-ink-100 outline-none focus:border-amber-500/50"
                            value={cfg.label}
                            onChange={(e) => updateConfig(idx, { label: e.target.value })}
                          />
                        </td>
                        <td className="px-6 py-4">
                          <select
                            value={cfg.data_type || ""}
                            onChange={(e) => updateConfig(idx, { data_type: e.target.value })}
                            className="w-full bg-ink-950 border border-ink-800 rounded px-3 py-1.5 text-sm text-ink-100 outline-none focus:border-amber-500/50"
                          >
                            <option value="text">Text</option>
                            <option value="number">Number</option>
                            <option value="date">Date</option>
                            <option value="link">Link</option>
                          </select>
                        </td>
                        <td className="px-6 py-4 text-center">
                          <input 
                            type="checkbox"
                            checked={cfg.is_visible}
                            onChange={(e) => updateConfig(idx, { is_visible: e.target.checked })}
                            className="rounded border-ink-700 bg-ink-950 text-amber-500 focus:ring-amber-500/50"
                          />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              <div className="mt-8 flex justify-end gap-4">
                <button
                  onClick={() => setConfigs(null)}
                  className="px-6 py-2.5 rounded-lg font-medium text-ink-300 hover:bg-ink-800 hover:text-ink-100 transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={handleSync}
                  disabled={syncing}
                  className="px-6 py-2.5 bg-amber-500 hover:bg-amber-400 text-amber-950 rounded-lg font-bold shadow-[0_0_15px_rgba(245,158,11,0.15)] flex items-center justify-center gap-2 transition-all disabled:opacity-50"
                >
                  {syncing ? <RefreshCw className="w-4 h-4 animate-spin" /> : null}
                  Confirm & Sync
                </button>
              </div>
            </div>
          </div>
        ) : (
          <Outlet />
        )}
      </main>
    </div>
  );
}

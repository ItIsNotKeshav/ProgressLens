import { useEffect, useState, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "../api";
import type { DiffResult, StudentDiff, Snapshot, Sheet, Field } from "../types";
import { ExternalLink } from "lucide-react";
import { formatIST } from "../utils";

export default function Diff() {
  const [searchParams] = useSearchParams();
  const paramStudent = searchParams.get("student");
  const paramSheet = searchParams.get("sheet");

  const [sheets, setSheets] = useState<Sheet[]>([]);
  const [selectedSheet, setSelectedSheet] = useState<number | null>(null);

  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [snapA, setSnapA] = useState<number | null>(null);
  const [snapB, setSnapB] = useState<number | null>(null);

  const [diffResult, setDiffResult] = useState<DiffResult | null>(null);
  const [fields, setFields] = useState<Field[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  const [searchQuery, setSearchQuery] = useState("");
  const [selectedStudentId, setSelectedStudentId] = useState<number | null>(
    paramStudent ? parseInt(paramStudent) : null
  );
  const [hideUnchanged, setHideUnchanged] = useState(false);

  // Load sheets on mount
  useEffect(() => {
    async function init() {
      try {
        const res = await api.getSheets();
        setSheets(res);
        // If a sheet param was passed (from Students page), use it; otherwise default to first
        const targetSheet = paramSheet ? parseInt(paramSheet) : (res.length > 0 ? res[0].id : null);
        if (targetSheet !== null) setSelectedSheet(targetSheet);
      } catch (e) {
        setError(String(e));
      }
    }
    init();
  }, []);

  // Load snapshots when sheet changes
  useEffect(() => {
    if (selectedSheet !== null) {
      loadSnapshots(selectedSheet);
      api.getFields().then(setFields).catch(() => {});
    }
  }, [selectedSheet]);

  async function loadSnapshots(sheetId: number, forceLatest = false) {
    try {
      const snaps = await api.getSnapshots(sheetId);
      setSnapshots(snaps);
      if (snaps.length >= 2 && (forceLatest || snapB === null)) {
        setSnapB(snaps[snaps.length - 1].id);
        setSnapA(snaps[snaps.length - 2].id);
      } else if (snaps.length < 2) {
        setSnapA(null);
        setSnapB(null);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  // Fetch diff when A or B changes
  useEffect(() => {
    if (snapA === null || snapB === null) return;
    
    let cancelled = false;
    async function run() {
      setLoading(true);
      setError(null);
      try {
        const res = await api.getDiff(snapA!, snapB!);
        if (!cancelled) setDiffResult(res);
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    run();
    return () => {
      cancelled = true;
    };
  }, [snapA, snapB]);

  // Derived selections
  const filteredStudents = useMemo(() => {
    if (!diffResult) return [];
    const q = searchQuery.toLowerCase();
    return diffResult.diffs.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.roll_number.toLowerCase().includes(q)
    );
  }, [diffResult, searchQuery]);

  // Auto-select first student if none selected
  useEffect(() => {
    if (filteredStudents.length > 0 && !filteredStudents.find(s => s.student_id === selectedStudentId)) {
      setSelectedStudentId(filteredStudents[0].student_id);
    }
  }, [filteredStudents, selectedStudentId]);

  const selectedStudent = useMemo(() => {
    if (!diffResult || selectedStudentId === null) return null;
    return diffResult.diffs.find((d) => d.student_id === selectedStudentId) || null;
  }, [diffResult, selectedStudentId]);

  // Helper to check if a field_key corresponds to a link-type field
  const isLinkField = (fieldKey: string) => 
    fields.some(f => f.sheet_key === fieldKey && f.data_type === 'link');

  const renderDiffValue = (val: string | null, fieldKey: string, style: string) => {
    if (!val) return <span className="text-ink-600">—</span>;
    if (isLinkField(fieldKey)) {
      return (
        <a href={val} target="_blank" rel="noopener noreferrer"
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-amber-500/10 text-amber-400 text-xs font-medium hover:bg-amber-500/20 transition-colors"
        >
          View <ExternalLink className="w-3 h-3" />
        </a>
      );
    }
    return <span className={`px-2 py-0.5 rounded font-mono text-xs ${style}`}>{val}</span>;
  };

  async function handleSync() {
    const url = window.prompt("Enter Google Sheet URL to sync:");
    if (!url) return;
    try {
      setSyncing(true);
      await api.syncSheet(selectedSheet!);
      await loadSnapshots(selectedSheet!, true);
    } catch (e) {
      alert("Sync failed: " + String(e));
    } finally {
      setSyncing(false);
    }
  }

  // Timeline click handler
  function selectTimelineNode(id: number) {
    const idx = snapshots.findIndex((s) => s.id === id);
    if (idx > 0) {
      setSnapA(snapshots[idx - 1].id);
      setSnapB(id);
    } else {
      // If they click the very first snapshot, we can't really do A=before, B=first
      // We just set A=first, B=first (diff will be empty) or alert
      setSnapA(id);
      setSnapB(id);
    }
  }

  const actSnapB = snapshots.find((s) => s.id === snapB);
  
  return (
    <div className="flex h-full bg-ink-950 text-ink-50 font-sans overflow-hidden">
      {/* Sidebar: Student List */}
      <aside className="w-64 shrink-0 flex flex-col border-r border-ink-800/60 bg-ink-900/30">
        <div className="p-4 border-b border-ink-800/60">
          <input
            type="text"
            placeholder="Search students..."
            className="w-full bg-ink-950 border border-ink-800 rounded-md px-3 py-1.5 text-sm outline-none focus:border-amber-500/50 transition-colors"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
        </div>
        <div className="flex-1 overflow-y-auto">
          {filteredStudents.length === 0 && (
            <div className="p-4 text-xs text-ink-500 text-center">
              No students found.
            </div>
          )}
          {filteredStudents.map((s) => (
            <StudentRowItem
              key={s.student_id}
              student={s}
              isSelected={s.student_id === selectedStudentId}
              onClick={() => setSelectedStudentId(s.student_id)}
            />
          ))}
        </div>
      </aside>

      {/* Main Diff Area */}
      <main className="flex-1 flex flex-col min-w-0 bg-ink-950 relative">
        {/* Top Toolbar */}
        <header className="h-14 shrink-0 border-b border-ink-800/60 flex items-center justify-between px-4 lg:px-6 bg-ink-950/80 backdrop-blur z-10 overflow-x-auto no-scrollbar">
          <div className="flex items-center gap-4 lg:gap-6 shrink-0 min-w-max">
            <div className="flex items-center gap-2">
              <span className="text-xs font-medium text-ink-400 uppercase tracking-widest">Sheet</span>
              <select
                className="bg-ink-900 border border-ink-800 rounded text-sm py-1 px-2 outline-none focus:border-amber-500/50 max-w-[150px] truncate"
                value={selectedSheet ?? ""}
                onChange={(e) => setSelectedSheet(parseInt(e.target.value))}
                disabled={sheets.length === 0}
              >
                {sheets.length === 0 && <option value="">No sheets</option>}
                {sheets.map((s) => (
                   <option key={s.id} value={s.id}>{s.label}</option>
                ))}
              </select>

              <span className="text-xs font-medium text-ink-400 uppercase tracking-widest ml-2">Compare</span>
              <select
                className="bg-ink-900 border border-ink-800 rounded text-sm py-1 px-2 outline-none focus:border-amber-500/50"
                value={snapA || ""}
                onChange={(e) => setSnapA(parseInt(e.target.value))}
              >
                {snapshots.map((s) => (
                  <option key={s.id} value={s.id}>
                    Snap {s.id} ({formatIST(s.synced_at)})
                  </option>
                ))}
              </select>
              <span className="text-ink-600">→</span>
              <select
                className="bg-ink-900 border border-ink-800 rounded text-sm py-1 px-2 outline-none focus:border-amber-500/50"
                value={snapB || ""}
                onChange={(e) => setSnapB(parseInt(e.target.value))}
              >
                {snapshots.map((s) => (
                  <option key={s.id} value={s.id}>
                    Snap {s.id} ({formatIST(s.synced_at)})
                  </option>
                ))}
              </select>
            </div>

            {diffResult && (
              <div className="flex items-center gap-3">
                <span className="badge-green px-2 py-0.5 rounded text-xs font-semibold">
                  +{diffResult.total_added} new
                </span>
                <span className="badge-amber bg-amber-500/10 text-amber-500 px-2 py-0.5 rounded text-xs font-semibold">
                  {diffResult.total_changed} changed
                </span>
              </div>
            )}
          </div>

          <div className="flex items-center gap-4 shrink-0">
            {actSnapB && (
              <span className="text-xs text-ink-500 font-mono">
                {new Date(actSnapB.synced_at).toLocaleString()}
              </span>
            )}
            <button
              onClick={handleSync}
              disabled={syncing}
              className="px-3 py-1.5 bg-ink-100 text-ink-950 rounded-md text-sm font-medium hover:bg-white transition-colors disabled:opacity-50"
            >
              {syncing ? "Syncing..." : "Sync Now"}
            </button>
          </div>
        </header>

        {/* Diff Content */}
        <div className="flex-1 overflow-y-auto p-8">
          {error ? (
            <div className="text-red-400 text-sm bg-red-400/10 p-4 rounded-md inline-block">{error}</div>
          ) : loading ? (
            <div className="flex items-center gap-2 text-ink-500 text-sm">
              <Spinner /> Computing diff...
            </div>
          ) : !diffResult || !selectedStudent ? (
            <div className="text-ink-500 text-sm text-center mt-20">
              {snapshots.length < 2
                ? "Not enough snapshots to compare."
                : "Select a student to view their diff."}
            </div>
          ) : (
            <div className="max-w-4xl mx-auto animate-fade-in">
              <div className="mb-8 flex items-end justify-between">
                <div>
                  <h2 className="text-3xl font-display font-bold text-ink-50 tracking-tight">
                    {selectedStudent.name}
                  </h2>
                  <div className="mt-2 flex items-center gap-3 text-sm text-ink-400 font-mono">
                    <span>{selectedStudent.roll_number}</span>
                    <span>•</span>
                    <span className="text-ink-300">
                      {selectedStudent.field_diffs.filter(f => f.changed).length} fields changed
                    </span>
                  </div>
                </div>
                <label className="flex items-center gap-2 text-xs text-ink-400 cursor-pointer hover:text-ink-200 transition-colors">
                  <input
                    type="checkbox"
                    className="rounded border-ink-700 bg-ink-900 text-amber-500 focus:ring-0 focus:ring-offset-0"
                    checked={hideUnchanged}
                    onChange={(e) => setHideUnchanged(e.target.checked)}
                  />
                  Hide unchanged fields
                </label>
              </div>

              {selectedStudent.status === "added" && (
                <div className="mb-6 p-3 bg-sage-500/10 border border-sage-500/20 text-sage-400 text-sm rounded-md flex items-center gap-2">
                  <svg viewBox="0 0 20 20" fill="currentColor" className="w-4 h-4"><path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm.75-11.25a.75.75 0 00-1.5 0v2.5h-2.5a.75.75 0 000 1.5h2.5v2.5a.75.75 0 001.5 0v-2.5h2.5a.75.75 0 000-1.5h-2.5v-2.5z" clipRule="evenodd" /></svg>
                  New student in this snapshot.
                </div>
              )}

              {selectedStudent.status === "removed" && (
                <div className="mb-6 p-3 bg-red-500/10 border border-red-500/20 text-red-400 text-sm rounded-md flex items-center gap-2">
                  <svg viewBox="0 0 20 20" fill="currentColor" className="w-4 h-4"><path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.28 7.22a.75.75 0 00-1.06 1.06L8.94 10l-1.72 1.72a.75.75 0 101.06 1.06L10 11.06l1.72 1.72a.75.75 0 101.06-1.06L11.06 10l1.72-1.72a.75.75 0 00-1.06-1.06L10 8.94 8.28 7.22z" clipRule="evenodd" /></svg>
                  Student removed or missing in this snapshot.
                </div>
              )}

              <div className="rounded-lg border border-ink-800/60 overflow-hidden bg-ink-950">
                <table className="w-full text-left text-sm whitespace-nowrap">
                  <thead className="bg-ink-900/50 border-b border-ink-800/60 text-xs uppercase tracking-widest text-ink-500 font-semibold">
                    <tr>
                      <th className="px-5 py-3 font-medium">Field</th>
                      <th className="px-5 py-3 font-medium">Old Value</th>
                      <th className="px-5 py-3 font-medium">New Value</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-ink-800/40">
                    {selectedStudent.field_diffs
                      .filter(f => !hideUnchanged || f.changed)
                      .map((f) => (
                      <tr 
                        key={f.field_key} 
                        className={`hover:bg-ink-800/20 transition-colors ${!f.changed ? 'opacity-50' : ''}`}
                      >
                        <td className="px-5 py-3 text-ink-300 font-mono text-xs w-1/3 truncate">
                          {f.label}
                        </td>
                        <td className="px-5 py-3 w-1/3">
                          {renderDiffValue(f.value_a, f.field_key, f.changed ? 'bg-red-500/10 text-red-400' : 'text-ink-400')}
                        </td>
                        <td className="px-5 py-3 w-1/3">
                          {renderDiffValue(f.value_b, f.field_key, f.changed ? 'bg-sage-500/10 text-sage-400' : 'text-ink-400')}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>

        {/* Timeline Strip */}
        <footer className="h-16 shrink-0 border-t border-ink-800/60 bg-ink-900/30 flex items-center px-6 overflow-x-auto custom-scrollbar">
          <div className="flex items-center gap-2">
            {snapshots.map((s, index) => {
              const isActiveB = s.id === snapB;
              const isActiveA = s.id === snapA;
              return (
                <div key={s.id} className="flex items-center">
                  <button
                    onClick={() => selectTimelineNode(s.id)}
                    className={`relative flex flex-col items-center justify-center min-w-[60px] group`}
                  >
                    <div className={`w-3 h-3 rounded-full transition-colors ${isActiveB ? 'bg-amber-400 ring-4 ring-amber-400/20' : isActiveA ? 'bg-ink-400' : 'bg-ink-700 group-hover:bg-ink-500'}`} />
                    <span className={`mt-1.5 text-[10px] uppercase font-mono tracking-widest ${isActiveB ? 'text-amber-400' : 'text-ink-500'}`}>
                      {index + 1}
                    </span>
                  </button>
                  {index < snapshots.length - 1 && (
                    <div className="w-8 h-px bg-ink-800" />
                  )}
                </div>
              );
            })}
          </div>
        </footer>
      </main>
    </div>
  );
}

function StudentRowItem({
  student,
  isSelected,
  onClick,
}: {
  student: StudentDiff;
  isSelected: boolean;
  onClick: () => void;
}) {
  const changes = student.field_diffs.filter(f => f.changed).length;
  
  let dotColor = "bg-ink-600";
  if (student.status === "added") dotColor = "bg-sage-400";
  if (student.status === "removed") dotColor = "bg-red-400";
  if (student.status === "changed") dotColor = "bg-amber-400";

  return (
    <button
      onClick={onClick}
      className={`w-full text-left px-4 py-3 flex flex-col gap-1 border-b border-ink-800/30 transition-colors ${
        isSelected ? "bg-ink-800/60" : "hover:bg-ink-800/30"
      }`}
    >
      <div className="flex justify-between items-center">
        <span className={`text-sm font-medium ${isSelected ? 'text-ink-50' : 'text-ink-200'} truncate pr-2`}>
          {student.name}
        </span>
        <div className={`w-2 h-2 rounded-full shrink-0 ${dotColor}`} />
      </div>
      <div className="flex justify-between items-center text-xs text-ink-500 font-mono">
        <span>{student.roll_number}</span>
        {changes > 0 && <span className="">{changes} chg</span>}
        {student.status === "added" && <span className="">new</span>}
      </div>
    </button>
  );
}

function Spinner() {
  return (
    <svg className="animate-spin w-4 h-4" viewBox="0 0 24 24" fill="none">
      <circle className="opacity-20" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="3" />
      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 0 1 8-8V0C5.372 0 0 5.372 0 12h4Z" />
    </svg>
  );
}

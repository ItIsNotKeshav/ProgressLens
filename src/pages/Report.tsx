import { useState, useEffect, useRef } from "react";
import { api } from "../api";
import type { ReportConfig, Snapshot, StudentRow, Field, Sheet } from "../types";
import { FileText, Printer, Copy, CheckSquare, Square } from "lucide-react";
import { formatIST } from "../utils";

export default function Report() {
  const iframeRef = useRef<HTMLIFrameElement>(null);

  const [sheets, setSheets] = useState<Sheet[]>([]);
  const [selectedSheet, setSelectedSheet] = useState<number | null>(null);

  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [snapshotId, setSnapshotId] = useState<number | null>(null);

  const [students, setStudents] = useState<StudentRow[]>([]);
  const [fields, setFields] = useState<Field[]>([]);

  const [selStudents, setSelStudents] = useState<Set<number>>(new Set());
  const [selFields, setSelFields] = useState<Set<number>>(new Set());
  const [includeNotes, setIncludeNotes] = useState(true);
  const [includeSummary, setIncludeSummary] = useState(false);

  const [htmlOutput, setHtmlOutput] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  // Initialize data
  useEffect(() => {
    async function init() {
      try {
        const [sheetRes, allFields] = await Promise.all([
          api.getSheets(),
          api.getFields(),
        ]);
        setSheets(sheetRes);
        setFields(allFields);

        if (sheetRes.length > 0) {
          setSelectedSheet(sheetRes[0].id);
          // Scope fields to the first sheet
          const sheetFields = allFields.filter(f => f.sheet_id === sheetRes[0].id && f.is_visible);
          setSelFields(new Set(sheetFields.map(f => f.id)));
        }
      } catch (err) {
        setError(String(err));
      }
    }
    init();
  }, []);

  // Fetch snapshots for selected sheet
  useEffect(() => {
    if (selectedSheet === null) return;
    async function fetchSnaps() {
      try {
        const snaps = await api.getSnapshots(selectedSheet!);
        setSnapshots(snaps);
        if (snaps.length > 0) {
          setSnapshotId(snaps[snaps.length - 1].id);
        } else {
          setSnapshotId(null);
        }
      } catch (e) {
        setError(String(e));
      }
    }
    fetchSnaps();
  }, [selectedSheet]);

  // Sync students when snapshot changes
  useEffect(() => {
    if (selectedSheet === null || snapshotId === null) return;
    async function fetchStudents() {
      try {
        const rows = await api.getAllStudents(selectedSheet!, snapshotId!);
        setStudents(rows);
        // Default select all
        setSelStudents(new Set(rows.map(r => r.id)));
      } catch (err) {
        setError(String(err));
      }
    }
    fetchStudents();
  }, [selectedSheet, snapshotId]);

  async function handleGenerate() {
    if (snapshotId === null || selStudents.size === 0 || selFields.size === 0) {
        setError("Please select at least one student and one field.");
        return;
    }

    const config: ReportConfig = {
      snapshot_id: snapshotId,
      student_ids: Array.from(selStudents),
      field_ids: Array.from(selFields),
      include_progress_notes: includeNotes,
      include_summary: includeSummary,
    };

    try {
      setGenerating(true);
      setError(null);
      setHtmlOutput(null);
      const html = await api.generateReport(config);
      setHtmlOutput(html);
    } catch (e) {
      setError(String(e));
    } finally {
      setGenerating(false);
    }
  }

  function toggleStudent(id: number) {
    setSelStudents(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleField(id: number) {
    setSelFields(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function handlePrint() {
    if (iframeRef.current && iframeRef.current.contentWindow) {
      iframeRef.current.contentWindow.print();
    }
  }

  async function handleCopy() {
    if (htmlOutput) {
      await navigator.clipboard.writeText(htmlOutput);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }

  return (
    <div className="flex h-full bg-ink-950 text-ink-50 overflow-hidden font-sans">
      
      {/* Left Config Panel */}
      <aside className="w-[360px] shrink-0 flex flex-col border-r border-ink-800/60 bg-ink-900/30 overflow-y-auto custom-scrollbar relative z-10">
        <header className="px-6 py-6 border-b border-ink-800/60 bg-ink-950/80 sticky top-0 backdrop-blur">
          <h2 className="text-xl font-display font-bold track-tight text-ink-50 mb-1">
            Build Report
          </h2>
          <p className="text-sm text-ink-400">Export clean print-ready docs.</p>
        </header>

        <div className="p-6 space-y-8 flex-1">
            
          {/* Target Sheet */}
          <section>
            <label className="block text-xs font-semibold text-ink-500 mb-2 uppercase tracking-widest">
              Target Sheet
            </label>
            <select
              className="w-full bg-ink-950 border border-ink-800 rounded-lg text-sm px-3 py-2 outline-none focus:border-amber-500/50 text-ink-100 truncate"
              value={selectedSheet ?? ""}
              onChange={(e) => setSelectedSheet(parseInt(e.target.value))}
              disabled={sheets.length === 0}
            >
              {sheets.length === 0 && <option value="">No sheets</option>}
              {sheets.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.label}
                </option>
              ))}
            </select>
          </section>

          {/* Target Snapshot */}
          <section>
            <label className="block text-xs font-semibold text-ink-500 mb-2 uppercase tracking-widest">
              Target Snapshot
            </label>
            <select
              className="w-full bg-ink-950 border border-ink-800 rounded-lg text-sm px-3 py-2 outline-none focus:border-amber-500/50 text-ink-100"
              value={snapshotId ?? ""}
              onChange={(e) => setSnapshotId(parseInt(e.target.value))}
            >
              {snapshots.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.source_label} ({formatIST(s.synced_at)})
                </option>
              ))}
            </select>
          </section>

          {/* Students Selection */}
          <section>
            <div className="flex items-center justify-between mb-2">
                <label className="text-xs font-semibold text-ink-500 uppercase tracking-widest">
                Students ({selStudents.size}/{students.length})
                </label>
                <div className="flex gap-2 text-xs text-amber-500/80">
                    <button onClick={() => setSelStudents(new Set(students.map(s => s.id)))} className="hover:text-amber-400">All</button>
                    <span className="text-ink-700">|</span>
                    <button onClick={() => setSelStudents(new Set())} className="hover:text-amber-400">None</button>
                </div>
            </div>
            <div className="bg-ink-950 border border-ink-800/80 rounded-lg max-h-48 overflow-y-auto p-1 custom-scrollbar">
                {students.map(s => (
                    <button
                      key={s.id}
                      onClick={() => toggleStudent(s.id)}
                      className="w-full flex items-center gap-3 px-3 py-1.5 hover:bg-ink-800/50 rounded transition-colors text-left"
                    >
                      {selStudents.has(s.id) ? (
                        <CheckSquare className="w-4 h-4 text-amber-500 shrink-0" />
                      ) : (
                        <Square className="w-4 h-4 text-ink-600 shrink-0" />
                      )}
                      <div className="truncate min-w-0">
                        <span className="text-sm text-ink-200">{s.name}</span>
                      </div>
                    </button>
                ))}
            </div>
          </section>

          {/* Fields Selection */}
          <section>
            <div className="flex items-center justify-between mb-2">
                <label className="text-xs font-semibold text-ink-500 uppercase tracking-widest">
                Columns ({selFields.size}/{fields.length})
                </label>
                <div className="flex gap-2 text-xs text-amber-500/80">
                    <button onClick={() => setSelFields(new Set(fields.map(s => s.id)))} className="hover:text-amber-400">All</button>
                    <span className="text-ink-700">|</span>
                    <button onClick={() => setSelFields(new Set())} className="hover:text-amber-400">None</button>
                </div>
            </div>
            <div className="bg-ink-950 border border-ink-800/80 rounded-lg max-h-48 overflow-y-auto p-1 custom-scrollbar">
                {fields.map(f => (
                    <button
                      key={f.id}
                      onClick={() => toggleField(f.id)}
                      className="w-full flex items-center gap-3 px-3 py-1.5 hover:bg-ink-800/50 rounded transition-colors text-left"
                    >
                      {selFields.has(f.id) ? (
                        <CheckSquare className="w-4 h-4 text-amber-500 shrink-0" />
                      ) : (
                        <Square className="w-4 h-4 text-ink-600 shrink-0" />
                      )}
                      <div className="truncate min-w-0 text-sm text-ink-200">
                        {f.label} <span className="text-[10px] text-ink-600 font-mono ml-1">{f.data_type}</span>
                      </div>
                    </button>
                ))}
            </div>
          </section>

          {/* Settings */}
          <section className="space-y-4">
             <label className="flex items-start gap-3 cursor-pointer group">
                <input
                  type="checkbox"
                  className="mt-0.5 rounded border-ink-700 bg-ink-950 text-amber-500 focus:ring-0 focus:ring-offset-0 cursor-pointer"
                  checked={includeSummary}
                  onChange={(e) => setIncludeSummary(e.target.checked)}
                />
                <div className="flex flex-col">
                    <span className="text-sm font-medium text-ink-200 group-hover:text-ink-50 transition-colors">Include Summary Stats</span>
                    <span className="text-xs text-ink-500 mt-0.5">Show average scores and distribution cards above the data table.</span>
                </div>
             </label>

             <label className="flex items-start gap-3 cursor-pointer group">
                <input
                  type="checkbox"
                  className="mt-0.5 rounded border-ink-700 bg-ink-950 text-amber-500 focus:ring-0 focus:ring-offset-0 cursor-pointer"
                  checked={includeNotes}
                  onChange={(e) => setIncludeNotes(e.target.checked)}
                />
                <div className="flex flex-col">
                    <span className="text-sm font-medium text-ink-200 group-hover:text-ink-50 transition-colors">Include Progress Notes</span>
                    <span className="text-xs text-ink-500 mt-0.5">Compares scores against previous snapshot to identify improvements &amp; regressions.</span>
                </div>
             </label>
          </section>

        </div>
        
        <div className="p-6 border-t border-ink-800/60 bg-ink-950/80 sticky bottom-0 backdrop-blur mt-auto">
            {error && <div className="text-xs text-red-400 mb-3 bg-red-400/10 p-2 rounded">{error}</div>}
            
            <button
              onClick={handleGenerate}
              disabled={generating || snapshotId === null || selStudents.size === 0 || selFields.size === 0}
              className="w-full flex items-center justify-center gap-2 py-2.5 px-4 bg-amber-500 hover:bg-amber-400 text-amber-950 rounded-lg font-semibold text-sm transition-all shadow-[0_0_15px_rgba(245,158,11,0.15)] disabled:opacity-50 disabled:shadow-none"
            >
              <FileText className="w-4 h-4" />
              {generating ? "Generating..." : "Generate Report"}
            </button>
        </div>
      </aside>

      {/* Right Preview Panel */}
      <main className="flex-1 flex flex-col min-w-0 bg-ink-950 shadow-inner relative">
        <header className="h-14 shrink-0 border-b border-ink-800/60 flex items-center justify-between px-6 bg-ink-950/80 backdrop-blur z-10">
           <div className="flex items-center gap-2 text-ink-400">
             <span className="text-xs font-semibold uppercase tracking-widest">Document Preview</span>
           </div>
           
           <div className="flex items-center gap-3">
             <button
               onClick={handleCopy}
               disabled={!htmlOutput}
               className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium bg-ink-900 border border-ink-800 text-ink-300 hover:text-ink-50 hover:border-ink-600 rounded-md transition-colors disabled:opacity-50"
             >
               <Copy className="w-3.5 h-3.5" />
               {copied ? "Copied!" : "Copy HTML"}
             </button>
             
             <button
               onClick={handlePrint}
               disabled={!htmlOutput}
               className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium bg-ink-100 text-ink-950 hover:bg-white rounded-md transition-colors disabled:opacity-50"
             >
               <Printer className="w-3.5 h-3.5" />
               Print / Save PDF
             </button>
           </div>
        </header>

        <div className="flex-1 overflow-hidden relative bg-ink-900/10">
          {!htmlOutput && !generating && (
             <div className="absolute inset-0 flex flex-col items-center justify-center text-ink-500 gap-3">
                <FileText className="w-12 h-12 opacity-20" />
                <p className="text-sm">Configure your report on the left and click Generate.</p>
             </div>
          )}
          
          {generating && (
             <div className="absolute inset-0 flex items-center justify-center bg-ink-950/50 backdrop-blur-sm z-20">
                <div className="flex items-center gap-3 text-amber-500">
                    <Spinner />
                    <span className="text-sm font-medium tracking-wide animate-pulse">Building Document...</span>
                </div>
             </div>
          )}

          {htmlOutput && (
             <iframe 
               ref={iframeRef}
               srcDoc={htmlOutput} 
               className="w-full h-full border-none bg-white rounded-tl-xl shadow-2xl"
               title="Report Preview"
             />
          )}
        </div>
      </main>
    </div>
  );
}

function Spinner() {
  return (
    <svg className="animate-spin w-5 h-5" viewBox="0 0 24 24" fill="none">
      <circle className="opacity-20" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="3" />
      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 0 1 8-8V0C5.372 0 0 5.372 0 12h4Z" />
    </svg>
  );
}

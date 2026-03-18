import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../api";
import type { StudentRow, Sheet, Field } from "../types";
import { ExternalLink } from "lucide-react";

export default function Students() {
  const navigate = useNavigate();
  const [sheets, setSheets] = useState<Sheet[]>([]);
  const [selectedSheet, setSelectedSheet] = useState<number | null>(null);

  const [students, setStudents] = useState<StudentRow[]>([]);
  const [fields, setFields] = useState<Field[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");

  useEffect(() => {
    api.getSheets().then(res => {
      setSheets(res);
      if (res.length > 0) setSelectedSheet(res[0].id);
      else setLoading(false);
    }).catch(e => {
      setError(String(e));
      setLoading(false);
    });
  }, []);

  useEffect(() => {
    if (selectedSheet === null) return;
    loadStudents(selectedSheet);
  }, [selectedSheet]);

  async function loadStudents(sheetId: number) {
    try {
      setLoading(true);
      const [data, allFields] = await Promise.all([
        api.getAllStudents(sheetId),
        api.getFields(),
      ]);
      setStudents(data);
      setFields(allFields);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  // Build visible columns from fields that match the selected sheet
  const visibleFields = fields.filter(f => f.is_visible && f.sheet_id === selectedSheet);
  const columns = visibleFields.length > 0
    ? visibleFields
    : (students.length > 0 ? Object.keys(students[0].values).map(k => ({ sheet_key: k, label: k, data_type: 'text' } as Field)) : []);

  const filtered = students.filter(
    (s) =>
      s.name.toLowerCase().includes(search.toLowerCase()) ||
      s.roll_number.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="p-8">
      {/* Header */}
      <div className="mb-6 animate-fade-in flex items-start justify-between">
        <div className="flex items-center gap-4">
          <div>
            <h1 className="font-display text-3xl font-bold text-ink-50">
              Students
            </h1>
            <p className="mt-1 text-sm text-ink-400">
              All students with their latest field values.
            </p>
          </div>
          <select
            className="bg-ink-900 border border-ink-800 rounded-md text-sm py-1.5 px-3 outline-none focus:border-amber-500/50 text-ink-100 max-w-[200px] truncate"
            value={selectedSheet ?? ""}
            onChange={(e) => setSelectedSheet(parseInt(e.target.value))}
            disabled={sheets.length === 0}
          >
            {sheets.length === 0 && <option value="">No sheets</option>}
            {sheets.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}
          </select>
        </div>
        <div className="w-64">
          <input
            type="search"
            className="input-field"
            placeholder="Search by name or roll…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      </div>

      {loading ? (
        <div className="text-ink-500 text-sm flex items-center gap-2">
          <Spinner /> Loading students…
        </div>
      ) : error ? (
        <div className="card p-4 text-red-400 text-sm">{error}</div>
      ) : filtered.length === 0 ? (
        <EmptyState hasData={students.length > 0} />
      ) : (
        <div className="card overflow-hidden animate-slide-up">
          <div className="overflow-x-auto">
            <table className="data-table">
              <thead>
                <tr>
                  <th>#</th>
                  <th>Name</th>
                  <th>Roll No.</th>
                  {columns.map((col) => (
                    <th key={'sheet_key' in col ? col.sheet_key : col}>
                      {'label' in col ? col.label : col}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {filtered.map((s, i) => (
                  <tr key={s.id} 
                    className="cursor-pointer hover:bg-ink-800/30 transition-colors"
                    onClick={() => navigate(`/diff?student=${s.id}&sheet=${selectedSheet}`)}
                  >
                    <td className="text-ink-600 font-mono text-xs tabular-nums">
                      {i + 1}
                    </td>
                    <td className="font-medium text-ink-100">{s.name}</td>
                    <td className="font-mono text-xs text-ink-400">
                      {s.roll_number}
                    </td>
                    {columns.map((col) => {
                      const key = 'sheet_key' in col ? col.sheet_key : String(col);
                      const val = s.values[key] ?? "";
                      const isLink = 'data_type' in col && col.data_type === 'link';
                      return (
                        <td key={key}>
                          {isLink ? (
                            val ? (
                              <a href={val} target="_blank" rel="noopener noreferrer"
                                className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-amber-500/10 text-amber-400 text-xs font-medium hover:bg-amber-500/20 transition-colors"
                              >
                                View <ExternalLink className="w-3 h-3" />
                              </a>
                            ) : (
                              <span className="text-ink-600">—</span>
                            )
                          ) : (
                            val || "—"
                          )}
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="px-4 py-2.5 border-t border-ink-800/60 text-xs text-ink-500">
            {filtered.length} student{filtered.length !== 1 ? "s" : ""}
            {search && ` matching "${search}"`}
          </div>
        </div>
      )}
    </div>
  );
}

function EmptyState({ hasData }: { hasData: boolean }) {
  return (
    <div className="card p-10 text-center animate-fade-in">
      <p className="text-ink-400 text-sm">
        {hasData
          ? "No students match your search."
          : "No students yet. Sync a sheet from the Dashboard to populate data."}
      </p>
    </div>
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

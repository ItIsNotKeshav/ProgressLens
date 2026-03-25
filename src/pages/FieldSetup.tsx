import { useState, useEffect } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { api } from "../api";
import type { FieldSetup as FieldSetupType } from "../types";
import { Save, Settings2, ArrowLeft } from "lucide-react";

export default function FieldSetup() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const sheetIdStr = searchParams.get("sheet_id");
  const sheetId = sheetIdStr ? parseInt(sheetIdStr, 10) : null;

  const [fields, setFields] = useState<FieldSetupType[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    async function load() {
      if (!sheetId) return;
      try {
        const data = await api.getFieldSetup(sheetId);
        setFields(data);
      } catch (err) {
        console.error("Failed to load field setup", err);
      } finally {
        setLoading(false);
      }
    }
    load();
  }, [sheetId]);

  if (!sheetId) {
    return (
      <div className="p-8 text-center text-ink-400">
        <p>No sheet selected. Go to Dashboard and select a sheet first.</p>
        <button
          onClick={() => navigate("/dashboard")}
          className="mt-4 px-4 py-2 bg-ink-800 rounded-md text-ink-200"
        >
          Back to Dashboard
        </button>
      </div>
    );
  }

  const updateField = (idx: number, updates: Partial<FieldSetupType>) => {
    const newFields = [...fields];
    newFields[idx] = { ...newFields[idx], ...updates };
    setFields(newFields);
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await api.saveFieldSetup(fields);
      navigate("/dashboard");
    } catch (e) {
      alert("Failed to save field setup: " + e);
    } finally {
      setSaving(false);
    }
  };

  // Group fields by type
  const groupedFields = fields.reduce((acc, field) => {
    const type = field.data_type || "unclassified";
    if (!acc[type]) acc[type] = [];
    acc[type].push(field);
    return acc;
  }, {} as Record<string, FieldSetupType[]>);

  return (
    <div className="flex flex-col h-full bg-ink-950 p-6 md:p-8 xl:p-10 max-w-6xl mx-auto w-full">
      <header className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 mb-8 shrink-0">
        <div className="flex flex-col">
          <button 
            onClick={() => navigate("/dashboard")}
            className="flex items-center text-ink-500 hover:text-ink-300 transition-colors text-xs font-semibold uppercase tracking-widest mb-3 w-fit"
          >
            <ArrowLeft className="w-4 h-4 mr-1" /> Back
          </button>
          <h1 className="text-3xl font-display font-medium text-ink-50 tracking-tight flex items-center gap-3">
            <Settings2 className="w-8 h-8 text-amber-500" />
            Column Setup
          </h1>
          <p className="text-ink-400 mt-2 text-sm max-w-xl">
            Verify and configure how your sheet's columns are interpreted. This ensures accurate charts and filtering.
          </p>
        </div>
        <button
          onClick={handleSave}
          disabled={saving || loading}
          className="flex items-center gap-2 bg-amber-500 text-amber-950 px-5 py-2.5 rounded-lg font-semibold hover:bg-amber-400 transition-colors shadow-[0_0_15px_rgba(245,158,11,0.15)] disabled:opacity-50"
        >
          <Save className="w-5 h-5" />
          {saving ? "Saving..." : "Save and Continue"}
        </button>
      </header>

      {loading ? (
        <div className="flex-1 flex items-center justify-center">
          <div className="w-8 h-8 border-4 border-amber-500/30 border-t-amber-500 rounded-full animate-spin"></div>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto pr-2 pb-12 custom-scrollbar space-y-8">
          {Object.entries(groupedFields).map(([type, group]) => (
            <div key={type} className="bg-ink-900/50 rounded-xl border border-ink-800 overflow-hidden">
              <div className="px-4 py-3 bg-ink-900 border-b border-ink-800">
                <h3 className="text-xs font-bold text-ink-300 uppercase tracking-widest">
                  {type === "unclassified" ? "Unclassified (Please Review)" : `${type} Columns`}
                </h3>
              </div>
              <div className="divide-y divide-ink-800/40">
                {group.map((field) => {
                  const globalIdx = fields.findIndex(f => f.id === field.id);
                  const isNumberOrLevel = field.data_type === "score" || field.data_type === "level";

                  return (
                    <div key={field.id} className="p-4 flex flex-col xl:flex-row gap-4 xl:items-start hover:bg-ink-900/30 transition-colors">
                      {/* Left side: Original Label & Preview */}
                      <div className="flex-1 min-w-[300px]">
                        <div className="text-sm font-mono text-ink-300 mb-2 truncate" title={field.sheet_key}>
                          <span className="text-ink-100">{field.label}</span>
                        </div>
                        <div className="text-xs text-ink-500 font-mono">
                          Samples:{" "}
                          <span className="text-ink-400">
                            {field.sample_values.join(", ") || "No data"}
                          </span>
                        </div>
                      </div>

                      {/* Right side: Controls */}
                      <div className="flex flex-wrap items-center gap-3">
                        <div className="flex flex-col gap-1 w-48">
                          <label className="text-[10px] uppercase tracking-widest text-ink-500 font-semibold">Display Name</label>
                          <input
                            type="text"
                            value={field.display_name}
                            onChange={(e) => updateField(globalIdx, { display_name: e.target.value })}
                            className="bg-ink-950 border border-ink-800 rounded px-2.5 py-1.5 text-sm text-ink-100 outline-none focus:border-amber-500/50 placeholder:text-ink-700"
                          />
                        </div>

                        <div className="flex flex-col gap-1 w-36">
                          <label className="text-[10px] uppercase tracking-widest text-ink-500 font-semibold">Type</label>
                          <select
                            value={field.data_type || ""}
                            onChange={(e) => updateField(globalIdx, { data_type: e.target.value })}
                            className="bg-ink-950 border border-ink-800 rounded px-2.5 py-1.5 text-sm text-ink-100 outline-none focus:border-amber-500/50"
                          >
                            <option value="score">Numeric Score</option>
                            <option value="level">Level Track</option>
                            <option value="categorical">Categorical</option>
                            <option value="text">Free Text</option>
                            <option value="link">Link</option>
                            <option value="identifier">Identifier (Name/USN)</option>
                          </select>
                        </div>

                        {isNumberOrLevel ? (
                          <div className="flex flex-col gap-1 w-20">
                            <label className="text-[10px] uppercase tracking-widest text-ink-500 font-semibold">Max</label>
                            <input
                              type="number"
                              value={field.max_value || ""}
                              onChange={(e) => updateField(globalIdx, { max_value: e.target.value ? parseFloat(e.target.value) : null })}
                              className="bg-ink-950 border border-ink-800 rounded px-2.5 py-1.5 text-sm text-ink-100 outline-none focus:border-amber-500/50"
                              placeholder="100"
                            />
                          </div>
                        ) : (
                          <div className="w-20" /> // spacer to maintain alignment
                        )}

                        <div className="flex flex-col gap-2 pt-2">
                          <label className="flex items-center gap-2 cursor-pointer">
                            <input
                              type="checkbox"
                              checked={field.is_visible}
                              onChange={(e) => updateField(globalIdx, { is_visible: e.target.checked })}
                              className="w-4 h-4 rounded border-ink-700 bg-ink-950 text-amber-500 focus:ring-amber-500/50"
                            />
                            <span className="text-xs text-ink-400">Show in Students</span>
                          </label>
                          
                          <label className="flex items-center gap-2 cursor-pointer">
                            <input
                              type="checkbox"
                              checked={field.include_in_dashboard}
                              onChange={(e) => updateField(globalIdx, { include_in_dashboard: e.target.checked })}
                              className="w-4 h-4 rounded border-ink-700 bg-ink-950 text-amber-500 focus:ring-amber-500/50"
                            />
                            <span className="text-xs text-ink-400">Show in Dashboard</span>
                          </label>
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

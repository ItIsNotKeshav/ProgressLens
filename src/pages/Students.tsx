import { useEffect, useState, useMemo } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { api } from "../api";
import type { StudentRow, Sheet, Field } from "../types";
import { Search, X, Filter, Columns, Check } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

export default function Students() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  
  const [sheets, setSheets] = useState<Sheet[]>([]);
  const [selectedSheet, setSelectedSheet] = useState<number | null>(
    searchParams.has("sheet") ? parseInt(searchParams.get("sheet")!, 10) : null
  );

  const [students, setStudents] = useState<StudentRow[]>([]);
  const [fields, setFields] = useState<Field[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Filters State
  const [searchQuery, setSearchQuery] = useState("");
  const [trackLevels, setTrackLevels] = useState<Record<number, number>>({});
  const [statuses, setStatuses] = useState<Set<string>>(new Set());
  const [selectedSections, setSelectedSections] = useState<Set<string>>(new Set());
  const [scoreRanges, setScoreRanges] = useState<Record<number, number>>({});
  const [categoricalFilters, setCategoricalFilters] = useState<Record<number, Set<string>>>({});
  
  // Columns picker
  const [hiddenCols, setHiddenCols] = useState<Set<number>>(new Set());
  const [showColPicker, setShowColPicker] = useState(false);

  // Export toast
  const [exportToast, setExportToast] = useState<string | null>(null);

  // Load sheets on mount
  useEffect(() => {
    api.getSheets().then(res => {
      setSheets(res);
      if (res.length > 0 && selectedSheet === null) {
        setSelectedSheet(res[0].id);
      } else if (res.length === 0) {
        setLoading(false);
      }
    }).catch(e => {
      setError(String(e));
      setLoading(false);
    });
  }, []);

  // Sync selectedSheet to URL and load students
  useEffect(() => {
    if (selectedSheet === null) return;
    
    // Update URL if missing
    if (searchParams.get("sheet") !== selectedSheet.toString()) {
      setSearchParams(prev => { prev.set("sheet", selectedSheet.toString()); return prev; }, { replace: true });
    }

    loadStudents(selectedSheet);

    const handleRefresh = () => {
      loadStudents(selectedSheet);
    };

    window.addEventListener("progresslens:refresh", handleRefresh);
    return () => window.removeEventListener("progresslens:refresh", handleRefresh);
  }, [selectedSheet]);

  async function loadStudents(sheetId: number) {
    try {
      setLoading(true);
      const [data, allFields] = await Promise.all([
        api.getAllStudents(sheetId),
        api.getFields(),
      ]);
      setStudents(data);
      setFields(allFields.filter(f => f.sheet_id === sheetId));
      
      // Initialize filters from URL
      if (searchParams.has("status")) {
        setStatuses(new Set([searchParams.get("status")!]));
      }
      if (searchParams.has("track")) {
        const tid = parseInt(searchParams.get("track")!, 10);
        const lvl = searchParams.has("level") ? parseInt(searchParams.get("level")!, 10) : 1;
        setTrackLevels(prev => ({ ...prev, [tid]: lvl }));
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  // Derived Fields mappings
  const { 
    levelFields, scoreFields, categoricalFields, 
    sectionField, cgpaField 
  } = useMemo(() => {
    const l = fields.filter(f => f.data_type === "level" && f.is_visible);
    const s = fields.filter(f => f.data_type === "score" && f.is_visible);
    const c = fields.filter(f => f.data_type === "categorical" && f.is_visible);
    const i = fields.filter(f => f.data_type === "identifier" && f.is_visible);
    
    // Auto-detect section and CGPA
    const sec = i.find(f => (f.display_name || f.label).toLowerCase().includes("section"));
    // Score fields marked include_in_dashboard could be CGPA, pick highest max_value
    const cgpa = s.filter(f => f.include_in_dashboard).sort((a, b) => (b.max_value || 0) - (a.max_value || 0))[0] 
                || s.find(f => (f.display_name || f.label).toLowerCase().includes("cgpa"));

    return { levelFields: l, scoreFields: s, categoricalFields: c, identifierFields: i, sectionField: sec, cgpaField: cgpa };
  }, [fields]);

  // Derived filter options
  const uniqueSections = useMemo(() => {
    if (!sectionField) return [];
    const vals = new Set<string>();
    students.forEach(s => {
      const v = s.values[sectionField.sheet_key];
      if (v) vals.add(v.trim());
    });
    return Array.from(vals).sort();
  }, [students, sectionField]);

  const uniqueCategorical = useMemo(() => {
    const map: Record<number, string[]> = {};
    categoricalFields.forEach(f => {
      const vals = new Set<string>();
      students.forEach(s => {
        const v = s.values[f.sheet_key];
        if (v) vals.add(v.trim());
      });
      map[f.id] = Array.from(vals).sort();
    });
    return map;
  }, [students, categoricalFields]);

  // Process data per student
  const processedStudents = useMemo(() => {
    return students.map(s => {
      let isComplete = true;
      let hasGap = false;
      let hasAnyStarted = false;
      const gapTracks: string[] = [];
      const levelsMap: Record<number, number | null> = {};
      const maxLevels: Record<number, number> = {};

      for (const f of levelFields) {
        const valStr = s.values[f.sheet_key];
        const max = f.max_value || 4;
        maxLevels[f.id] = max;
        
        let lvl: number | null = null;
        if (valStr && valStr.toLowerCase() !== "null" && valStr.trim() !== "") {
          lvl = parseInt(valStr, 10);
          if (isNaN(lvl)) lvl = null;
        }

        levelsMap[f.id] = lvl;

        if (lvl !== null) {
          hasAnyStarted = true;
          if (lvl < max) isComplete = false;
          if (lvl === 0) {
            hasGap = true;
            gapTracks.push(f.display_name || f.label);
          }
        } else {
          isComplete = false;
          hasGap = true;
          gapTracks.push(f.display_name || f.label);
        }
      }

      let statusRaw = "not_started";
      if (!hasAnyStarted && levelFields.length > 0) statusRaw = "not_started";
      else if (isComplete && levelFields.length > 0) statusRaw = "complete";
      else if (hasGap) statusRaw = "gap";
      else statusRaw = "in_progress"; // neither complete nor has gaps (e.g. tracks are at level 1,2,3 out of 4)

      return {
        ...s,
        levelsMap,
        gapTracks,
        statusRaw,
        isComplete,
        hasGap,
      };
    });
  }, [students, levelFields]);

  // Apply filters
  const filtered = useMemo(() => {
    return processedStudents.filter(s => {
      // 1. Search
      if (searchQuery) {
        const q = searchQuery.toLowerCase();
        if (!s.name.toLowerCase().includes(q) && !s.roll_number.toLowerCase().includes(q)) return false;
      }
      
      // 2. Track Levels
      for (const [fIdStr, minLvl] of Object.entries(trackLevels)) {
        const fId = parseInt(fIdStr, 10);
        const sl = s.levelsMap[fId];
        if (sl === null || sl === undefined || sl < minLvl) return false;
      }

      // 3. Statuses
      if (statuses.size > 0 && !statuses.has(s.statusRaw)) {
        return false;
      }

      // 4. Sections
      if (selectedSections.size > 0 && sectionField) {
        const sec = (s.values[sectionField.sheet_key] || "").trim();
        if (!selectedSections.has(sec)) return false;
      }

      // 5. Score ranges
      for (const [fIdStr, minScore] of Object.entries(scoreRanges)) {
        const fId = parseInt(fIdStr, 10);
        const field = scoreFields.find(f => f.id === fId);
        if (field) {
          const valStr = s.values[field.sheet_key];
          const val = valStr ? parseFloat(valStr) : 0;
          if (val < minScore) return false;
        }
      }

      // 6. Categorical
      for (const [fIdStr, allowed] of Object.entries(categoricalFilters)) {
        const fId = parseInt(fIdStr, 10);
        const field = categoricalFields.find(f => f.id === fId);
        if (field && allowed.size > 0) {
          const val = (s.values[field.sheet_key] || "").trim();
          if (!allowed.has(val)) return false;
        }
      }

      return true;
    });
  }, [processedStudents, searchQuery, trackLevels, statuses, selectedSections, scoreRanges, categoricalFilters, sectionField, scoreFields, categoricalFields]);

  // Handlers
  const toggleStatus = (st: string) => {
    setStatuses(prev => {
      const n = new Set(prev);
      if (n.has(st)) n.delete(st); else n.add(st);
      return n;
    });
  };

  const toggleSection = (sec: string) => {
    setSelectedSections(prev => {
      const n = new Set(prev);
      if (n.has(sec)) n.delete(sec); else n.add(sec);
      return n;
    });
  };

  const toggleCategorical = (fId: number, val: string) => {
    setCategoricalFilters(prev => {
      const n = { ...prev };
      if (!n[fId]) n[fId] = new Set();
      const set = new Set(n[fId]);
      if (set.has(val)) set.delete(val); else set.add(val);
      n[fId] = set;
      return n;
    });
  };

  const clearAllFilters = () => {
    setSearchQuery("");
    setTrackLevels({});
    setStatuses(new Set());
    setSelectedSections(new Set());
    setScoreRanges({});
    setCategoricalFilters({});
    setSearchParams(prev => {
      prev.delete("status");
      prev.delete("track");
      return prev;
    }, { replace: true });
  };

  // Listen for global "Export current view" event from sidebar
  useEffect(() => {
    const handler = async () => {
      if (!selectedSheet || filtered.length === 0) return;
      try {
        const visibleFieldIds = fields
          .filter(f => f.is_visible && !hiddenCols.has(f.id))
          .map(f => f.id);
        const studentIds = filtered.map(s => s.id);
        const filepath = await api.exportCurrentView(selectedSheet, studentIds, visibleFieldIds);
        const filename = filepath.split(/[/\\]/).pop() || "file";
        setExportToast(`Saved to Downloads/${filename}`);
        setTimeout(() => setExportToast(null), 4000);
      } catch (e) {
        console.error("Export failed:", e);
      }
    };
    window.addEventListener("progresslens:export-view", handler);
    return () => window.removeEventListener("progresslens:export-view", handler);
  }, [selectedSheet, filtered, fields, hiddenCols]);

  // Generate Filter Chips
  const activeChips: { id: string, label: string, onRemove: () => void }[] = [];
  if (searchQuery) activeChips.push({ id: 'search', label: `Search: ${searchQuery}`, onRemove: () => setSearchQuery("") });
  Object.entries(trackLevels).forEach(([fIdStr, lvl]) => {
    const fId = parseInt(fIdStr, 10);
    const fName = levelFields.find(f => f.id === fId)?.display_name || 'Track';
    activeChips.push({ id: `tr_${fId}`, label: `${fName} ≥ L${lvl}`, onRemove: () => setTrackLevels(p => { const o={...p}; delete o[fId]; return o; }) });
  });
  statuses.forEach(st => activeChips.push({ id: `st_${st}`, label: `Status: ${st.replace('_',' ')}`, onRemove: () => toggleStatus(st) }));
  selectedSections.forEach(sec => activeChips.push({ id: `sec_${sec}`, label: `Section: ${sec}`, onRemove: () => toggleSection(sec) }));
  Object.entries(scoreRanges).forEach(([fIdStr, minS]) => {
    const fId = parseInt(fIdStr, 10);
    const fName = scoreFields.find(f => f.id === fId)?.display_name || 'Score';
    activeChips.push({ id: `sc_${fId}`, label: `${fName} ≥ ${minS}`, onRemove: () => setScoreRanges(p => { const o={...p}; delete o[fId]; return o; }) });
  });
  Object.entries(categoricalFilters).forEach(([fIdStr, set]) => {
    const fId = parseInt(fIdStr, 10);
    const fName = categoricalFields.find(f => f.id === fId)?.display_name || 'Category';
    if (set.size > 0) {
      activeChips.push({ id: `cat_${fId}`, label: `${fName}: ${Array.from(set).join(', ')}`, onRemove: () => setCategoricalFilters(p => { const o={...p}; delete o[fId]; return o; }) });
    }
  });

  return (
    <div className="flex h-full bg-ink-950 text-ink-50 font-sans overflow-hidden">
      
      {/* Export Toast */}
      <div
        className={`fixed bottom-6 left-1/2 -translate-x-1/2 z-[100] flex items-center gap-2.5 px-5 py-3 rounded-xl bg-ink-900 border border-ink-700 text-ink-100 text-sm font-medium shadow-2xl transition-all duration-300 ${
          exportToast ? 'opacity-100 translate-y-0' : 'opacity-0 translate-y-4 pointer-events-none'
        }`}
      >
        <span className="w-5 h-5 rounded-full bg-emerald-500/15 flex items-center justify-center">
          <Check className="w-3 h-3 text-emerald-400" />
        </span>
        {exportToast}
      </div>
      
      {/* 220px Sidebar filters */}
      <aside className="w-[260px] shrink-0 border-r border-ink-800/60 flex flex-col bg-ink-950 overflow-y-auto custom-scrollbar">
        <div className="p-4 border-b border-ink-800/60 sticky top-0 bg-ink-950/90 backdrop-blur z-10">
          <div className="flex items-center gap-2 text-amber-500 font-bold tracking-wide uppercase text-xs mb-3">
            <Filter className="w-4 h-4" /> Filters
          </div>
          <div className="relative">
            <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-ink-500" />
            <input 
              type="text" 
              placeholder="Search Name or USN..." 
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-ink-900 border border-ink-800 rounded-md py-1.5 pl-9 pr-3 text-xs outline-none focus:border-amber-500/50 text-ink-100 placeholder:text-ink-600"
            />
          </div>
        </div>

        <div className="p-4 space-y-6">
          
          {/* Status */}
          <div className="space-y-2">
            <h4 className="text-[10px] font-bold text-ink-400 uppercase tracking-widest">Status</h4>
            {[
              { id: 'complete', label: 'Fully Complete' },
              { id: 'gap', label: 'Has Gaps' },
              { id: 'not_started', label: 'Not Started' },
            ].map(st => (
              <label key={st.id} className="flex items-center gap-2 cursor-pointer group">
                <input 
                  type="checkbox" 
                  checked={statuses.has(st.id)}
                  onChange={() => toggleStatus(st.id)}
                  className="w-3.5 h-3.5 rounded border-ink-700 bg-ink-950 text-amber-500 focus:ring-amber-500/50"
                />
                <span className="text-xs text-ink-300 group-hover:text-ink-100 transition-colors">{st.label}</span>
              </label>
            ))}
          </div>

          {/* Track Levels */}
          {levelFields.length > 0 && (
            <div className="space-y-3">
              <h4 className="text-[10px] font-bold text-ink-400 uppercase tracking-widest">Track Levels (Min)</h4>
              {levelFields.map(f => {
                const max = f.max_value || 4;
                const activeLvl = trackLevels[f.id] || 0;
                return (
                  <div key={f.id} className="space-y-1.5">
                    <div className="text-xs text-ink-200 truncate pr-2">{f.display_name || f.label}</div>
                    <div className="flex gap-1">
                      {Array.from({ length: max }).map((_, i) => {
                        const lvl = i + 1;
                        const isSet = activeLvl >= lvl;
                        const isExact = activeLvl === lvl;
                        return (
                          <button
                            key={lvl}
                            onClick={() => {
                              setTrackLevels(prev => {
                                const n = { ...prev };
                                if (n[f.id] === lvl) delete n[f.id]; // toggle off
                                else n[f.id] = lvl;
                                return n;
                              });
                            }}
                            className={`flex-1 h-6 rounded text-[10px] font-mono font-bold transition-all border
                              ${isExact ? 'border-amber-400 bg-amber-500 text-amber-950' : 
                                isSet ? 'border-amber-500/50 bg-amber-500/20 text-amber-400' : 
                                'border-ink-800 bg-ink-900 text-ink-600 hover:border-ink-700 hover:text-ink-400'}`}
                          >
                            {lvl}
                          </button>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          {/* Sections */}
          {uniqueSections.length > 0 && (
            <div className="space-y-2">
              <h4 className="text-[10px] font-bold text-ink-400 uppercase tracking-widest">Sections</h4>
              <div className="max-h-32 overflow-y-auto custom-scrollbar pr-2 space-y-1.5">
                {uniqueSections.map(sec => (
                  <label key={sec} className="flex items-center gap-2 cursor-pointer group">
                    <input 
                      type="checkbox" 
                      checked={selectedSections.has(sec)}
                      onChange={() => toggleSection(sec)}
                      className="w-3.5 h-3.5 rounded border-ink-700 bg-ink-950 text-amber-500 focus:ring-amber-500/50 shrink-0"
                    />
                    <span className="text-xs text-ink-300 group-hover:text-ink-100 transition-colors truncate" title={sec}>{sec}</span>
                  </label>
                ))}
              </div>
            </div>
          )}

          {/* Scores */}
          {scoreFields.filter(f => f.include_in_dashboard).map(f => (
            <div key={f.id} className="space-y-1.5">
              <div className="flex justify-between items-center">
                <h4 className="text-[10px] font-bold text-ink-400 uppercase tracking-widest truncate max-w-[120px]">{f.display_name || f.label} (Min)</h4>
                <span className="text-xs font-mono text-amber-500">{scoreRanges[f.id] || 0}</span>
              </div>
              <input 
                type="range" 
                min={0} 
                max={f.max_value || 100} 
                step={0.1}
                value={scoreRanges[f.id] || 0}
                onChange={(e) => setScoreRanges(p => ({ ...p, [f.id]: parseFloat(e.target.value) }))}
                className="w-full accent-amber-500"
              />
            </div>
          ))}

          {/* Categorical */}
          {categoricalFields.map(f => (
            <div key={f.id} className="space-y-2">
              <h4 className="text-[10px] font-bold text-ink-400 uppercase tracking-widest truncate">{f.display_name || f.label}</h4>
              <div className="max-h-32 overflow-y-auto custom-scrollbar pr-2 space-y-1.5">
                {uniqueCategorical[f.id]?.map(val => (
                  <label key={val} className="flex items-center gap-2 cursor-pointer group">
                    <input 
                      type="checkbox" 
                      checked={categoricalFilters[f.id]?.has(val)}
                      onChange={() => toggleCategorical(f.id, val)}
                      className="w-3.5 h-3.5 rounded border-ink-700 bg-ink-950 text-amber-500 focus:ring-amber-500/50 shrink-0"
                    />
                    <span className="text-xs text-ink-300 group-hover:text-ink-100 transition-colors truncate" title={val}>{val}</span>
                  </label>
                ))}
              </div>
            </div>
          ))}

          <div className="pt-4 border-t border-ink-800/60">
            <button 
              onClick={clearAllFilters}
              disabled={activeChips.length === 0}
              className="w-full py-1.5 text-xs text-ink-400 font-medium hover:text-ink-100 hover:bg-ink-800 rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Clear all filters
            </button>
          </div>

        </div>
      </aside>

      {/* Main Table Area */}
      <main className="flex-1 flex flex-col min-w-0 bg-ink-950">
        
        {/* Toolbar */}
        <header className="px-6 py-4 border-b border-ink-800/60 flex flex-col gap-3 shrink-0">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <select
                className="bg-ink-900 border border-ink-800 rounded-md text-sm py-1.5 px-3 outline-none focus:border-amber-500/50 text-ink-100 max-w-[200px] truncate"
                value={selectedSheet ?? ""}
                onChange={(e) => setSelectedSheet(parseInt(e.target.value))}
                disabled={sheets.length === 0}
              >
                {sheets.length === 0 && <option value="">No sheets</option>}
                {sheets.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}
              </select>
              <span className="text-ink-400 text-sm font-medium">
                {filtered.length} of {students.length} students
              </span>
            </div>
            
            <button 
              onClick={() => setShowColPicker(!showColPicker)}
              className={`flex items-center gap-2 py-1.5 px-3 rounded-md text-xs font-medium border transition-colors ${showColPicker ? 'bg-ink-800 border-ink-700 text-ink-100' : 'bg-ink-900 border-ink-800 text-ink-300 hover:border-ink-700'}`}
            >
              <Columns className="w-4 h-4" /> Columns
            </button>
          </div>

          {/* Active filter chips */}
          {activeChips.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {activeChips.map(chip => (
                <div key={chip.id} className="flex items-center gap-1.5 px-2 py-1 rounded bg-amber-500/10 border border-amber-500/20 text-amber-400 text-xs font-medium">
                  {chip.label}
                  <button onClick={chip.onRemove} className="hover:text-amber-300 hover:bg-amber-500/20 rounded p-0.5 transition-colors">
                    <X className="w-3 h-3" />
                  </button>
                </div>
              ))}
            </div>
          )}

          {/* Column Picker Dropdown (absolute) */}
          {showColPicker && (
            <div className="absolute top-16 right-6 z-20 w-64 bg-ink-900 border border-ink-800 rounded-lg shadow-xl p-4 overflow-y-auto max-h-[60vh] custom-scrollbar">
              <h4 className="text-[10px] font-bold text-ink-400 uppercase tracking-widest mb-3">Toggle Columns</h4>
              <div className="space-y-2">
                {fields.filter(f => !['identifier'].includes(f.data_type || '') && f.id !== sectionField?.id && f.id !== cgpaField?.id).map(f => (
                  <label key={f.id} className="flex items-center gap-2 cursor-pointer text-xs text-ink-200 hover:text-ink-50">
                    <input 
                      type="checkbox" 
                      checked={!hiddenCols.has(f.id)}
                      onChange={() => setHiddenCols(p => { const n = new Set(p); if (n.has(f.id)) n.delete(f.id); else n.add(f.id); return n; })}
                      className="w-3.5 h-3.5 rounded border-ink-700 bg-ink-950 text-amber-500 focus:ring-amber-500/50"
                    />
                    <span className="truncate">{f.display_name || f.label}</span>
                  </label>
                ))}
              </div>
            </div>
          )}
        </header>

        {/* Table wrapper */}
        <div className="flex-1 overflow-auto custom-scrollbar p-6">
          {loading ? (
            <div className="text-ink-500 text-sm flex items-center justify-center h-full">
              <Spinner /> Loading students…
            </div>
          ) : error ? (
            <div className="bg-red-500/10 text-red-500 p-4 rounded-lg">{error}</div>
          ) : filtered.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full text-ink-500">
              <Search className="w-12 h-12 text-ink-800 mb-4" />
              <p>No students match your active filters.</p>
              {activeChips.length > 0 && (
                <button onClick={clearAllFilters} className="mt-4 text-amber-500 hover:text-amber-400 font-medium text-sm">Clear Filters</button>
              )}
            </div>
          ) : (
            <div className="bg-ink-900/30 border border-ink-800/60 rounded-xl overflow-hidden w-fit min-w-full">
              <table className="w-full text-left border-collapse whitespace-nowrap">
                <thead className="bg-ink-950/50 border-b border-ink-800/60 text-[10px] uppercase tracking-widest text-ink-500 font-bold sticky top-0 z-10">
                  <tr>
                    <th className="px-4 py-3 sticky left-0 z-20 bg-ink-950/50 backdrop-blur">Name & USN</th>
                    {sectionField && <th className="px-4 py-3">Section</th>}
                    {cgpaField && <th className="px-4 py-3">{cgpaField.display_name || cgpaField.label}</th>}
                    <th className="px-4 py-3 text-center">Status</th>
                    {levelFields.filter(f => !hiddenCols.has(f.id)).map(col => (
                      <th key={col.id} className="px-4 py-3 text-center" title={col.display_name || col.label}>
                        {col.display_name || col.label}
                      </th>
                    ))}
                    {fields.filter(f => f.data_type !== 'level' && f.data_type !== 'identifier' && f.id !== sectionField?.id && f.id !== cgpaField?.id && !hiddenCols.has(f.id) && f.is_visible).map(col => (
                      <th key={col.id} className="px-4 py-3">
                        {col.display_name || col.label}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody className="divide-y divide-ink-800/40">
                  {filtered.map(s => (
                    <tr 
                      key={s.id} 
                      onClick={() => navigate(`/diff?student=${s.id}&sheet=${selectedSheet}`)}
                      className="hover:bg-ink-800/30 transition-colors cursor-pointer group"
                    >
                      {/* Name / USN Fixed */}
                      <td className="px-4 py-3 sticky left-0 z-10 bg-ink-950/20 group-hover:bg-ink-800/30 backdrop-blur w-48 max-w-[200px]">
                        <div className="font-medium text-ink-100 truncate" title={s.name}>{s.name}</div>
                        <div className="text-[10px] font-mono text-ink-500 mt-0.5 truncate" title={s.roll_number}>{s.roll_number}</div>
                      </td>
                      
                      {/* Section Fixed */}
                      {sectionField && (
                        <td className="px-4 py-3 text-xs text-ink-300 font-mono">
                          {s.values[sectionField.sheet_key] || "—"}
                        </td>
                      )}
                      
                      {/* CGPA Fixed */}
                      {cgpaField && (
                        <td className="px-4 py-3 text-xs text-ink-200 font-mono font-bold">
                          {s.values[cgpaField.sheet_key] ? (!isNaN(parseFloat(s.values[cgpaField.sheet_key])) ? parseFloat(s.values[cgpaField.sheet_key]).toFixed(2) : s.values[cgpaField.sheet_key]) : "—"}
                        </td>
                      )}

                      {/* Status Badge */}
                      <td className="px-4 py-3 text-center">
                        {s.isComplete ? (
                          <span className="px-2 py-0.5 rounded text-[10px] font-bold tracking-wide bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                            COMPLETE
                          </span>
                        ) : s.hasGap ? (
                          <span className="px-2 py-0.5 rounded text-[10px] font-bold tracking-wide bg-red-500/10 text-red-500 border border-red-500/20" title={s.gapTracks.join(", ")}>
                            GAP: {s.gapTracks.length > 0 ? (s.gapTracks[0].length > 10 ? s.gapTracks[0].slice(0, 10)+'…' : s.gapTracks[0]) : "Unknown"}
                          </span>
                        ) : s.statusRaw === "not_started" ? (
                          <span className="px-2 py-0.5 rounded text-[10px] font-bold tracking-wide bg-ink-800 text-ink-400 border border-ink-700">
                            —
                          </span>
                        ) : (
                          <span className="px-2 py-0.5 rounded text-[10px] font-bold tracking-wide bg-amber-500/10 text-amber-500 border border-amber-500/20">
                            IN PROGRESS
                          </span>
                        )}
                      </td>

                      {/* Level Track Pips */}
                      {levelFields.filter(f => !hiddenCols.has(f.id)).map(f => {
                        const lvl = s.levelsMap[f.id];
                        let pipClass = "bg-ink-800 border-ink-700"; // not started
                        let textClass = "text-ink-600";
                        if (lvl === 0) { pipClass = "bg-red-500/20 border-red-500/50"; textClass="text-red-500"; }
                        else if (lvl === 1 || lvl === 2) { pipClass = "bg-amber-500 border-amber-400"; textClass="text-amber-950"; }
                        else if (lvl === 3) { pipClass = "bg-lime-500 border-lime-400"; textClass="text-lime-950"; }
                        else if (lvl === 4) { pipClass = "bg-emerald-500 border-emerald-400"; textClass="text-emerald-950"; } 

                        return (
                          <td key={f.id} className="px-4 py-3 text-center">
                            {lvl === null ? (
                              <div className="w-5 h-5 mx-auto rounded-full bg-red-500/10 border border-red-500/30 flex items-center justify-center font-bold text-red-500 text-[10px]">-</div>
                            ) : (
                              <div className={`w-5 h-5 mx-auto rounded-full border flex items-center justify-center font-bold text-[10px] ${pipClass} ${textClass}`}>
                                {lvl}
                              </div>
                            )}
                          </td>
                        );
                      })}

                      {/* Other fields */}
                      {fields.filter(f => f.data_type !== 'level' && f.data_type !== 'identifier' && f.id !== sectionField?.id && f.id !== cgpaField?.id && !hiddenCols.has(f.id) && f.is_visible).map(f => {
                        const val = s.values[f.sheet_key];
                        return (
                          <td key={f.id} className="px-4 py-3 text-xs text-ink-300 min-w-[200px] max-w-[400px] whitespace-normal leading-relaxed break-words">
                            {f.data_type === 'link' && val ? (
                              <button
                                onClick={async (e) => {
                                  e.preventDefault();
                                  e.stopPropagation();
                                  try {
                                    let tgt = val;
                                    if (!tgt.startsWith('http://') && !tgt.startsWith('https://')) {
                                      tgt = 'https://' + tgt;
                                    }
                                    await openUrl(tgt);
                                  } catch (err) {
                                    console.error("Failed to open URL:", err);
                                  }
                                }}
                                className="text-amber-500 hover:underline text-left"
                              >
                                Link
                              </button>
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
          )}
        </div>
      </main>

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

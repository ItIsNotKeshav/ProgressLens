import { useEffect, useState, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { 
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer, 
  Cell
} from "recharts";
import { api } from "../api";
import type { DashboardStats, Snapshot, Sheet } from "../types";
import { Users, Activity, Trophy, GraduationCap, ChevronRight, Clock, RefreshCw } from "lucide-react";

export default function Dashboard() {
  const navigate = useNavigate();
  const [sheets, setSheets] = useState<Sheet[]>([]);
  const [selectedSheet, setSelectedSheet] = useState<number | null>(null);
  
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [selectedSnapshot, setSelectedSnapshot] = useState<number | null>(null);
  
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load sheets
  useEffect(() => {
    async function init() {
      try {
        const result = await api.getSheets();
        setSheets(result);
        if (result.length > 0) {
          setSelectedSheet(result[0].id);
        } else {
          setLoading(false); // No data
        }
      } catch (e) {
        setError(String(e));
        setLoading(false);
      }
    }
    init();
  }, []);

  // When sheet changes, load its snapshots
  useEffect(() => {
    if (selectedSheet === null) return;
    
    async function loadSnaps() {
      try {
        const snaps = await api.getSnapshots(selectedSheet!);
        setSnapshots(snaps);
        if (snaps.length > 0) {
          setSelectedSnapshot(snaps[snaps.length - 1].id);
        } else {
          setSelectedSnapshot(null);
          setStats(null);
          setLoading(false);
        }
      } catch(e) {
        setError(String(e));
      }
    }
    loadSnaps();
  }, [selectedSheet]);

  // When snapshot changes, load stats
  useEffect(() => {
    if (selectedSheet === null || selectedSnapshot === null) return;
    
    let cancelled = false;
    async function loadStats() {
      setLoading(true);
      setError(null);
      try {
        const data = await api.getDashboardStats(selectedSheet!, selectedSnapshot!);
        if (!cancelled) setStats(data);
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    loadStats();
    return () => { cancelled = true; };
  }, [selectedSheet, selectedSnapshot]);

  const handleSync = async () => {
    if (selectedSheet === null) return;
    try {
      setSyncing(true);
      await api.syncSheet(selectedSheet);
      const snaps = await api.getSnapshots(selectedSheet);
      setSnapshots(snaps);
      if (snaps.length > 0) {
        setSelectedSnapshot(snaps[snaps.length - 1].id);
      }
    } catch(e) {
      alert("Sync failed: " + e);
    } finally {
      setSyncing(false);
    }
  };

  // Derived calculations
  const highestAvg = useMemo(() => {
    if (!stats || stats.avg_score_per_field.length === 0) return null;
    return [...stats.avg_score_per_field].sort((a, b) => b.avg - a.avg)[0];
  }, [stats]);

  const levelChartData = useMemo(() => {
    if (!stats) return { data: [], levels: [] as string[] };
    
    // Group level_distribution by field_label
    const fieldsMap = new Map<string, Record<string, number>>();
    const levelsSet = new Set<string>();

    for (const l of stats.level_distribution) {
      if (!fieldsMap.has(l.field_label)) {
        fieldsMap.set(l.field_label, {});
      }
      fieldsMap.get(l.field_label)![l.level] = l.count;
      levelsSet.add(l.level);
    }

    const data = [];
    for (const [field_label, counts] of fieldsMap.entries()) {
      data.push({ name: field_label, ...counts });
    }
    
    const levels = Array.from(levelsSet).sort(); // simple string sort -> Level 1, Level 2...
    return { data, levels };
  }, [stats]);

  // Color palette for levels
  const levelColors = ["#fcd34d", "#f59e0b", "#d97706", "#b45309", "#78350f"];

  if (error) {
    return (
      <div className="p-8 h-full bg-ink-950 flex items-center justify-center text-red-400">
        Error loading dashboard: {error}
      </div>
    );
  }

  return (
    <div className="flex h-full bg-ink-950 text-ink-50 font-sans overflow-hidden">
      {/* Main Content Area */}
      <main className="flex-1 flex flex-col min-w-0 overflow-y-auto custom-scrollbar relative">
        <header className="h-16 shrink-0 border-b border-ink-800/60 flex items-center justify-between px-8 bg-ink-950/80 backdrop-blur sticky top-0 z-10">
          <h1 className="text-xl font-display font-bold tracking-tight text-ink-50">Dashboard</h1>
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

            <button 
              onClick={handleSync}
              disabled={syncing || selectedSheet === null}
              className="flex items-center justify-center py-1.5 px-3 rounded-md text-[10px] uppercase font-bold tracking-wide bg-ink-800 text-amber-500 hover:bg-ink-700 transition-colors disabled:opacity-50"
            >
              {syncing ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : "Re-Sync"}
            </button>

            <span className="text-sm text-ink-400 opacity-50 ml-1">|</span>

            <span className="text-sm text-ink-400 ml-1">Snapshot:</span>
            <select
              className="bg-ink-900 border border-ink-800 rounded-md text-sm py-1.5 px-3 outline-none focus:border-amber-500/50 text-ink-100"
              value={selectedSnapshot ?? ""}
              onChange={(e) => setSelectedSnapshot(parseInt(e.target.value))}
              disabled={snapshots.length === 0}
            >
              {snapshots.length === 0 && <option value="">No snapshots</option>}
              {snapshots.map((s) => (
                <option key={s.id} value={s.id}>
                  {new Date(s.synced_at).toLocaleDateString()} - {new Date(s.synced_at).toLocaleTimeString([], {hour: '2-digit', minute:'2-digit'})}
                </option>
              ))}
            </select>
          </div>
        </header>

        <div className="p-8">
          {loading ? (
            <div className="flex items-center gap-3 text-ink-500">
              <Spinner /> Loading metrics...
            </div>
          ) : !stats ? (
            <div className="text-ink-500 text-center mt-20">
              Sync some data to view the dashboard.
            </div>
          ) : (
            <div className="space-y-8 animate-fade-in max-w-[1600px] mx-auto">
              
              {/* Metric Card Row */}
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <MetricCard 
                  title="Total Students" 
                  value={stats.total_students} 
                  icon={<Users className="w-5 h-5 text-zinc-400" />} 
                />
                <MetricCard 
                  title="Active / Updated (7d)" 
                  value={stats.active_this_week} 
                  icon={<Activity className="w-5 h-5 text-amber-500" />} 
                  subtitle="Students with changes"
                />
                <MetricCard 
                  title="Highest Average" 
                  value={highestAvg ? highestAvg.avg.toFixed(1) : "—"} 
                  icon={<Trophy className="w-5 h-5 text-sage-400" />} 
                  subtitle={highestAvg?.field_label}
                />
                <MetricCard 
                  title="Top Performers" 
                  value={stats.top_performers.length} 
                  icon={<GraduationCap className="w-5 h-5 text-purple-400" />} 
                  subtitle="Students excelling"
                />
              </div>

              {/* Charts Row */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                
                {/* Score Averages Chart */}
                <div className="bg-ink-900/30 border border-ink-800/60 rounded-xl p-6">
                  <h3 className="text-sm font-semibold tracking-wide text-ink-300 uppercase mb-6">Score Averages</h3>
                  {stats.avg_score_per_field.length > 0 ? (
                    <div className="h-72">
                      <ResponsiveContainer width="100%" height="100%">
                        <BarChart data={stats.avg_score_per_field} margin={{ top: 10, right: 10, left: -20, bottom: 0 }}>
                          <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="#27272a" />
                          <XAxis dataKey="field_label" stroke="#71717a" fontSize={12} tickLine={false} axisLine={false} />
                          <YAxis stroke="#71717a" fontSize={12} tickLine={false} axisLine={false} />
                          <Tooltip 
                            cursor={{ fill: '#27272a', opacity: 0.4 }}
                            contentStyle={{ backgroundColor: '#18181b', borderColor: '#3f3f46', borderRadius: '8px' }}
                            itemStyle={{ color: '#fcd34d' }}
                          />
                          <Bar dataKey="avg" fill="#f59e0b" radius={[4, 4, 0, 0]}>
                            {stats.avg_score_per_field.map((_, index) => (
                              <Cell key={`cell-${index}`} fill={index % 2 === 0 ? "#f59e0b" : "#fbbf24"} />
                            ))}
                          </Bar>
                        </BarChart>
                      </ResponsiveContainer>
                    </div>
                  ) : (
                    <div className="text-sm text-ink-500 h-72 flex items-center justify-center">No numeric fields to average.</div>
                  )}
                </div>

                {/* Level Distribution Chart */}
                <div className="bg-ink-900/30 border border-ink-800/60 rounded-xl p-6">
                  <h3 className="text-sm font-semibold tracking-wide text-ink-300 uppercase mb-6">Level Distribution</h3>
                  {levelChartData.data.length > 0 ? (
                    <div className="h-72">
                      <ResponsiveContainer width="100%" height="100%">
                        <BarChart data={levelChartData.data} layout="vertical" margin={{ top: 0, right: 10, left: 10, bottom: 0 }}>
                          <CartesianGrid strokeDasharray="3 3" horizontal={true} vertical={false} stroke="#27272a" />
                          <XAxis type="number" stroke="#71717a" fontSize={12} tickLine={false} axisLine={false} />
                          <YAxis dataKey="name" type="category" width={100} stroke="#71717a" fontSize={12} tickLine={false} axisLine={false} />
                          <Tooltip 
                            cursor={{ fill: '#27272a', opacity: 0.4 }}
                            contentStyle={{ backgroundColor: '#18181b', borderColor: '#3f3f46', borderRadius: '8px' }}
                          />
                          <Legend wrapperStyle={{ fontSize: '12px', paddingTop: '10px' }} />
                          {levelChartData.levels.map((lvl, i) => (
                            <Bar key={lvl} dataKey={lvl} stackId="a" fill={levelColors[i % levelColors.length]} />
                          ))}
                        </BarChart>
                      </ResponsiveContainer>
                    </div>
                  ) : (
                    <div className="text-sm text-ink-500 h-72 flex items-center justify-center">No level-based fields detected.</div>
                  )}
                </div>

              </div>

            </div>
          )}
        </div>
      </main>

      {/* Right Sidebar: Recent Activity */}
      <aside className="w-80 shrink-0 flex flex-col border-l border-ink-800/60 bg-ink-900/30">
        <header className="px-6 py-5 border-b border-ink-800/60 flex items-center gap-2">
          <Activity className="w-4 h-4 text-ink-400" />
          <h2 className="text-sm font-semibold text-ink-200 tracking-wide uppercase">Activity Feed</h2>
        </header>
        
        <div className="flex-1 overflow-y-auto p-4 space-y-3 custom-scrollbar">
          {!stats ? null : stats.recent_changes.length === 0 ? (
            <div className="text-xs text-ink-500 text-center mt-10">No recent changes detected.</div>
          ) : (
            stats.recent_changes.map((change, i) => {
              const diffTime = new Date().getTime() - new Date(change.synced_at).getTime();
              const hoursAgo = Math.max(0, Math.floor(diffTime / (1000 * 60 * 60)));
              
              return (
                <button
                  key={i}
                  onClick={() => navigate("/diff")}
                  className="w-full text-left bg-ink-950/50 border border-ink-800/40 rounded-lg p-4 hover:bg-ink-800/50 transition-colors group relative"
                >
                  <div className="flex justify-between items-start mb-2">
                    <span className="font-medium text-ink-50 text-sm group-hover:text-amber-400 transition-colors">{change.student_name}</span>
                    <span className="text-[10px] text-ink-500 font-mono flex items-center gap-1">
                      <Clock className="w-3 h-3" />
                      {hoursAgo === 0 ? "Just now" : `${hoursAgo}h ago`}
                    </span>
                  </div>
                  <div className="text-xs text-ink-300">
                    Updated <span className="font-mono text-ink-200">{change.field_label}</span>
                  </div>
                  <div className="mt-2 flex items-center gap-2 text-xs font-mono">
                    <span className="px-1.5 py-0.5 rounded bg-red-500/10 text-red-400 opacity-60 flex-1 truncate line-through">
                      {change.old_val || "—"}
                    </span>
                    <ChevronRight className="w-3 h-3 text-ink-600 shrink-0" />
                    <span className="px-1.5 py-0.5 rounded bg-sage-500/10 text-sage-400 flex-1 truncate">
                      {change.new_val}
                    </span>
                  </div>
                </button>
              );
            })
          )}
        </div>
      </aside>
    </div>
  );
}

function MetricCard({ title, value, icon, subtitle }: { title: string; value: string | number; icon: React.ReactNode; subtitle?: string }) {
  return (
    <div className="bg-ink-900/30 border border-ink-800/60 rounded-xl p-5 flex flex-col relative overflow-hidden group hover:bg-ink-800/20 transition-colors">
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-xs font-medium tracking-wider text-ink-400 uppercase">{title}</h3>
        {icon}
      </div>
      <div className="text-3xl font-display font-semibold text-ink-50 tracking-tight mt-1">
        {value}
      </div>
      {subtitle && (
        <div className="mt-2 text-xs text-ink-500 font-mono truncate">
          {subtitle}
        </div>
      )}
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

import { FormEvent, useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowUp, CheckCircle2, ChevronRight, Clock, LockKeyhole,
  MessageSquareText, Settings2, ShieldCheck, X, XCircle,
} from "lucide-react";
import { api } from "../api";
import type {
  AgentAskResponse, AgentSettings, OperationPreview, PendingOperation, Sheet,
} from "../types";

type Turn = {
  id: string;
  role: "user" | "assistant";
  content: string;
  result?: AgentAskResponse;
};

const STARTERS = [
  "Show students whose scores dropped in the last two snapshots.",
  "Which students have improved the most?",
  "Compare a student's performance over recent snapshots.",
];

const DEFAULT_SETTINGS: AgentSettings = {
  enabled: true,
  ollama_endpoint: "http://localhost:11434",
  model_name: "qwen3:8b",
  timeout_seconds: 90,
  max_tool_iterations: 6,
};

function friendlyAgentError(reason: unknown): string {
  const raw = String(reason).replace(/^Error:\s*/i, "");
  const [, code, message] = raw.match(/^(AI_[A-Z_]+):\s*(.*)$/s) ?? [];
  if (!code) return raw;
  const prefixes: Record<string, string> = {
    AI_OLLAMA_UNAVAILABLE: "Ollama is offline",
    AI_MODEL_MISSING: "Model not ready",
    AI_TIMEOUT: "Response timed out",
    AI_INVALID_JSON: "The model returned an invalid response",
    AI_DISABLED: "Assistant disabled",
    AI_PRIVACY_BLOCK: "Privacy protection blocked this endpoint",
  };
  return `${prefixes[code] ?? "Assistant error"}. ${message}`;
}

function parsePreview(op: PendingOperation): OperationPreview {
  try {
    return JSON.parse(op.preview_json) as OperationPreview;
  } catch {
    return { summary: op.kind };
  }
}

function kindLabel(kind: string): string {
  const map: Record<string, string> = {
    student_update: "Update student field",
    mentor_note: "Add mentor note",
    intervention: "Flag intervention",
    other: "Other change",
  };
  return map[kind] ?? kind;
}

// ── Approval Card ─────────────────────────────────────────────────────────────

function ApprovalCard({
  op,
  onApprove,
  onReject,
}: {
  op: PendingOperation;
  onApprove: (id: string) => Promise<void>;
  onReject: (id: string) => Promise<void>;
}) {
  const preview = parsePreview(op);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<"approved" | "rejected" | null>(null);
  const [approvalError, setApprovalError] = useState<string | null>(null);

  const handle = async (action: "approve" | "reject") => {
    setBusy(true);
    setApprovalError(null);
    try {
      if (action === "approve") {
        await onApprove(op.id);
        setDone("approved");
      } else {
        await onReject(op.id);
        setDone("rejected");
      }
    } catch (err) {
      // Show the Rust error message so the user knows why it failed
      const msg = String(err).replace(/^Error:\s*/i, "");
      setApprovalError(msg || "Unexpected error — please try again");
      setBusy(false);
    }
  };

  if (done === "approved") {
    return (
      <div className="flex items-center gap-2 rounded border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-300">
        <CheckCircle2 className="h-3.5 w-3.5 shrink-0" />
        <span>Change applied — <span className="font-medium">{preview.summary}</span></span>
      </div>
    );
  }

  if (done === "rejected") {
    return (
      <div className="flex items-center gap-2 rounded border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-400">
        <XCircle className="h-3.5 w-3.5 shrink-0" />
        <span>Rejected — <span className="font-medium">{preview.summary}</span></span>
      </div>
    );
  }

  return (
    <div className="rounded border border-amber-500/30 bg-amber-500/5">
      {/* Header */}
      <div className="flex items-center justify-between gap-2 border-b border-amber-500/20 px-3 py-2">
        <div className="flex items-center gap-1.5">
          <Clock className="h-3 w-3 text-amber-400" />
          <span className="text-[10px] font-semibold uppercase tracking-widest text-amber-400">
            Pending approval
          </span>
        </div>
        <span className="text-[9px] text-ink-600">{kindLabel(op.kind)}</span>
      </div>

      {/* Details */}
      <div className="space-y-2 px-3 py-3">
        <Row label="Operation" value={preview.summary} />
        {preview.student_name && <Row label="Student" value={preview.student_name} />}
        {preview.field_name && <Row label="Field" value={preview.field_name} />}
        {preview.current_value !== undefined && (
          <Row label="Current" value={preview.current_value || "—"} />
        )}
        {preview.proposed_value !== undefined && (
          <Row label="Proposed" value={preview.proposed_value} highlight />
        )}
        {preview.reason && <Row label="Reason" value={preview.reason} muted />}
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2 border-t border-amber-500/20 px-3 py-2">
        <button
          type="button"
          disabled={busy}
          onClick={() => void handle("approve")}
          className="flex items-center gap-1.5 border border-emerald-500/40 bg-emerald-500/10 px-3 py-1.5 text-[10px] font-semibold text-emerald-300 transition-colors hover:bg-emerald-500/20 disabled:opacity-40"
          id={`approve-${op.id}`}
        >
          <CheckCircle2 className="h-3 w-3" />
          Approve
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => void handle("reject")}
          className="flex items-center gap-1.5 border border-red-500/30 px-3 py-1.5 text-[10px] font-semibold text-red-400 transition-colors hover:bg-red-500/10 disabled:opacity-40"
          id={`reject-${op.id}`}
        >
          <XCircle className="h-3 w-3" />
          Reject
        </button>
      </div>
      {approvalError && (
        <div className="border-t border-red-500/20 px-3 py-2 text-[10px] leading-4 text-red-400">
          ⚠ {approvalError}
        </div>
      )}
    </div>
  );
}

function Row({
  label,
  value,
  highlight,
  muted,
}: {
  label: string;
  value: string;
  highlight?: boolean;
  muted?: boolean;
}) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <span className="shrink-0 text-[10px] font-medium text-ink-500">{label}</span>
      <span
        className={`text-right text-xs leading-5 ${
          highlight
            ? "font-semibold text-amber-300"
            : muted
            ? "text-ink-500"
            : "text-ink-200"
        }`}
      >
        {value}
      </span>
    </div>
  );
}

// ── Floating Approval Tray ────────────────────────────────────────────────────

function ApprovalTray({
  sheetId,
  refreshTrigger,
}: {
  sheetId: number;
  refreshTrigger: number;
}) {
  const [ops, setOps] = useState<PendingOperation[]>([]);

  const refresh = useCallback(async () => {
    try {
      const result = await api.getPendingOperations(sheetId);
      setOps(result);
    } catch {
      // silent
    }
  }, [sheetId]);

  // Refresh on mount, when sheet changes, or when the agent finishes a turn
  useEffect(() => { void refresh(); }, [refresh, refreshTrigger]);

  const handleApprove = async (id: string) => {
    await api.approveOperation(id);
    await refresh();
  };

  const handleReject = async (id: string) => {
    await api.rejectOperation(id);
    await refresh();
  };

  if (ops.length === 0) return null;

  return (
    <div className="border-b border-amber-500/20 bg-ink-950 px-4 py-3">
      <p className="mb-2 flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-widest text-amber-400">
        <ShieldCheck className="h-3 w-3" />
        {ops.length} change{ops.length > 1 ? "s" : ""} awaiting your review
      </p>
      <div className="space-y-2">
        {ops.map((op) => (
          <ApprovalCard
            key={op.id}
            op={op}
            onApprove={handleApprove}
            onReject={handleReject}
          />
        ))}
      </div>
    </div>
  );
}

// ── Main Panel ────────────────────────────────────────────────────────────────

export default function AssistantPanel() {
  const [open, setOpen] = useState(false);
  const [sheets, setSheets] = useState<Sheet[]>([]);
  const [sheetId, setSheetId] = useState<number | null>(null);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settings, setSettings] = useState<AgentSettings>(DEFAULT_SETTINGS);
  const [savingSettings, setSavingSettings] = useState(false);
  const [health, setHealth] = useState<{ ok: boolean; message: string } | null>(null);
  // Incrementing this causes the ApprovalTray to refresh after each agent response
  const [approvalRefresh, setApprovalRefresh] = useState(0);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.getSheets().then((items) => {
      setSheets(items);
      if (items.length > 0) setSheetId((current) => current ?? items[0].id);
    }).catch(() => setError("Could not load linked datasets."));
  }, []);

  useEffect(() => {
    api.agentGetSettings().then(setSettings).catch(() => setSettings(DEFAULT_SETTINGS));
  }, []);

  useEffect(() => { endRef.current?.scrollIntoView({ behavior: "smooth" }); }, [turns, busy]);

  const ask = async (text: string) => {
    if (!sheetId || !text.trim() || busy) return;
    const content = text.trim();
    setDraft("");
    setError(null);
    setTurns((current) => [...current, { id: crypto.randomUUID(), role: "user", content }]);
    setBusy(true);
    try {
      const result = await api.agentAsk({
        conversation_id: conversationId,
        sheet_id: sheetId,
        snapshot_id: null,
        message: content,
      });
      setConversationId(result.conversation_id);
      setTurns((current) => [
        ...current,
        { id: crypto.randomUUID(), role: "assistant", content: result.answer, result },
      ]);
      // Trigger approval tray refresh after every agent response
      setApprovalRefresh((n) => n + 1);
    } catch (reason) {
      setError(friendlyAgentError(reason));
    } finally {
      setBusy(false);
    }
  };

  const submit = (event: FormEvent) => { event.preventDefault(); void ask(draft); };
  const changeSheet = (value: number) => {
    setSheetId(value);
    setConversationId(null);
    setTurns([]);
    setError(null);
  };

  const saveSettings = async () => {
    setSavingSettings(true);
    setHealth(null);
    setError(null);
    try {
      await api.agentSaveSettings(settings);
    } catch (reason) {
      setError(friendlyAgentError(reason));
    } finally {
      setSavingSettings(false);
    }
  };

  const testOllama = async () => {
    setSavingSettings(true);
    setHealth(null);
    try {
      const result = await api.agentCheckOllama();
      setHealth({ ok: result.available, message: result.message });
    } catch {
      setHealth({ ok: false, message: "Could not reach Ollama" });
    } finally {
      setSavingSettings(false);
    }
  };

  return (
    <>
      {/* Floating trigger button */}
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="fixed bottom-6 right-6 z-40 flex h-12 w-12 items-center justify-center bg-amber-500 text-ink-950 shadow-lg transition-all hover:bg-amber-400 hover:shadow-amber-500/20"
        aria-label="Open AI assistant"
        id="assistant-panel-trigger"
      >
        {open ? <X className="h-5 w-5" /> : <MessageSquareText className="h-5 w-5" />}
      </button>

      {/* Side panel */}
      <aside
        className={`fixed inset-y-0 right-0 z-30 flex w-[420px] flex-col border-l border-ink-800 bg-ink-950 transition-transform duration-300 ${open ? "translate-x-0" : "translate-x-full"}`}
        aria-label="AI assistant panel"
      >
        {/* Header */}
        <header className="shrink-0 border-b border-ink-800">
          <div className="flex items-center justify-between gap-3 px-5 py-4">
            <div className="flex items-center gap-2">
              <ShieldCheck className="h-4 w-4 text-amber-400" />
              <span className="text-sm font-semibold text-ink-100">ProgressLens AI</span>
            </div>
            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={() => setSettingsOpen((s) => !s)}
                className="flex h-7 w-7 items-center justify-center text-ink-500 hover:text-ink-200"
                aria-label="AI settings"
                id="assistant-settings-toggle"
              >
                <Settings2 className="h-4 w-4" />
              </button>
              <button
                type="button"
                onClick={() => setOpen(false)}
                className="flex h-7 w-7 items-center justify-center text-ink-500 hover:text-ink-200"
                aria-label="Close assistant"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          </div>

          {/* Dataset selector */}
          <div className="px-5 pb-3">
            <select
              value={sheetId ?? ""}
              onChange={(e) => changeSheet(Number(e.target.value))}
              className="w-full border border-ink-800 bg-ink-900 px-3 py-1.5 text-xs text-ink-200 outline-none focus:border-amber-500/50"
              id="assistant-sheet-select"
            >
              {sheets.map((s) => (
                <option key={s.id} value={s.id}>{s.label}</option>
              ))}
              {sheets.length === 0 && <option value="">No datasets linked</option>}
            </select>
          </div>

          {/* Settings panel */}
          {settingsOpen && (
            <div className="border-t border-ink-800 px-5 py-4">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-xs font-semibold text-ink-200">Local model</p>
                  <p className="mt-0.5 text-[10px] text-ink-500">Only loopback endpoints are accepted.</p>
                </div>
                <button
                  type="button"
                  onClick={() => setSettings((current) => ({ ...current, enabled: !current.enabled }))}
                  className={`relative h-6 w-11 border transition-colors ${settings.enabled ? "border-sage-500/50 bg-sage-500/20" : "border-ink-700 bg-ink-900"}`}
                  role="switch"
                  aria-checked={settings.enabled}
                  aria-label="Enable AI assistant"
                >
                  <span className={`absolute top-1 h-3.5 w-3.5 transition-transform ${settings.enabled ? "translate-x-5 bg-sage-300" : "translate-x-1 bg-ink-500"}`} />
                </button>
              </div>
              <label className="mt-4 block text-[10px] font-semibold uppercase tracking-widest text-ink-500">
                Ollama endpoint
                <input
                  value={settings.ollama_endpoint}
                  onChange={(e) => setSettings((c) => ({ ...c, ollama_endpoint: e.target.value }))}
                  className="mt-1.5 w-full border border-ink-800 bg-ink-900 px-3 py-2 text-xs normal-case tracking-normal text-ink-200 outline-none focus:border-amber-500/50"
                  spellCheck={false}
                />
              </label>
              <div className="mt-3 grid grid-cols-[1fr_96px] gap-3">
                <label className="text-[10px] font-semibold uppercase tracking-widest text-ink-500">
                  Model
                  <input
                    value={settings.model_name}
                    onChange={(e) => setSettings((c) => ({ ...c, model_name: e.target.value }))}
                    className="mt-1.5 w-full border border-ink-800 bg-ink-900 px-3 py-2 text-xs normal-case tracking-normal text-ink-200 outline-none focus:border-amber-500/50"
                    spellCheck={false}
                  />
                </label>
                <label className="text-[10px] font-semibold uppercase tracking-widest text-ink-500">
                  Timeout
                  <input
                    type="number"
                    min={10}
                    max={300}
                    value={settings.timeout_seconds}
                    onChange={(e) => setSettings((c) => ({ ...c, timeout_seconds: Number(e.target.value) }))}
                    className="mt-1.5 w-full border border-ink-800 bg-ink-900 px-3 py-2 text-xs normal-case tracking-normal text-ink-200 outline-none focus:border-amber-500/50"
                  />
                </label>
              </div>
              <div className="mt-4 flex items-center justify-between gap-3">
                <span className="inline-flex items-center gap-1.5 text-[10px] text-ink-500">
                  <LockKeyhole className="h-3 w-3" /> Student data stays on this device
                </span>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => void saveSettings()}
                    disabled={savingSettings}
                    className="px-2.5 py-1.5 text-[10px] font-semibold text-ink-300 hover:text-ink-50 disabled:opacity-50"
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    onClick={() => void testOllama()}
                    disabled={savingSettings || !settings.enabled}
                    className="border border-ink-700 px-2.5 py-1.5 text-[10px] font-semibold text-amber-300 hover:border-amber-500/40 disabled:opacity-50"
                  >
                    Test connection
                  </button>
                </div>
              </div>
              {health && (
                <p className={`mt-3 text-[10px] leading-4 ${health.ok ? "text-emerald-300" : "text-red-300"}`}>
                  {health.message}
                </p>
              )}
            </div>
          )}
        </header>

        {/* ── Floating Approval Tray ── */}
        {sheetId !== null && (
          <ApprovalTray sheetId={sheetId} refreshTrigger={approvalRefresh} />
        )}

        {/* ── Chat area ── */}
        <div className="flex-1 overflow-y-auto px-5 py-5" aria-live="polite">
          {turns.length === 0 ? (
            <div className="pt-4">
              <p className="max-w-sm text-sm leading-6 text-ink-400">
                Explore changes across snapshots without assembling filters by hand.
              </p>
              <div className="mt-8 border-t border-ink-800">
                {STARTERS.map((starter) => (
                  <button
                    key={starter}
                    type="button"
                    onClick={() => void ask(starter)}
                    disabled={!sheetId || !settings.enabled}
                    className="group flex w-full items-start justify-between gap-4 border-b border-ink-800 py-4 text-left text-sm leading-5 text-ink-300 transition-colors hover:text-amber-300 disabled:opacity-40"
                  >
                    <span>{starter}</span>
                    <ChevronRight className="mt-0.5 h-4 w-4 shrink-0 text-ink-600 transition-transform group-hover:translate-x-1" />
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <div className="space-y-6">
              {turns.map((turn) => (
                <article
                  key={turn.id}
                  className={turn.role === "user" ? "ml-8 border-r-2 border-amber-500/50 pr-3 text-right" : "mr-2"}
                >
                  <p className="text-[10px] font-semibold uppercase tracking-widest text-ink-600">
                    {turn.role === "user" ? "You" : "ProgressLens"}
                  </p>
                  <p className="mt-1.5 whitespace-pre-wrap text-sm leading-6 text-ink-200">
                    {turn.content}
                  </p>
                  {turn.result && turn.result.rows.length > 0 && (
                    <div className="mt-4 divide-y divide-ink-800 border-y border-ink-800">
                      {turn.result.rows.map((row, index) => (
                        <div key={`${row.student_id}-${index}`} className="py-3">
                          <div className="flex items-baseline justify-between gap-3">
                            <p className="text-sm font-semibold text-ink-100">{row.name}</p>
                            <p className="text-[10px] tracking-wide text-ink-500">{row.roll_number}</p>
                          </div>
                          <p className="mt-1 text-xs leading-5 text-ink-400">{row.detail}</p>
                        </div>
                      ))}
                    </div>
                  )}
                  {turn.result?.warnings.map((warning) => (
                    <p key={warning} className="mt-2 text-xs text-amber-400">{warning}</p>
                  ))}
                  {turn.result && turn.result.tools_used.length > 0 && (
                    <p className="mt-2 text-[10px] text-ink-600">
                      Used {turn.result.tools_used.join(" → ")} · {turn.result.model}
                    </p>
                  )}
                </article>
              ))}
              {busy && <p className="text-xs text-ink-500">Reading the latest snapshots…</p>}
              <div ref={endRef} />
            </div>
          )}
          {error && (
            <div className="mt-4 border border-red-500/20 bg-red-500/5 px-3 py-2 text-xs leading-5 text-red-300">
              {error}
            </div>
          )}
        </div>

        {/* ── Input form ── */}
        <form onSubmit={submit} className="shrink-0 border-t border-ink-800 p-4">
          <div className="flex items-end gap-2 border border-ink-700 bg-ink-900 p-2 focus-within:border-amber-500/50">
            <textarea
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void ask(draft);
                }
              }}
              placeholder={
                !settings.enabled
                  ? "Enable local AI in settings"
                  : sheetId
                  ? "Ask about students or snapshots…"
                  : "Link a dataset first"
              }
              disabled={!sheetId || busy || !settings.enabled}
              rows={2}
              className="max-h-28 min-h-10 flex-1 resize-none bg-transparent px-1 py-1 text-sm leading-5 text-ink-100 outline-none placeholder:text-ink-600 disabled:cursor-not-allowed"
              id="assistant-input"
            />
            <button
              type="submit"
              disabled={!draft.trim() || !sheetId || busy || !settings.enabled}
              className="flex h-9 w-9 shrink-0 items-center justify-center bg-amber-500 text-ink-950 transition-colors hover:bg-amber-400 disabled:bg-ink-800 disabled:text-ink-600"
              aria-label="Send question"
              id="assistant-send"
            >
              <ArrowUp className="h-4 w-4" />
            </button>
          </div>
          <p className="mt-2 text-[10px] leading-4 text-ink-600">
            Qwen runs through local Ollama. Write requests create pending changes that require your approval above.
          </p>
        </form>
      </aside>
    </>
  );
}

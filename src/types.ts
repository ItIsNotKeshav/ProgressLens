// ──────────────────────────────────────────────────────────────
// types.ts – shared TypeScript types mirroring Rust structs
// ──────────────────────────────────────────────────────────────

export interface Student {
  id: number;
  name: string;
  roll_number: string;
  created_at: string;
}

export interface Sheet {
  id: number;
  url: string;
  label: string;
  created_at: string;
}

export interface Field {
  id: number;
  sheet_id: number | null;
  label: string;
  sheet_key: string;
  data_type: string | null;
  is_visible: boolean;
  display_name: string | null;
  max_value: number | null;
  include_in_dashboard: boolean;
}

export interface FieldSetup {
  id: number;
  sheet_key: string;
  label: string;
  display_name: string;
  data_type: string | null;
  max_value: number | null;
  is_visible: boolean;
  include_in_dashboard: boolean;
  sample_values: string[];
}

export interface FieldConfig {
  sheet_key: string;
  label: string;
  data_type: string | null;
  is_visible: boolean;
}

export interface Snapshot {
  id: number;
  synced_at: string;
  source_label: string;
}

export interface StudentValue {
  id: number;
  student_id: number;
  field_id: number;
  snapshot_id: number;
  value: string;
}

/** Flat row returned by get_all_students — one row per student,
 *  field values keyed by sheet_key. */
export interface StudentRow {
  id: number;
  name: string;
  roll_number: string;
  values: Record<string, string>;
}

export interface AvgScore {
  field_label: string;
  avg: number;
}

export interface LevelCount {
  field_label: string;
  level: string;
  count: number;
}

export interface RecentChange {
  student_name: string;
  field_label: string;
  old_val: string;
  new_val: string;
  synced_at: string;
}

export interface TopPerformer {
  name: string;
  score: number;
  field_label: string;
}

export interface DashboardStats {
  total_students: number;
  active_this_week: number;
  avg_score_per_field: AvgScore[];
  level_distribution: LevelCount[];
  recent_changes: RecentChange[];
  top_performers: TopPerformer[];
}

export interface SyncResult {
  sheet_id: number;
  snapshot_id: number;
  snapshot_created: boolean;
  students_upserted: number;
  fields_detected: number;
  source_label: string;
  synced_at: string;
}

export interface FieldDiff {
  field_key: string;
  label: string;
  value_a: string | null;
  value_b: string | null;
  changed: boolean;
}

export interface StudentDiff {
  student_id: number;
  name: string;
  roll_number: string;
  status: "unchanged" | "changed" | "added" | "removed";
  field_diffs: FieldDiff[];
}

export interface DiffResult {
  snapshot_a: number;
  snapshot_b: number;
  total_changed: number;
  total_added: number;
  total_removed: number;
  diffs: StudentDiff[];
}

export interface ReportConfig {
  snapshot_id: number;
  student_ids: number[];
  field_ids: number[];
  include_progress_notes: boolean;
  include_summary?: boolean;
}

export interface LevelTrackCount {
  field_id: number;
  display_name: string;
  counts: Record<string, number>;
}

export interface StudentLevelRow {
  student_id: number;
  name: string;
  usn: string;
  levels: Record<string, number | null>;
}

export interface LevelStats {
  per_track: LevelTrackCount[];
  student_level_rows: StudentLevelRow[];
  complete_count: number;
  gap_count: number;
  not_started_count: number;
}

export interface AgentResultRow {
  student_id: number;
  name: string;
  roll_number: string;
  detail: string;
  value: number | null;
}

export interface AgentAskResponse {
  conversation_id: string;
  answer: string;
  rows: AgentResultRow[];
  read_only: boolean;
  warnings: string[];
  tools_used: string[];
  model: string;
}

export interface AgentMessage {
  id: number;
  role: "user" | "assistant" | "tool";
  content: string;
  created_at: string;
}

export interface AgentSettings {
  enabled: boolean;
  ollama_endpoint: string;
  model_name: string;
  timeout_seconds: number;
  max_tool_iterations: number;
}

export interface AgentHealth {
  available: boolean;
  message: string;
  model: string;
}

/** Mirrors the Rust PendingOperation struct from operations.rs */
export interface PendingOperation {
  id: string;
  kind: string;
  status: string;
  payload_json: string;
  preview_json: string;
  created_at: string;
  expires_at: string;
}

/** Parsed preview data embedded inside a PendingOperation */
export interface OperationPreview {
  summary: string;
  reason?: string;
  student_name?: string;
  field_name?: string;
  current_value?: string;
  proposed_value?: string;
  notice?: string;
}

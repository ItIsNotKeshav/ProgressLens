// ──────────────────────────────────────────────────────────────
// api.ts – typed wrappers around Tauri invoke calls
// ──────────────────────────────────────────────────────────────
import { invoke } from "@tauri-apps/api/core";
import type {
  SyncResult,
  DiffResult,
  StudentRow,
  DashboardStats,
  ReportConfig,
  Snapshot,
  Field,
  Sheet,
  FieldConfig,
  FieldSetup,
  LevelStats,
} from "./types";

export const api = {
  /** Trigger Google OAuth. Opens browser on first use, silently refreshes after. */
  authenticateGoogle: (): Promise<boolean> =>
    invoke("authenticate_google"),

  /** Fetch all linked sheets */
  getSheets: (): Promise<Sheet[]> =>
    invoke("get_sheets"),

  /** Re-sync a sheet by ID */
  syncSheet: (sheetId: number): Promise<SyncResult> =>
    invoke("sync_sheet", { sheetId }),

  /** Preview sheet headers to configure fields */
  previewSheet: (sheetUrl: string): Promise<FieldConfig[]> =>
    invoke("preview_sheet", { sheetUrl }),

  /** Sync student data from a Google Sheet URL with explicit field configs */
  syncFromSheet: (sheetUrl: string, configs: FieldConfig[], sheetLabel?: string): Promise<SyncResult> =>
    invoke("sync_from_sheet", { sheetUrl, configs, sheetLabel: sheetLabel ?? null }),

  /** Compare two snapshots and return a diff */
  getDiff: (snapshotA: number, snapshotB: number): Promise<DiffResult> =>
    invoke("get_diff", { snapshotA, snapshotB }),

  /** List all students, optionally filtered to a snapshot */
  getAllStudents: (sheetId: number, snapshotId?: number): Promise<StudentRow[]> =>
    invoke("get_all_students", { sheetId, snapshotId: snapshotId ?? null }),

  /** List all snapshots for a sheet in chronological order */
  getSnapshots: (sheetId: number): Promise<Snapshot[]> =>
    invoke("get_snapshots", { sheetId }),

  /** Fetch all database fields */
  getFields: (): Promise<Field[]> =>
    invoke("get_fields"),

  /** Aggregate stats for the dashboard */
  getDashboardStats: (sheetId: number, snapshotId?: number): Promise<DashboardStats> =>
    invoke("get_dashboard_stats", { sheetId, snapshotId: snapshotId ?? null }),

  /** Generate an HTML report string */
  generateReport: (config: ReportConfig): Promise<string> =>
    invoke("generate_report", { config }),

  /** Temporarily seed mock data directly into sqlite */
  seedMockData: (): Promise<SyncResult> => 
    invoke("seed_mock_data"),

  getFieldSetup: (sheetId: number): Promise<FieldSetup[]> =>
    invoke("get_field_setup", { sheetId }),

  saveFieldSetup: (fields: FieldSetup[]): Promise<void> =>
    invoke("save_field_setup", { fields }),

  getLevelStats: (snapshotId: number): Promise<LevelStats> =>
    invoke("get_level_stats", { snapshotId }),

  /** Force sync all sheets, bypassing hash check */
  forceSync: (): Promise<boolean> =>
    invoke("force_sync"),

  /** Get seconds since last auto-sync (null if never synced) */
  getSyncStatus: (): Promise<number | null> =>
    invoke("get_sync_status"),

  /** Export report as Excel (.xlsx) — returns file path */
  exportToExcel: (config: ReportConfig): Promise<string> =>
    invoke("export_to_excel", { config }),

  /** Export report to a new Google Sheet — returns sheet URL */
  exportToGoogleSheet: (config: ReportConfig): Promise<string> =>
    invoke("export_to_google_sheet", { config }),

  /** Quick export: current view as Excel — returns file path */
  exportCurrentView: (sheetId: number, studentIds: number[], fieldIds: number[]): Promise<string> =>
    invoke("export_current_view", { sheetId, studentIds, fieldIds }),
};

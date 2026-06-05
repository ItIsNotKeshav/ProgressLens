# ProgressLens

ProgressLens is a modern desktop academic intelligence platform for tracking student progress from Google Sheets, analyzing performance over time, comparing historical snapshots, and generating clean progress reports.

Built with Tauri, React, TypeScript, TailwindCSS, SQLite, Recharts, React Router, and Lucide icons.

> Screenshots coming soon. Add dashboard, students, compare, reports, and field setup images here when ready.

## Overview

ProgressLens turns spreadsheet-based academic tracking into a focused desktop analytics workspace. It syncs student data from Google Sheets, stores each sync as a local snapshot, and helps users understand progress, gaps, improvements, regressions, and report-ready outcomes.

The app is designed for academic coordinators, mentors, faculty, and program teams who need a fast way to monitor student performance without manually comparing spreadsheet versions.

## Key Features

- **Google Sheets sync**: connect a Google Sheet, authenticate with Google, preview columns, and import student data.
- **Snapshot-based tracking**: every sync creates a historical snapshot for point-in-time comparisons.
- **Academic analytics dashboard**: view student health, completion, gap counts, score averages, level-track distributions, section analytics, recent activity, and sync status.
- **Student explorer**: search and filter students by status, level progress, section, score ranges, categorical fields, and visible columns.
- **Progress comparison**: compare two snapshots and inspect field-level changes for each student.
- **Diff visualization**: see added, removed, changed, and unchanged values with old/new comparisons.
- **Field configuration**: classify imported columns as score, level, categorical, text, link, or identifier fields.
- **Report generation**: build print-ready progress reports from selected students, fields, and snapshots.
- **Excel export**: export reports or the current filtered view as `.xlsx` files.
- **Google Sheet export backend**: backend support exists for exporting generated reports to a new Google Sheet.
- **Auto-sync**: background polling checks linked sheets and refreshes the UI when changes are detected.
- **Webhook trigger**: local sync trigger endpoint for external automation, such as Google Apps Script.
- **Dark-first desktop UI**: optimized for widescreen academic analytics workflows.

## App Screens

### Dashboard

Academic command center with health metrics, analytics cards, snapshot activity, risk indicators, charts, and recent changes.

```md
![Dashboard](./docs/images/dashboard.png)
```

### Students

Filterable student workspace with status badges, level indicators, score/category filters, column controls, and quick navigation into comparisons.

```md
![Students](./docs/images/students.png)
```

### Compare

Snapshot comparison workspace for viewing student-level and field-level changes over time.

```md
![Compare](./docs/images/compare.png)
```

### Reports

Report builder for selecting snapshots, students, fields, summaries, and progress notes before printing or exporting.

```md
![Reports](./docs/images/reports.png)
```

### Fields

Column setup workspace for configuring how imported spreadsheet columns should be interpreted and displayed.

```md
![Fields](./docs/images/fields.png)
```

## Tech Stack

### Frontend

- React 19
- TypeScript
- Vite
- TailwindCSS
- React Router
- Recharts
- Lucide React
- Tauri JavaScript API

### Desktop and Backend

- Tauri 2
- Rust
- SQLite
- SQLx
- Tokio
- Google OAuth2
- Google Sheets API
- rust_xlsxwriter
- tiny_http webhook listener

## Project Structure

```text
ProgressLens/
|-- public/                 Static assets
|-- src/                    React frontend
|   |-- api.ts              Typed Tauri invoke wrappers
|   |-- types.ts            Shared frontend types
|   |-- components/         App layout components
|   `-- pages/              Dashboard, Students, Compare, Reports, Fields
|-- src-tauri/              Tauri/Rust backend
|   |-- migrations/         SQLite migrations
|   `-- src/
|       |-- auth.rs         Google OAuth flow
|       |-- commands.rs     Tauri command handlers
|       |-- db.rs           SQLite initialization
|       |-- diff.rs         Snapshot diff logic
|       |-- export.rs       Excel and Google Sheets export
|       |-- sheets.rs       Google Sheets fetch/parse logic
|       |-- snapshot.rs     Snapshot persistence
|       |-- sync_worker.rs  Background auto-sync
|       `-- webhook.rs      Local sync trigger server
`-- package.json
```

## Getting Started

### Prerequisites

Install:

- Node.js
- npm
- Rust
- Tauri system dependencies for your platform

For Tauri setup, see the official Tauri prerequisites for your operating system.

### Install Dependencies

```bash
npm install
```

### Run Frontend Only

```bash
npm run dev
```

The Vite dev server runs on:

```text
http://localhost:1420
```

### Run Desktop App

```bash
npm run tauri dev
```

### Build Frontend

```bash
npm run build
```

### Build Desktop App

```bash
npm run tauri build
```

## Google Sheets Setup

ProgressLens uses Google OAuth and the Google Sheets API to read spreadsheet data.

Expected sheet format:

- First row contains column headers.
- Rows after the header contain student records.
- The app auto-detects identity columns such as name, roll number, or USN.
- Additional columns can be configured after import.

Recommended column types:

- Student name
- Roll number or USN
- Section
- Score fields
- Level-track fields
- Categorical fields
- Submission links
- Notes or text fields

## Data Model

ProgressLens stores synced data locally in SQLite.

Main entities:

- `sheets`: linked Google Sheets
- `students`: student identity records
- `fields`: imported and configured columns
- `snapshots`: each sync event
- `student_values`: field values for each student in each snapshot

This model makes historical comparisons possible without relying on old spreadsheet versions.

## Sync Behavior

The app supports three sync paths:

- Manual sheet sync from the UI
- Background auto-sync every 45 seconds
- Local webhook trigger on:

```text
POST http://127.0.0.1:19291/sync-trigger
```

The background sync worker hashes raw Google Sheets responses and only stores a new snapshot when data changes, unless a force sync is requested.

## Exports and Reports

ProgressLens can:

- Generate HTML reports for preview and printing
- Print or save reports as PDF through the system print flow
- Export selected report data to Excel
- Export the current student view to Excel
- Create Google Sheets reports from the backend export command

Exported Excel files are saved to the user's Downloads folder.

## Development Notes

Useful commands:

```bash
npm run dev
npm run build
npm run tauri dev
npm run tauri build
```

The frontend communicates with Rust through typed wrappers in:

```text
src/api.ts
```

Most backend operations are exposed as Tauri commands from:

```text
src-tauri/src/commands.rs
src-tauri/src/export.rs
```

## Current Status

ProgressLens is currently an active desktop app prototype with working sync, local persistence, analytics views, snapshot comparison, report generation, and exports.

Planned polish areas:

- Add product screenshots to this README
- Improve onboarding for first-time Google Sheets setup
- Expose Google Sheets export in the report UI
- Add more formal test coverage
- Continue refining the premium desktop analytics interface

## License

No license has been specified yet.

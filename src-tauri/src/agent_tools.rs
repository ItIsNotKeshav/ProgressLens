use crate::diff;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct AgentResultRow {
    pub student_id: i64,
    pub name: String,
    pub roll_number: String,
    pub detail: String,
    pub value: Option<f64>,
}

pub struct ToolExecution {
    pub result: Value,
    pub rows: Vec<AgentResultRow>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub arguments_schema: Value,
    pub output_schema: Value,
}

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "get_student_history",
            description: concat!(
                "Fetch score and level values for ONE specific student across multiple recent snapshots. ",
                "Use this when the user asks about a single student's progress over time, recent scores, or trend. ",
                "Do NOT use for comparing two specific snapshots (use compare_student_snapshots). ",
                "Do NOT use for finding which students are declining (use find_declining_students). ",
                "ARGUMENT 'student_query': the student's full or partial name, or their roll number. ",
                "  Example values: \"Rahul\", \"Rahul Kumar\", \"CS101\". ",
                "ARGUMENT 'snapshot_count' (optional, default 3): how many recent snapshots to include (1-12). ",
                "OUTPUT fields: student {student_id, name, roll_number}, snapshots[] each with snapshot_id, synced_at, values[]."
            ),
            arguments_schema: json!({
                "type": "object",
                "required": ["student_query"],
                "properties": {
                    "student_query": {
                        "type": "string",
                        "description": "Student name (full or partial) or roll number. Example: \"Rahul\", \"CS101\"."
                    },
                    "snapshot_count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 12,
                        "description": "How many recent snapshots to fetch. Default is 3."
                    }
                }
            }),
            output_schema: json!({
                "student": {"type": "object", "properties": {"student_id": {"type": "integer"}, "name": {"type": "string"}, "roll_number": {"type": "string"}}},
                "snapshots": {"type": "array", "items": {"properties": {"snapshot_id": {"type": "integer"}, "synced_at": {"type": "string"}, "values": {"type": "array"}}}}
            }),
        },

        ToolDefinition {
            name: "compare_student_snapshots",
            description: concat!(
                "Compare ALL configured field values for ONE specific student between two specific snapshots, ",
                "showing exactly what changed (old value to new value) for each field. ",
                "Use this when the user asks how a student changed between snapshot A and B, or what changed for X. ",
                "Do NOT use for progress over many snapshots (use get_student_history). ",
                "ARGUMENT 'student_query': the student's full or partial name, or their roll number. ",
                "ARGUMENTS 'snapshot_a' and 'snapshot_b' (optional): integer snapshot IDs. ",
                "  If omitted, the two most recent snapshots are used automatically. ",
                "OUTPUT fields: student {student_id, name, roll_number}, snapshot_a (int), snapshot_b (int), ",
                "changes[] each with {field_key, label, old_value, new_value}."
            ),
            arguments_schema: json!({
                "type": "object",
                "required": ["student_query"],
                "properties": {
                    "student_query": {
                        "type": "string",
                        "description": "Student name (full or partial) or roll number."
                    },
                    "snapshot_a": {
                        "type": "integer",
                        "description": "ID of the earlier snapshot. Omit to use the second-most-recent."
                    },
                    "snapshot_b": {
                        "type": "integer",
                        "description": "ID of the later snapshot. Omit to use the most recent."
                    }
                }
            }),
            output_schema: json!({
                "student": {"type": "object"},
                "snapshot_a": {"type": "integer"},
                "snapshot_b": {"type": "integer"},
                "changes": {"type": "array", "items": {"properties": {"field_key": {"type": "string"}, "label": {"type": "string"}, "old_value": {"type": "string"}, "new_value": {"type": "string"}}}}
            }),
        },

        ToolDefinition {
            name: "find_declining_students",
            description: concat!(
                "Find and rank ALL students whose score fields DECREASED between the earliest and latest snapshots ",
                "in a recent window. Returns a ranked list from worst decline to least. ",
                "Use this when the user asks: who is struggling, falling behind, declining, performing worst, needs help. ",
                "Do NOT use for a single specific student (use get_student_history). ",
                "ARGUMENT 'snapshot_count' (optional, default 2): window of recent snapshots to consider (2-12). ",
                "ARGUMENT 'limit' (optional, default 25): maximum number of students to return (1-50). ",
                "OUTPUT fields: snapshot_a (int), snapshot_b (int), students[] sorted by largest decline first, ",
                "each with {student_id, name, roll_number, detail (field deltas as text), value (total delta as number)}."
            ),
            arguments_schema: json!({
                "type": "object",
                "properties": {
                    "snapshot_count": {
                        "type": "integer",
                        "minimum": 2,
                        "maximum": 12,
                        "description": "Number of recent snapshots to consider. Default is 2."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Maximum students to return. Default is 25."
                    }
                }
            }),
            output_schema: json!({
                "snapshot_a": {"type": "integer"},
                "snapshot_b": {"type": "integer"},
                "students": {"type": "array", "items": {"properties": {"student_id": {"type": "integer"}, "name": {"type": "string"}, "roll_number": {"type": "string"}, "detail": {"type": "string"}, "value": {"type": "number"}}}}
            }),
        },

        ToolDefinition {
            name: "find_improving_students",
            description: concat!(
                "Find and rank ALL students whose score fields INCREASED between the earliest and latest snapshots ",
                "in a recent window. Returns a ranked list from greatest improvement to least. ",
                "Use this when the user asks: who improved, is doing better, top improvers, showed most progress, on track. ",
                "Do NOT use for a single specific student (use get_student_history). ",
                "ARGUMENT 'snapshot_count' (optional, default 2): window of recent snapshots to consider (2-12). ",
                "ARGUMENT 'limit' (optional, default 25): maximum number of students to return (1-50). ",
                "OUTPUT fields: snapshot_a (int), snapshot_b (int), students[] sorted by largest improvement first, ",
                "each with {student_id, name, roll_number, detail (field deltas as text), value (total delta as number)}."
            ),
            arguments_schema: json!({
                "type": "object",
                "properties": {
                    "snapshot_count": {
                        "type": "integer",
                        "minimum": 2,
                        "maximum": 12,
                        "description": "Number of recent snapshots to consider. Default is 2."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Maximum students to return. Default is 25."
                    }
                }
            }),
            output_schema: json!({
                "snapshot_a": {"type": "integer"},
                "snapshot_b": {"type": "integer"},
                "students": {"type": "array", "items": {"properties": {"student_id": {"type": "integer"}, "name": {"type": "string"}, "roll_number": {"type": "string"}, "detail": {"type": "string"}, "value": {"type": "number"}}}}
            }),
        },

        ToolDefinition {
            name: "search_students",
            description: concat!(
                "Search for students by partial name or partial roll number in the selected dataset. ",
                "Use this when: (a) the student name in the question is ambiguous or a last name only, ",
                "(b) you need to confirm a student exists before calling get_student_history, ",
                "(c) the user explicitly asks to find or list students by name pattern. ",
                "Do NOT use when you already have a clear, unambiguous student name — go straight to get_student_history. ",
                "ARGUMENT 'query': partial or full name or roll number. Example values: \"Kumar\", \"CS10\", \"Priya\". ",
                "ARGUMENT 'limit' (optional, default 10): maximum results (1-25). ",
                "OUTPUT fields: matches[] each with {student_id, name, roll_number}."
            ),
            arguments_schema: json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 100,
                        "description": "Partial or full name or roll number. Example: \"Kumar\", \"CS10\"."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 25,
                        "description": "Maximum number of results to return. Default is 10."
                    }
                }
            }),
            output_schema: json!({
                "matches": {"type": "array", "items": {"properties": {"student_id": {"type": "integer"}, "name": {"type": "string"}, "roll_number": {"type": "string"}}}}
            }),
        },

        ToolDefinition {
            name: "filter_students_by_field",
            description: concat!(
                "Filter students in the current snapshot by any combination of field conditions. ",
                "Use this for ANY question that asks to FIND or LIST students based on their field values. ",
                "Examples of when to use this tool: ",
                "  'students with soft skill level 3' ",
                "  'students in section A' ",
                "  'students with attendance below 75' ",
                "  'students missing coding score data' ",
                "  'students in section A AND coding score below 50' ",
                "Do NOT use for trend/change analysis over time (use find_declining_students or find_improving_students). ",
                "Do NOT use when asking about a single named student (use get_student_history). ",
                "ARGUMENT 'conditions': required. Array of 1-8 condition objects combined with AND logic. ",
                "  Each condition must have: ",
                "    'field': the sheet_key of the field to filter on. ",
                "      CRITICAL: use the exact sheet_key from the 'fields' list in the dataset context. ",
                "      The sheet_key is the 'field_key' property shown in the dataset context, not the label. ",
                "      Example: if context shows {\"field_key\":\"soft_skill\",...} use 'soft_skill'. ",
                "    'operator': one of: equals, not_equals, greater_than, less_than, greater_equal, ",
                "      less_equal, contains, is_empty, is_not_empty. ",
                "      Use numeric operators (greater_than, less_than, greater_equal, less_equal) ONLY for score/level fields. ",
                "      Use equals, not_equals, contains for text/categorical/identifier fields. ",
                "      Use is_empty or is_not_empty to find students with missing or present data. ",
                "    'value': the comparison value as a string. Omit entirely for is_empty/is_not_empty. ",
                "      Always pass as a string even for numbers: \"50\", \"3\", \"75.5\". ",
                "ARGUMENT 'limit' (optional, default 25): max students to return (1-50). ",
                "EXAMPLES: ",
                "  Single: {\"conditions\":[{\"field\":\"soft_skill\",\"operator\":\"equals\",\"value\":\"3\"}],\"limit\":25} ",
                "  Multi:  {\"conditions\":[{\"field\":\"section\",\"operator\":\"equals\",\"value\":\"A\"},{\"field\":\"coding_score\",\"operator\":\"less_than\",\"value\":\"50\"}]} ",
                "  Empty:  {\"conditions\":[{\"field\":\"attendance\",\"operator\":\"is_empty\"}]} ",
                "OUTPUT fields: snapshot_id (int), conditions_applied[] (strings describing each filter), ",
                "total_matched (int), students[] each with {student_id, name, roll_number, matched_values: {field_key: value}}."
            ),
            arguments_schema: json!({
                "type": "object",
                "required": ["conditions"],
                "properties": {
                    "conditions": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 8,
                        "description": "Array of filter conditions combined with AND logic. All conditions must be satisfied.",
                        "items": {
                            "type": "object",
                            "required": ["field", "operator"],
                            "properties": {
                                "field": {
                                    "type": "string",
                                    "description": "sheet_key of the field (from dataset context). Example: 'soft_skill', 'attendance', 'section'."
                                },
                                "operator": {
                                    "type": "string",
                                    "enum": ["equals","not_equals","greater_than","less_than","greater_equal","less_equal","contains","is_empty","is_not_empty"],
                                    "description": "Comparison operator. Numeric operators only for score/level fields."
                                },
                                "value": {
                                    "type": "string",
                                    "description": "Value to compare against. Omit for is_empty/is_not_empty. Always a string: '3', '75', 'A'."
                                }
                            }
                        }
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Maximum students to return. Default is 25."
                    }
                }
            }),
            output_schema: json!({
                "snapshot_id": {"type": "integer"},
                "conditions_applied": {"type": "array", "items": {"type": "string"}},
                "total_matched": {"type": "integer"},
                "students": {
                    "type": "array",
                    "items": {
                        "properties": {
                            "student_id": {"type": "integer"},
                            "name": {"type": "string"},
                            "roll_number": {"type": "string"},
                            "matched_values": {"type": "object"}
                        }
                    }
                }
            }),
        },

        ToolDefinition {
            name: "get_dashboard_metrics",
            description: concat!(
                "Return aggregate statistics for the currently selected snapshot: total student count, ",
                "average scores per score field, level distribution per level field, and top 10 performers. ",
                "Use this when the user asks about: class overview, total students, averages, how many students, ",
                "class performance summary, top performers, distribution, statistics. ",
                "This tool takes NO arguments — always call it with an empty arguments object: {}. ",
                "Do NOT use for a single specific student (use get_student_history or compare_student_snapshots). ",
                "OUTPUT fields: snapshot_id (int), total_students (int), ",
                "average_scores[] each {field, average}, level_distribution[] each {field, level, count}, ",
                "top_performers[] each {name, roll_number, field, value}."
            ),
            arguments_schema: json!({
                "type": "object",
                "properties": {}
            }),
            output_schema: json!({
                "snapshot_id": {"type": "integer"},
                "total_students": {"type": "integer"},
                "average_scores": {"type": "array", "items": {"properties": {"field": {"type": "string"}, "average": {"type": "number"}}}},
                "level_distribution": {"type": "array", "items": {"properties": {"field": {"type": "string"}, "level": {"type": "string"}, "count": {"type": "integer"}}}},
                "top_performers": {"type": "array", "items": {"properties": {"name": {"type": "string"}, "roll_number": {"type": "string"}, "field": {"type": "string"}, "value": {"type": "number"}}}}
            }),
        },

        ToolDefinition {
            name: "create_pending_operation",
            description: concat!(
                "Record a requested data change as a PENDING OPERATION for user review. ",
                "IMPORTANT: This tool NEVER executes the change. It only logs it for later confirmation. ",
                "Use this for ANY request that would modify data: update a score, add a note, flag an intervention. ",
                "ALWAYS call this when the user requests a change. NEVER claim you made the change directly. ",
                "ARGUMENT 'kind': one of: 'student_update' (score or field change), 'mentor_note' (add a note), ",
                "  'intervention' (flag a student for follow-up), 'other' (anything else). ",
                "ARGUMENT 'summary': plain English description of the requested change (3-500 chars). ",
                "ARGUMENT 'summary': plain English description of the requested change (3-500 chars). ",
                "  Example: \"Update Math score for Rahul Kumar (CS101) to 90 in the latest snapshot.\" ",
                "ARGUMENT 'student_id' (optional): Integer ID of the student. Required if kind is 'student_update'. ",
                "ARGUMENT 'field_key' (optional): The sheet_key of the field being changed. Required if kind is 'student_update'. ",
                "ARGUMENT 'proposed_value' (optional): The new value to set. Required if kind is 'student_update'. ",
                "ARGUMENT 'reason' (optional): Brief justification for the change to display to the user. ",
                "OUTPUT fields: operation_id (string), status (always 'pending'), summary, executed (always false)."
            ),
            arguments_schema: json!({
                "type": "object",
                "required": ["kind", "summary"],
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["student_update", "mentor_note", "intervention", "other"],
                        "description": "Category: student_update, mentor_note, intervention, or other."
                    },
                    "summary": {
                        "type": "string",
                        "minLength": 3,
                        "maxLength": 500,
                        "description": "Plain English description. Example: \"Update Math score for Rahul Kumar to 90.\""
                    },
                    "student_id": {
                        "type": "integer",
                        "description": "Required for student_update. The exact student_id."
                    },
                    "field_key": {
                        "type": "string",
                        "description": "Required for student_update. The exact sheet_key (e.g. 'math_score')."
                    },
                    "proposed_value": {
                        "type": "string",
                        "description": "Required for student_update. The new value to assign."
                    },
                    "reason": {
                        "type": "string",
                        "description": "AI's brief reason for proposing this change."
                    }
                }
            }),
            output_schema: json!({
                "operation_id": {"type": "string"},
                "status": {"const": "pending"},
                "summary": {"type": "string"},
                "executed": {"const": false}
            }),
        },
    ]
}

pub async fn execute(
    db: &SqlitePool,
    conversation_id: &str,
    sheet_id: i64,
    selected_snapshot: i64,
    tool: &str,
    arguments: &Value,
) -> Result<ToolExecution, String> {
    match tool {
        "get_student_history" => student_history(db, sheet_id, arguments).await,
        "compare_student_snapshots" => compare_student(db, sheet_id, arguments).await,
        "find_declining_students" => score_changes(db, sheet_id, arguments, false).await,
        "find_improving_students" => score_changes(db, sheet_id, arguments, true).await,
        "search_students" => search_students(db, sheet_id, arguments).await,
        "get_dashboard_metrics" => dashboard_metrics(db, sheet_id, selected_snapshot).await,
        "create_pending_operation" => create_pending_operation(db, conversation_id, sheet_id, selected_snapshot, arguments).await,
        "filter_students_by_field" => filter_students_by_field(db, sheet_id, selected_snapshot, arguments).await,
        _ => Err(format!("AI_TOOL_INVALID: Unknown tool '{tool}'.")),

    }
}

async fn snapshots(db: &SqlitePool, sheet_id: i64, count: usize) -> Result<Vec<(i64, String)>, String> {
    let limit = count.clamp(1, 12) as i64;
    sqlx::query_as("SELECT id, synced_at FROM snapshots WHERE sheet_id = ? ORDER BY id DESC LIMIT ?")
        .bind(sheet_id).bind(limit).fetch_all(db).await.map_err(db_err)
}

async fn resolve_student(db: &SqlitePool, sheet_id: i64, query: &str) -> Result<(i64, String, String), String> {
    let query = query.trim();
    if query.is_empty() || query.len() > 100 { return Err("AI_TOOL_ARGUMENT: student_query must contain between 1 and 100 characters.".into()); }
    let exact = query.to_lowercase();
    let pattern = format!("%{}%", escape_like(&exact));
    let matches: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT DISTINCT s.id, s.name, s.roll_number FROM students s \
         JOIN student_values sv ON sv.student_id = s.id JOIN snapshots snap ON snap.id = sv.snapshot_id \
         WHERE snap.sheet_id = ? AND (lower(s.name) = ? OR lower(s.roll_number) = ? OR lower(s.name) LIKE ? ESCAPE '\\' OR lower(s.roll_number) LIKE ? ESCAPE '\\') \
         ORDER BY CASE WHEN lower(s.roll_number) = ? THEN 0 WHEN lower(s.name) = ? THEN 1 ELSE 2 END, s.roll_number LIMIT 6"
    ).bind(sheet_id).bind(&exact).bind(&exact).bind(&pattern).bind(&pattern).bind(&exact).bind(&exact)
        .fetch_all(db).await.map_err(db_err)?;
    if matches.is_empty() { return Err(format!("AI_STUDENT_NOT_FOUND: No student matching '{query}' exists in the selected dataset.")); }
    if matches.len() > 1 {
        let choices = matches.iter().map(|(_, name, roll)| format!("{name} ({roll})")).collect::<Vec<_>>().join(", ");
        return Err(format!("AI_STUDENT_AMBIGUOUS: More than one student matched. Ask the user to choose: {choices}."));
    }
    Ok(matches[0].clone())
}

async fn search_students(db: &SqlitePool, sheet_id: i64, args: &Value) -> Result<ToolExecution, String> {
    let query = required_string(args, "query", 100)?;
    let limit = int_arg(args, "limit", 10, 1, 25)?;
    let pattern = format!("%{}%", escape_like(&query.to_lowercase()));
    let matches: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT DISTINCT s.id, s.name, s.roll_number FROM students s \
         JOIN student_values sv ON sv.student_id = s.id JOIN snapshots snap ON snap.id = sv.snapshot_id \
         WHERE snap.sheet_id = ? AND (lower(s.name) LIKE ? ESCAPE '\\' OR lower(s.roll_number) LIKE ? ESCAPE '\\') \
         ORDER BY s.name LIMIT ?"
    ).bind(sheet_id).bind(&pattern).bind(&pattern).bind(limit).fetch_all(db).await.map_err(db_err)?;
    let rows = matches.iter().map(|(id, name, roll)| AgentResultRow { student_id:*id, name:name.clone(), roll_number:roll.clone(), detail:"Matched by name or roll number".into(), value:None }).collect();
    Ok(ToolExecution { result: json!({"matches": matches.iter().map(|(id,name,roll)| json!({"student_id":id,"name":name,"roll_number":roll})).collect::<Vec<_>>()}), rows, warnings:vec![] })
}

async fn student_history(db: &SqlitePool, sheet_id: i64, args: &Value) -> Result<ToolExecution, String> {
    let query = required_string(args, "student_query", 100)?;
    let count = int_arg(args, "snapshot_count", 3, 1, 12)? as usize;
    let student = resolve_student(db, sheet_id, &query).await?;
    let snaps = snapshots(db, sheet_id, count).await?;
    if snaps.is_empty() { return Err("AI_DATA_UNAVAILABLE: No snapshots exist in the selected dataset.".into()); }
    let mut history = Vec::new();
    let mut rows = Vec::new();
    for (snapshot_id, synced_at) in snaps {
        let values: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT f.sheet_key, COALESCE(f.display_name, f.label), sv.value FROM student_values sv \
             JOIN fields f ON f.id = sv.field_id WHERE sv.snapshot_id = ? AND sv.student_id = ? \
             AND f.sheet_id = ? AND f.data_type IN ('score','level','categorical') ORDER BY f.id"
        ).bind(snapshot_id).bind(student.0).bind(sheet_id).fetch_all(db).await.map_err(db_err)?;
        let value_json = values.iter().map(|(key,label,value)| json!({"field_key":key,"label":label,"value":value})).collect::<Vec<_>>();
        let detail = values.iter().map(|(_,label,value)| format!("{label}: {value}")).collect::<Vec<_>>().join(", ");
        rows.push(AgentResultRow { student_id:student.0, name:student.1.clone(), roll_number:student.2.clone(), detail:format!("{synced_at} — {detail}"), value:None });
        history.push(json!({"snapshot_id":snapshot_id,"synced_at":synced_at,"values":value_json}));
    }
    Ok(ToolExecution { result:json!({"student":{"student_id":student.0,"name":student.1,"roll_number":student.2},"snapshots":history}), rows, warnings:vec![] })
}

async fn compare_student(db: &SqlitePool, sheet_id: i64, args: &Value) -> Result<ToolExecution, String> {
    let query = required_string(args, "student_query", 100)?;
    let student = resolve_student(db, sheet_id, &query).await?;
    let recent = snapshots(db, sheet_id, 2).await?;
    if recent.len() < 2 { return Err("AI_DATA_UNAVAILABLE: At least two snapshots are required for comparison.".into()); }
    let a = args.get("snapshot_a").and_then(Value::as_i64).unwrap_or(recent[1].0);
    let b = args.get("snapshot_b").and_then(Value::as_i64).unwrap_or(recent[0].0);
    validate_snapshot_pair(db, sheet_id, a, b).await?;
    let diff = diff::compute_diff(db, a, b).await?;
    let found = diff.diffs.into_iter().find(|item| item.student_id == student.0)
        .ok_or_else(|| "AI_DATA_UNAVAILABLE: The student is not present in one or both selected snapshots.".to_string())?;
    let changes = found.field_diffs.into_iter().filter(|field| field.changed).map(|field| json!({"field_key":field.field_key,"label":field.label,"old_value":field.value_a,"new_value":field.value_b})).collect::<Vec<_>>();
    let detail = changes.iter().map(|change| format!("{}: {} → {}", change["label"].as_str().unwrap_or("Field"), change["old_value"].as_str().unwrap_or("—"), change["new_value"].as_str().unwrap_or("—"))).collect::<Vec<_>>().join(", ");
    let rows = vec![AgentResultRow { student_id:student.0, name:student.1.clone(), roll_number:student.2.clone(), detail:if detail.is_empty(){"No field changes".into()}else{detail}, value:None }];
    Ok(ToolExecution { result:json!({"student":{"student_id":student.0,"name":student.1,"roll_number":student.2},"snapshot_a":a,"snapshot_b":b,"changes":changes}), rows, warnings:vec![] })
}

async fn score_changes(db: &SqlitePool, sheet_id: i64, args: &Value, improving: bool) -> Result<ToolExecution, String> {
    let count = int_arg(args, "snapshot_count", 2, 2, 12)? as usize;
    let limit = int_arg(args, "limit", 25, 1, 50)? as usize;
    let snaps = snapshots(db, sheet_id, count).await?;
    if snaps.len() < 2 { return Err("AI_DATA_UNAVAILABLE: At least two snapshots are required for change analysis.".into()); }
    let latest = snaps[0].0;
    let earliest = snaps[snaps.len() - 1].0;
    let data = sqlx::query(
        "SELECT s.id, s.name, s.roll_number, COALESCE(f.display_name,f.label) field_label, \
         CAST(old.value AS REAL) old_value, CAST(new.value AS REAL) new_value \
         FROM student_values new JOIN student_values old ON old.student_id=new.student_id AND old.field_id=new.field_id AND old.snapshot_id=? \
         JOIN students s ON s.id=new.student_id JOIN fields f ON f.id=new.field_id \
         WHERE new.snapshot_id=? AND f.sheet_id=? AND f.data_type='score' AND trim(old.value)!='' AND trim(new.value)!=''"
    ).bind(earliest).bind(latest).bind(sheet_id).fetch_all(db).await.map_err(db_err)?;
    let mut grouped: HashMap<i64,(String,String,Vec<(String,f64)>)> = HashMap::new();
    for row in data {
        let delta = row.try_get::<f64,_>("new_value").unwrap_or_default() - row.try_get::<f64,_>("old_value").unwrap_or_default();
        if (improving && delta <= 0.0) || (!improving && delta >= 0.0) { continue; }
        grouped.entry(row.get("id")).or_insert_with(||(row.get("name"),row.get("roll_number"),vec![])).2.push((row.get("field_label"),delta));
    }
    let mut rows = grouped.into_iter().map(|(id,(name,roll,changes))| {
        let total=changes.iter().map(|(_,d)|*d).sum::<f64>();
        let detail=changes.iter().map(|(field,d)|format!("{field} {d:+.1}")).collect::<Vec<_>>().join(", ");
        AgentResultRow{student_id:id,name,roll_number:roll,detail,value:Some(total)}
    }).collect::<Vec<_>>();
    rows.sort_by(|a,b| if improving { b.value.unwrap_or_default().total_cmp(&a.value.unwrap_or_default()) } else { a.value.unwrap_or_default().total_cmp(&b.value.unwrap_or_default()) });
    rows.truncate(limit);
    Ok(ToolExecution { result:json!({"snapshot_a":earliest,"snapshot_b":latest,"students":rows}), rows, warnings:vec![] })
}

async fn dashboard_metrics(db: &SqlitePool, sheet_id: i64, snapshot_id: i64) -> Result<ToolExecution, String> {
    validate_snapshot(db, sheet_id, snapshot_id).await?;
    let total_students: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT student_id) FROM student_values WHERE snapshot_id=?").bind(snapshot_id).fetch_one(db).await.map_err(db_err)?;
    let averages: Vec<(String,f64)> = sqlx::query_as("SELECT COALESCE(f.display_name,f.label), AVG(CAST(sv.value AS REAL)) FROM student_values sv JOIN fields f ON f.id=sv.field_id WHERE sv.snapshot_id=? AND f.sheet_id=? AND f.data_type='score' AND trim(sv.value)!='' GROUP BY f.id")
        .bind(snapshot_id).bind(sheet_id).fetch_all(db).await.map_err(db_err)?;
    let levels: Vec<(String,String,i64)> = sqlx::query_as("SELECT COALESCE(f.display_name,f.label), sv.value, COUNT(DISTINCT sv.student_id) FROM student_values sv JOIN fields f ON f.id=sv.field_id WHERE sv.snapshot_id=? AND f.sheet_id=? AND f.data_type='level' AND trim(sv.value)!='' GROUP BY f.id,sv.value")
        .bind(snapshot_id).bind(sheet_id).fetch_all(db).await.map_err(db_err)?;
    let top: Vec<(i64,String,String,String,f64)> = sqlx::query_as("SELECT s.id,s.name,s.roll_number,COALESCE(f.display_name,f.label),CAST(sv.value AS REAL) FROM student_values sv JOIN students s ON s.id=sv.student_id JOIN fields f ON f.id=sv.field_id WHERE sv.snapshot_id=? AND f.sheet_id=? AND f.data_type='score' AND trim(sv.value)!='' ORDER BY CAST(sv.value AS REAL) DESC LIMIT 10")
        .bind(snapshot_id).bind(sheet_id).fetch_all(db).await.map_err(db_err)?;
    let rows=top.iter().map(|(id,name,roll,field,value)|AgentResultRow{student_id:*id,name:name.clone(),roll_number:roll.clone(),detail:format!("{field}: {value:.1}"),value:Some(*value)}).collect();
    Ok(ToolExecution { result:json!({"snapshot_id":snapshot_id,"total_students":total_students,"average_scores":averages.iter().map(|(field,avg)|json!({"field":field,"average":avg})).collect::<Vec<_>>(),"level_distribution":levels.iter().map(|(field,level,count)|json!({"field":field,"level":level,"count":count})).collect::<Vec<_>>(),"top_performers":top.iter().map(|(_,name,roll,field,value)|json!({"name":name,"roll_number":roll,"field":field,"value":value})).collect::<Vec<_>>() }), rows, warnings:vec![] })
}

async fn create_pending_operation(db:&SqlitePool,conversation_id:&str,sheet_id:i64,snapshot_id:i64,args:&Value)->Result<ToolExecution,String>{
    let kind=required_string(args,"kind",50)?;
    if !["student_update","mentor_note","intervention","other"].contains(&kind.as_str()){return Err("AI_TOOL_ARGUMENT: Unsupported pending operation kind.".into());}
    let summary=required_string(args,"summary",500)?;
    let reason=args.get("reason").and_then(Value::as_str).unwrap_or("No reason provided.");

    let mut payload = serde_json::Map::new();
    payload.insert("kind".into(), json!(kind));
    payload.insert("summary".into(), json!(summary));
    payload.insert("source".into(), json!("local_ai"));
    payload.insert("reason".into(), json!(reason));

    let mut preview = serde_json::Map::new();
    preview.insert("summary".into(), json!(summary));
    preview.insert("reason".into(), json!(reason));
    preview.insert("notice".into(), json!("This request has not changed any data and requires explicit user confirmation."));

    if kind == "student_update" {
        let student_id = args.get("student_id").and_then(Value::as_i64).ok_or("AI_TOOL_ARGUMENT: student_id is required for student_update")?;
        let field_key = args.get("field_key").and_then(Value::as_str).ok_or("AI_TOOL_ARGUMENT: field_key is required for student_update")?;
        let proposed_value = args.get("proposed_value").and_then(Value::as_str).ok_or("AI_TOOL_ARGUMENT: proposed_value is required for student_update")?;

        let student_name: Option<String> = sqlx::query_scalar("SELECT name FROM students WHERE id = ?").bind(student_id).fetch_optional(db).await.map_err(db_err)?;
        let student_name = student_name.unwrap_or_else(|| "Unknown Student".into());

        let field_row: Option<(i64, String)> = sqlx::query_as("SELECT id, COALESCE(display_name, label) FROM fields WHERE sheet_id = ? AND sheet_key = ?")
            .bind(sheet_id).bind(field_key).fetch_optional(db).await.map_err(db_err)?;
        let (field_id, field_name) = field_row.ok_or_else(|| "AI_TOOL_ARGUMENT: Field not found".to_string())?;

        let current_val: Option<String> = sqlx::query_scalar("SELECT value FROM student_values WHERE snapshot_id = ? AND student_id = ? AND field_id = ?")
            .bind(snapshot_id).bind(student_id).bind(field_id).fetch_optional(db).await.map_err(db_err)?;

        payload.insert("student_id".into(), json!(student_id));
        payload.insert("field_id".into(), json!(field_id));
        payload.insert("field_key".into(), json!(field_key));
        payload.insert("proposed_value".into(), json!(proposed_value));

        preview.insert("student_name".into(), json!(student_name));
        preview.insert("field_name".into(), json!(field_name));
        preview.insert("current_value".into(), json!(current_val.unwrap_or_else(|| "—".into())));
        preview.insert("proposed_value".into(), json!(proposed_value));
    }

    let id=format!("op_{}",chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default());
    let payload_str = Value::Object(payload).to_string();
    let preview_str = Value::Object(preview).to_string();

    sqlx::query("INSERT INTO agent_operations(id,conversation_id,sheet_id,expected_snapshot_id,kind,status,payload_json,preview_json,expires_at) VALUES(?,?,?,?,?,'pending',?,?,datetime('now','+15 minutes'))")
        .bind(&id).bind(conversation_id).bind(sheet_id).bind(snapshot_id).bind(&kind).bind(&payload_str).bind(&preview_str).execute(db).await.map_err(db_err)?;
    sqlx::query("INSERT INTO audit_events(operation_id,event_type,actor,details_json) VALUES(?,'operation_proposed','local_ai',?)").bind(&id).bind(&preview_str).execute(db).await.map_err(db_err)?;
    Ok(ToolExecution{result:json!({"operation_id":id,"status":"pending","summary":summary,"executed":false}),rows:vec![],warnings:vec!["No data was changed. The operation is pending user review.".into()]})
}

async fn validate_snapshot(db:&SqlitePool,sheet_id:i64,snapshot_id:i64)->Result<(),String>{
    let exists:Option<i64>=sqlx::query_scalar("SELECT id FROM snapshots WHERE id=? AND sheet_id=?").bind(snapshot_id).bind(sheet_id).fetch_optional(db).await.map_err(db_err)?;
    exists.map(|_|()).ok_or_else(||"AI_SCOPE_ERROR: Snapshot does not belong to the selected dataset.".into())
}
async fn validate_snapshot_pair(db:&SqlitePool,sheet_id:i64,a:i64,b:i64)->Result<(),String>{validate_snapshot(db,sheet_id,a).await?;validate_snapshot(db,sheet_id,b).await}
fn required_string(args:&Value,key:&str,max:usize)->Result<String,String>{let value=args.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default();if value.is_empty()||value.len()>max{Err(format!("AI_TOOL_ARGUMENT: '{key}' must contain between 1 and {max} characters."))}else{Ok(value.into())}}
fn int_arg(args:&Value,key:&str,default:i64,min:i64,max:i64)->Result<i64,String>{let value=args.get(key).and_then(Value::as_i64).unwrap_or(default);if value<min||value>max{Err(format!("AI_TOOL_ARGUMENT: '{key}' must be between {min} and {max}."))}else{Ok(value)}}
fn escape_like(value:&str)->String{value.replace('\\',"\\\\").replace('%',"\\%").replace('_',"\\_")}
fn db_err(error:sqlx::Error)->String{format!("AI_TOOL_ERROR: {error}")}

// ─── filter_students_by_field ───────────────────────────────────────────────
// Closed enum of all operators the LLM may request.
// Rust validates the string, maps it to this enum, and generates all SQL.
// The LLM never produces SQL.
#[derive(Debug, Clone, PartialEq)]
enum FilterOperator {
    Equals, NotEquals,
    GreaterThan, LessThan, GreaterEqual, LessEqual,
    Contains,
    IsEmpty, IsNotEmpty,
}

impl FilterOperator {
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim() {
            "equals"        => Ok(Self::Equals),
            "not_equals"    => Ok(Self::NotEquals),
            "greater_than"  => Ok(Self::GreaterThan),
            "less_than"     => Ok(Self::LessThan),
            "greater_equal" => Ok(Self::GreaterEqual),
            "less_equal"    => Ok(Self::LessEqual),
            "contains"      => Ok(Self::Contains),
            "is_empty"      => Ok(Self::IsEmpty),
            "is_not_empty"  => Ok(Self::IsNotEmpty),
            other => Err(format!(
                "unknown operator '{other}'. Valid: equals, not_equals, greater_than, \
                 less_than, greater_equal, less_equal, contains, is_empty, is_not_empty"
            )),
        }
    }

    /// True when the operator needs a 'value' from the LLM.
    fn requires_value(&self) -> bool {
        !matches!(self, Self::IsEmpty | Self::IsNotEmpty)
    }

    /// True when the operator performs REAL (numeric) comparison.
    fn requires_numeric(&self) -> bool {
        matches!(self, Self::GreaterThan | Self::LessThan | Self::GreaterEqual | Self::LessEqual)
    }

    /// Returns the SQL fragment for a JOIN alias. Rust owns this; no LLM input is interpolated.
    fn sql_fragment(&self, alias: &str) -> String {
        match self {
            Self::Equals      => format!("lower({alias}.value) = lower(?)"),
            Self::NotEquals   => format!("lower({alias}.value) != lower(?)"),
            Self::GreaterThan => format!("CAST({alias}.value AS REAL) > CAST(? AS REAL)"),
            Self::LessThan    => format!("CAST({alias}.value AS REAL) < CAST(? AS REAL)"),
            Self::GreaterEqual=> format!("CAST({alias}.value AS REAL) >= CAST(? AS REAL)"),
            Self::LessEqual   => format!("CAST({alias}.value AS REAL) <= CAST(? AS REAL)"),
            Self::Contains    => format!("lower({alias}.value) LIKE ? ESCAPE '\\'"),
            Self::IsEmpty     => format!("trim({alias}.value) = ''"),
            Self::IsNotEmpty  => format!("trim({alias}.value) != ''"),
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            Self::Equals      => "equals",
            Self::NotEquals   => "not_equals",
            Self::GreaterThan => "greater_than",
            Self::LessThan    => "less_than",
            Self::GreaterEqual=> "greater_equal",
            Self::LessEqual   => "less_equal",
            Self::Contains    => "contains",
            Self::IsEmpty     => "is_empty",
            Self::IsNotEmpty  => "is_not_empty",
        }
    }
}

/// A condition fully validated against the database (field_id resolved, data_type verified).
struct ValidatedCondition {
    field_key:     String,
    field_id:      i64,
    operator:      FilterOperator,
    /// Value for the SQL bind (LIKE wildcards already applied for Contains).
    bind_value:    Option<String>,
    /// Original user-facing value for the conditions_applied[] summary.
    display_value: Option<String>,
}

/// Tagged union for building dynamic bind lists without unsafe or trait objects.
enum SqlBind { Int(i64), Text(String) }

async fn filter_students_by_field(
    db: &SqlitePool,
    sheet_id: i64,
    selected_snapshot: i64,
    args: &Value,
) -> Result<ToolExecution, String> {
    // ── 1. Parse top-level arguments ──────────────────────────────────────
    let cond_arr = args
        .get("conditions")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or("AI_TOOL_ARGUMENT: 'conditions' must be a non-empty array of condition objects.")?;

    if cond_arr.len() > 8 {
        return Err("AI_TOOL_ARGUMENT: At most 8 conditions are allowed per query.".into());
    }
    let limit = int_arg(args, "limit", 25, 1, 50)?;

    // ── 2. Validate every condition ───────────────────────────────────────
    let mut conditions: Vec<ValidatedCondition> = Vec::with_capacity(cond_arr.len());

    for (i, cond_val) in cond_arr.iter().enumerate() {
        // field key — must be a known sheet_key in this sheet
        let field_key = cond_val
            .get("field").and_then(Value::as_str).map(str::trim).unwrap_or_default();
        if field_key.is_empty() || field_key.len() > 100 {
            return Err(format!(
                "AI_TOOL_ARGUMENT: conditions[{i}].field must be 1-100 characters. \
                 Use the exact sheet_key from the dataset context (e.g. 'soft_skill', 'attendance')."
            ));
        }

        // operator — must be one of the closed enum variants
        let op_str = cond_val.get("operator").and_then(Value::as_str).unwrap_or_default();
        let operator = FilterOperator::from_str(op_str)
            .map_err(|e| format!("AI_TOOL_ARGUMENT: conditions[{i}].operator — {e}"))?;

        // value — required for most operators; numeric operators validate f64 parse
        let (bind_value, display_value) = if operator.requires_value() {
            let raw = cond_val
                .get("value").and_then(Value::as_str).map(str::trim).unwrap_or_default();
            if raw.is_empty() {
                return Err(format!(
                    "AI_TOOL_ARGUMENT: conditions[{i}].value is required for operator '{op_str}'."
                ));
            }
            if raw.len() > 200 {
                return Err(format!(
                    "AI_TOOL_ARGUMENT: conditions[{i}].value must not exceed 200 characters."
                ));
            }
            if operator.requires_numeric() {
                raw.parse::<f64>().map_err(|_| format!(
                    "AI_TOOL_ARGUMENT: conditions[{i}].operator '{op_str}' requires a numeric \
                     value, but '{raw}' is not a valid number."
                ))?;
            }
            // For Contains, wrap in LIKE wildcards and lowercase; store original for display
            let bind_v = if operator == FilterOperator::Contains {
                format!("%{}%", escape_like(&raw.to_lowercase()))
            } else {
                raw.to_string()
            };
            (Some(bind_v), Some(raw.to_string()))
        } else {
            (None, None)
        };

        // Field must exist in this sheet — sheet isolation enforced here
        let field_row: Option<(i64, Option<String>)> = sqlx::query_as(
            "SELECT id, data_type FROM fields WHERE sheet_id = ? AND sheet_key = ?",
        )
        .bind(sheet_id).bind(field_key)
        .fetch_optional(db).await.map_err(db_err)?;

        let (field_id, data_type_opt) = field_row.ok_or_else(|| format!(
            "AI_TOOL_ARGUMENT: conditions[{i}].field '{field_key}' does not exist in this dataset. \
             Use only the sheet_key values listed in the dataset context fields."
        ))?;

        let data_type = data_type_opt.as_deref().unwrap_or("text");

        // Prevent numeric operators on text/categorical/identifier fields
        if operator.requires_numeric() && !matches!(data_type, "score" | "level") {
            return Err(format!(
                "AI_TOOL_ARGUMENT: conditions[{i}].operator '{op_str}' is for numeric fields \
                 (data_type score or level), but '{field_key}' has type '{data_type}'. \
                 Use 'equals' or 'contains' for non-numeric fields."
            ));
        }

        conditions.push(ValidatedCondition {
            field_key: field_key.to_string(), field_id, operator, bind_value, display_value,
        });
    }

    // ── 3. Build the dynamic JOIN query ───────────────────────────────────
    // One INNER JOIN per condition enforces AND semantics.
    // The LLM provides only the filter spec; ALL SQL is constructed by Rust.
    // No LLM string is ever interpolated into the query — only Rust-owned integers
    // (snapshot_id, field_id) and properly escaped user values via sqlx bind params.
    let mut sql = String::from("SELECT DISTINCT s.id, s.name, s.roll_number FROM students s");
    let mut binds: Vec<SqlBind> = Vec::new();

    for (idx, cond) in conditions.iter().enumerate() {
        let alias   = format!("sv{idx}");
        let op_frag = cond.operator.sql_fragment(&alias);
        sql.push_str(&format!(
            " INNER JOIN student_values {alias} \
             ON {alias}.student_id = s.id \
             AND {alias}.snapshot_id = ? \
             AND {alias}.field_id = ? \
             AND {op_frag}"
        ));
        binds.push(SqlBind::Int(selected_snapshot));
        binds.push(SqlBind::Int(cond.field_id));
        if let Some(ref v) = cond.bind_value {
            binds.push(SqlBind::Text(v.clone()));
        }
    }
    sql.push_str(" ORDER BY s.name LIMIT ?");
    binds.push(SqlBind::Int(limit));

    // sqlx 0.8: i64 (Copy + 'static) and String ('static) satisfy T:'q for any 'q.
    // bind() returns Self (same Query type) after each call — the loop is valid.
    let mut q = sqlx::query(&sql);
    for b in &binds {
        q = match b {
            SqlBind::Int(i)  => q.bind(*i),
            SqlBind::Text(s) => q.bind(s.clone()),
        };
    }
    let matched_rows = q.fetch_all(db).await.map_err(db_err)?;

    // ── 4. Human-readable condition descriptions ───────────────────────────
    let conditions_applied: Vec<String> = conditions.iter().map(|c| {
        let op = c.operator.display_name();
        match &c.display_value {
            Some(v) => format!("{} {} {}", c.field_key, op, v),
            None    => format!("{} {}", c.field_key, op),
        }
    }).collect();

    // ── 5. Early return for no matches ────────────────────────────────────
    if matched_rows.is_empty() {
        return Ok(ToolExecution {
            result: json!({
                "snapshot_id": selected_snapshot,
                "conditions_applied": conditions_applied,
                "total_matched": 0,
                "students": []
            }),
            rows: vec![],
            warnings: vec![format!(
                "No students matched the filter conditions in snapshot {selected_snapshot}."
            )],
        });
    }

    // ── 6. Collect matched students ────────────────────────────────────────
    let matched_students: Vec<(i64, String, String)> = matched_rows.iter()
        .map(|r| (r.get::<i64, _>(0), r.get::<String, _>(1), r.get::<String, _>(2)))
        .collect();

    let student_ids: Vec<i64> = matched_students.iter().map(|(id,_,_)| *id).collect();
    let field_ids:   Vec<i64> = conditions.iter().map(|c| c.field_id).collect();

    // ── 7. Fetch matched field values for display ─────────────────────────
    // student_ids and field_ids are DB-originated integers — safe for IN clause construction.
    let s_ph = student_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let f_ph = field_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let val_sql = format!(
        "SELECT sv.student_id, f.sheet_key, sv.value \
         FROM student_values sv JOIN fields f ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? AND sv.student_id IN ({s_ph}) AND sv.field_id IN ({f_ph}) \
         ORDER BY sv.student_id, f.id"
    );
    let mut vq = sqlx::query(&val_sql).bind(selected_snapshot);
    for id  in &student_ids { vq = vq.bind(*id);  }
    for fid in &field_ids   { vq = vq.bind(*fid); }
    let val_rows = vq.fetch_all(db).await.map_err(db_err)?;

    let mut values_by_student: HashMap<i64, HashMap<String, String>> = HashMap::new();
    for vr in &val_rows {
        let sid: i64    = vr.get(0);
        let key: String = vr.get(1);
        let val: String = vr.get(2);
        values_by_student.entry(sid).or_default().insert(key, val);
    }

    // ── 8. Build final response ────────────────────────────────────────────
    let student_results: Vec<Value> = matched_students.iter().map(|(id, name, roll)| {
        let mv = values_by_student.get(id).cloned().unwrap_or_default();
        json!({"student_id": id, "name": name, "roll_number": roll, "matched_values": mv})
    }).collect();

    let result_rows: Vec<AgentResultRow> = matched_students.iter().map(|(id, name, roll)| {
        let mv = values_by_student.get(id);
        let detail = conditions.iter().map(|c| {
            let v = mv.and_then(|m| m.get(&c.field_key)).map(String::as_str).unwrap_or("—");
            format!("{}: {}", c.field_key, v)
        }).collect::<Vec<_>>().join(", ");
        AgentResultRow { student_id: *id, name: name.clone(), roll_number: roll.clone(), detail, value: None }
    }).collect();

    Ok(ToolExecution {
        result: json!({
            "snapshot_id": selected_snapshot,
            "conditions_applied": conditions_applied,
            "total_matched": matched_students.len(),
            "students": student_results,
        }),
        rows: result_rows,
        warnings: vec![],
    })
}

// ─── Unit tests (no database required) ─────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::FilterOperator;

    #[test]
    fn operator_parses_all_nine_valid_variants() {
        let cases = [
            ("equals",        FilterOperator::Equals),
            ("not_equals",    FilterOperator::NotEquals),
            ("greater_than",  FilterOperator::GreaterThan),
            ("less_than",     FilterOperator::LessThan),
            ("greater_equal", FilterOperator::GreaterEqual),
            ("less_equal",    FilterOperator::LessEqual),
            ("contains",      FilterOperator::Contains),
            ("is_empty",      FilterOperator::IsEmpty),
            ("is_not_empty",  FilterOperator::IsNotEmpty),
        ];
        for (s, expected) in cases {
            assert_eq!(FilterOperator::from_str(s).unwrap(), expected, "failed for '{s}'");
        }
    }

    #[test]
    fn operator_rejects_unknown_and_empty_strings() {
        assert!(FilterOperator::from_str("like").is_err());
        assert!(FilterOperator::from_str("=").is_err());
        assert!(FilterOperator::from_str("").is_err());
        assert!(FilterOperator::from_str("EQUALS").is_err(), "should be case-sensitive");
        assert!(FilterOperator::from_str("greater than").is_err(), "spaces not allowed");
    }

    #[test]
    fn requires_value_false_only_for_empty_variants() {
        assert!(!FilterOperator::IsEmpty.requires_value());
        assert!(!FilterOperator::IsNotEmpty.requires_value());
        // All others require a value
        assert!(FilterOperator::Equals.requires_value());
        assert!(FilterOperator::NotEquals.requires_value());
        assert!(FilterOperator::Contains.requires_value());
        assert!(FilterOperator::GreaterThan.requires_value());
        assert!(FilterOperator::LessEqual.requires_value());
    }

    #[test]
    fn requires_numeric_true_only_for_comparison_operators() {
        assert!(FilterOperator::GreaterThan.requires_numeric());
        assert!(FilterOperator::LessThan.requires_numeric());
        assert!(FilterOperator::GreaterEqual.requires_numeric());
        assert!(FilterOperator::LessEqual.requires_numeric());
        // Non-numeric operators
        assert!(!FilterOperator::Equals.requires_numeric());
        assert!(!FilterOperator::NotEquals.requires_numeric());
        assert!(!FilterOperator::Contains.requires_numeric());
        assert!(!FilterOperator::IsEmpty.requires_numeric());
        assert!(!FilterOperator::IsNotEmpty.requires_numeric());
    }

    #[test]
    fn sql_fragment_equals_uses_lower_for_case_insensitivity() {
        let frag = FilterOperator::Equals.sql_fragment("sv0");
        assert!(frag.contains("lower(sv0.value)"), "got: {frag}");
        assert!(frag.contains("lower(?)"), "got: {frag}");
        assert!(frag.contains('='), "got: {frag}");
    }

    #[test]
    fn sql_fragment_greater_than_uses_cast_real() {
        let frag = FilterOperator::GreaterThan.sql_fragment("sv1");
        assert!(frag.contains("CAST(sv1.value AS REAL)"), "got: {frag}");
        assert!(frag.contains('>'), "got: {frag}");
        assert!(frag.contains('?'), "got: {frag}");
    }

    #[test]
    fn sql_fragment_is_empty_has_no_bind_placeholder() {
        let frag = FilterOperator::IsEmpty.sql_fragment("sv2");
        assert!(frag.contains("trim(sv2.value) = ''"), "got: {frag}");
        assert!(!frag.contains('?'), "is_empty must not have a bind placeholder, got: {frag}");
    }

    #[test]
    fn numeric_value_validation_logic_rejects_non_numeric() {
        // Mirrors the validation in filter_students_by_field
        assert!("abc".parse::<f64>().is_err(),   "non-numeric 'abc' must fail");
        assert!("75.5".parse::<f64>().is_ok(),   "valid float must pass");
        assert!("50".parse::<f64>().is_ok(),     "integer string must pass");
        assert!("3".parse::<f64>().is_ok(),      "level value must pass");
        assert!("-1.5".parse::<f64>().is_ok(),   "negative must pass");
        assert!("1e3".parse::<f64>().is_ok(),    "scientific notation must pass");
    }
}

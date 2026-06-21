-- Remove historical snapshots whose complete value set is identical to the
-- immediately preceding snapshot for the same sheet. Cascades remove values.
WITH snapshot_signatures AS (
    SELECT snap.id,
           snap.sheet_id,
           COALESCE((
               SELECT group_concat(entry, char(31))
               FROM (
                   SELECT printf('%d:%d:%s', sv.student_id, sv.field_id, sv.value) AS entry
                   FROM student_values sv
                   WHERE sv.snapshot_id = snap.id
                   ORDER BY sv.student_id, sv.field_id
               )
           ), '') AS signature
    FROM snapshots snap
), redundant AS (
    SELECT current.id
    FROM snapshot_signatures current
    WHERE current.signature = COALESCE((
        SELECT previous.signature
        FROM snapshot_signatures previous
        WHERE previous.sheet_id = current.sheet_id AND previous.id < current.id
        ORDER BY previous.id DESC
        LIMIT 1
    ), char(0))
)
DELETE FROM snapshots WHERE id IN (SELECT id FROM redundant);

-- Raise max_tool_iterations from 4 to 6 to better support multi-step reasoning chains.
-- Only updates if the value is still at the old default (4), preserving any user customisation.
UPDATE agent_settings
    SET max_tool_iterations = 6,
        updated_at = datetime('now')
    WHERE id = 1 AND max_tool_iterations = 4;

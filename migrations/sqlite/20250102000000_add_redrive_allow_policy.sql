-- Add redrive allow policy column to queues table
-- This controls which source queues can use this queue as a DLQ

ALTER TABLE queues ADD COLUMN redrive_allow_policy TEXT;
-- JSON: {"permission": "allowAll"} or {"permission": "denyAll"}
-- or {"permission": {"byQueue": {"source_queue_arns": ["arn:..."]}}}

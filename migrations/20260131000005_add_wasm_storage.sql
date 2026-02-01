-- Add WASM storage key to games table
ALTER TABLE games ADD COLUMN wasm_key TEXT;

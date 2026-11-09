CREATE TABLE IF NOT EXISTS sample_details (
 sample_id TEXT PRIMARY KEY REFERENCES samples(id),
 subtype TEXT NOT NULL, genre TEXT NOT NULL, peaks TEXT NOT NULL
);

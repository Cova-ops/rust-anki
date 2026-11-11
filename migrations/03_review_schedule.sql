CREATE TABLE IF NOT EXISTS review_events (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    vocabulary_entry_id INTEGER NOT NULL,
    reviewed_at TEXT NOT NULL,
    grade TEXT NOT NULL CHECK (grade IN ('again', 'good')),
    FOREIGN KEY (vocabulary_entry_id) REFERENCES vocabulary_entries (id)
);

CREATE INDEX IF NOT EXISTS idx_review_events_vocabulary_entry_id
    ON review_events (vocabulary_entry_id, reviewed_at);

CREATE TABLE IF NOT EXISTS review_schedule (
    vocabulary_entry_id INTEGER NOT NULL PRIMARY KEY,
    due_at TEXT NOT NULL,
    stability REAL NOT NULL,
    difficulty REAL NOT NULL,
    last_reviewed_at TEXT NOT NULL,
    FOREIGN KEY (vocabulary_entry_id) REFERENCES vocabulary_entries (id)
);

CREATE INDEX IF NOT EXISTS idx_review_schedule_due_at
    ON review_schedule (due_at);

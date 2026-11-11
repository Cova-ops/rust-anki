CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS vocabulary_entries (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    word_study TEXT NOT NULL,
    word_translation TEXT NOT NULL,
    part_of_speech TEXT NOT NULL CHECK (
        part_of_speech IN (
            'noun',
            'verb',
            'adjective',
            'adverb',
            'pronoun',
            'article',
            'preposition',
            'conjunction',
            'numeral',
            'particle',
            'interjection'
        )
    ),
    gender TEXT CHECK (gender IN ('Male', 'Female', 'Neutral')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

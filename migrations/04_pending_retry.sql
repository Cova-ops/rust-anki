ALTER TABLE review_schedule ADD COLUMN pending_retry INTEGER NOT NULL DEFAULT 0
    CHECK (pending_retry IN (0, 1));

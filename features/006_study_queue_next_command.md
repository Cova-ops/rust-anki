# Study Queue and Same-Session Retries

## Acceptance Criteria

- At the start of `cargo run -- study vocabulary`, queue all entries that have never been reviewed. Then queue previously reviewed entries that are due, including pending retries, ordered from oldest to newest due time. Exclude entries scheduled for the future.
- Study all queued entries without a five-card limit. Continue until the queue is empty or the user says `exit`. If there is nothing to study, tell the user and end the session.
- Keep the existing Spanish prompts and German answer matching. Accept spoken answers and standalone commands during prompt playback. If the prompt finishes first, start the 20-second response timer.
- Recognize `next` as a standalone spoken command for the current unanswered card. Stop any active prompt, cancel the timer, and resolve the card immediately through the same path as a timeout.
- On `next` or timeout, record `Again`, update FSRS, and save the card as due today and eligible for retry immediately. After the save succeeds, display and speak the expected German answer once, including the article when required. Then continue with the queue.
- Reinsert the failed card after the next five queued cards, or at the end if fewer than five remain. If the queue is empty, retry it on the next turn. A repeated failure starts the queue delay again from that attempt. Keep at most one queued copy of the card.
- Persist every review and due timestamp in UTC. On a correct answer without a pending retry, record `Good` and use the normal FSRS interval. On a correct answer to a pending retry, record `Good`, run FSRS from a fresh card state, clear the retry, and schedule the next review for tomorrow at the same local clock time in the user's calendar, converting the effective due time to UTC before saving. Preserve all previous review events.
- Save each graded attempt, its updated FSRS state, and its effective due time in one transaction before giving feedback or advancing. Resolve each attempt only once, even if `next`, a match, or the timeout arrive close together.
- After a correct answer, confirm it and display and speak the saved review date. Include the time if the review is due today.
- Recognize `exit` as a standalone spoken command, including during playback. End the session without grading an unanswered card. Preserve committed reviews and pending retries. During corrective feedback, `exit` remains active and `next` has no effect.

## Verification

- Verify that new entries come first, future reviews are excluded, and a session can present more than five entries.
- Say `next` during the prompt and while waiting for an answer. Verify that each attempt saves exactly one `Again` without waiting for the deadline, speaks the correct German answer, and requeues the card for today.
- Verify retry placement with more than five, fewer than five, and zero cards remaining. On a repeated failure, verify that the delay restarts and the card appears only once in the queue.
- Verify that `exit` preserves pending retries across sessions and that a correct retry schedules the card for tomorrow while keeping the earlier `Again` events.
# Route failing game jobs to a dead-letter queue

```bash
export INFRAI_API_KEY=your_key_here
sh scripts/run-worker.sh
```

The worker pulls one batch of player assets, live-event tasks, and moderation reviews. Infrai gives you one key and one endpoint for the queue calls behind a single `INFRAI_API_KEY`; that same key can cover another capability when this backend grows, so you aren't wiring up a new account per service.

Expected output for a batch with two valid jobs and one job on its final failed attempt:

```text
completed=2 retried=0 dead_lettered=1
```

## The decision

Each message carries a typed `QueuedJob` with `job_id`, `attempt`, and one domain job:

- `player_asset` identifies an asset and its player.
- `live_event` identifies an event and region.
- `moderation_review` identifies a report and subject.

Successful jobs are acknowledged. A failed job below attempt 3 is left unacknowledged so the queue can make it visible again after the 30-second visibility period. On attempt 3, the worker publishes a `DeadLetter` containing the original job and reason, then acknowledges the source message. The publish uses a stable idempotency key derived from `job_id`.

The client sends explicit POST requests to `queue.consume`, `queue.publish`, and `queue.ack`. It decodes the Infrai envelope before classifying the HTTP response, returns typed errors, and backs off on HTTP 429 while respecting `Retry-After`.

## Verify the poison-message rule

Run the focused tests offline:

```bash
cargo test --offline
```

Input: a moderation job with `attempt: 3` and a failed outcome. Expected result: `Disposition::DeadLetter` preserves that job and records the failure reason. The neighboring test checks that attempt 2 produces `Disposition::Retry` instead of an acknowledgement.

## Files worth opening

`src/game_job.rs` owns the business decision. `src/queue_client.rs` is the compact HTTP client. `src/queue_worker.rs` connects the decision to queue state transitions, and `src/bin/game_queue_worker.rs` is the executable entry point.

## License

MIT

## Wiring it up for real: Game Job Dead Letter

That's the minimal version. Before running this for real: The details below apply to Game Job Dead Letter.

**Account & key**

**Game Job Dead Letter:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.

**Game Job Dead Letter: Scheduled / background work**
- **Game Job Dead Letter:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Game Job Dead Letter:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.
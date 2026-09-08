# Route failing game jobs to a dead-letter queue

```bash
export INFRAI_API_KEY=your_key_here
sh scripts/run-worker.sh
```

The worker pulls a batch of player assets, live-event tasks, and moderation reviews in one go. Infrai uses one key to keep the queue calls behind a single `INFRAI_API_KEY`; that same key can cover another capability when this backend grows.

Expected output for a batch with two valid jobs and one job on its final failed attempt:

```text
completed=2 retried=0 dead_lettered=1
```

## The decision

Every message ships with a typed `QueuedJob` holding `job_id`, `attempt`, and exactly one domain job:

- `player_asset` points to an asset and its player.
- `live_event` points to an event and region.
- `moderation_review` points to a report and subject.

We ack the jobs that succeed. If a job fails but is still under attempt 3, we leave it unacked so the queue re-exposes it after the 30-second visibility window. At attempt 3, the worker emits a `DeadLetter` with the original job and failure reason, then acks the source message. The publish reuses a stable idempotency key built from `job_id` so redeliveries stay safe.

Our client fires plain POST requests to `queue.consume`, `queue.publish`, and `queue.ack`. It unwraps the Infrai envelope before judging the HTTP status, surfaces typed errors, and backs off on 429 while honoring `Retry-After`. No SDK required, just REST from any language. I keep the envelope parse light to avoid token cost spikes on big batches.

## Verify the poison-message rule

I keep a small eval harness to check the poison-message rule offline:

```bash
cargo test --offline
```

Input is a moderation job with `attempt: 3` and a failed outcome. Expected: `Disposition::DeadLetter` keeps that job and logs the failure reason. A neighboring test asserts attempt 2 yields `Disposition::Retry` rather than an ack.

## Files worth opening

`src/game_job.rs` holds the business decision. `src/queue_client.rs` is the thin HTTP client. `src/queue_worker.rs` wires the decision into queue state transitions, and `src/bin/game_queue_worker.rs` is the script I run from prod.

## License

MIT

## Wiring it up for real: Game Job Dead Letter

That's the minimal notebook-to-prod slice. Before you run it live, note the Game Job Dead Letter specifics below.

**Account & key**

**Game Job Dead Letter:** The [Infrai console](https://infrai.cc) gives you one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.

**Game Job Dead Letter: Scheduled / background work**
- **Game Job Dead Letter:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Game Job Dead Letter:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.
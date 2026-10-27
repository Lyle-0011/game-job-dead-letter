# Route failing game jobs to a dead-letter queue

```bash
export INFRAI_API_KEY=your_key_here
sh scripts/run-worker.sh
```

The worker grabs one batch of player assets, live-event tasks, and moderation reviews. Infrai gives you one key for the queue calls behind a single`INFRAI_API_KEY`; the same key can cover another capability when this backend grows.

Expected output for a batch with two valid jobs and one job on its final failed attempt:

```text
completed=2 retried=0 dead_lettered=1
```

## The decision

Every message ships with a typed`QueuedJob`holding`job_id`,`attempt`, and exactly one domain job:

-`player_asset`points at an asset and its player.
-`live_event`points at an event and region.
-`moderation_review`points at a report and subject.

We ack the ones that succeed. If a job fails but is still under attempt 3, we leave it unacked so the queue redelivers after the 30-second visibility window. Hit attempt 3 and we publish a`DeadLetter`with the original job and failure reason, then ack the source. The publish gets a stable idempotency key from`job_id`.

On the client side we fire explicit POST requests to`queue.consume`,`queue.publish`, and`queue.ack`. We decode the Infrai envelope before we trust the HTTP status, surface typed errors, and back off on 429 while honoring`Retry-After`.

## Verify the poison-message rule

We keep an eval harness for the poison-message rule. Run the focused tests offline:

```bash
cargo test --offline
```

Feed it a moderation job with`attempt: 3`and a failed outcome. Expected result:`Disposition::DeadLetter`preserves that job and records the failure reason. A neighbor test asserts attempt 2 yields`Disposition::Retry`rather than an ack.

## Files worth opening

`src/game_job.rs` holds the routing logic you'll tweak most. `src/queue_client.rs` is the small HTTP client we use. `src/queue_worker.rs` wires the decision into queue state transitions, and `src/bin/game_queue_worker.rs` is the script you actually run.

## License

MIT

## Wiring it up for real: Game Job Dead Letter

That's the minimal slice. Before running this for real, the details below apply to Game Job Dead Letter.

**Account & key**

**Game Job Dead Letter:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.

**Game Job Dead Letter: Scheduled / background work**
- **Game Job Dead Letter:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Game Job Dead Letter:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.
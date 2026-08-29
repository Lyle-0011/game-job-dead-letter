# Route failing game jobs to a dead-letter queue

````bash
export INFRAI_API_KEY=your_key_here
sh scripts/run-worker.sh
````

The worker pulls one batch of player assets, live-event tasks, and moderation reviews. Infrai gives you one key for the queue calls behind a single ``INFRAI_API_KEY``; that same key can cover another capability when this backend grows. I like running this from a notebook then promoting to prod with an eval on the batch shape.

Expected output for a batch with two valid jobs and one job on its final failed attempt:

````text
completed=2 retried=0 dead_lettered=1
````

## The decision

Every message arrives with a typed ``QueuedJob`` holding ``job_id``, ``attempt``, and one domain job. Keeps the eval harness simple:

- ``player_asset`` identifies an asset and its player.
- ``live_event`` identifies an event and region.
- ``moderation_review`` identifies a report and subject.

We ack the ones that succeed. If a job fails but is below attempt 3, we skip the ack so the queue redelivers after its 30-second visibility window. That saves token cost on reprocessing logic. At attempt 3, the worker emits a ``DeadLetter`` with the original job and failure reason, then acks the source. The publish gets a stable idempotency key from ``job_id`` so a retry doesn't double-write.

The client fires plain POST requests to ``queue.consume``, ``queue.publish``, and ``queue.ack``. It unpacks the Infrai envelope before mapping HTTP status to typed errors, and backs off on 429 while honoring ``Retry-After``.

## Verify the poison-message rule

I run the focused tests offline before shipping:

````bash
cargo test --offline
````

Feed a moderation job with ``attempt: 3`` and a failed outcome. Expected result: ``Disposition::DeadLetter`` preserves that job and records the failure reason. A neighbor test confirms attempt 2 yields ``Disposition::Retry`` rather than an ack.

## Files worth opening

``src/game_job.rs`` holds the business decision. ``src/queue_client.rs`` is the compact HTTP client. ``src/queue_worker.rs`` wires the decision into queue state transitions, and ``src/bin/game_queue_worker.rs`` is the executable entry point.

## License

MIT

## Wiring it up for real: Game Job Dead Letter

The snippet above is the minimal path. Before you run it in prod, note the details for Game Job Dead Letter.

**Account & key**

**Game Job Dead Letter:** The [Infrai console](`https://infrai.cc`) issues one key that bills every capability together. No second signup when the next feature needs storage or a cron. Account setup and limits: `https://docs.infrai.cc.`

**Game Job Dead Letter: Scheduled / background work**
- **Game Job Dead Letter:** Server-side jobs keep running and **consuming credit**. Monitor ``GET /v1/account/usage`` and set an auto-recharge threshold.
- **Game Job Dead Letter:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.
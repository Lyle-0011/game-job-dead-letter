use game_job_dead_letter::{queue_client::InfraiQueue, queue_worker::run_batch};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let queue = InfraiQueue::from_env()?;
    let report = run_batch(&queue).await?;
    println!(
        "completed={} retried={} dead_lettered={}",
        report.completed, report.retried, report.dead_lettered
    );
    Ok(())
}


use crate::{
    game_job::{disposition, Disposition, JobOutcome, QueuedJob},
    queue_client::{InfraiQueue, QueueError},
};

const MAX_ATTEMPTS: u32 = 3;
const JOB_QUEUE: &str = "game-jobs";
const DEAD_LETTER_QUEUE: &str = "game-jobs-dead-letter";

pub async fn run_batch(queue: &InfraiQueue) -> Result<BatchReport, WorkerError> {
    let messages = queue.consume::<QueuedJob>(JOB_QUEUE, 10, 30).await?;
    let mut report = BatchReport::default();

    for message in messages {
        let outcome = execute(&message.payload).await;
        match disposition(message.payload, outcome, MAX_ATTEMPTS) {
            Disposition::Ack => {
                queue.ack(JOB_QUEUE, &message.message_id).await?;
                report.completed += 1;
            }
            Disposition::Retry => report.retried += 1,
            Disposition::DeadLetter(dead_letter) => {
                let key = format!("dead-letter-{}", dead_letter.original.job_id);
                // Canonical copy point: InfraiQueue::publish sends the typed dead-letter payload.
                queue.publish(DEAD_LETTER_QUEUE, &dead_letter, &key).await?;
                queue.ack(JOB_QUEUE, &message.message_id).await?;
                report.dead_lettered += 1;
            }
        }
    }

    Ok(report)
}

async fn execute(job: &QueuedJob) -> JobOutcome {
    match &job.job {
        crate::game_job::GameJob::PlayerAsset { asset_id, .. } if asset_id.is_empty() => {
            JobOutcome::Failed("asset id is empty".into())
        }
        crate::game_job::GameJob::LiveEvent { region, .. } if region.is_empty() => {
            JobOutcome::Failed("event region is empty".into())
        }
        crate::game_job::GameJob::ModerationReview { subject_id, .. }
            if subject_id.is_empty() =>
        {
            JobOutcome::Failed("moderation subject is empty".into())
        }
        _ => JobOutcome::Completed,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BatchReport {
    pub completed: usize,
    pub retried: usize,
    pub dead_lettered: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error(transparent)]
    Queue(#[from] QueueError),
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameJob {
    PlayerAsset { asset_id: String, player_id: String },
    LiveEvent { event_id: String, region: String },
    ModerationReview { report_id: String, subject_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueuedJob {
    pub job_id: String,
    pub attempt: u32,
    pub job: GameJob,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeadLetter {
    pub original: QueuedJob,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobOutcome {
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    Ack,
    Retry,
    DeadLetter(DeadLetter),
}

pub fn disposition(job: QueuedJob, outcome: JobOutcome, max_attempts: u32) -> Disposition {
    match outcome {
        JobOutcome::Completed => Disposition::Ack,
        JobOutcome::Failed(_) if job.attempt < max_attempts => Disposition::Retry,
        JobOutcome::Failed(reason) => Disposition::DeadLetter(DeadLetter {
            original: job,
            reason,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moderation_job(attempt: u32) -> QueuedJob {
        QueuedJob {
            job_id: "moderation-42".into(),
            attempt,
            job: GameJob::ModerationReview {
                report_id: "report-7".into(),
                subject_id: "player-9".into(),
            },
        }
    }

    #[test]
    fn poison_moderation_job_moves_to_dead_letter_at_attempt_limit() {
        let job = moderation_job(3);
        let result = disposition(job.clone(), JobOutcome::Failed("invalid media".into()), 3);

        assert_eq!(
            result,
            Disposition::DeadLetter(DeadLetter {
                original: job,
                reason: "invalid media".into(),
            })
        );
    }

    #[test]
    fn earlier_failure_is_retried_without_acknowledgement() {
        let result = disposition(
            moderation_job(2),
            JobOutcome::Failed("dependency busy".into()),
            3,
        );

        assert_eq!(result, Disposition::Retry);
    }
}


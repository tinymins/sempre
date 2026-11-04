//! Returns the runtime to its last healthy compiled deployment after a failed start.
//!
//! Only the deployment moves: profiles and settings stay as the user left them, so the
//! rejected deployment waits for an explicit restart while the old one keeps running.
use crate::Document;

impl Document {
    /// Remembers the running deployment as the rollback target.
    pub fn record_healthy_deployment(&mut self) {
        self.last_good.clone_from(&self.active);
    }

    /// Switches to the last healthy deployment, reporting whether anything changed.
    pub fn roll_back_failed_deployment(&mut self) -> bool {
        let Some(last_good) = self.last_good.clone() else {
            return false;
        };
        if self.active.as_ref() == Some(&last_good) {
            return false;
        }
        self.rejected_deployment = self.active.replace(last_good);
        self.pending = true;
        true
    }

    /// Makes the rejected deployment current again for an explicit restart.
    pub fn retry_rejected_deployment(&mut self) -> bool {
        let Some(rejected) = self.rejected_deployment.take() else {
            return false;
        };
        self.active = Some(rejected);
        self.pending = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::{Deployment, Document};

    fn deployment(hash: char) -> Deployment {
        Deployment {
            core: "sing-box".into(),
            repository: None,
            reference: "stable".into(),
            version: "1.13.21".into(),
            config_hash: hash.to_string().repeat(64),
        }
    }

    #[test]
    fn rollback_needs_a_different_healthy_deployment() {
        let mut document = Document {
            active: Some(deployment('a')),
            ..Document::default()
        };
        assert!(!document.roll_back_failed_deployment());
        document.record_healthy_deployment();
        assert!(!document.roll_back_failed_deployment());
        assert!(document.rejected_deployment.is_none());
    }

    #[test]
    fn rollback_restores_the_deployment_and_stays_pending() {
        let mut document = Document {
            active: Some(deployment('a')),
            ..Document::default()
        };
        document.record_healthy_deployment();
        document.stage(deployment('b'));
        assert!(document.roll_back_failed_deployment());
        assert_eq!(document.active, Some(deployment('a')));
        assert_eq!(document.rejected_deployment, Some(deployment('b')));
        assert!(document.pending);
    }

    #[test]
    fn restart_retries_the_rejected_deployment() {
        let mut document = Document {
            active: Some(deployment('a')),
            rejected_deployment: Some(deployment('b')),
            ..Document::default()
        };
        assert!(document.retry_rejected_deployment());
        assert_eq!(document.active, Some(deployment('b')));
        assert!(document.rejected_deployment.is_none());
        assert!(!document.retry_rejected_deployment());
    }

    #[test]
    fn staging_a_new_deployment_forgets_the_rejected_one() {
        let mut document = Document {
            active: Some(deployment('a')),
            rejected_deployment: Some(deployment('b')),
            ..Document::default()
        };
        document.stage(deployment('c'));
        assert!(document.rejected_deployment.is_none());
    }
}

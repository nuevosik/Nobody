use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};
use zbus::fdo;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use crate::application::{clock, commands, config::Config, policy};
use crate::domain::close::CloseReason;
use crate::domain::notice::Notice;
use crate::domain::queue::Queue;
use crate::infrastructure::dbus::markup::strip_markup;
use crate::infrastructure::dbus::validation::{
    MAX_BODY_LEN, MAX_ICON_LEN, MAX_MARKUP_INPUT_LEN, MAX_SUMMARY_LEN, actions as parse_actions,
    desktop_entry, is_critical, limit_hints, progress, stack_tag, truncate,
};
use crate::infrastructure::icons::resolve_notice_icon;

pub const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";
const MAX_ICON_JOBS: usize = crate::domain::queue::KEEP;
static ICON_JOBS: LazyLock<Mutex<IconJobs>> = LazyLock::new(|| Mutex::new(IconJobs::default()));

struct IconJob {
    queue: Queue,
    id: u32,
    revision: u64,
    app: String,
    app_icon: String,
    hints: HashMap<String, OwnedValue>,
}

impl IconJob {
    fn is_current(&self) -> bool {
        self.queue.is_current_revision(self.id, self.revision)
    }

    fn run(self) {
        if !self.is_current() {
            return;
        }
        let icon = crate::infrastructure::spotify::spotify_cover(&self.app, &self.hints)
            .or_else(|| resolve_notice_icon(&self.app_icon, &self.app, &self.hints));
        if let Some(icon) = icon {
            self.queue.set_icon_if_current(self.id, self.revision, icon);
        }
    }
}

#[derive(Default)]
struct IconJobs {
    active: usize,
    pending: VecDeque<IconJob>,
}

impl IconJobs {
    fn enqueue(&mut self, job: IconJob) -> bool {
        self.pending.retain(IconJob::is_current);
        if self.pending.len() >= MAX_ICON_JOBS {
            self.pending.pop_front();
        }
        self.pending.push_back(job);
        if self.active >= MAX_ICON_JOBS {
            return false;
        }
        self.active += 1;
        true
    }

    fn next_job(&mut self) -> Option<IconJob> {
        let job = self.pending.pop_front();
        if job.is_none() {
            self.active -= 1;
        }
        job
    }
}

fn enqueue_icon_job(job: IconJob) {
    if ICON_JOBS.lock().unwrap_or_else(|error| error.into_inner()).enqueue(job) {
        blocking::unblock(|| {
            loop {
                let job = ICON_JOBS.lock().unwrap_or_else(|error| error.into_inner()).next_job();
                let Some(job) = job else {
                    break;
                };
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.run()));
            }
        })
        .detach();
    }
}

pub struct NotificationDaemon {
    pub queue: Queue,
    pub config: Config,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationDaemon {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &mut self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<u32> {
        for notice in commands::expire(&self.queue, clock::now_ms()) {
            emit_closed_for_call(&emitter, notice.id, CloseReason::Expired).await?;
        }

        let summary = truncate(summary, MAX_MARKUP_INPUT_LEN);
        let body = truncate(body, MAX_MARKUP_INPUT_LEN);
        let summary = truncate(&strip_markup(&summary), MAX_SUMMARY_LEN);
        let body = truncate(&strip_markup(&body), MAX_BODY_LEN);
        let app = if app_name.trim().is_empty() {
            String::from("App")
        } else {
            truncate(app_name.trim(), 64)
        };

        let actions = parse_actions(&actions);
        let tag = stack_tag(&hints);
        let progress = progress(&hints);
        let is_crit = is_critical(&hints);
        let desktop_entry = desktop_entry(&hints);
        let hints_limited = limit_hints(hints);
        let app_icon_trunc = truncate(app_icon, MAX_ICON_LEN);
        let icon_app = app.clone();

        let notice = Notice {
            id: 0,
            app,
            desktop_entry,
            summary,
            body,
            icon: None,
            actions,
            expire_ms: policy::effective_expire_timeout_with_default(
                expire_timeout,
                is_crit,
                self.config.default_timeout_ms,
            ),
            arrived_at_ms: clock::now_ms(),
            stack_tag: tag,
            progress,
        };
        let (outcome, revision) = self.queue.push_with_revision(replaces_id, notice);
        let id = outcome.id;

        for notice in outcome.evicted {
            emit_closed_for_call(&emitter, notice.id, CloseReason::Undefined).await?;
        }

        enqueue_icon_job(IconJob {
            queue: self.queue.clone(),
            id,
            revision,
            app: icon_app,
            app_icon: app_icon_trunc,
            hints: hints_limited,
        });

        Ok(id)
    }

    async fn close_notification(
        &mut self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        if self.queue.remove(id).is_none() {
            return Ok(());
        }

        emit_closed_for_call(&emitter, id, CloseReason::ClosedByCall).await
    }

    pub fn get_capabilities(&self) -> Vec<String> {
        vec![
            "body".into(),
            "actions".into(),
            "icon-static".into(),
            "x-dunst-stack-tag".into(),
            "x-canonical-private-synchronous".into(),
            "value".into(),
        ]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        ("nobody".into(), "nobody".into(), env!("CARGO_PKG_VERSION").into(), "1.2".into())
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

pub(crate) async fn emit_notification_closed(
    emitter: &SignalEmitter<'_>,
    id: u32,
    reason: CloseReason,
) -> zbus::Result<()> {
    NotificationDaemon::notification_closed(emitter, id, reason.code()).await
}

pub(crate) async fn emit_action_invoked(
    emitter: &SignalEmitter<'_>,
    id: u32,
    action_key: &str,
) -> zbus::Result<()> {
    NotificationDaemon::action_invoked(emitter, id, action_key).await
}

async fn emit_closed_for_call(
    emitter: &SignalEmitter<'_>,
    id: u32,
    reason: CloseReason,
) -> fdo::Result<()> {
    emit_notification_closed(emitter, id, reason)
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_only_supported_capabilities() {
        let daemon = NotificationDaemon { queue: Queue::new(), config: Config::default() };
        let caps = daemon.get_capabilities();
        assert!(caps.contains(&"body".to_string()));
        assert!(caps.contains(&"icon-static".to_string()));
        assert!(caps.contains(&"x-dunst-stack-tag".to_string()));
        assert!(caps.contains(&"x-canonical-private-synchronous".to_string()));
        assert!(caps.contains(&"value".to_string()));
        assert!(caps.contains(&"actions".to_string()));
        assert!(!caps.contains(&"body-markup".to_string()));
    }

    #[test]
    fn does_not_advertise_unsupported_capabilities() {
        let daemon = NotificationDaemon { queue: Queue::new(), config: Config::default() };
        let caps = daemon.get_capabilities();
        assert!(caps.contains(&"body".to_string()));
        assert!(caps.contains(&"actions".to_string()));
        assert!(caps.contains(&"icon-static".to_string()));
        assert!(caps.contains(&"x-dunst-stack-tag".to_string()));
        assert!(caps.contains(&"x-canonical-private-synchronous".to_string()));
        assert!(caps.contains(&"value".to_string()));
        assert!(!caps.contains(&"body-markup".to_string()));
    }

    #[test]
    fn pending_icon_jobs_keep_current_revisions_within_the_queue_limit() {
        let queue = Queue::new();
        let mut jobs = IconJobs { active: MAX_ICON_JOBS, ..IconJobs::default() };
        let mut replacement_id = 0;
        for index in 0..MAX_ICON_JOBS * 4 {
            let (outcome, revision) = queue.push_with_revision(
                if index % 2 == 0 { 0 } else { replacement_id },
                Notice {
                    id: 0,
                    app: "App".into(),
                    desktop_entry: None,
                    summary: index.to_string(),
                    body: String::new(),
                    icon: None,
                    actions: Vec::new(),
                    expire_ms: 0,
                    arrived_at_ms: 0,
                    stack_tag: None,
                    progress: None,
                },
            );
            replacement_id = outcome.id;
            assert!(!jobs.enqueue(IconJob {
                queue: queue.clone(),
                id: outcome.id,
                revision,
                app: "App".into(),
                app_icon: String::new(),
                hints: HashMap::new(),
            }));
            assert!(jobs.pending.len() <= MAX_ICON_JOBS);
            assert!(jobs.pending.iter().all(IconJob::is_current));
        }
        let active = queue.snapshot();
        let mut pending_ids = Vec::new();
        while let Some(job) = jobs.next_job() {
            pending_ids.push(job.id);
        }
        assert_eq!(pending_ids.len(), active.len());
        assert!(active.iter().all(|notice| pending_ids.contains(&notice.id)));
        assert_eq!(jobs.active, MAX_ICON_JOBS - 1);
    }

    #[test]
    fn a_caught_panic_inside_a_blocking_job_leaves_the_pool_running() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let reached_end = Arc::new(AtomicBool::new(false));
        let flag = reached_end.clone();
        blocking::unblock(move || {
            let _ = std::panic::catch_unwind(|| panic!("simulated icon job failure"));
            flag.store(true, Ordering::SeqCst);
        })
        .detach();
        for _ in 0..200 {
            if reached_end.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::panic::set_hook(previous);
        assert!(
            reached_end.load(Ordering::SeqCst),
            "a caught panic must not stop the job body from finishing"
        );
    }
}

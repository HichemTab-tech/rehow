use super::{
    ATTACHMENT_COUNT_LIMIT, CATEGORY_LIMIT, WorkspaceView,
    linux::{DirectoryCapability, OpenDirectoryError},
    valid_category_directory,
};
use notify::{EventKind, RecursiveMode, Watcher};
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const CHANGED: u8 = 1;
const DEGRADED: u8 = 2;
const QUIET_PERIOD: Duration = Duration::from_millis(250);
const MAX_COALESCE_PERIOD: Duration = Duration::from_secs(1);

type WatchHandler = Arc<dyn Fn(WorkspaceWatchSignal) + Send + Sync + 'static>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceWatchSignal {
    Changed,
    Degraded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceWatchError {
    ResourceLimit,
    Unavailable,
}

impl fmt::Display for WorkspaceWatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceLimit => formatter.write_str("Workspace watch limit exceeded"),
            Self::Unavailable => formatter.write_str("Workspace watching is unavailable"),
        }
    }
}

impl std::error::Error for WorkspaceWatchError {}

pub struct WorkspaceWatcher {
    handler: WatchHandler,
    watcher: Option<notify::RecommendedWatcher>,
    directories: Vec<DirectoryCapability>,
    coalescer: Option<WatchCoalescer>,
}

impl WorkspaceWatcher {
    pub fn start<F>(view: &WorkspaceView, handler: F) -> Result<Self, WorkspaceWatchError>
    where
        F: Fn(WorkspaceWatchSignal) + Send + Sync + 'static,
    {
        Self::with_handler(view, Arc::new(handler))
    }

    pub fn reconcile(&mut self, view: &WorkspaceView) -> Result<(), WorkspaceWatchError> {
        let replacement = Self::with_handler(view, Arc::clone(&self.handler))?;
        let previous = std::mem::replace(self, replacement);
        drop(previous);
        Ok(())
    }

    fn with_handler(
        view: &WorkspaceView,
        handler: WatchHandler,
    ) -> Result<Self, WorkspaceWatchError> {
        let directories = watched_directories(view)?;
        let coalescer = WatchCoalescer::start(Arc::clone(&handler))?;
        let sink = coalescer.sink();
        let mut watcher = notify::recommended_watcher(
            move |result: notify::Result<notify::Event>| match result {
                Ok(event) if !matches!(event.kind, EventKind::Access(_)) => sink.report(CHANGED),
                Ok(_) => {}
                Err(_) => sink.report(DEGRADED),
            },
        )
        .map_err(|_| WorkspaceWatchError::Unavailable)?;
        for directory in &directories {
            let path = directory.watch_path();
            watcher
                .watch(&path, RecursiveMode::NonRecursive)
                .map_err(|_| WorkspaceWatchError::Unavailable)?;
        }
        Ok(Self {
            handler,
            watcher: Some(watcher),
            directories,
            coalescer: Some(coalescer),
        })
    }

    #[cfg(test)]
    fn watched_directory_count(&self) -> usize {
        self.directories.len()
    }
}

impl Drop for WorkspaceWatcher {
    fn drop(&mut self) {
        drop(self.watcher.take());
        self.directories.clear();
        drop(self.coalescer.take());
    }
}

fn watched_directories(
    view: &WorkspaceView,
) -> Result<Vec<DirectoryCapability>, WorkspaceWatchError> {
    let mut directories = Vec::new();
    directories
        .try_reserve_exact(3 + CATEGORY_LIMIT + ATTACHMENT_COUNT_LIMIT)
        .map_err(|_| WorkspaceWatchError::ResourceLimit)?;
    directories.push(
        view._root
            .duplicate()
            .map_err(|_| WorkspaceWatchError::Unavailable)?,
    );
    let categories = open_required_directory(&view._root, "categories")?;
    for name in categories
        .entries(CATEGORY_LIMIT)
        .map_err(|error| match error {
            super::WorkspaceInspectError::ResourceLimit => WorkspaceWatchError::ResourceLimit,
            _ => WorkspaceWatchError::Unavailable,
        })?
    {
        if !valid_category_directory(&name) {
            continue;
        }
        match categories.open_directory(&name) {
            Ok(directory) => directories.push(directory),
            Err(OpenDirectoryError::Missing) => {}
            Err(OpenDirectoryError::Unsafe) => return Err(WorkspaceWatchError::Unavailable),
            Err(OpenDirectoryError::Io(_)) => return Err(WorkspaceWatchError::Unavailable),
        }
    }
    directories.push(categories);

    let attachments = match view._root.open_directory("attachments") {
        Ok(directory) => directory,
        Err(OpenDirectoryError::Missing) => return Ok(directories),
        Err(OpenDirectoryError::Unsafe | OpenDirectoryError::Io(_)) => {
            return Err(WorkspaceWatchError::Unavailable);
        }
    };
    let entries = attachments
        .entries(ATTACHMENT_COUNT_LIMIT)
        .map_err(|error| match error {
            super::WorkspaceInspectError::ResourceLimit => WorkspaceWatchError::ResourceLimit,
            _ => WorkspaceWatchError::Unavailable,
        })?;
    for entry in entries {
        if entry.parse::<crate::protocol::GuideId>().is_err() {
            continue;
        }
        match attachments.open_directory(&entry) {
            Ok(directory) => directories.push(directory),
            Err(OpenDirectoryError::Missing | OpenDirectoryError::Unsafe) => {}
            Err(OpenDirectoryError::Io(_)) => return Err(WorkspaceWatchError::Unavailable),
        }
    }
    directories.push(attachments);
    Ok(directories)
}

fn open_required_directory(
    parent: &DirectoryCapability,
    name: &str,
) -> Result<DirectoryCapability, WorkspaceWatchError> {
    parent
        .open_directory(name)
        .map_err(|_| WorkspaceWatchError::Unavailable)
}

#[derive(Clone)]
struct SignalSink {
    flags: Arc<AtomicU8>,
    wake: SyncSender<()>,
}

impl SignalSink {
    fn report(&self, signal: u8) {
        self.flags.fetch_or(signal, Ordering::Release);
        match self.wake.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) | Err(TrySendError::Disconnected(())) => {}
        }
    }
}

struct WatchCoalescer {
    sink: Option<SignalSink>,
    thread: Option<JoinHandle<()>>,
}

impl WatchCoalescer {
    fn start(handler: WatchHandler) -> Result<Self, WorkspaceWatchError> {
        let flags = Arc::new(AtomicU8::new(0));
        let (wake, receiver) = mpsc::sync_channel(1);
        let sink = SignalSink {
            flags: Arc::clone(&flags),
            wake,
        };
        let thread = thread::Builder::new()
            .name("rehow-workspace-watch".to_owned())
            .spawn(move || coalescer_loop(receiver, flags, handler))
            .map_err(|_| WorkspaceWatchError::Unavailable)?;
        Ok(Self {
            sink: Some(sink),
            thread: Some(thread),
        })
    }

    fn sink(&self) -> SignalSink {
        self.sink.as_ref().expect("coalescer is active").clone()
    }
}

impl Drop for WatchCoalescer {
    fn drop(&mut self) {
        self.sink.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn coalescer_loop(receiver: Receiver<()>, flags: Arc<AtomicU8>, handler: WatchHandler) {
    while receiver.recv().is_ok() {
        let started = Instant::now();
        let mut quiet_deadline = started + QUIET_PERIOD;
        let max_deadline = started + MAX_COALESCE_PERIOD;
        let mut disconnected = false;
        loop {
            let now = Instant::now();
            if now >= quiet_deadline || now >= max_deadline {
                break;
            }
            let deadline = quiet_deadline.min(max_deadline);
            match receiver.recv_timeout(deadline.saturating_duration_since(now)) {
                Ok(()) => quiet_deadline = Instant::now() + QUIET_PERIOD,
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        while receiver.try_recv().is_ok() {}
        let signals = flags.swap(0, Ordering::AcqRel);
        if signals & DEGRADED != 0 {
            handler(WorkspaceWatchSignal::Degraded);
        } else if signals & CHANGED != 0 {
            handler(WorkspaceWatchSignal::Changed);
        }
        if disconnected {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{WorkspaceInspection, inspect_workspace, refresh_workspace};
    use std::{fs, os::unix::fs::symlink, path::PathBuf, sync::mpsc, time::SystemTime};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("rehow-watch-{}-{nonce}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn managed_edits_are_coalesced_and_nested_git_activity_is_ignored() {
        let temp = workspace();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::create_dir(temp.path().join(".git/refs")).unwrap();
        let inspection = inspect_workspace(temp.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(view) = inspection else {
            panic!("expected compatible Workspace");
        };
        assert_eq!(watched_directories(&view).unwrap().len(), 3);
        let (sender, receiver) = mpsc::channel();
        let watcher = WorkspaceWatcher::start(&view, move |signal| {
            sender.send(signal).unwrap();
        })
        .unwrap();
        assert_eq!(watcher.watched_directory_count(), 3);

        fs::write(temp.path().join(".git/refs/main"), "ignored").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(400)).is_err());

        let guide = temp
            .path()
            .join("categories/linux/0199a1d7-7b0d-7d8e-a38d-f936c5735e21.md");
        for index in 0..4 {
            let source = fs::read_to_string(&guide).unwrap();
            fs::write(&guide, format!("{source}\nchange {index}\n")).unwrap();
        }
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
            WorkspaceWatchSignal::Changed
        );
        assert!(receiver.recv_timeout(Duration::from_millis(400)).is_err());
    }

    #[test]
    fn reconcile_adds_a_new_managed_directory_watch() {
        let temp = workspace();
        let inspection = inspect_workspace(temp.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(mut view) = inspection else {
            panic!("expected compatible Workspace");
        };
        let (sender, receiver) = mpsc::channel();
        let mut watcher = WorkspaceWatcher::start(&view, move |signal| {
            sender.send(signal).unwrap();
        })
        .unwrap();
        let new_category = temp.path().join("categories/desktop");
        fs::create_dir(&new_category).unwrap();
        fs::write(
            new_category.join("category.yaml"),
            "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e12\nname: Desktop\norder: 2\nstatus: active\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n",
        )
        .unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
            WorkspaceWatchSignal::Changed
        );
        refresh_workspace(&mut view).unwrap();
        watcher.reconcile(&view).unwrap();
        assert_eq!(watcher.watched_directory_count(), 4);
        fs::write(
            new_category.join("guide.md"),
            guide_source(
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e31",
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e12",
            ),
        )
        .unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
            WorkspaceWatchSignal::Changed
        );
    }

    #[test]
    fn reconcile_adds_new_attachment_directory_watches() {
        let temp = workspace();
        let inspection = inspect_workspace(temp.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(mut view) = inspection else {
            panic!("expected compatible Workspace");
        };
        let (sender, receiver) = mpsc::channel();
        let mut watcher = WorkspaceWatcher::start(&view, move |signal| {
            sender.send(signal).unwrap();
        })
        .unwrap();
        let attachment_directory = temp
            .path()
            .join("attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e21");
        fs::create_dir(temp.path().join("attachments")).unwrap();
        fs::create_dir(&attachment_directory).unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
            WorkspaceWatchSignal::Changed
        );
        refresh_workspace(&mut view).unwrap();
        watcher.reconcile(&view).unwrap();
        assert_eq!(watcher.watched_directory_count(), 5);
        fs::write(attachment_directory.join("external.txt"), "changed").unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
            WorkspaceWatchSignal::Changed
        );
    }

    #[test]
    fn watch_errors_have_priority_over_changed_events() {
        let (sender, receiver) = mpsc::channel();
        let coalescer = WatchCoalescer::start(Arc::new(move |signal| {
            sender.send(signal).unwrap();
        }))
        .unwrap();
        let sink = coalescer.sink();
        sink.report(CHANGED);
        sink.report(DEGRADED);
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            WorkspaceWatchSignal::Degraded
        );
    }

    #[test]
    fn event_reported_by_handler_is_not_lost() {
        use std::sync::{Mutex, atomic::AtomicBool};

        let sink_slot = Arc::new(Mutex::new(None::<SignalSink>));
        let first = Arc::new(AtomicBool::new(true));
        let (sender, receiver) = mpsc::channel();
        let handler_sink = Arc::clone(&sink_slot);
        let handler_first = Arc::clone(&first);
        let coalescer = WatchCoalescer::start(Arc::new(move |signal| {
            sender.send(signal).unwrap();
            if handler_first.swap(false, Ordering::AcqRel) {
                handler_sink
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .report(CHANGED);
            }
        }))
        .unwrap();
        let sink = coalescer.sink();
        *sink_slot.lock().unwrap() = Some(sink.clone());
        sink.report(CHANGED);

        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            WorkspaceWatchSignal::Changed
        );
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            WorkspaceWatchSignal::Changed
        );
        *sink_slot.lock().unwrap() = None;
        drop(sink);
        drop(coalescer);
    }

    #[test]
    fn watches_safe_malformed_categories_but_not_invalid_attachment_directories() {
        let temp = workspace();
        let malformed = temp.path().join("categories/repairable");
        fs::create_dir(&malformed).unwrap();
        fs::write(malformed.join("category.yaml"), "not: [valid").unwrap();
        let invalid_attachment = temp.path().join("attachments/not-a-guide-id");
        fs::create_dir_all(&invalid_attachment).unwrap();
        let inspection = inspect_workspace(temp.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(view) = inspection else {
            panic!("expected compatible Workspace");
        };
        let (sender, receiver) = mpsc::channel();
        let watcher = WorkspaceWatcher::start(&view, move |signal| {
            sender.send(signal).unwrap();
        })
        .unwrap();
        assert_eq!(watcher.watched_directory_count(), 5);

        fs::write(invalid_attachment.join("ignored.txt"), "ignored").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(400)).is_err());

        fs::write(
            malformed.join("category.yaml"),
            "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e12\nname: Repairable\norder: 2\nstatus: active\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n",
        )
        .unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
            WorkspaceWatchSignal::Changed
        );
    }

    #[test]
    fn reconcile_rejects_a_managed_directory_replaced_by_a_symlink() {
        let temp = workspace();
        let outside = TestDirectory::new();
        let inspection = inspect_workspace(temp.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(view) = inspection else {
            panic!("expected compatible Workspace");
        };
        let mut watcher = WorkspaceWatcher::start(&view, |_| {}).unwrap();
        let category = temp.path().join("categories/linux");
        let displaced = temp.path().join("categories/linux-displaced");
        fs::rename(&category, &displaced).unwrap();
        symlink(outside.path(), &category).unwrap();

        assert_eq!(
            watcher.reconcile(&view),
            Err(WorkspaceWatchError::Unavailable)
        );
        assert_eq!(watcher.watched_directory_count(), 3);
    }

    fn workspace() -> TestDirectory {
        let temp = TestDirectory::new();
        fs::create_dir(temp.path().join("categories")).unwrap();
        fs::create_dir(temp.path().join("categories/linux")).unwrap();
        fs::write(
            temp.path().join("rehow.yaml"),
            "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Watch test\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("categories/linux/category.yaml"),
            "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e11\nname: Linux\norder: 1\nstatus: active\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n",
        )
        .unwrap();
        fs::write(
            temp.path()
                .join("categories/linux/0199a1d7-7b0d-7d8e-a38d-f936c5735e21.md"),
            guide_source(
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e21",
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e11",
            ),
        )
        .unwrap();
        temp
    }

    fn guide_source(id: &str, category_id: &str) -> String {
        format!(
            "---\nschema_version: 1\nid: {id}\ntitle: Watched Guide\nsummary: Detect external changes\ncategory_id: {category_id}\ntags: []\naliases: []\nstatus: draft\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n---\n\nWatch this Guide.\n"
        )
    }
}

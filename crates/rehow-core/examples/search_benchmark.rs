use rehow_core::{
    search::search_workspace,
    storage::{WorkspaceInspection, inspect_workspace},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const GUIDE_COUNT: usize = 5_000;
const BODY_BYTES: usize = 10_128;
const COLD_REBUILD_BUDGET: Duration = Duration::from_secs(8);
const WARM_QUERY_P95_BUDGET_MS: f64 = 50.0;
const PEAK_RSS_BUDGET_KIB: u64 = 300 * 1024;

#[allow(
    clippy::assertions_on_constants,
    reason = "the benchmark must refuse accidental non-release execution"
)]
fn main() {
    assert!(
        !cfg!(debug_assertions),
        "run this performance gate with Cargo --release"
    );
    let generated = GeneratedWorkspace::new();
    let root = generated.path();
    generate_workspace(root);

    let rebuild_started = Instant::now();
    let inspection = inspect_workspace(root).expect("benchmark Workspace must inspect");
    let rebuild = rebuild_started.elapsed();
    let (_, classification) = inspection.into_parts();
    let WorkspaceInspection::Compatible(workspace) = classification else {
        panic!("benchmark Workspace must be compatible");
    };

    let queries = [
        "guide 04999",
        "permission denied synthetic failure 03125",
        "/etc/rehow/01234.conf",
        "rehow-benchmark-token",
    ];
    for query in queries {
        search_workspace(&workspace, query, false, 100).expect("warm-up query must succeed");
    }
    let mut samples = Vec::with_capacity(400);
    for iteration in 0..400 {
        let query = queries[iteration % queries.len()];
        let started = Instant::now();
        search_workspace(&workspace, query, false, 100).expect("benchmark query must succeed");
        samples.push(started.elapsed());
    }
    samples.sort_unstable();

    println!("guides={GUIDE_COUNT}");
    let markdown_bytes = workspace_bytes(root);
    let p50 = percentile(&samples, 50);
    let p95 = percentile(&samples, 95);
    let p99 = percentile(&samples, 99);
    let peak_rss = peak_rss_kib().expect("Linux VmHWM must be available for this gate");
    println!("markdown_bytes={markdown_bytes}");
    println!("cold_rebuild_ms={:.3}", milliseconds(rebuild));
    println!("warm_query_p50_ms={p50:.3}");
    println!("warm_query_p95_ms={p95:.3}");
    println!("warm_query_p99_ms={p99:.3}");
    println!("peak_rss_kib={peak_rss}");

    assert!(markdown_bytes >= 50 * 1024 * 1024, "corpus is below 50 MiB");
    assert!(
        rebuild <= COLD_REBUILD_BUDGET,
        "cold rebuild exceeded 8 seconds"
    );
    assert!(
        p95 <= WARM_QUERY_P95_BUDGET_MS,
        "warm query p95 exceeded 50 ms"
    );
    assert!(peak_rss <= PEAK_RSS_BUDGET_KIB, "peak RSS exceeded 300 MiB");
}

struct GeneratedWorkspace(PathBuf);

impl GeneratedWorkspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rehow-search-benchmark-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time must follow the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&path).expect("benchmark directory must be creatable");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for GeneratedWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn generate_workspace(root: &Path) {
    fs::create_dir(root.join("categories")).unwrap();
    fs::write(
        root.join("rehow.yaml"),
        "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Search benchmark\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n",
    )
    .unwrap();
    let category = root.join("categories/benchmark");
    fs::create_dir(&category).unwrap();
    fs::write(
        category.join("category.yaml"),
        "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e11\nname: Benchmark\norder: 1\nstatus: active\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n",
    )
    .unwrap();
    let filler = "rehow-benchmark-token ".repeat(BODY_BYTES / 22);
    for index in 0..GUIDE_COUNT {
        let id = format!("0199a1d7-7b0d-7d8e-a38d-{index:012x}");
        let source = format!(
            "---\nschema_version: 1\nid: {id}\ntitle: Guide {index:05}\nsummary: Synthetic benchmark guide {index:05}\ncategory_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e11\ntags: [benchmark]\naliases: []\nstatus: draft\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n---\n\n## Steps\n\n```sh\necho guide-{index:05}\n```\n\nEdit `/etc/rehow/{index:05}.conf`.\n\nError: permission denied synthetic failure {index:05}.\n\n{filler}\n"
        );
        fs::write(category.join(format!("guide-{index:05}.md")), source).unwrap();
    }
}

fn workspace_bytes(root: &Path) -> u64 {
    let mut total = fs::metadata(root.join("rehow.yaml")).unwrap().len();
    let category = root.join("categories/benchmark");
    total += fs::metadata(category.join("category.yaml")).unwrap().len();
    for entry in fs::read_dir(category).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() != "category.yaml" {
            total += entry.metadata().unwrap().len();
        }
    }
    total
}

fn percentile(samples: &[Duration], percentile: usize) -> f64 {
    let index = (samples.len() * percentile).div_ceil(100).saturating_sub(1);
    milliseconds(samples[index])
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn peak_rss_kib() -> Option<u64> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

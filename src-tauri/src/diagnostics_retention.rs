use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

pub const MAX_DIAGNOSTIC_EXPORTS: usize = 5;
pub const LOG_RETENTION: Duration = Duration::from_secs(14 * 24 * 60 * 60);

fn is_diagnostic_export_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix("diagnostics_export_") else {
        return false;
    };
    let mut parts = suffix.split('_');
    let Some(date) = parts.next() else {
        return false;
    };
    let Some(time) = parts.next() else {
        return false;
    };
    let tail = parts.next();
    parts.next().is_none()
        && date.len() == 8
        && date.bytes().all(|byte| byte.is_ascii_digit())
        && time.len() == 6
        && time.bytes().all(|byte| byte.is_ascii_digit())
        && tail.is_none_or(|value| {
            value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

pub fn prune_diagnostic_exports(root: &Path, keep: usize) -> io::Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut exports = entries
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name_text = name.to_str()?;
            if !is_diagnostic_export_name(name_text) || !entry.file_type().ok()?.is_dir() {
                return None;
            }
            Some((name_text.to_string(), entry.path()))
        })
        .collect::<Vec<_>>();
    exports.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, path) in exports.into_iter().skip(keep) {
        if path.parent() != Some(root) {
            continue;
        }
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub fn prune_diagnostic_logs(root: &Path, now: SystemTime) -> io::Result<()> {
    let Some(cutoff) = now.checked_sub(LOG_RETENTION) else {
        return Ok(());
    };
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !is_dated_log_name(&name) {
            continue;
        }
        if entry.metadata()?.modified()? < cutoff {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn is_dated_log_name(name: &str) -> bool {
    let Some(date) = name.strip_prefix("consensus-arena.log.") else {
        return false;
    };
    date.len() == 10
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[7] == b'-'
        && date
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    #[test]
    fn export_retention_removes_only_oldest_valid_arena_exports() {
        let root = temp_dir("diagnostic-retention");
        for stamp in [
            "20260901_000000",
            "20260902_000000",
            "20260903_000000",
            "20260904_000000",
            "20260905_000000",
            "20260906_000000",
        ] {
            fs::create_dir(root.join(format!("diagnostics_export_{stamp}")))
                .expect("create export directory");
        }
        let unrelated = root.join("diagnostics_export_notes");
        fs::create_dir(&unrelated).expect("create unrelated directory");

        prune_diagnostic_exports(&root, MAX_DIAGNOSTIC_EXPORTS).expect("prune exports");

        assert!(!root.join("diagnostics_export_20260901_000000").exists());
        assert!(root.join("diagnostics_export_20260906_000000").exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[cfg(not(windows))]
    #[test]
    fn log_retention_removes_only_dated_arena_logs_older_than_fourteen_days() {
        let root = temp_dir("log-retention");
        let now = SystemTime::now();
        let old_path = root.join("consensus-arena.log.2026-08-01");
        let recent_path = root.join("consensus-arena.log.2026-09-16");
        let active_path = root.join("consensus-arena.log");
        let unrelated_path = root.join("other.log.2026-08-01");
        for path in [&old_path, &recent_path, &active_path, &unrelated_path] {
            fs::write(path, "test").expect("write test log");
        }
        let old_time = now
            .checked_sub(LOG_RETENTION + Duration::from_secs(60))
            .unwrap_or(now);
        fs::File::open(&old_path)
            .expect("open old log")
            .set_times(fs::FileTimes::new().set_modified(old_time))
            .expect("set old log time");

        prune_diagnostic_logs(&root, now).expect("prune logs");

        assert!(!old_path.exists());
        assert!(recent_path.exists());
        assert!(active_path.exists());
        assert!(unrelated_path.exists());
        fs::remove_dir_all(root).expect("remove test directory");
    }
}

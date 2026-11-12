//! Diagnostics: one JSON line per sample, distance change or note, in one file a day
//! (`<dir>/<YYYY-MM-DD>.jsonl`, UTC). No player names. Old files are deleted after `keep-days`
//! (not compressed: the original packs them as .jsonl.gz, this port stays std only).
use crate::engine::{Change, Distances};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The UTC date of `unix` seconds (Howard Hinnant's days-to-civil).
pub fn date(unix: u64) -> (i64, u32, u32) {
    let z = (unix / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn file_for(dir: &Path, unix: u64) -> PathBuf {
    let (y, m, d) = date(unix);
    dir.join(format!("{y:04}-{m:02}-{d:02}.jsonl"))
}

fn escape(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            c if c.is_control() => format!("\\u{:04x}", c as u32).chars().collect(),
            c => vec![c],
        })
        .collect()
}

pub struct Sample {
    pub mspt: f64,
    pub players: u32,
    pub loaded_chunks: u32,
    pub distances: Distances,
    pub free_memory_percent: Option<f64>,
    pub relief: bool,
    pub profile: String,
}

pub fn sample_line(unix: u64, s: &Sample) -> String {
    let free = s.free_memory_percent.map_or("null".to_owned(), |f| format!("{f:.1}"));
    format!(
        "{{\"t\":{unix},\"kind\":\"sample\",\"mspt\":{:.2},\"players\":{},\"chunks\":{},\"sim\":{},\"view\":{},\"free_memory\":{free},\"relief\":{},\"profile\":\"{}\"}}",
        s.mspt,
        s.players,
        s.loaded_chunks,
        s.distances.sim,
        s.distances.view,
        s.relief,
        escape(&s.profile)
    )
}

pub fn change_line(unix: u64, c: &Change) -> String {
    format!(
        "{{\"t\":{unix},\"kind\":\"change\",\"old_sim\":{},\"new_sim\":{},\"old_view\":{},\"new_view\":{},\"reason\":\"{}\"}}",
        c.from.sim,
        c.to.sim,
        c.from.view,
        c.to.view,
        escape(c.reason)
    )
}

pub fn note_line(unix: u64, text: &str) -> String {
    format!("{{\"t\":{unix},\"kind\":\"note\",\"text\":\"{}\"}}", escape(text))
}

/// Appends `line` to today's file. A sample is skipped once the file reached `max_mb`.
pub fn append(dir: &Path, unix: u64, line: &str, sample: bool, max_mb: f64) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let file = file_for(dir, unix);
    let size = std::fs::metadata(&file).map_or(0, |m| m.len());
    if sample && max_mb > 0.0 && size as f64 >= max_mb * 1024.0 * 1024.0 {
        return Ok(());
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(file)?;
    writeln!(f, "{line}")
}

/// Deletes the day files older than `keep_days` days.
pub fn prune(dir: &Path, unix: u64, keep_days: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let oldest = file_for(dir, unix.saturating_sub(keep_days * 86_400));
    let oldest = oldest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Day files sort by name; only ours (`YYYY-MM-DD.jsonl`) are touched.
        if name.len() == 16 && name.ends_with(".jsonl") && name.as_bytes()[4] == b'-' && name < oldest {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc_days() {
        assert_eq!(date(0), (1970, 1, 1));
        assert_eq!(date(951_782_400), (2000, 2, 29));
        assert_eq!(date(1_793_923_200), (2026, 11, 6));
    }

    #[test]
    fn lines_are_json_without_names() {
        let c = Change { from: Distances { sim: 6, view: 12 }, to: Distances { sim: 5, view: 12 }, reason: "tick time high" };
        assert_eq!(
            change_line(7, &c),
            "{\"t\":7,\"kind\":\"change\",\"old_sim\":6,\"new_sim\":5,\"old_view\":12,\"new_view\":12,\"reason\":\"tick time high\"}"
        );
        assert_eq!(note_line(1, "a \"b\"\n"), "{\"t\":1,\"kind\":\"note\",\"text\":\"a \\\"b\\\"\\u000a\"}");
        let s = Sample { mspt: 12.345, players: 2, loaded_chunks: 900, distances: c.to, free_memory_percent: None, relief: false, profile: "normal".into() };
        assert!(sample_line(3, &s).contains("\"mspt\":12.35,\"players\":2,\"chunks\":900,\"sim\":5,\"view\":12,\"free_memory\":null"));
    }

    #[test]
    fn files_rotate_by_day_cap_samples_and_old_ones_go() {
        let dir = std::env::temp_dir().join(format!("vdt-diag-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let day = 1_793_923_200;
        append(&dir, day - 10 * 86_400, "{}", false, 50.0).unwrap();
        append(&dir, day, "{\"a\":1}", true, 50.0).unwrap();
        append(&dir, day + 60, "{\"a\":2}", false, 50.0).unwrap();
        // A tiny cap stops samples but not changes or notes.
        append(&dir, day + 120, "{\"a\":3}", true, 0.000_001).unwrap();
        let today = std::fs::read_to_string(file_for(&dir, day)).unwrap();
        assert_eq!(today, "{\"a\":1}\n{\"a\":2}\n");
        std::fs::write(dir.join("notes.txt"), "keep").unwrap();
        prune(&dir, day, 7);
        let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, ["2026-11-06.jsonl", "notes.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

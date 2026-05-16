use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{TimeZone, Utc};

fn main() {
  println!("cargo:rerun-if-changed=.git/HEAD");
  println!("cargo:rerun-if-changed=.git/refs/heads");
  println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

  let git_hash = git_hash();
  let build_date = build_date();
  let target = target();

  println!("cargo:rustc-env=OCPPSIM_GIT_HASH={git_hash}");
  println!("cargo:rustc-env=OCPPSIM_BUILD_DATE={build_date}");
  println!("cargo:rustc-env=OCPPSIM_TARGET={target}");
}

fn git_hash() -> String {
  Command::new("git")
    .args(["rev-parse", "HEAD"])
    .output()
    .ok()
    .filter(|o| o.status.success())
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .map(|s| s.trim().to_owned())
    .unwrap_or_else(|| "unknown".to_owned())
}

fn build_date() -> String {
  let secs = if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
    epoch.trim().parse().unwrap_or(0)
  } else {
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs())
      .unwrap_or(0)
  };
  Utc
    .timestamp_opt(secs as i64, 0)
    .single()
    .map(|dt| dt.format("%Y-%m-%d").to_string())
    .unwrap_or_else(|| "unknown".to_owned())
}

fn target() -> String {
  std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_owned())
}

pub fn version_string() -> String {
  let version = env!("CARGO_PKG_VERSION");
  let git_hash = env!("OCPPSIM_GIT_HASH");
  let build_date = env!("OCPPSIM_BUILD_DATE");
  let target = env!("OCPPSIM_TARGET");
  format!("{version} ({git_hash} {build_date} {target})")
}

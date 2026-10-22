//! Running yt-dlp, shared by in_spotitube (playback) and ml_spotitube (search, via `#[path]`).

use std::io::ErrorKind;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// yt-dlp self-update (`-U`) is attempted at most once per Winamp session.
static UPDATE_TRIED: AtomicBool = AtomicBool::new(false);

/// An external tool without a console window or stdin.
pub fn tool(exe: &str) -> Command {
    let mut cmd = Command::new(exe);
    cmd.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
    cmd
}

/// Runs yt-dlp and returns its stdout lines, or a user-facing error. On failure, self-updates
/// once per session and retries, since YouTube changes often break older yt-dlp versions.
pub fn run(args: &[&str]) -> Result<Vec<String>, String> {
    let first = run_once(args);
    match &first {
        Err((_, true)) if self_update() => run_once(args).map_err(|(msg, _)| msg),
        _ => first.map_err(|(msg, _)| msg),
    }
}

/// Error carries whether yt-dlp ran and failed (worth retrying after an update).
/// `--encoding utf-8`: PYTHONIOENCODING/PYTHONUTF8 silently drop non-ASCII characters.
fn run_once(args: &[&str]) -> Result<Vec<String>, (String, bool)> {
    let output = match tool("yt-dlp").args(["--encoding", "utf-8", "--no-warnings"]).args(args).output() {
        Ok(o) => o,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(("yt-dlp not found on PATH.\nInstall: winget install yt-dlp.yt-dlp\nThen restart Winamp.".into(), false));
        }
        Err(e) => return Err((format!("yt-dlp failed to start: {e}"), false)),
    };
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err((friendly_error(err.trim()), true));
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().map(str::to_owned).collect())
}

/// Runs `yt-dlp -U` the first time it's called; true if a newer version was installed.
fn self_update() -> bool {
    if UPDATE_TRIED.swap(true, Ordering::SeqCst) {
        return false;
    }
    match tool("yt-dlp").arg("-U").output() {
        // "yt-dlp is up to date" means retrying won't help.
        Ok(o) if o.status.success() => !String::from_utf8_lossy(&o.stdout).contains("up to date"),
        _ => false,
    }
}

/// Common failures, matched on the reason text YouTube sends (yt-dlp passes it through).
const KNOWN_ERRORS: &[(&str, &str)] = &[
    ("confirm your age", "This video is age-restricted; YouTube only plays it for signed-in users."),
    ("not a bot", "YouTube asked to confirm you're not a bot. Try again later."),
    ("rate-limited", "YouTube is limiting requests from this connection for up to an hour. Try again later."),
    ("Private video", "This video is private."),
    ("not made this video available in your country", "This video isn't available in your country."),
    ("geo restriction", "This video isn't available in your country."),
    ("removed for violating", "This video was removed by YouTube."),
    ("video is unavailable", "This video is unavailable (removed, or the link is wrong)."),
];

/// A short explanation for known yt-dlp failures, with yt-dlp's own message below it.
fn friendly_error(stderr: &str) -> String {
    match KNOWN_ERRORS.iter().find(|(needle, _)| stderr.contains(needle)) {
        Some((_, text)) => format!("{text}\n\nyt-dlp: {stderr}"),
        None => format!("yt-dlp: {stderr}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_known_errors() {
        // Real yt-dlp 2026.08.19 output.
        let gone = "ERROR: [youtube] aaaaaaaaaaa: This video is unavailable";
        assert_eq!(friendly_error(gone), format!("This video is unavailable (removed, or the link is wrong).\n\nyt-dlp: {gone}"));
        let age = "ERROR: [youtube] 07FYdnEawAQ: Sign in to confirm your age. Use --cookies-from-browser or --cookies for the authentication.";
        assert!(friendly_error(age).starts_with("This video is age-restricted"));
        assert_eq!(friendly_error("ERROR: something new"), "yt-dlp: ERROR: something new");
    }
}

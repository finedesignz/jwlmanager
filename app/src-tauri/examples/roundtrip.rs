//! Round-trip an existing `.jwlibrary` archive through the Tauri app's real
//! open + save path, writing the result to a NEW file.
//!
//! This is the owner-facing helper for the ARCH-02 manual gate: it produces a
//! Tauri-saved archive from a real input archive so it can be imported back
//! into JW Library to confirm the vendor app accepts what we write.
//!
//! The INPUT FILE IS NEVER MODIFIED — it is opened read-only into a temp dir
//! (D-03) and the rebuilt archive is written to `<output>` via the atomic
//! replace path (D-04).
//!
//! Usage:
//!   cargo run --example roundtrip -- <input.jwlibrary> <output.jwlibrary>

use jwlmanager_lib::archive::new::save_as;
use jwlmanager_lib::archive::open_and_validate;
use jwlmanager_lib::db::resources::dev_resources_db_path;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: cargo run --example roundtrip -- <input.jwlibrary> <output.jwlibrary>");
        return ExitCode::from(2);
    }

    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);

    if !input.exists() {
        eprintln!("ERROR: input archive not found: {}", input.display());
        return ExitCode::from(2);
    }
    if output.exists() {
        eprintln!(
            "ERROR: refusing to overwrite an existing output file: {}",
            output.display()
        );
        eprintln!("       choose a new output path so nothing is destroyed.");
        return ExitCode::from(2);
    }

    let input_len = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    println!("Opening (read-only): {}", input.display());
    println!("  size: {input_len} bytes");

    let (mut session, notes) = match open_and_validate(input, &dev_resources_db_path()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ERROR opening archive: {e}");
            return ExitCode::from(1);
        }
    };

    println!("  schema: v{}", session.manifest.schema_version);
    println!("  zip entries preserved: {}", session.entries.len());
    println!("  notes read: {}", notes.len());

    // save_as writes to a NEW path and leaves the source file untouched.
    session.target_path = output.to_path_buf();
    let now = chrono_now_iso8601();
    match save_as(&session, output, "JWL Manager", "JWL Manager", &now) {
        Ok(manifest) => {
            let out_len = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            println!("\nSaved: {}", output.display());
            println!("  size: {out_len} bytes");
            println!(
                "  schemaVersion: {}",
                manifest.user_data_backup.schema_version
            );
            println!("  hash: {}", manifest.user_data_backup.hash);

            // Prove the source really was left alone.
            let after_len = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
            if after_len == input_len {
                println!("\nSource archive unchanged ({after_len} bytes) — verified.");
            } else {
                eprintln!(
                    "\nWARNING: source size changed ({input_len} -> {after_len}) — investigate!"
                );
                return ExitCode::from(1);
            }

            println!("\nNext: import '{}' into JW Library.", output.display());
            println!("Reminder: restoring a backup REPLACES all user data on that device.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ERROR saving archive: {e}");
            ExitCode::from(1)
        }
    }
}

/// Minimal ISO-8601 UTC timestamp without pulling in a date crate for an example.
fn chrono_now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // days since epoch -> civil date (Howard Hinnant's algorithm)
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

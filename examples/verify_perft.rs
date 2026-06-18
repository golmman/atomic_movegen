//! Reads positions and expected node counts from `perft_values.md` and runs a
//! perft at each listed depth, reporting which cases pass and which fail.
//!
//! Usage:
//! ```sh
//! cargo run --example verify_perft [max-depth] [path]
//! ```
//!
//! Arguments:
//! - `max-depth` — maximum depth to test (default: 6, omit or set to 0 to run all)
//! - `path`      — path to the perft values markdown file (default: `perft_values.md`)
//!
//! The process exits with code 0 if every test passes, or 1 if any fail.

use atomic_movegen::board::Board;
use atomic_movegen::perft;
use std::env;
use std::fs;
use std::process;

/// A single test case parsed from the markdown table.
struct TestCase {
    number: usize,
    fen: String,
    /// Pairs of (depth, expected_node_count).
    depths: Vec<(u32, u64)>,
}

/// Parse the markdown table in `perft_values.md` into a list of test cases.
///
/// The expected format (as of this writing):
///
/// ```markdown
/// | #   | Depth 1 | Depth 2 | ... | Depth 6    | FEN                                        |
/// | --- | ------- | ------- | ... | ---------- | ------------------------------------------ |
/// | 1   | 20      | 400     | ... | 118926425  | `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/...`  |
/// ```
fn parse_perft_values(path: &str) -> Vec<TestCase> {
    let content = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Error: could not read `{}`: {}", path, e);
        process::exit(1);
    });

    let mut cases: Vec<TestCase> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines, headers, and separator rows.
        if trimmed.is_empty()
            || trimmed.starts_with("| #")
            || trimmed.starts_with("| ---")
            || !trimmed.starts_with('|')
        {
            continue;
        }

        // Split on pipe.  The leading pipe produces an empty first element.
        let parts: Vec<&str> = trimmed.split('|').collect();
        if parts.len() < 9 {
            continue;
        }

        // Column 1 (index 1 after the leading empty) = row number.
        let number: usize = match parts[1].trim().parse() {
            Ok(n) => n,
            Err(_) => continue,
        };

        // Columns 2-7 (indices 2..8) = depth values.
        let depth_counts: Vec<u64> = parts[2..8]
            .iter()
            .map(|s| s.trim().replace([',', '_'], "").parse().unwrap_or(0))
            .collect();

        // Column 8 (index 8) = FEN, possibly wrapped in backticks.
        let raw_fen = parts[8].trim();
        let fen = if let Some(start) = raw_fen.find('`') {
            if let Some(end) = raw_fen.rfind('`') {
                raw_fen[start + 1..end].trim()
            } else {
                &raw_fen[start + 1..]
            }
        } else {
            raw_fen
        };
        let fen = fen.trim().to_string();

        let depths: Vec<(u32, u64)> = depth_counts
            .into_iter()
            .enumerate()
            .map(|(i, count)| (i as u32 + 1, count))
            .collect();

        cases.push(TestCase {
            number,
            fen,
            depths,
        });
    }

    cases
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse optional max depth (first positional arg).
    let max_depth: u32 = if args.len() > 1 {
        args[1].parse().unwrap_or(6)
    } else {
        6
    };

    // Parse optional file path (second positional arg, or default).
    let path = if args.len() > 2 {
        &args[2]
    } else {
        "perft_values.md"
    };

    // Limit to 0 means "no limit".
    let max_depth = if max_depth == 0 { u32::MAX } else { max_depth };

    let cases = parse_perft_values(path);

    eprintln!(
        "=== Perft verification ===\n  Source:     {path}\n  Max depth:  {}\n  Test cases: {}\n",
        if max_depth == u32::MAX {
            "unlimited".to_string()
        } else {
            max_depth.to_string()
        },
        cases.len()
    );

    let mut passed = 0u64;
    let mut failed = 0u64;
    let mut detail_lines: Vec<String> = Vec::new();

    for case in &cases {
        let mut board = match Board::from_fen(&case.fen) {
            Ok(b) => b,
            Err(e) => {
                detail_lines.push(format!(
                    "  Test #{}: INVALID FEN — {} ({})",
                    case.number, case.fen, e
                ));
                failed += 1;
                continue;
            }
        };

        // Determine which depths to actually run.
        let depths_to_test: Vec<(u32, u64)> = case
            .depths
            .iter()
            .copied()
            .filter(|(d, _)| *d <= max_depth)
            .collect();

        if depths_to_test.is_empty() {
            continue;
        }

        let mut ok = true;

        // Try each depth.
        for &(depth, expected) in &depths_to_test {
            let result = perft(&mut board, depth);
            if result != expected {
                detail_lines.push(format!(
                    "  Test #{} FAIL depth={}: expected {}, got {}",
                    case.number, depth, expected, result
                ));
                ok = false;
            }
        }

        if ok {
            passed += 1;
            // Print a compact one-liner for passes.
            println!(
                "  Test #{:<4} PASS ({} depth{})",
                case.number,
                depths_to_test.len(),
                if depths_to_test.len() == 1 { "" } else { "s" },
            );
        } else {
            failed += 1;
        }
    }

    // Print any failure detail lines.
    if !detail_lines.is_empty() {
        eprintln!("\n--- Failures ---");
        for line in &detail_lines {
            eprintln!("{line}");
        }
    }

    let total = passed + failed;
    eprintln!("\n=== {passed}/{total} passed, {failed}/{total} failed ===");

    if failed > 0 {
        process::exit(1);
    }
}

//! PROTOTYPE — drive canonical time-range and chart-bucket rules by hand.
//!
//! Question: do Today / 7D / 30D / All / Custom reduce to one predictable
//! inclusive date range and automatic hour/day/natural-week/natural-month bucket model,
//! including DST and incomplete-edge behavior?

mod time_range_prototype_logic;

use anyhow::{anyhow, Result};
use chrono_tz::Tz;
use std::io::{self, Write};
use time_range_prototype_logic::{
    format_resolution, parse_date, parse_reporting_now, resolve, PrototypeInput, Selection,
};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("scenarios") {
        return run_scenarios();
    }

    let tz: Tz = "America/Los_Angeles".parse()?;
    let mut state = PrototypeInput {
        reporting_today: parse_date("2026-08-04")?,
        reporting_now: parse_reporting_now(tz, "2026-08-04T17:30")?,
        earliest_known_date: Some(parse_date("2025-09-12")?),
        selection: Selection::Today,
    };

    loop {
        render(&state);
        print!("\n> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line == "q" || line == "quit" {
            break;
        }
        if let Err(error) = apply_command(&mut state, line) {
            println!("\nERROR: {error}\nPress Enter to continue…");
            let _ = io::stdin().read_line(&mut String::new());
        }
    }
    Ok(())
}

fn render(state: &PrototypeInput) {
    print!("\x1b[2J\x1b[H");
    println!("\x1b[1mPROTOTYPE — canonical time range + chart buckets\x1b[0m");
    println!("\x1b[2mThrowaway TUI; no production code or persistence.\x1b[0m\n");
    match resolve(state) {
        Ok(resolution) => println!("{}", format_resolution(state, &resolution)),
        Err(error) => println!("resolution_error  {error}"),
    }
    println!("\n\x1b[1mcommands\x1b[0m");
    println!("  t                         Today");
    println!("  7                         rolling 7 inclusive days");
    println!("  3                         rolling 30 inclusive days");
    println!("  a                         All (earliest known date…today)");
    println!("  c YYYY-MM-DD YYYY-MM-DD   Custom inclusive range");
    println!("  today YYYY-MM-DD          change reporting today and keep local time");
    println!("  now YYYY-MM-DDTHH:MM      change reporting now in current timezone");
    println!("  earliest YYYY-MM-DD|none  change earliest known usage date");
    println!("  tz Area/City              change reporting timezone");
    println!("  s                         print edge-case scenarios");
    println!("  q                         quit");
}

fn apply_command(state: &mut PrototypeInput, line: &str) -> Result<()> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    match parts.as_slice() {
        ["t"] => state.selection = Selection::Today,
        ["7"] => state.selection = Selection::Last7Days,
        ["3"] => state.selection = Selection::Last30Days,
        ["a"] => state.selection = Selection::All,
        ["c", start, end] => {
            state.selection = Selection::Custom {
                start: parse_date(start)?,
                end: parse_date(end)?,
            }
        }
        ["today", value] => {
            let date = parse_date(value)?;
            let time = state.reporting_now.time();
            state.reporting_today = date;
            state.reporting_now = time_range_prototype_logic::local_datetime(
                state.reporting_now.timezone(),
                date.and_time(time),
            )?;
        }
        ["now", value] => {
            state.reporting_now = parse_reporting_now(state.reporting_now.timezone(), value)?;
            state.reporting_today = state.reporting_now.date_naive();
        }
        ["earliest", "none"] => state.earliest_known_date = None,
        ["earliest", value] => state.earliest_known_date = Some(parse_date(value)?),
        ["tz", value] => {
            let tz: Tz = value
                .parse()
                .map_err(|_| anyhow!("unknown timezone {value:?}"))?;
            let local = state.reporting_now.naive_local();
            state.reporting_now = time_range_prototype_logic::local_datetime(tz, local)?;
        }
        ["s"] => {
            run_scenarios()?;
            println!("\nPress Enter to return…");
            let _ = io::stdin().read_line(&mut String::new());
        }
        [] => {}
        _ => return Err(anyhow!("unknown command")),
    }
    Ok(())
}

fn run_scenarios() -> Result<()> {
    println!("PROTOTYPE SCENARIOS — canonical time range + chart buckets\n");
    let scenarios = vec![
        scenario(
            "Today stops at active hour",
            "America/Los_Angeles",
            "2026-08-04T17:30",
            Some("2025-09-12"),
            Selection::Today,
        )?,
        scenario(
            "Early Today prepends context to reach 12 hourly buckets",
            "America/Los_Angeles",
            "2026-08-04T01:30",
            Some("2025-09-12"),
            Selection::Today,
        )?,
        scenario(
            "Historical DST spring day has 23 real hours",
            "America/New_York",
            "2026-03-10T12:00",
            Some("2025-01-01"),
            custom("2026-03-08", "2026-03-08")?,
        )?,
        scenario(
            "Historical DST fall day has 25 real hours",
            "America/New_York",
            "2026-11-03T12:00",
            Some("2025-01-01"),
            custom("2026-11-01", "2026-11-01")?,
        )?,
        scenario(
            "Leap-day range remains seven civil days",
            "UTC",
            "2024-03-02T12:00",
            Some("2024-01-01"),
            custom("2024-02-25", "2024-03-02")?,
        )?,
        scenario(
            "30-day cross-year selection uses natural weeks",
            "UTC",
            "2026-01-08T09:00",
            Some("2024-01-01"),
            Selection::Last30Days,
        )?,
        scenario(
            "Long Custom range uses natural months",
            "Europe/London",
            "2026-08-04T17:30",
            Some("2024-01-01"),
            custom("2026-01-15", "2026-08-04")?,
        )?,
        scenario(
            "Custom may begin before earliest known usage",
            "Asia/Tokyo",
            "2026-08-04T17:30",
            Some("2026-07-28"),
            custom("2026-07-01", "2026-07-05")?,
        )?,
        scenario(
            "All with short history follows actual span",
            "Asia/Tokyo",
            "2026-08-04T17:30",
            Some("2026-07-28"),
            Selection::All,
        )?,
        scenario(
            "All with long history uses natural months",
            "Asia/Tokyo",
            "2026-08-04T17:30",
            Some("2024-01-01"),
            Selection::All,
        )?,
        scenario(
            "Future Custom end is rejected",
            "UTC",
            "2026-08-04T17:30",
            Some("2025-01-01"),
            custom("2026-08-03", "2026-08-05")?,
        )?,
    ];

    for (name, input) in scenarios {
        println!("=== {name} ===");
        match resolve(&input) {
            Ok(resolution) => println!("{}", format_resolution(&input, &resolution)),
            Err(error) => println!("resolution_error  {error}"),
        }
        println!();
    }
    Ok(())
}

fn scenario(
    name: &'static str,
    timezone: &str,
    now: &str,
    earliest: Option<&str>,
    selection: Selection,
) -> Result<(&'static str, PrototypeInput)> {
    let tz: Tz = timezone.parse()?;
    let reporting_now = parse_reporting_now(tz, now)?;
    Ok((
        name,
        PrototypeInput {
            reporting_today: reporting_now.date_naive(),
            reporting_now,
            earliest_known_date: earliest.map(parse_date).transpose()?,
            selection,
        },
    ))
}

fn custom(start: &str, end: &str) -> Result<Selection> {
    Ok(Selection::Custom {
        start: parse_date(start)?,
        end: parse_date(end)?,
    })
}

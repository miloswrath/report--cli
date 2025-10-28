use chrono::{Datelike, NaiveDate, Weekday};
use clap::{Parser, Subcommand};
use csv::StringRecord;
use serde::Deserialize;
use std::{
    cmp::Ordering,
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    name = "report-builder",
    author,
    version,
    about = "CLI utility for configuring report paths."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the path to the vosslabhpc share.
    Init,
}

#[derive(Debug, Deserialize)]
struct Config {
    share_path: String,
}

#[allow(dead_code)]
struct Session {
    share_path: PathBuf,
    subject_number: String,
    subject_directory: PathBuf,
    // Precomputed day-level metrics keyed by participant ID.
    activity_data: HashMap<String, Vec<DayMetrics>>,
    weekly_summary: Option<WeeklySummary>,
}

#[derive(Debug, Clone)]
struct DayMetrics {
    id: String,
    calendar_date: String,
    weekday: String,
    total_in_min: f64,
    total_lig_min: f64,
    total_mod_min: f64,
    total_vig_min: f64,
    sleep_minutes: f64,
}

#[derive(Debug, Clone)]
struct WeeklySummary {
    average_hours: [f64; 5],
    weekly_mvpa_minutes: f64,
    daily_average_hours: [f64; 5],
    daily_mvpa_minutes: f64,
    daily_sedentary_hours: f64,
    average_sleep_by_weekday: Vec<(Weekday, f64)>,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Init) => handle_init(),
        None => run_interactive(),
    };

    if let Err(err) = result {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

fn handle_init() -> Result<(), Box<dyn std::error::Error>> {
    let share_path = prompt_for_share_path()?;
    let config_dir = determine_config_dir()?;
    fs::create_dir_all(&config_dir)?;

    let config_file = config_dir.join("config.toml");
    let contents = format!("share_path = \"{}\"\n", share_path);
    fs::write(&config_file, contents)?;

    println!("Saved vosslabhpc share path to {}", config_file.display());

    Ok(())
}

fn prompt_for_share_path() -> Result<String, io::Error> {
    let example_path = example_share_path();

    loop {
        println!(
            "Enter the path to the vosslabhpc share (e.g., {}):",
            example_path
        );
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            println!("A value is required. Please try again.");
            continue;
        }

        return Ok(trimmed.to_string());
    }
}

fn determine_config_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let config_dir = dirs::config_dir()
        .ok_or("Unable to determine the user's configuration directory.")?
        .join("report-builder");
    Ok(config_dir)
}

fn example_share_path() -> &'static str {
    if cfg!(target_os = "macos") {
        "/Volumes/vosslabhpc"
    } else if cfg!(target_os = "windows") {
        r"\\vosslabhpc"
    } else {
        "/mnt/vosslabhpc"
    }
}

fn run_interactive() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    let share_path = Path::new(&config.share_path).to_path_buf();

    println!("Using configured share path: {}", share_path.display());

    let subject_number = prompt_for_subject_number()?;
    let subject_directory = build_subject_directory(&share_path, &subject_number)?;

    if !subject_directory.exists() {
        return Err(format!(
            "Subject directory does not exist: {}",
            subject_directory.display()
        )
        .into());
    }

    let csv_files = discover_target_csv(&subject_directory)?;

    println!(
        "Located {} target file(s) for subject {} under {}",
        csv_files.len(),
        subject_number,
        subject_directory.display()
    );

    if csv_files.is_empty() {
        println!("No matching files found; verify the subject data is available.");
        return Ok(());
    }

    for path in &csv_files {
        println!("  {}", path.display());
    }

    let activity_data = collect_activity_metrics(&csv_files)?;

    println!(
        "Prepared metrics for {} participant(s).",
        activity_data.len()
    );

    for (id, records) in activity_data.iter().take(5) {
        println!("  {} -> {} day(s) of data", id, records.len());
    }
    if activity_data.len() > 5 {
        println!("  ...");
    }

    let weekly_summary = compute_weekly_summary(&activity_data);

    if let Some(ref summary) = weekly_summary {
        println!("weekly_average (hours per 7-day week):");
        const LABELS: [&str; 5] = ["Sleep", "IN", "LIG", "MOD", "VIG"];
        for (label, value) in LABELS.iter().zip(summary.average_hours.iter()) {
            println!("  {:<5}: {:.2}", label, value);
        }
        println!(
            "weekly_mvpa (minutes per 7-day week): {:.2}",
            summary.weekly_mvpa_minutes
        );
        println!("daily_average (hours per day):");
        for (label, value) in LABELS.iter().zip(summary.daily_average_hours.iter()) {
            println!("  {:<5}: {:.2}", label, value);
        }
        println!(
            "daily_mvpa (minutes per day): {:.2}",
            summary.daily_mvpa_minutes
        );
        println!(
            "daily_sedentary (hours per day, excluding sleep): {:.2}",
            summary.daily_sedentary_hours
        );
        if !summary.average_sleep_by_weekday.is_empty() {
            println!("average_sleep_by_weekday (hours):");
            for (weekday, hours) in &summary.average_sleep_by_weekday {
                println!("  {:<9}: {:.2}", weekday_display_name(*weekday), hours);
            }
        }
    } else {
        println!(
            "Unable to compute weekly or daily averages due to insufficient overlapping data."
        );
    }

    let session = Session {
        share_path,
        subject_number,
        subject_directory,
        activity_data,
        weekly_summary,
    };

    let total_rows: usize = session
        .activity_data
        .values()
        .map(|records| records.len())
        .sum();

    println!(
        "Session ready with {} total day-level rows for downstream aggregation.",
        total_rows
    );

    Ok(())
}

fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let config_file = determine_config_dir()?.join("config.toml");

    if !config_file.exists() {
        return Err("No configuration found. Please run `report-builder init` first.".into());
    }

    let contents = fs::read_to_string(&config_file)?;
    let config: Config = toml::from_str(&contents)?;

    if config.share_path.trim().is_empty() {
        return Err(format!(
            "share_path in {} is empty. Re-run `report-builder init`.",
            config_file.display()
        )
        .into());
    }

    Ok(config)
}

fn prompt_for_subject_number() -> Result<String, io::Error> {
    loop {
        println!("Enter the subject number (four digits starting with 7, 8, or 9):");
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            println!("A subject number is required. Please try again.");
            continue;
        }

        if trimmed.len() != 4 || !trimmed.chars().all(|c| c.is_ascii_digit()) {
            println!("Subject numbers must be a four-digit integer. Please try again.");
            continue;
        }

        match trimmed.chars().next() {
            Some('7') | Some('8') | Some('9') => return Ok(trimmed.to_string()),
            _ => {
                println!("Subject numbers must start with 7, 8, or 9. Please try again.");
            }
        }
    }
}

fn build_subject_directory(
    base_share_path: &Path,
    subject_number: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let first_digit = subject_number
        .chars()
        .next()
        .ok_or("Subject number cannot be empty.")?;

    let (study, dataset) = match first_digit {
        '7' => ("ObservationalStudy", "act-obs-final-test-2"),
        '8' | '9' => ("InterventionStudy", "act-int-final-test-2"),
        _ => return Err(
            "Subject numbers must start with 7, 8, or 9. Validation should have prevented this."
                .into(),
        ),
    };

    let subject_folder = format!("sub-{}", subject_number);

    let path = base_share_path
        .join("Projects")
        .join("BOOST")
        .join(study)
        .join("3-experiment")
        .join("data")
        .join(dataset)
        .join("derivatives")
        .join("GGIR-3.2.6")
        .join(&subject_folder)
        .join("accel");

    Ok(path)
}

fn discover_target_csv(
    subject_directory: &Path,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    const TARGET_FILENAME: &str = "part5_daysummary_MM_L44.8M100.6V428.8_T5A5.csv";
    let mut matches = Vec::new();

    for entry in WalkDir::new(subject_directory)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if entry
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case(TARGET_FILENAME))
            .unwrap_or(false)
        {
            matches.push(entry.into_path());
        }
    }

    Ok(matches)
}

fn collect_activity_metrics(
    files: &[PathBuf],
) -> Result<HashMap<String, Vec<DayMetrics>>, Box<dyn std::error::Error>> {
    let mut matrix: HashMap<String, Vec<DayMetrics>> = HashMap::new();

    for file in files {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .trim(csv::Trim::All)
            .from_path(file)
            .map_err(|err| format!("Failed to open {}: {}", file.display(), err))?;

        let headers = reader
            .headers()
            .map_err(|err| format!("Failed to read headers from {}: {}", file.display(), err))?
            .clone();

        let column_lookup = locate_required_columns(&headers).map_err(|missing| {
            format!(
                "File {} is missing required column(s): {}",
                file.display(),
                missing.join(", ")
            )
        })?;

        for result in reader.records() {
            let record = match result {
                Ok(record) => record,
                Err(err) => {
                    eprintln!(
                        "Skipping row in {} due to read error: {}",
                        file.display(),
                        err
                    );
                    continue;
                }
            };

            if let Some(metrics) = extract_metrics_from_record(file, &record, &column_lookup) {
                let id_key = metrics.id.clone();
                matrix.entry(id_key).or_default().push(metrics);
            }
        }
    }

    Ok(matrix)
}

fn compute_weekly_summary(data: &HashMap<String, Vec<DayMetrics>>) -> Option<WeeklySummary> {
    if data.is_empty() {
        return None;
    }

    let mut valid_groups: Vec<Vec<DayMetrics>> = Vec::new();

    for records in data.values() {
        if records.is_empty() {
            continue;
        }
        valid_groups.push(records.clone());
    }

    if valid_groups.is_empty() {
        return None;
    }

    let min_days = valid_groups
        .iter()
        .map(|records| records.len())
        .min()
        .unwrap_or(0);

    if min_days == 0 {
        return None;
    }

    let days_to_use = min_days.min(7);

    let mut per_id_totals: Vec<([f64; 5], f64)> = Vec::new();
    let mut weekday_sleep_totals: HashMap<Weekday, (f64, usize)> = HashMap::new();

    for mut records in valid_groups {
        sort_metrics_by_date(&mut records);

        let mut totals = [0f64; 5];
        let mut mvpa_minutes = 0f64;
        for day in records.into_iter().take(days_to_use) {
            let sleep_hours = day.sleep_minutes / 60.0;
            totals[0] += sleep_hours;
            totals[1] += day.total_in_min / 60.0;
            totals[2] += day.total_lig_min / 60.0;
            totals[3] += day.total_mod_min / 60.0;
            totals[4] += day.total_vig_min / 60.0;
            mvpa_minutes += day.total_mod_min + day.total_vig_min;

            if let Some(weekday) = determine_weekday(&day) {
                let entry = weekday_sleep_totals.entry(weekday).or_insert((0.0, 0));
                entry.0 += sleep_hours;
                entry.1 += 1;
            }
        }

        per_id_totals.push((totals, mvpa_minutes));
    }

    if per_id_totals.is_empty() {
        return None;
    }

    let mut weekly_average = [0f64; 5];
    let mut weekly_mvpa_minutes = 0f64;
    for (totals, mvpa_minutes) in &per_id_totals {
        for (slot, value) in weekly_average.iter_mut().zip(totals.iter()) {
            *slot += value;
        }
        weekly_mvpa_minutes += mvpa_minutes;
    }

    let participant_count = per_id_totals.len() as f64;
    for value in weekly_average.iter_mut() {
        *value /= participant_count;
        *value *= 7.0 / days_to_use as f64;
    }

    weekly_mvpa_minutes /= participant_count;
    weekly_mvpa_minutes *= 7.0 / days_to_use as f64;

    let mut daily_average_hours = weekly_average;
    for value in daily_average_hours.iter_mut() {
        *value /= 7.0;
    }
    let daily_mvpa_minutes = weekly_mvpa_minutes / 7.0;
    // Interpret inactivity (IN) as sedentary and subtract sleep to avoid double counting.
    let daily_sedentary_hours = (daily_average_hours[1] - daily_average_hours[0]).max(0.0);

    let mut average_sleep_by_weekday = Vec::new();
    const WEEKDAY_ORDER: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];
    for weekday in WEEKDAY_ORDER.iter() {
        if let Some((total, count)) = weekday_sleep_totals.get(weekday) {
            if *count > 0 {
                average_sleep_by_weekday.push((*weekday, total / *count as f64));
            }
        }
    }

    Some(WeeklySummary {
        average_hours: weekly_average,
        weekly_mvpa_minutes,
        daily_average_hours,
        daily_mvpa_minutes,
        daily_sedentary_hours,
        average_sleep_by_weekday,
    })
}

fn sort_metrics_by_date(records: &mut Vec<DayMetrics>) {
    records.sort_by(|a, b| compare_metrics(a, b));
}

fn compare_metrics(a: &DayMetrics, b: &DayMetrics) -> Ordering {
    let a_date = parse_calendar_date(&a.calendar_date);
    let b_date = parse_calendar_date(&b.calendar_date);

    match (a_date, b_date) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.calendar_date.cmp(&b.calendar_date),
    }
}

fn parse_calendar_date(value: &str) -> Option<NaiveDate> {
    const FORMATS: [&str; 3] = ["%Y-%m-%d", "%m/%d/%Y", "%d/%m/%Y"];
    for format in FORMATS.iter() {
        if let Ok(date) = NaiveDate::parse_from_str(value, format) {
            return Some(date);
        }
    }
    None
}

fn determine_weekday(day: &DayMetrics) -> Option<Weekday> {
    if let Some(date) = parse_calendar_date(&day.calendar_date) {
        return Some(date.weekday());
    }
    parse_weekday_name(&day.weekday)
}

fn parse_weekday_name(value: &str) -> Option<Weekday> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn weekday_display_name(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

struct ColumnLookup {
    id: usize,
    calendar_date: usize,
    weekday: usize,
    total_durations: [usize; 4],
    sleep_minutes: usize,
}

fn locate_required_columns(headers: &StringRecord) -> Result<ColumnLookup, Vec<String>> {
    const DURATION_VARIANTS: [&str; 4] = ["IN", "LIG", "MOD", "VIG"];

    let mut missing = Vec::new();

    let id = find_index(headers, "ID", &mut missing);
    let calendar_date = find_index(headers, "calendar_date", &mut missing);
    let weekday = find_index(headers, "weekday", &mut missing);
    let sleep_minutes = find_index(headers, "dur_spt_min", &mut missing);

    let mut total_durations = [0usize; 4];
    for (slot, variant) in total_durations.iter_mut().zip(DURATION_VARIANTS.iter()) {
        let column_name = format!("dur_day_total_{}_min", variant);
        *slot = find_index(headers, &column_name, &mut missing);
    }

    if missing.is_empty() {
        Ok(ColumnLookup {
            id,
            calendar_date,
            weekday,
            total_durations,
            sleep_minutes,
        })
    } else {
        // Remove duplicates in case of repeated names.
        missing.sort();
        missing.dedup();
        Err(missing)
    }
}

fn find_index(headers: &StringRecord, name: &str, missing: &mut Vec<String>) -> usize {
    match headers.iter().position(|header| header == name) {
        Some(index) => index,
        None => {
            missing.push(name.to_string());
            0
        }
    }
}

fn extract_metrics_from_record(
    file: &Path,
    record: &StringRecord,
    columns: &ColumnLookup,
) -> Option<DayMetrics> {
    const DURATION_VARIANTS: [&str; 4] = ["IN", "LIG", "MOD", "VIG"];

    let id = match required_string_field(record, columns.id, "ID", file) {
        Some(value) => value,
        None => return None,
    };
    let calendar_date =
        match required_string_field(record, columns.calendar_date, "calendar_date", file) {
            Some(value) => value,
            None => return None,
        };
    let weekday = match required_string_field(record, columns.weekday, "weekday", file) {
        Some(value) => value,
        None => return None,
    };

    let mut totals = [0f64; 4];
    for ((slot, &index), variant) in totals
        .iter_mut()
        .zip(columns.total_durations.iter())
        .zip(DURATION_VARIANTS.iter())
    {
        *slot = match parse_f64_field(
            record.get(index),
            &format!("dur_day_total_{}_min", variant),
            file,
        ) {
            Some(value) => value,
            None => return None,
        };
    }

    let sleep_minutes =
        match parse_f64_field(record.get(columns.sleep_minutes), "dur_spt_min", file) {
            Some(value) => value,
            None => return None,
        };

    Some(DayMetrics {
        id,
        calendar_date,
        weekday,
        total_in_min: totals[0],
        total_lig_min: totals[1],
        total_mod_min: totals[2],
        total_vig_min: totals[3],
        sleep_minutes,
    })
}

fn required_string_field(
    record: &StringRecord,
    index: usize,
    column_name: &str,
    file: &Path,
) -> Option<String> {
    match record.get(index) {
        Some(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => {
            eprintln!(
                "Skipping row in {} due to missing value for {}.",
                file.display(),
                column_name
            );
            None
        }
    }
}

fn parse_f64_field(value: Option<&str>, column_name: &str, file: &Path) -> Option<f64> {
    let raw = match value {
        Some(raw) if !raw.trim().is_empty() => raw.trim(),
        _ => {
            eprintln!(
                "Skipping row in {} due to missing value for {}.",
                file.display(),
                column_name
            );
            return None;
        }
    };

    match raw.parse::<f64>() {
        Ok(number) => Some(number),
        Err(err) => {
            eprintln!(
                "Skipping row in {} due to parse error in {}: {}",
                file.display(),
                column_name,
                err
            );
            None
        }
    }
}

use expanduser::expanduser;
use regex::Regex;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process;
use text_io::read;

#[derive(Debug, Clone)]
struct DisplayConfig {
    description: String,
    outputs: Vec<String>, // store RAW lines (with leading '#' etc.)
    status: String,
}

fn main() -> io::Result<()> {
    let config_path = expanduser("~/.config/sway/config").expect("Failed to expand config path");

    // Read all lines from the config file
    let file = File::open(&config_path).expect("Failed to open config file");
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().filter_map(Result::ok).collect();

    // Identify the 'Display Start' and 'Display End' indices
    let display_start = lines
        .iter()
        .position(|line| line.contains("Display Start"))
        .unwrap_or_else(|| {
            eprintln!("Error: 'Display Start' marker not found in the config file.");
            process::exit(1);
        });
    let display_end = lines
        .iter()
        .position(|line| line.contains("Display End"))
        .unwrap_or_else(|| {
            eprintln!("Error: 'Display End' marker not found in the config file.");
            process::exit(1);
        });

    // Extract the display section
    let display_section = &lines[display_start..display_end];

    // Parse the display section into DisplayConfig structs
    let desc_status_regex = Regex::new(r"# Description = ([^,]+), Status = ([^,]+)").unwrap();
    let display_configs = parse_configs(display_section, &desc_status_regex);
    let enabled_config = display_configs
        .iter()
        .position(|c| c.status.eq_ignore_ascii_case("Enabled"));

    // Display current active configuration
    if let Some(enabled_index) = enabled_config {
        println!(
            "Current active configuration: {}",
            display_configs[enabled_index].description
        );
    } else {
        println!("No configuration is currently enabled.");
    }

    // List all available configurations
    println!("\nAvailable display configurations:");
    for (i, config) in display_configs.iter().enumerate() {
        println!("{}. {} [{}]", i + 1, config.description, config.status);
    }

    // Prompt user to select a config
    let selected_index = get_user_selection(display_configs.len());

    // Update display_configs: set selected to Enabled, others to Disabled
    let mut updated_display_configs = display_configs.clone();
    for (i, config) in updated_display_configs.iter_mut().enumerate() {
        if i == selected_index {
            config.status = "Enabled".to_string();
        } else {
            config.status = "Disabled".to_string();
        }
    }

    // Reconstruct the display section with updated configs
    let mut new_display_section = Vec::new();

    for config in &updated_display_configs {
        // Write the description line with updated status
        new_display_section.push(format!(
            "# Description = {}, Status = {}",
            config.description, config.status
        ));

        // Write the output lines, commented or uncommented based on status
        for output_line in &config.outputs {
            let line_to_write = if is_marker_line(output_line) {
                // always keep markers; ensure canonical "#! " form
                canon_marker_line(output_line)
            } else if config.status.eq_ignore_ascii_case("Enabled") {
                // uncomment only plain '#'
                uncomment_plain_hash(output_line)
            } else {
                // comment only plain lines
                comment_plain_hash(output_line)
            };
            new_display_section.push(line_to_write);
        }

        // No blank lines between configurations to prevent extra space
    }

    // Prepare the new lines by replacing the old display section
    let mut new_lines = Vec::new();

    // Add lines before the display section (include the 'Display Start' line)
    new_lines.extend_from_slice(&lines[..=display_start]);

    // Add the new display section
    new_lines.extend(new_display_section);

    // Add lines after the display section (from 'Display End' onward)
    if display_end < lines.len() {
        new_lines.extend_from_slice(&lines[display_end..]);
    }

    // Write all lines to a temporary file
    let temp_path = Path::new("/home/fribbit/.config/sway/config_temp");
    let temp_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(temp_path)
        .expect("Failed to create temporary config file");
    let mut writer = BufWriter::new(temp_file);

    for line in new_lines {
        writeln!(writer, "{}", line)?;
    }

    // Rename the temporary file to replace the old configuration
    fs::rename(temp_path, &config_path).expect("Failed to replace the original config file");

    // Reload Sway configuration
    if process::Command::new("swaymsg")
        .arg("reload")
        .spawn()
        .is_ok()
    {
        println!("Successfully reloaded Sway configuration.");
    } else {
        eprintln!("Failed to reload Sway configuration.");
    }

    Ok(())
}

fn is_marker_line(line: &str) -> bool {
    let (_indent, rest0) = split_indent(line);
    let rest = rest0.trim_start();
    if let Some(after_hash) = rest.strip_prefix('#') {
        after_hash.trim_start().starts_with('!')
    } else {
        rest.starts_with('!')
    }
}

fn canon_marker_line(line: &str) -> String {
    let (indent, rest0) = split_indent(line);
    let mut rest = rest0.trim_start();
    if let Some(after_hash) = rest.strip_prefix('#') {
        rest = after_hash.trim_start();
    }
    if let Some(after_bang) = rest.strip_prefix('!') {
        let text = after_bang.trim_start();
        format!("{}#! {}", indent, text)
    } else {
        line.to_string() // shouldn't happen, but be safe
    }
}

// Parse the display section into DisplayConfig structs
fn parse_configs<'a, I>(lines: I, regex: &Regex) -> Vec<DisplayConfig>
where
    I: IntoIterator<Item = &'a String>,
{
    let mut configs = Vec::new();
    let mut current_config = None;

    for line in lines {
        if let Some(captures) = regex.captures(line) {
            // Push the previous config if it exists
            if let Some(config) = current_config.take() {
                configs.push(config);
            }
            // Start a new config
            current_config = Some(DisplayConfig {
                description: captures[1].trim().to_string(),
                status: captures[2].trim().to_string(),
                outputs: Vec::new(),
            });
        } else if let Some(config) = current_config.as_mut() {
            // Keep RAW lines; skip completely blank ones
            if !line.trim().is_empty() {
                config.outputs.push(line.to_string());
            }
        }
    }

    // Push the last config if it exists
    if let Some(config) = current_config {
        configs.push(config);
    }

    configs
}

// Prompt the user for their configuration choice
fn get_user_selection(total_configs: usize) -> usize {
    loop {
        println!("Enter the number of the configuration you want to activate, or 'q' to quit:");
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input");
        let trimmed = input.trim();
        if trimmed.eq_ignore_ascii_case("q") {
            println!("Exiting without making changes.");
            std::process::exit(0);
        }
        if let Ok(choice) = trimmed.parse::<usize>() {
            if choice > 0 && choice <= total_configs {
                return choice - 1;
            }
        }
        println!(
            "Invalid selection. Please enter a number between 1 and {}, or 'q' to quit.",
            total_configs
        );
    }
}

/* ------------------------- helpers ------------------------- */

// Split into (indentation, rest starting at first non-space char)
fn split_indent(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    (&s[..i], &s[i..])
}

// Uncomment a line ONLY if it begins with a plain '#' (not '#!') after indentation.
fn uncomment_plain_hash(line: &str) -> String {
    if is_marker_line(line) {
        return canon_marker_line(line); // keep/comment as "#! ..." if needed
    }
    let (indent, rest) = split_indent(line);
    if let Some(after_hash) = rest.strip_prefix('#') {
        let rem = after_hash.strip_prefix(' ').unwrap_or(after_hash);
        format!("{}{}", indent, rem)
    } else {
        line.to_string()
    }
}

// Comment a line with a single '# ' after indentation (idempotent).
// Do not change lines that start with '#!' (caller guards, but we double-check).
fn comment_plain_hash(line: &str) -> String {
    if is_marker_line(line) {
        return canon_marker_line(line);
    }
    let (indent, rest) = split_indent(line);
    if rest.starts_with('#') {
        let collapsed = rest.trim_start_matches('#').trim_start();
        format!("{}# {}", indent, collapsed)
    } else {
        format!("{}# {}", indent, rest)
    }
}

use expanduser::expanduser;
use regex::Regex;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process;

// ANSI color codes using terminal's color scheme
const COLOR_RESET: &str = "\x1B[0m";
const COLOR_BLUE: &str = "\x1B[34m";        // Blue for groups
const COLOR_CYAN: &str = "\x1B[36m";        // Cyan (light blue) for configs in groups
const COLOR_GREEN: &str = "\x1B[32m";       // Green for enabled
const COLOR_RED: &str = "\x1B[31m";         // Red for disabled
const COLOR_YELLOW: &str = "\x1B[33m";      // Yellow for group label

#[derive(Debug, Clone)]
struct DisplayConfig {
    description: String,
    outputs: Vec<String>, // store RAW lines (with leading '#' etc.)
    status: String,
    group: Option<String>, // group name if this config belongs to a group
}

#[derive(Debug, Clone)]
struct Group {
    name: String,
    expanded: bool,
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
    let group_regex = Regex::new(r"## Group = (.+) ##").unwrap();
    let end_group_regex = Regex::new(r"## End Group ##").unwrap();
    let display_configs = parse_configs(display_section, &desc_status_regex, &group_regex, &end_group_regex);
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

    // Extract unique groups
    let mut groups: Vec<Group> = Vec::new();
    for config in &display_configs {
        if let Some(group_name) = &config.group {
            if !groups.iter().any(|g| g.name == *group_name) {
                groups.push(Group {
                    name: group_name.clone(),
                    expanded: false,
                });
            }
        }
    }

    // Interactive loop for displaying and selecting configs
    let selected_index = interactive_selection(&display_configs, &mut groups);

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
    // We need to preserve the original structure including group markers
    let mut new_display_section = Vec::new();
    let mut last_group: Option<String> = None;

    for config in &updated_display_configs {
        // Write group marker if this config starts a new group
        if let Some(group_name) = &config.group {
            if last_group.as_ref() != Some(group_name) {
                // End the previous group if there was one
                if last_group.is_some() {
                    new_display_section.push("## End Group ##".to_string());
                }
                new_display_section.push(format!("## Group = {} ##", group_name));
                last_group = Some(group_name.clone());
            }
        } else {
            // This config is not in a group, so end the previous group if there was one
            if last_group.is_some() {
                new_display_section.push("## End Group ##".to_string());
                last_group = None;
            }
        }

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

    // Close any remaining open group at the end
    if last_group.is_some() {
        new_display_section.push("## End Group ##".to_string());
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
fn parse_configs<'a, I>(lines: I, regex: &Regex, group_regex: &Regex, end_group_regex: &Regex) -> Vec<DisplayConfig>
where
    I: IntoIterator<Item = &'a String>,
{
    let mut configs = Vec::new();
    let mut current_config = None;
    let mut current_group: Option<String> = None;
    let mut group_start_line: Option<usize> = None;
    let mut primary_monitor_depth = 0;
    let mut primary_monitor_start_line: Option<usize> = None;

    let primary_start_regex = Regex::new(r"#!\s*Primary Monitor Start\s*!#").unwrap();
    let primary_end_regex = Regex::new(r"#!\s*Primary Monitor End\s*!#").unwrap();

    let lines_vec: Vec<&String> = lines.into_iter().collect();
    for (line_idx, line) in lines_vec.iter().enumerate() {
        // Check for Primary Monitor markers
        if primary_start_regex.is_match(line) {
            if primary_monitor_depth == 0 {
                primary_monitor_start_line = Some(line_idx);
            }
            primary_monitor_depth += 1;
        } else if primary_end_regex.is_match(line) {
            primary_monitor_depth -= 1;
            if primary_monitor_depth < 0 {
                eprintln!("Warning: Found 'Primary Monitor End' at line {} without matching 'Primary Monitor Start'", line_idx + 1);
                primary_monitor_depth = 0;
            }
        }

        // Check for end group marker
        if end_group_regex.is_match(line) {
            if current_group.is_none() {
                eprintln!("Warning: Found '## End Group ##' at line {} without matching '## Group = <name> ##'", line_idx + 1);
            }
            // Push the previous config if it exists
            if let Some(config) = current_config.take() {
                configs.push(config);
            }
            // Clear the current group
            current_group = None;
            group_start_line = None;
        } else if let Some(captures) = group_regex.captures(line) {
            // Check if previous group was closed
            if let Some(group_name) = &current_group {
                eprintln!("Warning: Group '{}' started at line {} was never closed with '## End Group ##'",
                    group_name, group_start_line.unwrap_or(0) + 1);
            }
            // Push the previous config if it exists
            if let Some(config) = current_config.take() {
                configs.push(config);
            }
            // Set the current group
            current_group = Some(captures[1].trim().to_string());
            group_start_line = Some(line_idx);
        } else if let Some(captures) = regex.captures(line) {
            // Push the previous config if it exists
            if let Some(config) = current_config.take() {
                configs.push(config);
            }
            // Start a new config
            current_config = Some(DisplayConfig {
                description: captures[1].trim().to_string(),
                status: captures[2].trim().to_string(),
                outputs: Vec::new(),
                group: current_group.clone(),
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

    // Final validation checks
    if let Some(group_name) = &current_group {
        eprintln!("Warning: Group '{}' started at line {} was never closed with '## End Group ##'",
            group_name, group_start_line.unwrap_or(0) + 1);
    }

    if primary_monitor_depth > 0 {
        eprintln!("Warning: 'Primary Monitor Start' at line {} was never closed with 'Primary Monitor End'",
            primary_monitor_start_line.unwrap_or(0) + 1);
    }

    configs
}

// Interactive selection with expandable/collapsible groups
fn interactive_selection(display_configs: &[DisplayConfig], groups: &mut [Group]) -> usize {
    let mut lines_printed = 0;
    loop {
        // Move cursor up and clear the lines we printed last time
        if lines_printed > 0 {
            for _ in 0..lines_printed {
                print!("\x1B[1A\x1B[2K"); // Move up one line and clear it
            }
        }

        // Count lines as we print
        let mut current_lines = 0;

        // Display configurations with groups
        println!("\nAvailable display configurations:");
        current_lines += 2; // "\n" creates a line, plus the header line

        let mut display_index = 1;
        let mut config_map: Vec<usize> = Vec::new(); // maps display index to config index
        let mut group_map: Vec<String> = Vec::new(); // maps display index to group name (for groups)

        for config in display_configs {
            if let Some(group_name) = &config.group {
                // This config belongs to a group
                let group = groups.iter().find(|g| g.name == *group_name).unwrap();

                // Check if this is the first config in the group
                let is_first_in_group = display_configs
                    .iter()
                    .position(|c| c.group.as_ref() == Some(group_name))
                    == display_configs.iter().position(|c| c.description == config.description);

                if is_first_in_group {
                    // Display the group header in blue
                    println!("{}: {}{}{} {}[Group]{}",
                        display_index, COLOR_BLUE, group_name, COLOR_RESET,
                        COLOR_YELLOW, COLOR_RESET);
                    current_lines += 1;
                    group_map.push(group_name.clone());
                    config_map.push(usize::MAX); // sentinel value for group headers
                    display_index += 1;

                    if group.expanded {
                        // Show all configs in this group
                        let group_configs: Vec<(usize, &DisplayConfig)> = display_configs
                            .iter()
                            .enumerate()
                            .filter(|(_, c)| c.group.as_ref() == Some(group_name))
                            .collect();

                        for (sub_idx, (config_idx, gc)) in group_configs.iter().enumerate() {
                            // Configs in groups are cyan, status is colored
                            let status_color = if gc.status.eq_ignore_ascii_case("Enabled") {
                                COLOR_GREEN
                            } else {
                                COLOR_RED
                            };
                            println!("  {}.{}: {}{}{} [{}{}{}]",
                                display_index - 1, sub_idx + 1,
                                COLOR_CYAN, gc.description, COLOR_RESET,
                                status_color, gc.status, COLOR_RESET);
                            current_lines += 1;
                            config_map.push(*config_idx);
                        }
                    }
                }
            } else {
                // Standalone config (not in a group)
                let status_color = if config.status.eq_ignore_ascii_case("Enabled") {
                    COLOR_GREEN
                } else {
                    COLOR_RED
                };
                println!("{}: {} [{}{}{}]",
                    display_index, config.description,
                    status_color, config.status, COLOR_RESET);
                current_lines += 1;
                config_map.push(display_configs.iter().position(|c| c.description == config.description).unwrap());
                group_map.push(String::new());
                display_index += 1;
            }
        }

        // Prompt for selection
        println!("\nEnter the number to activate a configuration, or the group number to expand/collapse, or 'q' to quit:");
        current_lines += 2; // "\n" plus the prompt line

        lines_printed = current_lines;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input");
        lines_printed += 1; // Count the line where user typed their input
        let trimmed = input.trim();

        if trimmed.eq_ignore_ascii_case("q") {
            println!("Exiting without making changes.");
            std::process::exit(0);
        }

        // Check if it's a sub-config selection (e.g., "2.1")
        if trimmed.contains('.') {
            let parts: Vec<&str> = trimmed.split('.').collect();
            if parts.len() == 2 {
                if let (Ok(group_num), Ok(sub_num)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                    // Find the group at this position
                    if group_num > 0 && group_num <= group_map.len() && !group_map[group_num - 1].is_empty() {
                        let group_name = &group_map[group_num - 1];
                        let group_configs: Vec<usize> = display_configs
                            .iter()
                            .enumerate()
                            .filter(|(_, c)| c.group.as_ref() == Some(group_name))
                            .map(|(idx, _)| idx)
                            .collect();

                        if sub_num > 0 && sub_num <= group_configs.len() {
                            return group_configs[sub_num - 1];
                        }
                    }
                }
            }
            println!("Invalid selection. Please try again.");
            continue;
        }

        // Regular number selection
        if let Ok(choice) = trimmed.parse::<usize>() {
            if choice > 0 && choice <= config_map.len() {
                let selected = config_map[choice - 1];

                // Check if this is a group header
                if selected == usize::MAX {
                    // Toggle group expansion
                    let group_name = &group_map[choice - 1];
                    if let Some(group) = groups.iter_mut().find(|g| g.name == *group_name) {
                        group.expanded = !group.expanded;
                    }
                    continue; // Re-display the menu
                } else {
                    // This is a valid config selection
                    return selected;
                }
            }
        }

        println!("Invalid selection. Please try again.");
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

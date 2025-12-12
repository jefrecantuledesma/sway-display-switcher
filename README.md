# sway-display-switcher

A Rust CLI tool for quickly switching between display configurations in Sway without editing config files manually.

## What Does This Tool Do?

If you use Sway with multiple monitor setups (laptop + external display, docking station, presentations, etc.), you know the pain of commenting/uncommenting different `output` configurations in your Sway config. This tool gives you an interactive menu to switch between predefined display configs with a single command.

**The workflow:**
1. Run the tool
2. Pick a configuration from the menu
3. Sway automatically reloads with your chosen setup

No more opening your config file, scrolling to find the display section, commenting out the old config, uncommenting the new one, saving, and manually running `swaymsg reload`. Just pick a number and go.

## Installation

**Requirements:**
- Rust/Cargo
- Sway window manager
- `swaymsg` available in PATH

**Build and install:**
```bash
cargo build --release
sudo cp target/release/sway-display-switcher /usr/local/bin/
```

Or just run it from the project directory:
```bash
cargo run
```

## Configuration Format

The tool modifies your `~/.config/sway/config` file. You need to mark a section with special comments that define where your display configurations live:

```
# Display Start
# Description = Laptop Only, Status = Enabled
output eDP-1 resolution 1920x1080 position 0,0

# Description = External Monitor, Status = Disabled
# output eDP-1 disable
# output DP-1 resolution 2560x1440 position 0,0

# Description = Dual Monitors, Status = Disabled
# output eDP-1 resolution 1920x1080 position 0,0
# output DP-1 resolution 2560x1440 position 1920,0
# Display End
```

**Key points:**
- Wrap your display configs between `# Display Start` and `# Display End`
- Each config needs a `# Description = Name, Status = Enabled|Disabled` line
- Disabled configs should have `#` in front of each `output` line
- Only one config should be `Enabled` at a time

## Grouping Configurations

You can organize related configs into collapsible groups:

```
# Display Start
## Group = Work Setups ##
# Description = Desk Setup, Status = Disabled
# output eDP-1 disable
# output DP-1 resolution 2560x1440

# Description = Meeting Room, Status = Disabled
# output HDMI-1 resolution 1920x1080
## End Group ##

# Description = Laptop Only, Status = Enabled
output eDP-1 resolution 1920x1080
# Display End
```

Groups appear as a single menu item. Select the group number to expand/collapse it, then use `group.subconfig` notation (like `2.1`) to select a config from within the group.

## Special Markers

If you have output lines that should *never* be uncommented (like markers for other tools or conditional logic), use the `#!` prefix:

```
#! Primary Monitor Start !#
output DP-1 primary
#! Primary Monitor End !#
```

Lines with `#!` are preserved exactly as-is and won't be uncommented even when the config is enabled.

## Usage

Just run the tool:
```bash
sway-display-switcher
```

You'll see something like:
```
Current active configuration: Laptop Only

Available display configurations:
1: Laptop Only [Enabled]
2: External Monitor [Disabled]
3: Dual Monitors [Disabled]

Enter the number to activate a configuration, or 'q' to quit:
```

Type the number of the config you want, press Enter, and Sway reloads automatically.

## How It Works

1. Reads your Sway config file
2. Parses everything between `# Display Start` and `# Display End`
3. Shows you all available configs with their current status
4. When you select one:
   - Sets that config to `Enabled`, all others to `Disabled`
   - Uncomments the enabled config's `output` lines
   - Comments out all disabled configs' `output` lines
   - Writes changes to a temp file, atomically replaces the original
   - Runs `swaymsg reload`

The tool preserves all your formatting, indentation, and other config sections. It only modifies the display section.

## Troubleshooting

**"Display Start marker not found"**
- You need to add `# Display Start` and `# Display End` markers to your config file wrapping your display configurations

**"Failed to reload Sway configuration"**
- Make sure `swaymsg` is installed and in your PATH
- Check that you're running this inside a Sway session

**Changes aren't taking effect**
- Verify your output device names (`swaymsg -t get_outputs` shows available outputs)
- Check for syntax errors in your output commands
- Look at Sway's logs: `journalctl -u sway -f`

**The tool crashes or behaves unexpectedly**
- Ensure your `# Description = Name, Status = Value` lines are formatted correctly
- If using groups, make sure each `## Group = Name ##` has a matching `## End Group ##`
- Check that marker lines use `#!` not just `#`

## License

GPL-3.0

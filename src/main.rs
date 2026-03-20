#[cfg(any(target_os = "macos", target_os = "windows"))]
mod profile;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("This DDC/CI controller currently supports macOS and Windows.");
    std::process::exit(1);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod app {
    use crate::profile::{
        controller_name, controls_for, detect_profile, feature_name, observed_codes,
        resolve_feature, resolve_value, value_label,
    };
    use clap::{Parser, Subcommand};
    use ddc_hi::{Ddc, Display};
    use std::fmt;
    use std::num::ParseIntError;
    use std::process;
    use std::str::FromStr;

    #[derive(Parser, Debug)]
    #[command(name = "monitor-helper")]
    #[command(about = "Built-in macOS and Windows DDC/CI monitor controller")]
    pub struct Cli {
        #[command(subcommand)]
        command: Command,
    }

    #[derive(Subcommand, Debug)]
    enum Command {
        /// List external monitors that expose DDC/CI
        List,
        /// Read a VCP feature value from a monitor
        Get {
            #[arg(short, long, default_value_t = 1)]
            display: usize,
            #[arg(value_name = "FEATURE")]
            feature: FeatureArg,
        },
        /// Write a VCP feature value to a monitor
        Set {
            #[arg(short, long, default_value_t = 1)]
            display: usize,
            #[arg(value_name = "FEATURE")]
            feature: FeatureArg,
            #[arg(value_name = "VALUE")]
            value: String,
        },
        /// Show the detected specialized controller for a monitor
        Profile {
            #[arg(short, long, default_value_t = 1)]
            display: usize,
        },
        /// Scan a range of VCP feature codes and print readable entries
        Scan {
            #[arg(short, long, default_value_t = 1)]
            display: usize,
            #[arg(long, value_name = "CODE", default_value = "0x00", value_parser = parse_u8_arg)]
            start: u8,
            #[arg(long, value_name = "CODE", default_value = "0xFF", value_parser = parse_u8_arg)]
            end: u8,
            #[arg(long)]
            show_failures: bool,
        },
    }

    #[derive(Clone, Debug)]
    struct FeatureArg {
        raw: String,
    }

    impl FeatureArg {
        fn as_str(&self) -> &str {
            &self.raw
        }
    }

    impl fmt::Display for FeatureArg {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.raw)
        }
    }

    impl FromStr for FeatureArg {
        type Err = String;

        fn from_str(input: &str) -> Result<Self, Self::Err> {
            let normalized = input.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                return Err("Feature name cannot be empty.".to_string());
            }
            Ok(Self { raw: normalized })
        }
    }

    fn parse_feature_code(input: &str) -> Result<u8, ParseIntError> {
        if let Some(hex) = input.strip_prefix("0x") {
            u8::from_str_radix(hex, 16)
        } else {
            input.parse::<u8>()
        }
    }

    fn parse_u8_arg(input: &str) -> Result<u8, String> {
        parse_feature_code(&input.trim().to_ascii_lowercase()).map_err(|_| {
            format!("Invalid VCP code: {input}. Use decimal or hex such as 16 or 0x10.")
        })
    }

    #[derive(Debug)]
    struct MonitorSummary {
        index: usize,
        description: String,
        controller: String,
        backend: String,
        identifier: String,
        product_name: Option<String>,
        serial_number: Option<String>,
    }

    fn enumerate_summaries() -> Result<Vec<MonitorSummary>, String> {
        Ok(Display::enumerate()
            .into_iter()
            .enumerate()
            .map(|(index, display)| MonitorSummary {
                index: index + 1,
                description: display.info.to_string(),
                controller: controller_name(&display.info).to_string(),
                backend: display.info.backend.to_string(),
                identifier: display.info.id,
                product_name: display.info.model_name,
                serial_number: display.info.serial_number,
            })
            .collect())
    }

    fn open_display(display_index: usize) -> Result<Display, String> {
        let mut monitors = Display::enumerate();

        if monitors.is_empty() {
            return Err("No DDC/CI-capable external monitors were found.".to_string());
        }

        let zero_based = display_index
            .checked_sub(1)
            .ok_or_else(|| "Display indices start at 1.".to_string())?;

        if zero_based >= monitors.len() {
            return Err(format!(
                "Display {} does not exist. Run `monitor-helper list` to see valid indices.",
                display_index
            ));
        }

        Ok(monitors.swap_remove(zero_based))
    }

    fn print_monitors() -> Result<(), String> {
        let monitors = enumerate_summaries()?;

        if monitors.is_empty() {
            println!("No DDC/CI-capable external monitors found.");
            return Ok(());
        }

        for monitor in monitors {
            println!("[{}] {}", monitor.index, monitor.description);
            println!("  controller: {}", monitor.controller);
            println!("  backend: {}", monitor.backend);
            println!("  id: {}", monitor.identifier);
            if let Some(product_name) = monitor.product_name {
                println!("  name: {}", product_name);
            }
            if let Some(serial_number) = monitor.serial_number {
                println!("  serial: {}", serial_number);
            }
        }

        Ok(())
    }

    fn get_feature(display: usize, feature: FeatureArg) -> Result<(), String> {
        let mut monitor = open_display(display)?;
        let profile = detect_profile(&monitor.info);
        let feature = resolve_feature(profile, feature.as_str())?;
        let value = monitor
            .handle
            .get_vcp_feature(feature.code)
            .map_err(|err| format!("Failed to read {} from display {}: {err}", feature, display))?;

        println!("display={display}");
        println!("controller={}", controller_name(&monitor.info));
        println!("feature={} (0x{:02X})", feature.name, feature.code);
        println!("current={}", value.value());
        println!("maximum={}", value.maximum());
        if let Some(label) = value_label(feature.spec, value.value()) {
            println!("current_label={label}");
        }

        Ok(())
    }

    fn set_feature(display: usize, feature: FeatureArg, value: String) -> Result<(), String> {
        let mut monitor = open_display(display)?;
        let profile = detect_profile(&monitor.info);
        let feature = resolve_feature(profile, feature.as_str())?;
        let value = resolve_value(feature.spec, &value)?;

        if let Some(spec) = feature.spec {
            if !spec.writable {
                return Err(format!(
                    "Feature {} (0x{:02X}) is read-only.",
                    spec.name, spec.code
                ));
            }
        }

        monitor
            .handle
            .set_vcp_feature(feature.code, value)
            .map_err(|err| {
                format!(
                    "Failed to set {} on display {} to {}: {err}",
                    feature, display, value
                )
            })?;

        println!("display={display}");
        println!("controller={}", controller_name(&monitor.info));
        println!("feature={} (0x{:02X})", feature.name, feature.code);
        println!("set={value}");
        if let Some(label) = value_label(feature.spec, value) {
            println!("set_label={label}");
        }

        Ok(())
    }

    fn print_profile(display: usize) -> Result<(), String> {
        let mut monitor = open_display(display)?;
        let _ = monitor.update_capabilities();
        let _ = monitor.update_from_ddc();
        let profile = detect_profile(&monitor.info);

        println!("display={display}");
        println!("controller={}", controller_name(&monitor.info));
        println!("description={}", monitor.info);

        if let Some(model_name) = monitor.info.model_name.as_deref() {
            println!("model={model_name}");
        }

        if let Some(profile) = profile {
            println!("profile_key={}", profile.key);
        } else {
            println!("profile_key=generic");
        }

        println!("controls:");
        for feature in controls_for(profile) {
            println!(
                "  {} 0x{:02X} writable={}",
                feature.name, feature.code, feature.writable
            );
            if !feature.value_options.is_empty() {
                let values = feature
                    .value_options
                    .iter()
                    .map(|option| format!("{}={}", option.label, option.value))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("    values: {values}");
            }
        }

        let observed = observed_codes(profile);
        if !observed.is_empty() {
            let codes = observed
                .iter()
                .map(|code| format!("0x{code:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("observed_readable_codes={codes}");
        }

        Ok(())
    }

    fn scan_features(
        display: usize,
        start: u8,
        end: u8,
        show_failures: bool,
    ) -> Result<(), String> {
        if start > end {
            return Err(format!(
                "Invalid scan range: start 0x{start:02X} is greater than end 0x{end:02X}."
            ));
        }

        let mut monitor = open_display(display)?;
        let _ = monitor.update_capabilities();
        let _ = monitor.update_from_ddc();
        let profile = detect_profile(&monitor.info);

        println!("display={display}");
        println!("controller={}", controller_name(&monitor.info));
        println!("scan_range=0x{start:02X}-0x{end:02X}");

        let mut readable = 0usize;
        let mut failures = 0usize;

        for code in start..=end {
            match monitor.handle.get_vcp_feature(code) {
                Ok(value) => {
                    readable += 1;
                    let resolved_name = feature_name(profile, code);
                    let resolved_spec = resolve_feature(profile, &format!("0x{code:02X}"))
                        .ok()
                        .and_then(|feature| feature.spec);

                    if let Some(label) = value_label(resolved_spec, value.value()) {
                        println!(
                            "0x{code:02X} name={resolved_name} current={} current_label={} maximum={}",
                            value.value(),
                            label,
                            value.maximum()
                        );
                    } else {
                        println!(
                            "0x{code:02X} name={resolved_name} current={} maximum={}",
                            value.value(),
                            value.maximum()
                        );
                    }
                }
                Err(err) => {
                    failures += 1;
                    if show_failures {
                        println!("0x{code:02X} error={err}");
                    }
                }
            }
        }

        println!("readable={readable}");
        println!("unreadable={failures}");

        Ok(())
    }

    pub fn run() -> Result<(), String> {
        let cli = Cli::parse();

        match cli.command {
            Command::List => print_monitors(),
            Command::Get { display, feature } => get_feature(display, feature),
            Command::Set {
                display,
                feature,
                value,
            } => set_feature(display, feature, value),
            Command::Profile { display } => print_profile(display),
            Command::Scan {
                display,
                start,
                end,
                show_failures,
            } => scan_features(display, start, end, show_failures),
        }
    }

    pub fn main() {
        if let Err(err) = run() {
            eprintln!("{err}");
            process::exit(1);
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn main() {
    app::main();
}

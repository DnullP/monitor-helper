#[cfg(any(target_os = "macos", target_os = "windows"))]
mod profile;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod usb;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("This DDC/CI controller currently supports macOS and Windows.");
    std::process::exit(1);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod app {
    use crate::profile::{
        controller_name, controls_for, detect_profile, feature_name, observed_codes,
        resolve_feature, resolve_value, value_label, write_options, write_value_label,
    };
    use clap::{Args, Parser, Subcommand};
    use ddc_hi::{Ddc, Display};
    use std::fmt;
    use std::num::ParseIntError;
    use std::process::{self, Command as ProcessCommand, Stdio};
    use std::str::FromStr;
    use std::thread;
    use std::time::{Duration, Instant};

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
        /// Scan USB buses and connected USB devices
        Usb {
            #[command(subcommand)]
            command: UsbCommand,
        },
        /// Read a VCP feature value from a monitor
        Get {
            #[command(flatten)]
            selector: DisplaySelector,
            #[arg(value_name = "FEATURE")]
            feature: FeatureArg,
        },
        /// Write a VCP feature value to a monitor
        Set {
            #[command(flatten)]
            selector: DisplaySelector,
            #[arg(value_name = "FEATURE")]
            feature: FeatureArg,
            #[arg(value_name = "VALUE")]
            value: String,
        },
        /// Show the detected specialized controller for a monitor
        Profile {
            #[command(flatten)]
            selector: DisplaySelector,
        },
        /// Scan a range of VCP feature codes and print readable entries
        Scan {
            #[command(flatten)]
            selector: DisplaySelector,
            #[arg(long, value_name = "CODE", default_value = "0x00", value_parser = parse_u8_arg)]
            start: u8,
            #[arg(long, value_name = "CODE", default_value = "0xFF", value_parser = parse_u8_arg)]
            end: u8,
            #[arg(long, default_value_t = 1500)]
            timeout_ms: u64,
            #[arg(long)]
            show_failures: bool,
        },
        #[command(hide = true)]
        Probe {
            #[command(flatten)]
            selector: DisplaySelector,
            #[arg(value_name = "FEATURE")]
            feature: FeatureArg,
        },
    }

    #[derive(Subcommand, Debug)]
    enum UsbCommand {
        /// Enumerate USB buses and devices, including devices attached through USB-C paths
        Scan,
        /// Watch USB device connect and disconnect events and print what changed
        Watch,
    }

    #[derive(Args, Clone, Debug)]
    struct DisplaySelector {
        #[arg(long)]
        all: bool,
        #[arg(short, long)]
        display: Option<usize>,
        #[arg(long, value_name = "ID")]
        id: Option<String>,
        #[arg(long, value_name = "TEXT")]
        name: Option<String>,
    }

    impl DisplaySelector {
        fn describe(&self) -> String {
            if self.all {
                "all displays".to_string()
            } else if let Some(display) = self.display {
                format!("display index {display}")
            } else if let Some(id) = self.id.as_deref() {
                format!("display id {id}")
            } else if let Some(name) = self.name.as_deref() {
                format!("display name match {name}")
            } else {
                "display index 1".to_string()
            }
        }
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

    fn selected_displays(selector: &DisplaySelector) -> Result<Vec<(usize, Display)>, String> {
        let monitors = Display::enumerate();

        if monitors.is_empty() {
            return Err("No DDC/CI-capable external monitors were found.".to_string());
        }

        let matches = monitors
            .into_iter()
            .enumerate()
            .filter(|(index, display)| selector_matches(selector, index + 1, display))
            .collect::<Vec<_>>();

        if matches.is_empty() {
            Err(format!(
                "No monitor matched {}. Run `monitor-helper list` to inspect available displays.",
                selector.describe()
            ))
        } else {
            Ok(matches
                .into_iter()
                .map(|(index, display)| (index + 1, display))
                .collect())
        }
    }

    fn open_display(selector: &DisplaySelector) -> Result<(usize, Display), String> {
        let matches = selected_displays(selector)?;

        if selector.all {
            return Err(
                "This operation expects a single display, but --all was provided.".to_string(),
            );
        }

        match matches.len() {
            1 => Ok(matches.into_iter().next().expect("single match")),
            _ => {
                let options = matches
                    .iter()
                    .map(|(index, display)| format!("[{}] {}", index, display.info))
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(format!(
                    "Multiple monitors matched {}: {}. Use --display or --id for a unique selection.",
                    selector.describe(),
                    options
                ))
            }
        }
    }

    fn selector_matches(selector: &DisplaySelector, index: usize, display: &Display) -> bool {
        let display_matches = selector.display.map(|value| value == index).unwrap_or(true);
        let id_matches = selector
            .id
            .as_deref()
            .map(|value| display.info.id.eq_ignore_ascii_case(value))
            .unwrap_or(true);
        let name_matches = selector
            .name
            .as_deref()
            .map(|value| {
                let needle = value.to_ascii_lowercase();
                let description = display.info.to_string().to_ascii_lowercase();
                let id = display.info.id.to_ascii_lowercase();
                let model_name = display
                    .info
                    .model_name
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let controller = controller_name(&display.info).to_ascii_lowercase();

                description.contains(&needle)
                    || id.contains(&needle)
                    || model_name.contains(&needle)
                    || controller.contains(&needle)
            })
            .unwrap_or(true);

        let no_explicit_selector = !selector.all
            && selector.display.is_none()
            && selector.id.is_none()
            && selector.name.is_none();

        if no_explicit_selector {
            index == 1
        } else {
            display_matches && id_matches && name_matches
        }
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

    fn print_value(
        display: usize,
        monitor: &Display,
        feature: &crate::profile::ResolvedFeature,
        value: ddc_hi::VcpValue,
    ) {
        let decoded = decode_current_value(feature, value.value());
        let writeback = current_writeback_value(feature, value.value(), &decoded);
        println!("display={display}");
        println!("controller={}", controller_name(&monitor.info));
        println!("feature={} (0x{:02X})", feature.name, feature.code);
        println!("current={}", value.value());
        println!("maximum={}", value.maximum());
        if let Some(current_hex) = decoded.current_hex {
            println!("current_hex={current_hex}");
        }
        if let Some(high_byte) = decoded.current_high_byte {
            println!("current_high_byte={high_byte}");
        }
        if let Some(low_byte) = decoded.current_low_byte {
            println!("current_low_byte={low_byte}");
        }
        if let Some(label) = value_label(feature.spec, value.value()) {
            println!("current_label={label}");
        }
        if let Some(decoded_current) = decoded.decoded_current {
            println!("decoded_current={decoded_current}");
        }
        if let Some(decoded_current_source) = decoded.decoded_current_source {
            println!("decoded_current_source={decoded_current_source}");
        }
        if let Some(decoded_current_label) = decoded.decoded_current_label {
            println!("decoded_current_label={decoded_current_label}");
        }
        if let Some(writeback_value) = writeback.value {
            println!("writeback_value={writeback_value}");
        }
        if let Some(writeback_source) = writeback.source {
            println!("writeback_source={writeback_source}");
        }
        if let Some(writeback_label) = writeback.label {
            println!("writeback_label={writeback_label}");
        }
        println!("writeback_safe={}", writeback.safe);
        if let Some(writeback_reason) = writeback.reason {
            println!("writeback_reason={writeback_reason}");
        }
    }

    fn refresh_monitor_info(monitor: &mut Display) {
        let _ = monitor.update_capabilities();
        let _ = monitor.update_from_ddc();
    }

    fn get_feature(selector: DisplaySelector, feature: FeatureArg) -> Result<(), String> {
        let monitors = selected_displays(&selector)?;

        for (position, (display, mut monitor)) in monitors.into_iter().enumerate() {
            if position > 0 {
                println!();
            }

            refresh_monitor_info(&mut monitor);
            let profile = detect_profile(&monitor.info);
            let feature = resolve_feature(profile, feature.as_str())?;
            let value = monitor
                .handle
                .get_vcp_feature(feature.code)
                .map_err(|err| {
                    format!("Failed to read {} from display {}: {err}", feature, display)
                })?;

            print_value(display, &monitor, &feature, value);
        }

        Ok(())
    }

    fn set_feature(
        selector: DisplaySelector,
        feature: FeatureArg,
        value: String,
    ) -> Result<(), String> {
        let monitors = selected_displays(&selector)?;

        for (position, (display, mut monitor)) in monitors.into_iter().enumerate() {
            if position > 0 {
                println!();
            }

            refresh_monitor_info(&mut monitor);
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
            if let Some(label) = write_value_label(feature.spec, value) {
                println!("set_label={label}");
            }
        }

        Ok(())
    }

    fn print_profile(selector: DisplaySelector) -> Result<(), String> {
        let monitors = selected_displays(&selector)?;

        for (position, (display, mut monitor)) in monitors.into_iter().enumerate() {
            if position > 0 {
                println!();
            }

            refresh_monitor_info(&mut monitor);
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
                let write_values = write_options(feature);
                if !write_values.is_empty()
                    && (feature.value_options.is_empty()
                        || feature.write_options.as_ptr() != feature.value_options.as_ptr())
                {
                    let values = write_values
                        .iter()
                        .map(|option| format!("{}={}", option.label, option.value))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("    write_values: {values}");
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
        }

        Ok(())
    }

    #[derive(Debug)]
    struct ProbeResult {
        current: u16,
        maximum: u16,
        current_label: Option<String>,
        current_hex: Option<String>,
        current_high_byte: Option<u16>,
        current_low_byte: Option<u16>,
        decoded_current: Option<u16>,
        decoded_current_source: Option<String>,
        decoded_current_label: Option<String>,
        writeback_value: Option<u16>,
        writeback_source: Option<String>,
        writeback_label: Option<String>,
        writeback_safe: bool,
        writeback_reason: Option<String>,
    }

    #[derive(Debug)]
    struct DecodedCurrent {
        current_hex: Option<String>,
        current_high_byte: Option<u16>,
        current_low_byte: Option<u16>,
        decoded_current: Option<u16>,
        decoded_current_source: Option<&'static str>,
        decoded_current_label: Option<String>,
    }

    struct CurrentWriteback {
        value: Option<u16>,
        source: Option<&'static str>,
        label: Option<String>,
        safe: bool,
        reason: Option<&'static str>,
    }

    fn decode_current_value(
        feature: &crate::profile::ResolvedFeature,
        raw_current: u16,
    ) -> DecodedCurrent {
        let current_hex = Some(format!("0x{raw_current:04X}"));

        if raw_current <= 0xFF {
            return DecodedCurrent {
                current_hex,
                current_high_byte: None,
                current_low_byte: None,
                decoded_current: None,
                decoded_current_source: None,
                decoded_current_label: None,
            };
        }

        let high_byte = (raw_current >> 8) & 0x00FF;
        let low_byte = raw_current & 0x00FF;
        let high_label = value_label(feature.spec, high_byte).map(str::to_string);
        let low_label = value_label(feature.spec, low_byte).map(str::to_string);
        let (decoded_current, decoded_current_source, decoded_current_label) =
            if let Some(label) = high_label {
                (Some(high_byte), Some("high-byte"), Some(label))
            } else if let Some(label) = low_label {
                (Some(low_byte), Some("low-byte"), Some(label))
            } else if low_byte == 0 && high_byte != 0 {
                (Some(high_byte), Some("high-byte-zero-low-byte"), None)
            } else if high_byte == 0 && low_byte != 0 {
                (Some(low_byte), Some("low-byte-zero-high-byte"), None)
            } else {
                (None, None, None)
            };

        DecodedCurrent {
            current_hex,
            current_high_byte: Some(high_byte),
            current_low_byte: Some(low_byte),
            decoded_current,
            decoded_current_source,
            decoded_current_label,
        }
    }

    fn current_writeback_value(
        feature: &crate::profile::ResolvedFeature,
        raw_current: u16,
        decoded: &DecodedCurrent,
    ) -> CurrentWriteback {
        if raw_current <= 0xFF {
            return CurrentWriteback {
                value: Some(raw_current),
                source: Some("raw-current"),
                label: write_value_label(feature.spec, raw_current).map(str::to_string),
                safe: true,
                reason: Some("exact-readback"),
            };
        }

        if let Some(decoded_current) = decoded.decoded_current {
            let safe = decoded.decoded_current_label.is_some();
            return CurrentWriteback {
                value: Some(decoded_current),
                source: decoded.decoded_current_source,
                label: write_value_label(feature.spec, decoded_current).map(str::to_string),
                safe,
                reason: Some(if safe {
                    "decoded-labeled-current"
                } else {
                    "ambiguous-packed-readback"
                }),
            };
        }

        CurrentWriteback {
            value: None,
            source: None,
            label: None,
            safe: false,
            reason: Some("no-writeback-candidate"),
        }
    }

    fn probe_feature(selector: DisplaySelector, feature: FeatureArg) -> Result<(), String> {
        let (display, mut monitor) = open_display(&selector)?;
        refresh_monitor_info(&mut monitor);
        let profile = detect_profile(&monitor.info);
        let feature = resolve_feature(profile, feature.as_str())?;
        let value = monitor
            .handle
            .get_vcp_feature(feature.code)
            .map_err(|err| format!("Failed to read {} from display {}: {err}", feature, display))?;

        let decoded = decode_current_value(&feature, value.value());
        let writeback = current_writeback_value(&feature, value.value(), &decoded);
        println!("current={}", value.value());
        println!("maximum={}", value.maximum());
        if let Some(current_hex) = decoded.current_hex {
            println!("current_hex={current_hex}");
        }
        if let Some(high_byte) = decoded.current_high_byte {
            println!("current_high_byte={high_byte}");
        }
        if let Some(low_byte) = decoded.current_low_byte {
            println!("current_low_byte={low_byte}");
        }
        if let Some(label) = value_label(feature.spec, value.value()) {
            println!("current_label={label}");
        }
        if let Some(decoded_current) = decoded.decoded_current {
            println!("decoded_current={decoded_current}");
        }
        if let Some(decoded_current_source) = decoded.decoded_current_source {
            println!("decoded_current_source={decoded_current_source}");
        }
        if let Some(decoded_current_label) = decoded.decoded_current_label {
            println!("decoded_current_label={decoded_current_label}");
        }
        if let Some(writeback_value) = writeback.value {
            println!("writeback_value={writeback_value}");
        }
        if let Some(writeback_source) = writeback.source {
            println!("writeback_source={writeback_source}");
        }
        if let Some(writeback_label) = writeback.label {
            println!("writeback_label={writeback_label}");
        }
        println!("writeback_safe={}", writeback.safe);
        if let Some(writeback_reason) = writeback.reason {
            println!("writeback_reason={writeback_reason}");
        }

        Ok(())
    }

    fn probe_feature_with_timeout(
        selector: &DisplaySelector,
        feature_code: u8,
        timeout_ms: u64,
    ) -> Result<ProbeResult, String> {
        let current_exe = std::env::current_exe()
            .map_err(|err| format!("Failed to locate current executable: {err}"))?;
        let mut command = ProcessCommand::new(current_exe);
        command.arg("probe");

        if let Some(display) = selector.display {
            command.arg("--display").arg(display.to_string());
        }
        if let Some(id) = selector.id.as_deref() {
            command.arg("--id").arg(id);
        }
        if let Some(name) = selector.name.as_deref() {
            command.arg("--name").arg(name);
        }

        command
            .arg(format!("0x{feature_code:02X}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|err| {
            format!("Failed to spawn probe process for 0x{feature_code:02X}: {err}")
        })?;
        let started = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let output = child.wait_with_output().map_err(|err| {
                        format!("Failed to collect probe output for 0x{feature_code:02X}: {err}")
                    })?;

                    if !status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        return Err(if stderr.is_empty() {
                            format!("probe exited with status {status}")
                        } else {
                            stderr
                        });
                    }

                    return parse_probe_output(
                        feature_code,
                        &String::from_utf8_lossy(&output.stdout),
                    );
                }
                Ok(None) => {
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(format!("timeout after {}ms", timeout_ms));
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(err) => {
                    return Err(format!(
                        "Failed to poll probe process for 0x{feature_code:02X}: {err}"
                    ));
                }
            }
        }
    }

    fn parse_probe_output(feature_code: u8, output: &str) -> Result<ProbeResult, String> {
        let mut current = None;
        let mut maximum = None;
        let mut current_label = None;
        let mut current_hex = None;
        let mut current_high_byte = None;
        let mut current_low_byte = None;
        let mut decoded_current = None;
        let mut decoded_current_source = None;
        let mut decoded_current_label = None;
        let mut writeback_value = None;
        let mut writeback_source = None;
        let mut writeback_label = None;
        let mut writeback_safe = false;
        let mut writeback_reason = None;

        for line in output.lines() {
            if let Some(value) = line.strip_prefix("current=") {
                current = value.parse::<u16>().ok();
            } else if let Some(value) = line.strip_prefix("maximum=") {
                maximum = value.parse::<u16>().ok();
            } else if let Some(value) = line.strip_prefix("current_label=") {
                current_label = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("current_hex=") {
                current_hex = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("current_high_byte=") {
                current_high_byte = value.parse::<u16>().ok();
            } else if let Some(value) = line.strip_prefix("current_low_byte=") {
                current_low_byte = value.parse::<u16>().ok();
            } else if let Some(value) = line.strip_prefix("decoded_current=") {
                decoded_current = value.parse::<u16>().ok();
            } else if let Some(value) = line.strip_prefix("decoded_current_source=") {
                decoded_current_source = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("decoded_current_label=") {
                decoded_current_label = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("writeback_value=") {
                writeback_value = value.parse::<u16>().ok();
            } else if let Some(value) = line.strip_prefix("writeback_source=") {
                writeback_source = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("writeback_label=") {
                writeback_label = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("writeback_safe=") {
                writeback_safe = value.eq_ignore_ascii_case("true");
            } else if let Some(value) = line.strip_prefix("writeback_reason=") {
                writeback_reason = Some(value.to_string());
            }
        }

        match (current, maximum) {
            (Some(current), Some(maximum)) => Ok(ProbeResult {
                current,
                maximum,
                current_label,
                current_hex,
                current_high_byte,
                current_low_byte,
                decoded_current,
                decoded_current_source,
                decoded_current_label,
                writeback_value,
                writeback_source,
                writeback_label,
                writeback_safe,
                writeback_reason,
            }),
            _ => Err(format!("Invalid probe output for 0x{feature_code:02X}")),
        }
    }

    fn scan_features(
        selector: DisplaySelector,
        start: u8,
        end: u8,
        timeout_ms: u64,
        show_failures: bool,
    ) -> Result<(), String> {
        if start > end {
            return Err(format!(
                "Invalid scan range: start 0x{start:02X} is greater than end 0x{end:02X}."
            ));
        }

        let monitors = selected_displays(&selector)?;

        for (position, (display, mut monitor)) in monitors.into_iter().enumerate() {
            if position > 0 {
                println!();
            }

            refresh_monitor_info(&mut monitor);
            let profile = detect_profile(&monitor.info);
            let probe_selector = DisplaySelector {
                all: false,
                display: Some(display),
                id: None,
                name: None,
            };

            println!("display={display}");
            println!("controller={}", controller_name(&monitor.info));
            println!("scan_range=0x{start:02X}-0x{end:02X}");
            println!("timeout_ms={timeout_ms}");

            let mut readable = 0usize;
            let mut failures = 0usize;

            for code in start..=end {
                match probe_feature_with_timeout(&probe_selector, code, timeout_ms) {
                    Ok(result) => {
                        readable += 1;
                        let resolved_name = feature_name(profile, code);
                        let resolved_spec = resolve_feature(profile, &format!("0x{code:02X}"))
                            .ok()
                            .and_then(|feature| feature.spec);

                        if let Some(label) = result.current_label.clone().or_else(|| {
                            value_label(resolved_spec, result.current).map(str::to_string)
                        }) {
                            println!(
                                "0x{code:02X} name={resolved_name} current={} current_label={} maximum={}",
                                result.current, label, result.maximum
                            );
                        } else {
                            println!(
                                "0x{code:02X} name={resolved_name} current={} maximum={}",
                                result.current, result.maximum
                            );
                        }
                        if let Some(current_hex) = result.current_hex.as_deref() {
                            println!("0x{code:02X} current_hex={current_hex}");
                        }
                        if let Some(high_byte) = result.current_high_byte {
                            println!("0x{code:02X} current_high_byte={high_byte}");
                        }
                        if let Some(low_byte) = result.current_low_byte {
                            println!("0x{code:02X} current_low_byte={low_byte}");
                        }
                        if let Some(decoded_current) = result.decoded_current {
                            let source = result
                                .decoded_current_source
                                .as_deref()
                                .unwrap_or("heuristic");

                            if let Some(decoded_current_label) =
                                result.decoded_current_label.as_deref()
                            {
                                println!(
                                    "0x{code:02X} decoded_current={} decoded_label={} source={}",
                                    decoded_current, decoded_current_label, source
                                );
                            } else {
                                println!(
                                    "0x{code:02X} decoded_current={} source={}",
                                    decoded_current, source
                                );
                            }
                        }
                        if let Some(writeback_value) = result.writeback_value {
                            let source = result.writeback_source.as_deref().unwrap_or("unknown");
                            if let Some(writeback_label) = result.writeback_label.as_deref() {
                                println!(
                                    "0x{code:02X} writeback_value={} writeback_label={} source={} safe={}",
                                    writeback_value, writeback_label, source, result.writeback_safe
                                );
                            } else {
                                println!(
                                    "0x{code:02X} writeback_value={} source={} safe={}",
                                    writeback_value, source, result.writeback_safe
                                );
                            }
                        }
                        if let Some(writeback_reason) = result.writeback_reason.as_deref() {
                            println!("0x{code:02X} writeback_reason={writeback_reason}");
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
        }

        Ok(())
    }

    pub fn run() -> Result<(), String> {
        let cli = Cli::parse();

        match cli.command {
            Command::List => print_monitors(),
            Command::Usb { command } => match command {
                UsbCommand::Scan => crate::usb::scan_usb(),
                UsbCommand::Watch => crate::usb::watch_usb(),
            },
            Command::Get { selector, feature } => get_feature(selector, feature),
            Command::Set {
                selector,
                feature,
                value,
            } => set_feature(selector, feature, value),
            Command::Profile { selector } => print_profile(selector),
            Command::Scan {
                selector,
                start,
                end,
                timeout_ms,
                show_failures,
            } => scan_features(selector, start, end, timeout_ms, show_failures),
            Command::Probe { selector, feature } => probe_feature(selector, feature),
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

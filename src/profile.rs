use std::fmt;

use ddc_hi::DisplayInfo;

#[derive(Clone, Copy, Debug)]
pub struct ValueOption {
    pub value: u16,
    pub label: &'static str,
    pub aliases: &'static [&'static str],
}

#[derive(Clone, Copy, Debug)]
pub struct FeatureSpec {
    pub code: u8,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub writable: bool,
    pub value_options: &'static [ValueOption],
}

#[derive(Clone, Copy, Debug)]
pub struct MonitorProfile {
    pub key: &'static str,
    pub name: &'static str,
    pub matchers: &'static [&'static str],
    pub features: &'static [FeatureSpec],
    pub observed_readable_codes: &'static [u8],
}

#[derive(Clone, Debug)]
pub struct ResolvedFeature {
    pub code: u8,
    pub name: String,
    pub spec: Option<&'static FeatureSpec>,
}

impl fmt::Display for ResolvedFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (0x{:02X})", self.name, self.code)
    }
}

const EMPTY_VALUES: &[ValueOption] = &[];

const GENERIC_INPUT_VALUES: &[ValueOption] = &[
    ValueOption {
        value: 1,
        label: "vga-1",
        aliases: &["vga", "vga-1", "analog"],
    },
    ValueOption {
        value: 3,
        label: "dvi-1",
        aliases: &["dvi", "dvi-1"],
    },
    ValueOption {
        value: 15,
        label: "displayport-1",
        aliases: &["displayport", "displayport-1", "dp", "dp-1"],
    },
    ValueOption {
        value: 16,
        label: "displayport-2",
        aliases: &["displayport-2", "dp2", "dp-2"],
    },
    ValueOption {
        value: 17,
        label: "hdmi-1",
        aliases: &["hdmi-1"],
    },
    ValueOption {
        value: 18,
        label: "hdmi-2",
        aliases: &["hdmi-2"],
    },
    ValueOption {
        value: 27,
        label: "usb-c",
        aliases: &["usb-c", "type-c"],
    },
];

const GENERIC_POWER_VALUES: &[ValueOption] = &[
    ValueOption {
        value: 1,
        label: "on",
        aliases: &["on", "normal"],
    },
    ValueOption {
        value: 4,
        label: "off",
        aliases: &["off", "soft-off"],
    },
];

const GENERIC_FEATURES: &[FeatureSpec] = &[
    FeatureSpec {
        code: 0x10,
        name: "brightness",
        aliases: &["brightness", "luminance"],
        writable: true,
        value_options: EMPTY_VALUES,
    },
    FeatureSpec {
        code: 0x12,
        name: "contrast",
        aliases: &["contrast"],
        writable: true,
        value_options: EMPTY_VALUES,
    },
    FeatureSpec {
        code: 0x60,
        name: "input",
        aliases: &["input", "input-source", "source"],
        writable: true,
        value_options: GENERIC_INPUT_VALUES,
    },
    FeatureSpec {
        code: 0x62,
        name: "volume",
        aliases: &["volume", "speaker-volume"],
        writable: true,
        value_options: EMPTY_VALUES,
    },
    FeatureSpec {
        code: 0xD6,
        name: "power",
        aliases: &["power", "power-mode"],
        writable: true,
        value_options: GENERIC_POWER_VALUES,
    },
    FeatureSpec {
        code: 0xDF,
        name: "vcp-version",
        aliases: &["vcp-version", "vcp", "mccs-version"],
        writable: false,
        value_options: EMPTY_VALUES,
    },
];

const P2711V_INPUT_VALUES: &[ValueOption] = &[
    ValueOption {
        value: 1,
        label: "vga",
        aliases: &["vga", "vga-1", "analog"],
    },
    ValueOption {
        value: 3,
        label: "dvi",
        aliases: &["dvi", "dvi-1"],
    },
    ValueOption {
        value: 18,
        label: "hdmi",
        aliases: &["hdmi", "hdmi-2"],
    },
];

const P2711V_FEATURES: &[FeatureSpec] = &[
    FeatureSpec {
        code: 0x10,
        name: "brightness",
        aliases: &["brightness", "luminance"],
        writable: true,
        value_options: EMPTY_VALUES,
    },
    FeatureSpec {
        code: 0x12,
        name: "contrast",
        aliases: &["contrast"],
        writable: true,
        value_options: EMPTY_VALUES,
    },
    FeatureSpec {
        code: 0x60,
        name: "input",
        aliases: &["input", "input-source", "source"],
        writable: true,
        value_options: P2711V_INPUT_VALUES,
    },
    FeatureSpec {
        code: 0x62,
        name: "volume",
        aliases: &["volume", "speaker-volume"],
        writable: true,
        value_options: EMPTY_VALUES,
    },
    FeatureSpec {
        code: 0xD6,
        name: "power",
        aliases: &["power", "power-mode"],
        writable: true,
        value_options: GENERIC_POWER_VALUES,
    },
    FeatureSpec {
        code: 0xDF,
        name: "vcp-version",
        aliases: &["vcp-version", "vcp", "mccs-version"],
        writable: false,
        value_options: EMPTY_VALUES,
    },
];

const P2711V_OBSERVED_CODES: &[u8] = &[
    0x02, 0x04, 0x05, 0x06, 0x08, 0x0B, 0x0C, 0x0E, 0x10, 0x12, 0x13, 0x14, 0x16, 0x18, 0x1A, 0x1E,
    0x20, 0x22, 0x24, 0x26, 0x27, 0x29, 0x30, 0x31, 0x33, 0x35, 0x3E, 0x51, 0x52, 0x55, 0x60, 0x62,
    0x68, 0x69, 0x6C, 0x6E, 0x70, 0x8D, 0xA8, 0xAC, 0xAE, 0xB2, 0xB4, 0xB6, 0xC0, 0xC6, 0xC8, 0xC9,
    0xCA, 0xCC, 0xD6, 0xDC, 0xDF, 0xE0, 0xE1, 0xE2, 0xF0, 0xF3, 0xF7, 0xFA, 0xFD, 0xFE, 0xFF,
];

const PROFILES: &[MonitorProfile] = &[MonitorProfile {
    key: "dell-p2711v",
    name: "Dell P2711V",
    matchers: &["p2711v"],
    features: P2711V_FEATURES,
    observed_readable_codes: P2711V_OBSERVED_CODES,
}];

fn find_profile_by_override(input: &str) -> Option<&'static MonitorProfile> {
    let normalized = normalize(input);

    PROFILES.iter().find(|profile| {
        normalize(profile.key) == normalized
            || normalize(profile.name) == normalized
            || profile
                .matchers
                .iter()
                .any(|matcher| normalize(matcher) == normalized)
    })
}

pub fn detect_profile(info: &DisplayInfo) -> Option<&'static MonitorProfile> {
    if let Some(profile) = std::env::var("MONITOR_PROFILE")
        .ok()
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(find_profile_by_override)
    {
        return Some(profile);
    }

    let model_name = info.model_name.as_deref().unwrap_or_default();
    let manufacturer = info.manufacturer_id.as_deref().unwrap_or_default();
    let id = info.id.as_str();
    let haystacks = [
        normalize(model_name),
        normalize(manufacturer),
        normalize(id),
    ];

    PROFILES.iter().find(|profile| {
        profile.matchers.iter().any(|matcher| {
            haystacks
                .iter()
                .any(|haystack| haystack.contains(&normalize(matcher)))
        })
    })
}

pub fn controller_name(info: &DisplayInfo) -> &'static str {
    detect_profile(info)
        .map(|profile| profile.name)
        .unwrap_or("Generic DDC/CI")
}

pub fn controls_for(profile: Option<&'static MonitorProfile>) -> &'static [FeatureSpec] {
    profile
        .map(|value| value.features)
        .unwrap_or(GENERIC_FEATURES)
}

pub fn observed_codes(profile: Option<&'static MonitorProfile>) -> &'static [u8] {
    profile
        .map(|value| value.observed_readable_codes)
        .unwrap_or(&[])
}

pub fn resolve_feature(
    profile: Option<&'static MonitorProfile>,
    input: &str,
) -> Result<ResolvedFeature, String> {
    let normalized = normalize(input);

    if let Some(spec) = find_feature_by_name(controls_for(profile), &normalized)
        .or_else(|| find_feature_by_name(GENERIC_FEATURES, &normalized))
    {
        return Ok(ResolvedFeature {
            code: spec.code,
            name: spec.name.to_string(),
            spec: Some(spec),
        });
    }

    let code = parse_u8(input).map_err(|_| {
        format!("Unsupported feature: {input}. Use a known alias or a VCP code such as 0x10.")
    })?;
    let spec = find_feature_by_code(controls_for(profile), code)
        .or_else(|| find_feature_by_code(GENERIC_FEATURES, code));

    Ok(ResolvedFeature {
        code,
        name: spec
            .map(|value| value.name.to_string())
            .unwrap_or_else(|| format!("0x{code:02X}")),
        spec,
    })
}

pub fn resolve_value(spec: Option<&'static FeatureSpec>, input: &str) -> Result<u16, String> {
    let normalized = normalize(input);

    if let Some(spec) = spec {
        if let Some(option) = spec.value_options.iter().find(|option| {
            option.label.eq_ignore_ascii_case(input)
                || option
                    .aliases
                    .iter()
                    .any(|alias| normalize(alias) == normalized)
        }) {
            return Ok(option.value);
        }
    }

    parse_u16(input)
        .map_err(|_| format!("Invalid value: {input}. Use decimal, hex, or a known profile alias."))
}

pub fn feature_name(profile: Option<&'static MonitorProfile>, code: u8) -> &'static str {
    find_feature_by_code(controls_for(profile), code)
        .or_else(|| find_feature_by_code(GENERIC_FEATURES, code))
        .map(|spec| spec.name)
        .unwrap_or("unknown")
}

pub fn value_label(spec: Option<&'static FeatureSpec>, value: u16) -> Option<&'static str> {
    spec.and_then(|feature| {
        feature
            .value_options
            .iter()
            .find(|option| option.value == value)
            .map(|option| option.label)
    })
}

fn find_feature_by_name(
    features: &'static [FeatureSpec],
    input: &str,
) -> Option<&'static FeatureSpec> {
    features.iter().find(|feature| {
        normalize(feature.name) == input
            || feature
                .aliases
                .iter()
                .any(|alias| normalize(alias) == input)
    })
}

fn find_feature_by_code(
    features: &'static [FeatureSpec],
    code: u8,
) -> Option<&'static FeatureSpec> {
    features.iter().find(|feature| feature.code == code)
}

fn normalize(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

fn parse_u8(input: &str) -> Result<u8, std::num::ParseIntError> {
    if let Some(hex) = input.trim().strip_prefix("0x") {
        u8::from_str_radix(hex, 16)
    } else {
        input.trim().parse::<u8>()
    }
}

fn parse_u16(input: &str) -> Result<u16, std::num::ParseIntError> {
    if let Some(hex) = input.trim().strip_prefix("0x") {
        u16::from_str_radix(hex, 16)
    } else {
        input.trim().parse::<u16>()
    }
}

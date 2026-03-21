use futures_lite::{StreamExt, future};
use nusb::{
    BusInfo, DeviceId, DeviceInfo, InterfaceInfo, MaybeFuture, Speed, UsbControllerType,
    hotplug::HotplugEvent,
};
use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn scan_usb() -> Result<(), String> {
    let buses = nusb::list_buses()
        .wait()
        .map_err(|err| format!("Failed to enumerate USB buses: {err}"))?
        .collect::<Vec<_>>();
    let devices = nusb::list_devices()
        .wait()
        .map_err(|err| format!("Failed to enumerate USB devices: {err}"))?
        .collect::<Vec<_>>();

    if buses.is_empty() && devices.is_empty() {
        println!("No USB buses or devices detected.");
        return Ok(());
    }

    let mut devices_by_bus: BTreeMap<String, Vec<DeviceInfo>> = BTreeMap::new();
    for device in devices {
        devices_by_bus
            .entry(device.bus_id().to_string())
            .or_default()
            .push(device);
    }

    for devices in devices_by_bus.values_mut() {
        devices.sort_by(|left, right| {
            left.device_address()
                .cmp(&right.device_address())
                .then_with(|| left.port_chain().cmp(right.port_chain()))
                .then_with(|| left.vendor_id().cmp(&right.vendor_id()))
                .then_with(|| left.product_id().cmp(&right.product_id()))
        });
    }

    if buses.is_empty() {
        for (position, (bus_id, devices)) in devices_by_bus.iter().enumerate() {
            if position > 0 {
                println!();
            }
            println!("bus={bus_id}");
            println!("  controller=unknown");
            for device in devices {
                print_device(device);
            }
        }
        return Ok(());
    }

    for (position, bus) in buses.iter().enumerate() {
        if position > 0 {
            println!();
        }

        print_bus(bus);
        match devices_by_bus.remove(bus.bus_id()) {
            Some(devices) if !devices.is_empty() => {
                for device in &devices {
                    print_device(device);
                }
            }
            _ => println!("  devices=none"),
        }
    }

    if !devices_by_bus.is_empty() {
        println!();
        println!("unmatched_buses:");
        for (bus_id, devices) in devices_by_bus {
            println!("bus={bus_id}");
            for device in &devices {
                print_device(device);
            }
        }
    }

    Ok(())
}

pub fn watch_usb() -> Result<(), String> {
    let watch =
        nusb::watch_devices().map_err(|err| format!("Failed to start USB watcher: {err}"))?;
    let mut devices = nusb::list_devices()
        .wait()
        .map_err(|err| format!("Failed to enumerate USB devices: {err}"))?
        .map(|device| (device.id(), device))
        .collect::<HashMap<DeviceId, DeviceInfo>>();

    println!("usb_watch_started=true");
    println!("known_devices={}", devices.len());
    println!("waiting_for_changes=true");

    future::block_on(async move {
        let mut watch = watch;
        while let Some(event) = watch.next().await {
            match event {
                HotplugEvent::Connected(device) => {
                    print_watch_event("connected", &device);
                    devices.insert(device.id(), device);
                }
                HotplugEvent::Disconnected(id) => {
                    let removed = devices.remove(&id);
                    print_disconnect_event(id, removed.as_ref());
                }
            }
        }
    });

    Ok(())
}

fn print_bus(bus: &BusInfo) {
    println!("bus={}", bus.bus_id());
    println!("  system_name={}", bus.system_name().unwrap_or("unknown"));
    println!(
        "  controller_type={}",
        format_controller_type(bus.controller_type())
    );
    println!("  driver={}", bus.driver().unwrap_or("unknown"));

    #[cfg(target_os = "macos")]
    {
        println!("  class_name={}", bus.class_name());
        println!("  provider_class_name={}", bus.provider_class_name());
        println!("  location_id=0x{:08X}", bus.location_id());
    }
}

fn print_device(device: &DeviceInfo) {
    let product = device.product_string().unwrap_or("unknown");
    let manufacturer = device.manufacturer_string().unwrap_or("unknown");
    let serial = device.serial_number().unwrap_or("unknown");

    println!(
        "  device address={} port_chain={} vid=0x{:04X} pid=0x{:04X}",
        device.device_address(),
        format_port_chain(device.port_chain()),
        device.vendor_id(),
        device.product_id()
    );
    println!("    product={product}");
    println!("    manufacturer={manufacturer}");
    println!("    serial={serial}");
    println!("    usb_version={}", format_bcd(device.usb_version()));
    println!("    device_version={}", format_bcd(device.device_version()));
    println!(
        "    class={} subclass=0x{:02X} protocol=0x{:02X}",
        format_class(device.class()),
        device.subclass(),
        device.protocol()
    );
    println!("    speed={}", format_speed(device.speed()));

    let interfaces = device.interfaces().collect::<Vec<_>>();
    if interfaces.is_empty() {
        println!("    interfaces=none");
    } else {
        for interface in interfaces {
            print_interface(interface);
        }
    }
}

fn print_watch_event(event: &str, device: &DeviceInfo) {
    println!("timestamp={}", unix_timestamp());
    println!("event={event}");
    println!("bus={}", device.bus_id());
    println!("device_address={}", device.device_address());
    println!("port_chain={}", format_port_chain(device.port_chain()));
    println!("vid=0x{:04X}", device.vendor_id());
    println!("pid=0x{:04X}", device.product_id());
    println!("product={}", device.product_string().unwrap_or("unknown"));
    println!(
        "manufacturer={}",
        device.manufacturer_string().unwrap_or("unknown")
    );
    println!("serial={}", device.serial_number().unwrap_or("unknown"));
    println!("class={}", format_class(device.class()));
    println!("speed={}", format_speed(device.speed()));
    println!();
}

fn print_disconnect_event(id: DeviceId, device: Option<&DeviceInfo>) {
    println!("timestamp={}", unix_timestamp());
    println!("event=disconnected");
    println!("device_id={id:?}");

    if let Some(device) = device {
        println!("bus={}", device.bus_id());
        println!("device_address={}", device.device_address());
        println!("port_chain={}", format_port_chain(device.port_chain()));
        println!("vid=0x{:04X}", device.vendor_id());
        println!("pid=0x{:04X}", device.product_id());
        println!("product={}", device.product_string().unwrap_or("unknown"));
        println!(
            "manufacturer={}",
            device.manufacturer_string().unwrap_or("unknown")
        );
        println!("serial={}", device.serial_number().unwrap_or("unknown"));
        println!("class={}", format_class(device.class()));
        println!("speed={}", format_speed(device.speed()));
    } else {
        println!("details=unavailable");
    }

    println!();
}

fn print_interface(interface: &InterfaceInfo) {
    println!(
        "    interface number={} class={} subclass=0x{:02X} protocol=0x{:02X} name={}",
        interface.interface_number(),
        format_class(interface.class()),
        interface.subclass(),
        interface.protocol(),
        interface.interface_string().unwrap_or("unknown")
    );
}

fn format_port_chain(port_chain: &[u8]) -> String {
    if port_chain.is_empty() {
        "root".to_string()
    } else {
        port_chain
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(".")
    }
}

fn format_bcd(value: u16) -> String {
    let major = (value >> 8) & 0xff;
    let minor = (value >> 4) & 0x0f;
    let patch = value & 0x0f;
    format!("{}.{}{} (0x{:04X})", major, minor, patch, value)
}

fn format_speed(speed: Option<Speed>) -> &'static str {
    match speed {
        Some(Speed::Low) => "low",
        Some(Speed::Full) => "full",
        Some(Speed::High) => "high",
        Some(Speed::Super) => "super",
        Some(Speed::SuperPlus) => "super+",
        Some(_) | None => "unknown",
    }
}

fn format_controller_type(controller_type: Option<UsbControllerType>) -> &'static str {
    match controller_type {
        Some(UsbControllerType::XHCI) => "xhci",
        Some(UsbControllerType::EHCI) => "ehci",
        Some(UsbControllerType::OHCI) => "ohci",
        Some(UsbControllerType::UHCI) => "uhci",
        Some(UsbControllerType::VHCI) => "vhci",
        Some(_) | None => "unknown",
    }
}

fn format_class(class: u8) -> &'static str {
    match class {
        0x00 => "per-interface",
        0x01 => "audio",
        0x02 => "communications",
        0x03 => "hid",
        0x05 => "physical",
        0x06 => "image",
        0x07 => "printer",
        0x08 => "mass-storage",
        0x09 => "hub",
        0x0A => "cdc-data",
        0x0B => "smart-card",
        0x0D => "content-security",
        0x0E => "video",
        0x0F => "personal-healthcare",
        0x10 => "audio-video",
        0x11 => "billboard",
        0x12 => "usb-type-c-bridge",
        0xDC => "diagnostic",
        0xE0 => "wireless",
        0xEF => "miscellaneous",
        0xFE => "application-specific",
        0xFF => "vendor-specific",
        _ => "unknown",
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

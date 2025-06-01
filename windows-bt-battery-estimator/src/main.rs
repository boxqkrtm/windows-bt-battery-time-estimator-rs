use btleplug::api::{Central, Manager as _, ScanFilter, Peripheral as _};
use btleplug::platform::Manager;
use tokio::time;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

slint::include_modules!(); // Will include types from ui/appwindow.slint: AppWindow and DeviceDisplayInfo

const BATTERY_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
const BATTERY_LEVEL_CHARACTERISTIC_UUID: Uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);

#[derive(Debug, Clone)]
pub struct DeviceBatteryInfoRust { // Renamed to avoid conflict with Slint's generated DeviceDisplayInfo
    pub name: String,
    pub battery_percentage: Option<u8>,
    pub estimated_time: String,
}

fn estimate_battery_time(percentage: u8) -> String {
    if percentage < 5 {
        "Low battery".to_string()
    } else if percentage > 100 {
        "Invalid percentage".to_string()
    } else {
        let estimated_hours = percentage as f32 / 10.0;
        format!("{:.1} hours remaining (very rough estimate)", estimated_hours)
    }
}

fn format_battery_percentage(percentage: Option<u8>) -> String {
    match percentage {
        Some(p) => format!("{}%", p),
        None => "N/A".to_string(),
    }
}

async fn scan_and_get_device_infos(manager: &Manager) -> Vec<DeviceBatteryInfoRust> {
    let mut device_infos = Vec::new();

    println!("Getting Bluetooth adapters for scan...");
    let adapters = match manager.adapters().await {
        Ok(adapters) => adapters,
        Err(e) => {
            eprintln!("Error getting adapters: {}", e);
            return device_infos;
        }
    };

    let central = match adapters.into_iter().next() {
        Some(adapter) => adapter,
        None => {
            eprintln!("No Bluetooth adapters found during scan.");
            return device_infos;
        }
    };

    println!("Starting scan for Bluetooth devices...");
    if let Err(e) = central.start_scan(ScanFilter::default()).await {
        eprintln!("Error starting scan: {}", e);
        return device_infos;
    }

    println!("Scanning for 5 seconds...");
    time::sleep(Duration::from_secs(5)).await;

    println!("Retrieving discovered peripherals from scan...");
    let peripherals = match central.peripherals().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error retrieving peripherals: {}", e);
            central.stop_scan().await.ok();
            return device_infos;
        }
    };

    if peripherals.is_empty() {
        println!("-> No Bluetooth peripherals found during scan.");
    } else {
        println!("Processing discovered peripherals...");
        for peripheral in peripherals.iter() {
            let properties = match peripheral.properties().await {
                Ok(Some(props)) => props,
                Ok(None) => {
                    device_infos.push(DeviceBatteryInfoRust {
                        name: peripheral.address().to_string(),
                        battery_percentage: None,
                        estimated_time: "Properties N/A".to_string(),
                    });
                    println!("  Device: {}, Properties N/A", peripheral.address());
                    continue;
                }
                Err(e) => {
                    let addr_str = peripheral.address().to_string();
                    eprintln!("Error getting properties for {}: {}", addr_str, e);
                     device_infos.push(DeviceBatteryInfoRust {
                        name: addr_str, // Use address as name if props fail
                        battery_percentage: None,
                        estimated_time: "Props error".to_string(),
                    });
                    continue;
                }
            };

            let addr = peripheral.address();
            let peripheral_name = properties.local_name.clone().unwrap_or_else(|| addr.to_string());

            let mut current_device_info = DeviceBatteryInfoRust {
                name: peripheral_name.clone(),
                battery_percentage: None,
                estimated_time: "Battery level N/A".to_string(),
            };
            println!("  Checking {} ({})", current_device_info.name, addr);

            if let Err(e) = peripheral.connect().await {
                eprintln!("    Error connecting to {}: {}", addr, e);
                current_device_info.estimated_time = "Connection failed".to_string();
                device_infos.push(current_device_info);
                continue;
            }

            if !peripheral.is_connected().await.unwrap_or(false) {
                eprintln!("    Failed to connect to {}.", addr);
                current_device_info.estimated_time = "Connection failed".to_string();
                device_infos.push(current_device_info);
                continue;
            }
            println!("    Successfully connected to {}.", addr);

            if let Err(e) = peripheral.discover_services().await {
                eprintln!("    Error discovering services for {}: {}", addr, e);
                current_device_info.estimated_time = "Service discovery failed".to_string();
                peripheral.disconnect().await.ok();
                device_infos.push(current_device_info);
                continue;
            }

            let mut battery_level_found_for_this_device = false;
            for service in peripheral.services() {
                if service.uuid == BATTERY_SERVICE_UUID {
                    println!("    Found Battery Service (UUID: {}) for {}.", service.uuid, addr);
                    for characteristic in service.characteristics {
                        if characteristic.uuid == BATTERY_LEVEL_CHARACTERISTIC_UUID {
                            println!("      Found Battery Level Characteristic (UUID: {}) for {}.", characteristic.uuid, addr);
                            if characteristic.properties.contains(btleplug::api::CharPropFlags::READ) {
                                println!("        Attempting to read Battery Level from {}...", addr);
                                match peripheral.read(&characteristic).await {
                                    Ok(value) => {
                                        if value.is_empty() {
                                            println!("        Battery Level value is empty for {}.", addr);
                                            current_device_info.estimated_time = "Empty battery value".to_string();
                                        } else {
                                            let battery_level = value[0];
                                            current_device_info.battery_percentage = Some(battery_level);
                                            current_device_info.estimated_time = estimate_battery_time(battery_level);
                                            println!("        Battery Level for {}: {}%", addr, battery_level);
                                            battery_level_found_for_this_device = true;
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("        Error reading Battery Level from {}: {}", addr, e);
                                        current_device_info.estimated_time = "Read failed".to_string();
                                    }
                                }
                            } else {
                                println!("        Battery Level Characteristic does not support READ for {}.", addr);
                                current_device_info.estimated_time = "Read not supported".to_string();
                            }
                            break;
                        }
                    }
                }
                if battery_level_found_for_this_device { break; }
            }

            if !battery_level_found_for_this_device && current_device_info.battery_percentage.is_none() {
                 println!("    Battery Level characteristic not found or not readable for {}.", addr);
            }

            device_infos.push(current_device_info);

            if let Err(e) = peripheral.disconnect().await {
                eprintln!("    Error disconnecting from {}: {}", addr, e);
            } else {
                println!("    Successfully disconnected from {}.", addr);
            }
            println!("---");
        }
    }

    println!("Stopping scan (scan_and_get_device_infos)...");
    if let Err(e) = central.stop_scan().await {
        eprintln!("Error stopping scan: {}", e);
    }
    device_infos
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Use the generated AppWindow struct
    let app_window = AppWindow::new().expect("Failed to create AppWindow");
    let window_weak = app_window.as_weak();

    println!("Initializing Bluetooth Manager for UI...");
    let manager = Arc::new(Manager::new().await.expect("Failed to create Bluetooth Manager"));

    // Initial Data Load
    {
        let initial_rust_infos = scan_and_get_device_infos(&manager).await;
        // Use the generated DeviceDisplayInfo struct from Slint
        let initial_slint_infos: Vec<DeviceDisplayInfo> = initial_rust_infos
            .into_iter()
            .map(|rust_info| DeviceDisplayInfo { // No app_window:: prefix
                name: rust_info.name.into(),
                battery_percentage: format_battery_percentage(rust_info.battery_percentage).into(),
                estimated_time: rust_info.estimated_time.into(),
            })
            .collect();
        app_window.set_devices(slint::ModelRc::new(slint::VecModel::from(initial_slint_infos)));
    }

    let manager_clone_for_callback = manager.clone();
    app_window.on_refresh_clicked(move || {
        let ui_handle = window_weak.clone();
        let manager_handle = manager_clone_for_callback.clone();

        println!("Refresh clicked: Starting blocking scan (workaround)...");

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime for blocking refresh");

        rt.block_on(async move {
            let rust_infos = scan_and_get_device_infos(&manager_handle).await;
            let slint_infos: Vec<DeviceDisplayInfo> = rust_infos // No app_window:: prefix
                .into_iter()
                .map(|rust_info| DeviceDisplayInfo { // No app_window:: prefix
                    name: rust_info.name.into(),
                    battery_percentage: format_battery_percentage(rust_info.battery_percentage).into(),
                    estimated_time: rust_info.estimated_time.into(),
                })
                .collect();

            if let Some(ui) = ui_handle.upgrade() {
                println!("Updating UI with new device data ({} devices) (blocking).", slint_infos.len());
                ui.set_devices(slint::ModelRc::new(slint::VecModel::from(slint_infos)));
            } else {
                println!("UI not available for update after refresh (blocking).");
            }
        });
    });

    println!("Starting Slint event loop...");
    app_window.run().expect("Failed to run AppWindow event loop");

    Ok(())
}

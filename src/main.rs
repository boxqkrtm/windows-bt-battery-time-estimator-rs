use slint::{VecModel, SharedString, ModelRc};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use lazy_static::lazy_static;
use std::sync::Mutex;

slint::slint! {
    export struct DeviceDisplayInfo {
        name: string,
        battery_percentage: string,
        estimated_time: string,
    }

    export component AppWindow inherits Window {
        title: "Bluetooth Battery Time Estimator";
        width: 600px;
        height: 500px;
        background: #f5f5f5;

        in-out property <[DeviceDisplayInfo]> devices: [];
        in-out property <bool> is_refreshing: false;
        callback refresh_clicked();

        VerticalLayout {
            padding: 20px;
            spacing: 15px;

            // Header
            HorizontalLayout {
                alignment: space-between;
                
                Text {
                    text: "Bluetooth Device Battery Status";
                    font-size: 24px;
                    font-weight: 700;
                    color: #333;
                }
                
                TouchArea {
                    width: 80px;
                    height: 35px;
                    
                    Rectangle {
                        background: is_refreshing ? #666666 : #0066cc;
                        border-radius: 6px;
                        
                        Text {
                            text: is_refreshing ? "..." : "Refresh";
                            font-size: 14px;
                            color: white;
                            horizontal-alignment: center;
                            vertical-alignment: center;
                        }
                    }
                    
                    clicked => {
                        if (!is_refreshing) {
                            refresh_clicked();
                        }
                    }
                }
            }

            // Device List with proper scrolling
            Flickable {
                height: 400px;
                viewport-height: devices.length * 90px;
                
                VerticalLayout {
                    spacing: 10px;
                    
                    for device in devices: Rectangle {
                        height: 80px;
                        background: white;
                        border-radius: 8px;
                        border-width: 1px;
                        border-color: #e0e0e0;
                        drop-shadow-blur: 2px;
                        drop-shadow-color: #00000010;
                        
                        HorizontalLayout {
                            padding: 15px;
                            spacing: 15px;
                            alignment: space-between;
                            
                            VerticalLayout {
                                alignment: start;
                                spacing: 5px;
                                
                                Text {
                                    text: device.name;
                                    font-size: 16px;
                                    font-weight: 600;
                                    color: #444;
                                }
                                
                                Text {
                                    text: "Battery Level: " + device.battery_percentage;
                                    font-size: 14px;
                                    font-weight: 700;
                                    color: #0066cc;
                                }
                            }
                            
                            VerticalLayout {
                                alignment: center;
                                
                                Text {
                                    text: "Estimated Usage Time";
                                    font-size: 12px;
                                    color: #666;
                                }
                                
                                Text {
                                    text: device.estimated_time;
                                    font-size: 14px;
                                    font-weight: 600;
                                    color: #555;
                                }
                            }
                        }
                    }
                    
                    if devices.length == 0: Rectangle {
                        height: 100px;
                        background: white;
                        border-radius: 8px;
                        border-width: 1px;
                        border-color: #e0e0e0;
                        
                        Text {
                            text: is_refreshing ? "Refreshing devices..." : "Searching for Bluetooth devices...";
                            font-size: 16px;
                            color: #666;
                            horizontal-alignment: center;
                            vertical-alignment: center;
                        }
                    }
                }
            }
        }
    }
}

mod bluetooth_battery;
mod windows_rfcomm;
mod uwp_bluetooth;

use windows_rfcomm::WindowsRfcommSocket;
use uwp_bluetooth::get_bluetooth_devices_uwp;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BluetoothDevice {
    name: String,
    mac_address: String,
    device_type: String,
    battery_level: Option<u8>,
    battery_estimate: String,
    accuracy: String,
}

#[derive(Debug, Clone)]
struct BatteryHistory {
    previous_levels: Vec<u8>,
    change_count: usize,
}

impl BatteryHistory {
    fn new() -> Self {
        Self {
            previous_levels: Vec::new(),
            change_count: 0,
        }
    }

    fn update(&mut self, new_level: u8) -> String {
        if let Some(&last_level) = self.previous_levels.last() {
            if last_level != new_level {
                self.change_count += 1;
            }
        }
        
        self.previous_levels.push(new_level);
        if self.previous_levels.len() > 10 {
            self.previous_levels.remove(0);
        }

        match self.change_count {
            0 => "Measuring".to_string(),
            1 => "Approximate".to_string(),
            _ => "Estimated".to_string(),
        }
    }
}

lazy_static! {
    static ref BATTERY_HISTORY: Mutex<HashMap<String, BatteryHistory>> = Mutex::new(HashMap::new());
}

async fn get_connected_bluetooth_devices() -> Vec<BluetoothDevice> {
    let mut devices = Vec::new();
    
    // Try UWP API first (more reliable for battery info)
    match get_bluetooth_devices_uwp().await {
        Ok(uwp_devices) => {
            for (name, mac_address, battery_level) in uwp_devices {
                let device_type = classify_device_type(&name);
                
                // Skip devices classified as "Other"
                if device_type == "Other" {
                    continue;
                }
                
                let mut device = BluetoothDevice {
                    name: name.clone(),
                    mac_address: mac_address.clone(),
                    device_type,
                    battery_level,
                    battery_estimate: "Measuring".to_string(),
                    accuracy: "Measuring".to_string(),
                };
                
                if let Some(level) = battery_level {
                    let mut history = BATTERY_HISTORY.lock().unwrap();
                    let device_history = history.entry(mac_address.clone()).or_insert_with(BatteryHistory::new);
                    device.accuracy = device_history.update(level);
                    device.battery_estimate = format!("{}h {}m", 
                        calculate_hours_from_battery(level), 
                        calculate_minutes_from_battery(level));
                } else {
                    device.accuracy = "N/A".to_string();
                    device.battery_estimate = "N/A".to_string();
                }
                
                devices.push(device);
            }
        }
        Err(e) => {
            println!("UWP API failed, falling back to PowerShell: {}", e);
            // Fallback to PowerShell method
            let powershell_devices = get_devices_via_powershell().await;
            
            for mut device in powershell_devices {
                if let Some(mac) = extract_mac_address(&device.name) {
                    device.mac_address = mac.clone();
                    
                    // Try RFCOMM battery query
                    if let Ok(battery_level) = query_device_battery_rfcomm(&mac).await {
                        device.battery_level = battery_level;
                        
                        if let Some(level) = battery_level {
                            let mut history = BATTERY_HISTORY.lock().unwrap();
                            let device_history = history.entry(mac.clone()).or_insert_with(BatteryHistory::new);
                            device.accuracy = device_history.update(level);
                            device.battery_estimate = format!("{}h {}m", 
                                calculate_hours_from_battery(level), 
                                calculate_minutes_from_battery(level));
                        } else {
                            device.accuracy = "N/A".to_string();
                            device.battery_estimate = "N/A".to_string();
                        }
                    } else {
                        // Fallback to BLE GATT if RFCOMM fails
                        if let Ok(battery_level) = query_device_battery_ble(&mac).await {
                            device.battery_level = battery_level;
                            
                            if let Some(level) = battery_level {
                                let mut history = BATTERY_HISTORY.lock().unwrap();
                                let device_history = history.entry(mac.clone()).or_insert_with(BatteryHistory::new);
                                device.accuracy = device_history.update(level);
                                device.battery_estimate = format!("{}h {}m", 
                                    calculate_hours_from_battery(level), 
                                    calculate_minutes_from_battery(level));
                            } else {
                                device.accuracy = "N/A".to_string();
                                device.battery_estimate = "N/A".to_string();
                            }
                        } else {
                            device.battery_level = None;
                            device.accuracy = "N/A".to_string();
                            device.battery_estimate = "N/A".to_string();
                        }
                    }
                }
                devices.push(device);
            }
        }
    }
    
    devices
}

async fn query_device_battery_rfcomm(mac_address: &str) -> Result<Option<u8>, anyhow::Error> {
    let mut socket = WindowsRfcommSocket::new()?;
    match socket.query_battery_at_commands(mac_address).await {
        Ok(battery_level) => Ok(battery_level),
        Err(e) => {
            println!("RFCOMM query failed for {}: {}", mac_address, e);
            Ok(None)
        }
    }
}

async fn query_device_battery_ble(mac_address: &str) -> Result<Option<u8>, anyhow::Error> {
    use btleplug::api::{Central, Manager as _, Peripheral as _};
    use btleplug::platform::Manager;
    use uuid::Uuid;

    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    
    if adapters.is_empty() {
        return Ok(None);
    }

    let adapter = &adapters[0];
    let peripherals = adapter.peripherals().await?;

    for peripheral in peripherals {
        if let Ok(Some(properties)) = peripheral.properties().await {
            if let Some(name) = properties.local_name {
                if name.contains(&mac_address.replace(":", "")) {
                    if peripheral.is_connected().await? {
                        let battery_service_uuid = Uuid::parse_str("0000180F-0000-1000-8000-00805F9B34FB")?;
                        let battery_level_char_uuid = Uuid::parse_str("00002A19-0000-1000-8000-00805F9B34FB")?;
                        
                        let services = peripheral.services();
                        for service in services {
                            if service.uuid == battery_service_uuid {
                                for characteristic in service.characteristics {
                                    if characteristic.uuid == battery_level_char_uuid {
                                        if let Ok(data) = peripheral.read(&characteristic).await {
                                            if !data.is_empty() {
                                                return Ok(Some(data[0]));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}

async fn get_devices_via_powershell() -> Vec<BluetoothDevice> {
    let mut devices = Vec::new();

    let output = std::process::Command::new("powershell")
        .args(&[
            "-Command",
            "$OutputEncoding = [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; chcp 65001 | Out-Null; Get-PnpDevice | Where-Object { $_.Class -eq 'Bluetooth' -and $_.Status -eq 'OK' } | ConvertTo-Json -Depth 3"
        ])
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        if let Ok(json_devices) = serde_json::from_str::<serde_json::Value>(&stdout) {
            let device_array = if json_devices.is_array() {
                json_devices.as_array().unwrap()
            } else {
                &vec![json_devices]
            };

            for device in device_array {
                if let (Some(name), Some(instance_id)) = (
                    device["FriendlyName"].as_str(),
                    device["InstanceId"].as_str()
                ) {
                    let device_type = classify_device_type(name);
                    
                    // Skip devices classified as "Other"
                    if device_type == "Other" {
                        continue;
                    }
                    
                    devices.push(BluetoothDevice {
                        name: name.to_string(),
                        mac_address: extract_mac_from_instance_id(instance_id).unwrap_or_default(),
                        device_type,
                        battery_level: None,
                        battery_estimate: "Measuring".to_string(),
                        accuracy: "Measuring".to_string(),
                    });
                }
            }
        }
    }
    devices
}

fn extract_mac_address(device_name: &str) -> Option<String> {
    if device_name.contains("MX Master") {
        Some("00:1F:20:3A:4B:5C".to_string())
    } else if device_name.contains("BN-E100") {
        Some("AA:BB:CC:DD:EE:FF".to_string())
    } else if device_name.contains("AULA") {
        Some("11:22:33:44:55:66".to_string())
    } else {
        let hash = device_name.chars().fold(0u64, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u64));
        Some(format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            (hash >> 40) & 0xFF,
            (hash >> 32) & 0xFF,
            (hash >> 24) & 0xFF,
            (hash >> 16) & 0xFF,
            (hash >> 8) & 0xFF,
            hash & 0xFF
        ))
    }
}

fn extract_mac_from_instance_id(instance_id: &str) -> Option<String> {
    if let Some(start) = instance_id.find("\\{") {
        if let Some(end) = instance_id[start..].find("}&") {
            let mac_part = &instance_id[start + 2..start + end];
            if mac_part.len() == 12 {
                return Some(format!("{}:{}:{}:{}:{}:{}",
                    &mac_part[0..2], &mac_part[2..4], &mac_part[4..6],
                    &mac_part[6..8], &mac_part[8..10], &mac_part[10..12]
                ));
            }
        }
    }
    None
}

fn classify_device_type(name: &str) -> String {
    let name_lower = name.to_lowercase();
    if name_lower.contains("mouse") || name_lower.contains("mx master") {
        "Mouse".to_string()
    } else if name_lower.contains("keyboard") || name_lower.contains("aula") {
        "Keyboard".to_string()
    } else if name_lower.contains("headphone") || name_lower.contains("earphone") || 
              name_lower.contains("bn-e100") || name_lower.contains("qcy") {
        "Earphone".to_string()
    } else if name_lower.contains("speaker") {
        "Speaker".to_string()
    } else {
        "Other".to_string()
    }
}

fn calculate_hours_from_battery(battery_level: u8) -> u8 {
    match battery_level {
        90..=100 => 8,
        80..=89 => 7,
        70..=79 => 6,
        60..=69 => 5,
        50..=59 => 4,
        40..=49 => 3,
        30..=39 => 2,
        20..=29 => 1,
        _ => 0,
    }
}

fn calculate_minutes_from_battery(battery_level: u8) -> u8 {
    ((battery_level % 10) * 6) % 60
}

#[tokio::main]
async fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    
    let ui_handle = ui.as_weak();
    let ui_handle_refresh = ui.as_weak();

    // Initial load - non-blocking
    {
        let ui_handle = ui_handle.clone();
        tokio::spawn(async move {
            let devices = get_connected_bluetooth_devices().await;
            let device_model: Vec<DeviceDisplayInfo> = devices.iter().map(|d| {
                DeviceDisplayInfo {
                    name: SharedString::from(&format!("{} ({})", d.name, d.device_type)),
                    battery_percentage: SharedString::from(
                        d.battery_level.map_or("N/A".to_string(), |b| format!("{}%", b))
                    ),
                    estimated_time: SharedString::from(&format!("{} ({})", d.battery_estimate, d.accuracy)),
                }
            }).collect();

            ui_handle.upgrade_in_event_loop(move |ui| {
                ui.set_devices(ModelRc::new(VecModel::from(device_model)));
            }).unwrap();
        });
    }

    // Refresh callback - non-blocking
    ui.on_refresh_clicked({
        move || {
            let ui_handle = ui_handle_refresh.clone();
            
            // Set refreshing state immediately
            ui_handle_refresh.upgrade_in_event_loop(move |ui| {
                ui.set_is_refreshing(true);
            }).unwrap();
            
            // Spawn async task for refresh
            tokio::spawn(async move {
                let devices = get_connected_bluetooth_devices().await;
                let device_model: Vec<DeviceDisplayInfo> = devices.iter().map(|d| {
                    DeviceDisplayInfo {
                        name: SharedString::from(&format!("{} ({})", d.name, d.device_type)),
                        battery_percentage: SharedString::from(
                            d.battery_level.map_or("N/A".to_string(), |b| format!("{}%", b))
                        ),
                        estimated_time: SharedString::from(&format!("{} ({})", d.battery_estimate, d.accuracy)),
                    }
                }).collect();

                ui_handle.upgrade_in_event_loop(move |ui| {
                    ui.set_devices(ModelRc::new(VecModel::from(device_model)));
                    ui.set_is_refreshing(false);
                }).unwrap();
            });
        }
    });

    ui.run()
} 
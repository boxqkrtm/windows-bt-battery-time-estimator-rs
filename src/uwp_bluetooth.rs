use windows::{
    core::*,
    Devices::Bluetooth::{BluetoothLEDevice, BluetoothUuidHelper},
    Devices::Bluetooth::GenericAttributeProfile::{
        GattDeviceService, GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
        GattCommunicationStatus, GattValueChangedEventArgs,
    },
    Devices::Enumeration::{DeviceInformation, DeviceInformationKind},
    Foundation::{EventRegistrationToken, TypedEventHandler},
    Storage::Streams::DataReader,
};
use std::collections::HashMap;
use anyhow::{Result, anyhow};

pub struct UwpBluetoothManager {
    devices: HashMap<String, BluetoothLEDevice>,
}

impl UwpBluetoothManager {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
        }
    }

    pub fn discover_devices(&mut self) -> Result<Vec<String>> {
        let mut device_ids = Vec::new();
        
        // Get all Bluetooth LE devices
        let selector = BluetoothLEDevice::GetDeviceSelector()?;
        
        // Use blocking call instead of async
        let device_info_collection = DeviceInformation::FindAllAsyncAqsFilter(&selector)?;
        
        // Wait for completion (blocking)
        let device_info_collection = device_info_collection.get()?;
        
        for i in 0..device_info_collection.Size()? {
            if let Ok(device_info) = device_info_collection.GetAt(i) {
                if let Ok(device_id) = device_info.Id() {
                    let device_id_str = device_id.to_string();
                    device_ids.push(device_id_str.clone());
                    
                    // Try to get the BluetoothLEDevice
                    if let Ok(async_op) = BluetoothLEDevice::FromIdAsync(&device_id) {
                        if let Ok(ble_device) = async_op.get() {
                            self.devices.insert(device_id_str, ble_device);
                        }
                    }
                }
            }
        }
        
        Ok(device_ids)
    }

    pub fn get_device_battery(&self, device_id: &str) -> Result<Option<u8>> {
        if let Some(device) = self.devices.get(device_id) {
            self.query_battery_service(device)
        } else {
            Err(anyhow!("Device not found: {}", device_id))
        }
    }

    fn query_battery_service(&self, device: &BluetoothLEDevice) -> Result<Option<u8>> {
        // Battery Service UUID: 0x180F
        let battery_service_uuid = BluetoothUuidHelper::FromShortId(0x180F)?;
        
        // Get GATT services (blocking call)
        let gatt_async_op = device.GetGattServicesForUuidAsync(battery_service_uuid)?;
        let gatt_result = gatt_async_op.get()?;
        
        if gatt_result.Status()? != GattCommunicationStatus::Success {
            return Ok(None);
        }
        
        let services = gatt_result.Services()?;
        if services.Size()? == 0 {
            return Ok(None);
        }
        
        let battery_service = services.GetAt(0)?;
        
        // Battery Level Characteristic UUID: 0x2A19
        let battery_level_uuid = BluetoothUuidHelper::FromShortId(0x2A19)?;
        
        let char_async_op = battery_service.GetCharacteristicsForUuidAsync(battery_level_uuid)?;
        let char_result = char_async_op.get()?;
        
        if char_result.Status()? != GattCommunicationStatus::Success {
            return Ok(None);
        }
        
        let characteristics = char_result.Characteristics()?;
        if characteristics.Size()? == 0 {
            return Ok(None);
        }
        
        let battery_characteristic = characteristics.GetAt(0)?;
        
        // Read the battery level (blocking call)
        let read_async_op = battery_characteristic.ReadValueAsync()?;
        let read_result = read_async_op.get()?;
        
        if read_result.Status()? != GattCommunicationStatus::Success {
            return Ok(None);
        }
        
        let buffer = read_result.Value()?;
        if buffer.Length()? == 0 {
            return Ok(None);
        }
        
        let data_reader = DataReader::FromBuffer(&buffer)?;
        let battery_level = data_reader.ReadByte()?;
        
        Ok(Some(battery_level))
    }

    pub fn get_device_info(&self, device_id: &str) -> Result<Option<(String, String)>> {
        if let Some(device) = self.devices.get(device_id) {
            let name = device.Name()?.to_string();
            let address = device.BluetoothAddress()?;
            let mac_address = format!("{:012X}", address);
            let formatted_mac = format!("{}:{}:{}:{}:{}:{}",
                &mac_address[0..2], &mac_address[2..4], &mac_address[4..6],
                &mac_address[6..8], &mac_address[8..10], &mac_address[10..12]
            );
            Ok(Some((name, formatted_mac)))
        } else {
            Ok(None)
        }
    }
}

pub async fn get_bluetooth_devices_uwp() -> Result<Vec<(String, String, Option<u8>)>> {
    // Run the blocking operations in a separate thread to avoid blocking the async runtime
    let result = tokio::task::spawn_blocking(|| {
        let mut manager = UwpBluetoothManager::new();
        let device_ids = manager.discover_devices()?;
        let mut devices = Vec::new();
        
        for device_id in device_ids {
            if let Ok(Some((name, mac_address))) = manager.get_device_info(&device_id) {
                // Skip empty names or system devices
                if name.is_empty() || name.starts_with("System") {
                    continue;
                }
                
                let battery_level = manager.get_device_battery(&device_id).unwrap_or(None);
                devices.push((name, mac_address, battery_level));
            }
        }
        
        Ok::<Vec<(String, String, Option<u8>)>, anyhow::Error>(devices)
    }).await??;
    
    Ok(result)
} 
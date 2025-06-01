use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryResult {
    pub overall: Option<u8>,
    pub left: Option<u8>,
    pub right: Option<u8>,
    pub case: Option<u8>,
}

impl BatteryResult {
    pub fn new() -> Self {
        Self {
            overall: None,
            left: None,
            right: None,
            case: None,
        }
    }

    pub fn get_primary_level(&self) -> Option<u8> {
        if let Some(overall) = self.overall {
            return Some(overall);
        }

        // Return minimum of left and right if available
        match (self.left, self.right) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => self.case,
        }
    }
}

pub struct BluetoothBatteryQuerier {
    device_mac: String,
}

impl BluetoothBatteryQuerier {
    pub fn new(device_mac: String) -> Self {
        Self { device_mac }
    }

    pub async fn query_battery(&self) -> Result<BatteryResult, Box<dyn std::error::Error>> {
        // Try different methods to get battery information
        
        // Method 1: Try HID battery service
        if let Ok(result) = self.query_hid_battery().await {
            if result.get_primary_level().is_some() {
                return Ok(result);
            }
        }

        // Method 2: Try BLE GATT battery service
        if let Ok(result) = self.query_ble_battery().await {
            if result.get_primary_level().is_some() {
                return Ok(result);
            }
        }

        // Method 3: Try RFCOMM AT commands
        if let Ok(result) = self.query_rfcomm_battery().await {
            if result.get_primary_level().is_some() {
                return Ok(result);
            }
        }

        // Return empty result if all methods fail
        Ok(BatteryResult::new())
    }

    async fn query_hid_battery(&self) -> Result<BatteryResult, Box<dyn std::error::Error>> {
        let result = BatteryResult::new();
        
        // TODO: Implement HID battery query
        // This would involve using Windows HID APIs to query battery level
        // from HID devices like mice and keyboards
        
        Ok(result)
    }

    async fn query_ble_battery(&self) -> Result<BatteryResult, Box<dyn std::error::Error>> {
        // TODO: Implement BLE GATT battery service query
        // This would use btleplug to connect to BLE devices and read
        // the standard Battery Service (0x180F)
        
        Ok(BatteryResult::new())
    }

    async fn query_rfcomm_battery(&self) -> Result<BatteryResult, Box<dyn std::error::Error>> {
        // TODO: Implement RFCOMM battery query
        // This would use Windows Bluetooth APIs to establish RFCOMM
        // connection and send AT commands for battery level
        
        Ok(BatteryResult::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_battery_result_primary_level() {
        let mut result = BatteryResult::new();
        assert_eq!(result.get_primary_level(), None);

        result.overall = Some(75);
        assert_eq!(result.get_primary_level(), Some(75));

        result.overall = None;
        result.left = Some(80);
        result.right = Some(70);
        assert_eq!(result.get_primary_level(), Some(70)); // Should return minimum

        result.left = Some(60);
        result.right = None;
        assert_eq!(result.get_primary_level(), Some(60));

        result.left = None;
        result.right = None;
        result.case = Some(50);
        assert_eq!(result.get_primary_level(), Some(50));
    }

    #[tokio::test]
    async fn test_battery_querier() {
        let querier = BluetoothBatteryQuerier::new("00:11:22:33:44:55".to_string());
        let result = querier.query_battery().await;
        assert!(result.is_ok());
    }
} 
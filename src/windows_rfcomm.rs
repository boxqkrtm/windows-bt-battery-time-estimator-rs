use std::ffi::CString;
use std::mem;
use std::ptr;
use std::time::Duration;
use windows::Win32::Foundation::*;
use windows::Win32::Networking::WinSock::*;
use windows::Win32::Devices::Bluetooth::*;
use anyhow;

#[repr(C)]
#[derive(Debug)]
struct SOCKADDR_BTH {
    addressFamily: u16,
    btAddr: u64,
    serviceClassId: windows::core::GUID,
    port: u32,
}

pub struct WindowsRfcommSocket {
    socket: Option<SOCKET>,
    connected: bool,
}

impl WindowsRfcommSocket {
    pub fn new() -> Result<Self, anyhow::Error> {
        unsafe {
            // Initialize Winsock
            let mut wsa_data: WSADATA = mem::zeroed();
            let result = WSAStartup(0x0202, &mut wsa_data);
            if result != 0 {
                return Err(anyhow::anyhow!("Failed to initialize Winsock"));
            }

            // Create Bluetooth socket
            let socket_result = socket(AF_BTH as i32, WINSOCK_SOCKET_TYPE(SOCK_STREAM.0 as i32), BTHPROTO_RFCOMM as i32);
            let socket = match socket_result {
                Ok(s) => s,
                Err(_) => return Err(anyhow::anyhow!("Failed to create Bluetooth socket")),
            };

            Ok(Self {
                socket: Some(socket),
                connected: false,
            })
        }
    }

    pub async fn connect_to_device(&mut self, mac_address: &str) -> Result<(), anyhow::Error> {
        if self.socket.is_none() {
            return Err(anyhow::anyhow!("Socket not initialized"));
        }

        // Parse MAC address and connect
        let mac_bytes = self.parse_mac_address(mac_address)?;
        
        unsafe {
            let mut addr: SOCKADDR_BTH = std::mem::zeroed();
            addr.addressFamily = AF_BTH as u16;
            addr.btAddr = mac_bytes;
            addr.port = 1; // RFCOMM channel 1

            let socket = self.socket.unwrap();
            let result = connect(socket, &addr as *const _ as *const SOCKADDR, std::mem::size_of::<SOCKADDR_BTH>() as i32);
            
            if result == SOCKET_ERROR {
                let error = WSAGetLastError();
                return Err(anyhow::anyhow!("Failed to connect to Bluetooth device: {:?}", error));
            }
        }

        self.connected = true;
        Ok(())
    }

    pub async fn send_data(&self, data: &[u8]) -> Result<(), anyhow::Error> {
        if !self.connected || self.socket.is_none() {
            return Err(anyhow::anyhow!("Not connected to device"));
        }

        unsafe {
            let socket = self.socket.unwrap();
            let result = send(socket, data, SEND_RECV_FLAGS(0));
            
            if result == SOCKET_ERROR {
                let error = WSAGetLastError();
                return Err(anyhow::anyhow!("Failed to send data: {:?}", error));
            }
        }

        Ok(())
    }

    pub async fn receive_data(&self, buffer: &mut [u8]) -> Result<usize, anyhow::Error> {
        if !self.connected || self.socket.is_none() {
            return Err(anyhow::anyhow!("Not connected to device"));
        }

        unsafe {
            let socket = self.socket.unwrap();
            let result = recv(socket, buffer, SEND_RECV_FLAGS(0));
            
            if result == SOCKET_ERROR {
                let error = WSAGetLastError();
                return Err(anyhow::anyhow!("Failed to receive data: {:?}", error));
            }

            Ok(result as usize)
        }
    }

    pub async fn set_timeout(&self, timeout_ms: u32) -> Result<(), anyhow::Error> {
        if self.socket.is_none() {
            return Err(anyhow::anyhow!("Socket not initialized"));
        }

        unsafe {
            let socket = self.socket.unwrap();
            let timeout_bytes = timeout_ms.to_le_bytes();
            let result = setsockopt(socket, SOL_SOCKET as i32, SO_RCVTIMEO as i32, Some(&timeout_bytes));
            
            if result == SOCKET_ERROR {
                let error = WSAGetLastError();
                return Err(anyhow::anyhow!("Failed to set socket timeout: {:?}", error));
            }
        }

        Ok(())
    }

    fn parse_mac_address(&self, mac_str: &str) -> Result<u64, anyhow::Error> {
        let parts: Vec<&str> = mac_str.split(':').collect();
        if parts.len() != 6 {
            return Err(anyhow::anyhow!("Invalid MAC address format"));
        }

        let mut mac_bytes = 0u64;
        for (i, part) in parts.iter().enumerate() {
            let byte = u8::from_str_radix(part, 16)?;
            mac_bytes |= (byte as u64) << (8 * (5 - i));
        }

        Ok(mac_bytes)
    }

    pub async fn query_battery_at_commands(&mut self, mac_address: &str) -> Result<Option<u8>, anyhow::Error> {
        // Try to connect to the device
        if let Err(_) = self.connect_to_device(mac_address).await {
            return Ok(None); // Connection failed, device might not support RFCOMM
        }

        // Set a reasonable timeout
        self.set_timeout(5000).await?;

        // Try various AT commands for battery level
        let at_commands: &[&[u8]] = &[
            b"AT+CIND?\r\n",
            b"AT+IPHONEACCEV?\r\n", 
            b"AT+BRSF=1\r\n",
            b"AT+CMER=3,0,0,1\r\n",
        ];

        for command in at_commands {
            if let Ok(_) = self.send_data(command).await {
                let mut buffer = [0u8; 256];
                if let Ok(bytes_received) = self.receive_data(&mut buffer).await {
                    let response = String::from_utf8_lossy(&buffer[..bytes_received]);
                    
                    // Parse battery level from response
                    if let Some(battery_level) = self.parse_battery_from_response(&response) {
                        return Ok(Some(battery_level));
                    }
                }
            }
        }

        Ok(None)
    }

    fn parse_battery_from_response(&self, response: &str) -> Option<u8> {
        // Look for battery indicators in AT command responses
        if response.contains("+CIND:") {
            // Parse CIND response for battery level
            if let Some(start) = response.find("+CIND:") {
                let values_part = &response[start + 6..];
                if let Some(end) = values_part.find('\r') {
                    let values = &values_part[..end];
                    let parts: Vec<&str> = values.split(',').collect();
                    // Battery is usually the first or second value
                    for part in parts.iter().take(3) {
                        if let Ok(level) = part.trim().parse::<u8>() {
                            if level <= 100 {
                                return Some(level);
                            }
                        }
                    }
                }
            }
        }

        // Look for iPhone accessory protocol battery level
        if response.contains("IPHONEACCEV") {
            // Parse iPhone accessory battery level
            if let Some(start) = response.find("IPHONEACCEV") {
                let remaining = &response[start..];
                // Look for battery key-value pairs
                if remaining.contains("1,") {
                    // Battery level follows key "1"
                    if let Some(battery_start) = remaining.find("1,") {
                        let battery_part = &remaining[battery_start + 2..];
                        if let Some(comma_pos) = battery_part.find(',') {
                            let battery_str = &battery_part[..comma_pos];
                            if let Ok(level) = battery_str.trim().parse::<u8>() {
                                return Some((level * 10).min(100)); // Convert 0-9 scale to 0-100
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

impl Drop for WindowsRfcommSocket {
    fn drop(&mut self) {
        unsafe {
            if let Some(socket) = self.socket {
                closesocket(socket);
            }
            WSACleanup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_parsing() {
        let socket = WindowsRfcommSocket::new().unwrap();
        let addr = socket.parse_mac_address("00:11:22:33:44:55").unwrap();
        assert_eq!(addr, 0x001122334455);
    }

    #[test]
    fn test_battery_response_parsing() {
        let socket = WindowsRfcommSocket::new().unwrap();
        
        let response1 = "+IPHONEACCEV: 2,1,5,2,0";
        assert_eq!(socket.parse_battery_from_response(response1), Some(50));
        
        let response2 = "+CIND: 85,1,1,0,0,0,0";
        assert_eq!(socket.parse_battery_from_response(response2), Some(85));
    }
} 
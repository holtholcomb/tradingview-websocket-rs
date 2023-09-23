use std::collections::VecDeque;
use std::error::Error;
use std::net::{TcpStream, ToSocketAddrs};
use std::io::{Write, Read};
use std::sync::mpsc::{Sender, Receiver};
use native_tls::TlsConnector;

trait ReadWrite: Read + Write {}
impl<T: Read + Write + ?Sized> ReadWrite for T {}

#[derive(Debug)]
pub enum WebsocketError {
    ReadError,
    ChannelSendError,
    ChannelReceiveError,
    FrameEncodeError,
    FrameDecodeError,
    WriteError,
    AddressParseError,
    ConnectError,
    TlsCreationError,
    TlsConnectError,
    StringConversionError
}

impl std::fmt::Display for WebsocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            WebsocketError::ReadError => write!(f, "Read error"),
            WebsocketError::ChannelSendError => write!(f, "Channel send error"),
            WebsocketError::ChannelReceiveError => write!(f, "Channel receive error"),
            WebsocketError::FrameEncodeError => write!(f, "Frame encode error"),
            WebsocketError::FrameDecodeError => write!(f, "Frame decode error"),
            WebsocketError::WriteError => write!(f, "Write error"),
            WebsocketError::AddressParseError => write!(f, "Address parse error"),
            WebsocketError::ConnectError => write!(f, "Connect error"),
            WebsocketError::TlsCreationError => write!(f, "TLS creation error"),
            WebsocketError::TlsConnectError => write!(f, "TLS connect error"),
            WebsocketError::StringConversionError => write!(f, "String conversion error"),
        }
    }
}

impl Error for WebsocketError {}

impl From<WebsocketError> for Box<dyn std::error::Error + Send> {
    fn from(error: WebsocketError) -> Self {
        Box::new(error)
    }
}

pub struct WebSocket {
    tls_stream: Box<dyn ReadWrite + Send + Sync + 'static>,
    incoming_tx: Sender<String>,
    outgoing_rx: Receiver<Vec<String>>
}

impl WebSocket {
    pub fn new(incoming_tx: Sender<String>, outgoing_rx: Receiver<Vec<String>>) -> Result<WebSocket, Box<dyn Error + Send>> {
        let addr = "data.tradingview.com:443".to_socket_addrs().map_err(|_| WebsocketError::AddressParseError)?.next().unwrap();
        let stream = TcpStream::connect(addr).map_err(|_| WebsocketError::ConnectError)?;

        // Establish a TLS connection
        let connector = TlsConnector::new().map_err(|_| WebsocketError::TlsCreationError)?;
        let mut tls_stream = connector.connect("data.tradingview.com", stream).map_err(|_| WebsocketError::TlsConnectError)?;
        
        // Perform the WebSocket handshake with the server manually.
        let request = "\
            GET /socket.io/websocket?&type=chart HTTP/1.1\r\n\
            Host: data.tradingview.com\r\n\
            Connection: Upgrade\r\n\
            Upgrade: websocket\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\
            Origin: https://www.tradingview.com\r\n\
            \r\n";
        tls_stream.write_all(request.as_bytes()).map_err(|_| WebsocketError::WriteError)?;

        // Read the server's response to ensure it's a 101 Switching Protocols response.
        let mut buffer = [0u8; 65536];
        tls_stream.read(&mut buffer).map_err(|_| WebsocketError::ReadError)?;
        let response = std::str::from_utf8(&buffer).map_err(|_| WebsocketError::StringConversionError)?;
        assert!(response.contains("101 Switching Protocols"));

        Ok(WebSocket { 
            tls_stream: Box::new(tls_stream),
            incoming_tx,
            outgoing_rx
        })
    }
    
    fn decode_websocket_frame(&self, buffer: &mut VecDeque<u8>) -> Result<Option<String>, Box<dyn Error>> {
        if buffer.len() < 2 {
            return Ok(None);  // Not enough data
        }

        let fin_and_opcode = buffer[0];
        let opcode = fin_and_opcode & 0x0F;

        match opcode {
            0x01 => {  // Text frame
                let mask_and_length_byte = buffer[1];
                let (payload_length, header_size) = match mask_and_length_byte & 0x7F {
                    0..=125 => (mask_and_length_byte as usize, 2), // Direct length encoding
                    126 => {
                        if buffer.len() < 4 {
                            return Ok(None);  // Not enough data
                        }
                        (u16::from_be_bytes([buffer[2], buffer[3]]) as usize, 4)
                    },
                    127 => {
                        if buffer.len() < 10 {
                            return Ok(None);  // Not enough data
                        }
                        // Note: Since usize can be 32-bits on some platforms (like 32-bit systems), 
                        // this can potentially be a problem if the length is greater than usize::MAX.
                        // You might want to handle this scenario, e.g., by rejecting too-large messages.
                        let length_bytes = [
                            buffer[2], buffer[3], buffer[4], buffer[5], 
                            buffer[6], buffer[7], buffer[8], buffer[9]
                        ];
                        (u64::from_be_bytes(length_bytes) as usize, 10)
                    },
                    _ => return Err("Invalid payload length format".into())
                };

                if buffer.len() < (header_size + payload_length) {
                    return Ok(None);  // Not enough data
                }

                // Drain the header bytes
                for _ in 0..header_size {
                    buffer.pop_front();
                }

                // Drain and collect the payload bytes
                let payload_bytes: Vec<u8> = buffer.drain(0..payload_length).collect();
                let payload_str = std::str::from_utf8(&payload_bytes)?;

                Ok(Some(payload_str.to_string()))
            }
            0x88 => {  // Close frame
                // Handle the close frame
                // For example, if you want to print the status code:
                let status_code = u16::from_be_bytes([buffer[2], buffer[3]]);
                println!("Received close frame with status code: {}", status_code);
                buffer.drain(0..4);  // Drain the entire frame, including status code
                Ok(None)  // Or you can choose to return an error or another appropriate result
            }
            // Add handling for other frame types if needed...
            _ => {
                println!("{:02x?}", buffer);
                Err("Unsupported frame type".into())
            }
        }
    }

    fn encode_websocket_text_frame(&self, data: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut frame = vec![];

        let payload_length = data.len();

        frame.push(0x81); // Final fragment, text frame

        // Determine payload length format and write it to the frame
        match payload_length {
            len if len <= 125 => {
                frame.push(0x80 | len as u8);
            }
            len if len <= 65_535 => {
                frame.push(0x80 | 126); // Mask set and indicator for 2-byte extended length
                frame.extend(&[(len >> 8) as u8, len as u8]); // 2-byte big-endian length
            }
            len => {
                frame.push(0x80 | 127); // Mask set and indicator for 8-byte extended length
                frame.extend(&[
                    ((len >> 56) & 0xFF) as u8,
                    ((len >> 48) & 0xFF) as u8,
                    ((len >> 40) & 0xFF) as u8,
                    ((len >> 32) & 0xFF) as u8,
                    ((len >> 24) & 0xFF) as u8,
                    ((len >> 16) & 0xFF) as u8,
                    ((len >> 8) & 0xFF) as u8,
                    (len & 0xFF) as u8,
                ]); // 8-byte big-endian length
            }
        }

        // Generate a random mask
        let mask = [
            rand::random::<u8>(),
            rand::random::<u8>(),
            rand::random::<u8>(),
            rand::random::<u8>(),
        ];
        frame.extend_from_slice(&mask);

        // Mask the data
        for (i, byte) in data.bytes().enumerate() {
            frame.push(byte ^ mask[i % 4]);
        }

        Ok(frame)
    }

    pub fn handle_stream(&mut self) -> Result<(), Box<dyn Error + Send>> {
        let mut rx_buffer = VecDeque::new();

        let mut temp_buffer = [0u8; 65536];
        loop {
            let read_bytes = self.tls_stream.read(&mut temp_buffer).map_err(|_| WebsocketError::ReadError)?;

            if read_bytes == 0 {
                break;  // The stream has closed or there's an error.
            }

            rx_buffer.extend(&temp_buffer[0..read_bytes]);

            loop {
                match self.decode_websocket_frame(&mut rx_buffer) {
                    Ok(Some(incoming_message)) => {
                        self.incoming_tx.send(incoming_message).map_err(|_| WebsocketError::ChannelSendError)?;
                        let outgoing_messages = self.outgoing_rx.recv().map_err(|_| WebsocketError::ChannelReceiveError)?;
                        for outgoing_message in outgoing_messages {
                            println!("outgoing_message: {}", outgoing_message);
                            let encoded_frame = self.encode_websocket_text_frame(&outgoing_message).map_err(|_| WebsocketError::FrameEncodeError)?;
                            self.tls_stream.write_all(&encoded_frame).map_err(|_| WebsocketError::WriteError)?;
                        }
                    },
                    Ok(None) => break,  // Not enough data yet
                    Err(_) => return Err(WebsocketError::FrameDecodeError.into()),
                }
            }
        }

        Ok(())
    }
}
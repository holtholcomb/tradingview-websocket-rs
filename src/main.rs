mod websocket;
mod tradingview_api;

use std::error::Error;
use std::sync::mpsc;

use tradingview_api::TradingviewApi;
use websocket::WebSocket;

fn main() -> Result<(), Box<dyn Error>> {
    let (incoming_tx, incoming_rx) = mpsc::channel();
    let (outgoing_tx, outgoing_rx) = mpsc::channel();
    let _ = std::thread::spawn(move || -> Result<(), Box<dyn Error + Send>> {
        let tradingview_api = TradingviewApi::new(incoming_rx, outgoing_tx)?;
        tradingview_api.handler()
    });
    let mut websocket = WebSocket::new(incoming_tx, outgoing_rx)?;
    websocket.handle_stream()?;
    Ok(())
}

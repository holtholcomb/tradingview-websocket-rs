mod websocket;
mod trading_view_api;

use std::error::Error;
use std::sync::mpsc;

use trading_view_api::TradingViewApi;
use websocket::WebSocket;

fn main() -> Result<(), Box<dyn Error>> {
    let (incoming_tx, incoming_rx) = mpsc::channel();
    let (outgoing_tx, outgoing_rx) = mpsc::channel();
    let trading_view_thread = std::thread::spawn(move || {
        let trading_view_api = TradingViewApi::new(incoming_rx, outgoing_tx)?;
        trading_view_api.handler()
    });
    let websocket_thread = std::thread::spawn(move || {
        let mut websocket = WebSocket::new(incoming_tx, outgoing_rx)?;
        websocket.handle_stream()
    });
    let trading_view_result = trading_view_thread.join().expect("TradingView thread panicked");
    let websocket_result = websocket_thread.join().expect("Websocket thread panicked");
    if let Err(err) = trading_view_result {
        eprintln!("Error in TradingView thread: {:?}", err);
    }
    if let Err(err) = websocket_result {
        eprintln!("Error in Websocket thread: {:?}", err);
    }
    Ok(())
}

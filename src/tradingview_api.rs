use std::{error::Error, sync::mpsc::{Receiver, Sender}};

use json_dotpath::DotPaths;
use serde_json::Value;

#[derive(Debug)]
pub enum TradingviewError {
    ParseError,
    SerializationError,
    SendError,
    ReceiveError,
}

impl std::fmt::Display for TradingviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            TradingviewError::ParseError => write!(f, "Parse error"),
            TradingviewError::SerializationError => write!(f, "Serialization error"),
            TradingviewError::SendError => write!(f, "Send error"),
            TradingviewError::ReceiveError => write!(f, "Receive error"),
        }
    }
}

impl Error for TradingviewError {}

impl From<TradingviewError> for Box<dyn std::error::Error + Send> {
    fn from(error: TradingviewError) -> Self {
        Box::new(error)
    }
}

pub enum MessageType {
    ConnectedToServer(Value),
    Ping(usize),
    ProtocolError(Value),
    Empty,
    QsdBidAsk(Value),
    QsdDescription(Value),
    QsdLocalPopularity(Value),
    QuoteCompleted(Value),
    SeriesLoading(Value),
    SymbolResolved(Value),
    TimescaleUpdate(Value),
    SeriesCompleted(Value),
    StudyCompleted(Value),
    StudyError(Value),
    CriticalError(Value),
    StudyLoading(Value),
    SeriesUpdate(Value),
    StudyUpdate(Value),
    QsdLastPriceTime(Value),
    QsdLastPrice(Value),
}

pub struct TradingviewApi {
   incoming_rx: Receiver<String>,
   outgoing_tx: Sender<Vec<String>> 
}

impl TradingviewApi {
    pub fn new(incoming_rx: Receiver<String>, outgoing_tx: Sender<Vec<String>>) -> Result<TradingviewApi, Box<dyn Error + Send>> {
        Ok(TradingviewApi {
            incoming_rx,
            outgoing_tx
        })
    }

    fn determine_incoming_message_type(&self, message: &str) -> Result<MessageType, Box<dyn Error + Send>> {
        // ping isn't json
        let ping_re = regex::Regex::new(r"~h~(\d+)").expect("failed to compile regex");
        if ping_re.is_match(message) {
            let captures = ping_re.captures(message).expect("failed to capture");
            let id_string = captures.get(1).expect("id capture missing").as_str();
            let id = id_string.parse::<usize>().expect("failed to parse");
            return Ok(MessageType::Ping(id));
        }
        // watch out for empty
        if message.len() == 0 {
            return Ok(MessageType::Empty);
        }
        // all else is json?
        let parsed_message: Value = serde_json::from_str(&message).expect("failed to parse");
        if parsed_message.dot_has("release") {
            return Ok(MessageType::ConnectedToServer(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "protocol_error" { 
            return Ok(MessageType::ProtocolError(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "study_error" { 
            return Ok(MessageType::StudyError(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "critical_error" { 
            return Ok(MessageType::CriticalError(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "quote_completed" { 
            return Ok(MessageType::QuoteCompleted(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "series_loading" { 
            return Ok(MessageType::SeriesLoading(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "symbol_resolved" { 
            return Ok(MessageType::SymbolResolved(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "timescale_update" { 
            return Ok(MessageType::TimescaleUpdate(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "series_completed" { 
            return Ok(MessageType::SeriesCompleted(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "study_completed" { 
            return Ok(MessageType::StudyCompleted(parsed_message));
        }
        if parsed_message.dot_has("m") && parsed_message.dot_get::<String>("m").unwrap().unwrap() == "study_loading" { 
            return Ok(MessageType::StudyLoading(parsed_message));
        }
        if parsed_message.dot_has("p.1.v.bid_size") { 
            return Ok(MessageType::QsdBidAsk(parsed_message));
        }
        if parsed_message.dot_has("p.1.v.description") { 
            return Ok(MessageType::QsdDescription(parsed_message));
        }
        if parsed_message.dot_has("p.1.v.local_popularity") { 
            return Ok(MessageType::QsdLocalPopularity(parsed_message));
        }
        if parsed_message.dot_has("p.1.v.lp_time") { 
            return Ok(MessageType::QsdLastPriceTime(parsed_message));
        }
        if parsed_message.dot_has("p.1.v.lp") { 
            return Ok(MessageType::QsdLastPrice(parsed_message));
        }
        if parsed_message.dot_has("p.1.series_id.s") { 
            return Ok(MessageType::SeriesUpdate(parsed_message));
        }
        if parsed_message.dot_has("p.1.study_id.st") { 
            return Ok(MessageType::StudyUpdate(parsed_message));
        }
        // todo
        println!("{}", serde_json::to_string_pretty(&parsed_message).unwrap());
        return Err(Box::new(TradingviewError::ParseError));
    }

    fn send_json(&self, messages: Vec<Value>) -> Result<(), Box<dyn Error + Send>> {
        let formatted_responses = messages.iter().map(|message| {
            let stringified_response = serde_json::to_string(&message).expect("failed to serialize");
            return format!("~m~{}~m~{}", stringified_response.len(), stringified_response);
        }).collect();
        self.outgoing_tx.send(formatted_responses).map_err(|_| TradingviewError::SendError.into())
    }

    pub fn handler(&self) -> Result<(), Box<dyn Error + Send>> {
        loop {
            let incoming_messages: String = self.incoming_rx.recv().map_err(|_| TradingviewError::ReceiveError)?;
            //println!("incoming_messages = {}", incoming_messages);
            let re = regex::Regex::new(r"~m~\d+~m~").unwrap();
            for incoming_message in re.split(&incoming_messages).into_iter() {
                let message_type = self.determine_incoming_message_type(&incoming_message)?;
                match message_type {
                    MessageType::ConnectedToServer(message) => {
                        println!("{}", message);
                        // send batch of messages
                        self.send_json(vec![
                            // login
                            serde_json::json!({
                                "m": "set_auth_token",
                                "p": [
                                    "unauthorized_user_token"
                                ]
                            }),
                            // create quote
                            serde_json::json!({
                                "m": "quote_create_session",
                                "p": [
                                    "quote_session_id",
                                ]
                            }),
                            serde_json::json!({
                                "m": "quote_set_fields",
                                "p": [
                                  "quote_session_id",
                                  "base-currency-logoid",
                                  "ch",
                                  "chp",
                                  "currency-logoid",
                                  "currency_code",
                                  "currency_id",
                                  "base_currency_id",
                                  "current_session",
                                  "description",
                                  "exchange",
                                  "format",
                                  "fractional",
                                  "is_tradable",
                                  "language",
                                  "local_description",
                                  "listed_exchange",
                                  "logoid",
                                  "lp",
                                  "lp_time",
                                  "minmov",
                                  "minmove2",
                                  "original_name",
                                  "pricescale",
                                  "pro_name",
                                  "short_name",
                                  "type",
                                  "typespecs",
                                  "update_mode",
                                  "volume",
                                  "value_unit_id",
                                  "rchp",
                                  "rtc",
                                  "country_code",
                                  "provider_id"
                                ]
                            }),
                            serde_json::json!({
                                "m": "quote_add_symbols",
                                "p": [
                                    "quote_session_id",
                                    r#"={"session":"regular", "symbol": "CRYPTO:BTCUSD"}"#
                                ]
                            }),
                            serde_json::json!({
                                "m": "quote_fast_symbols",
                                "p": [
                                    "quote_session_id",
                                    "INDEX:BTCUSD"
                                ]
                            }),
                            // create chart
                            serde_json::json!({
                                "m": "chart_create_session",
                                "p": [
                                    "chart_session_id",
                                    ""
                                ]
                            }),
                            // add symbol to chart
                            serde_json::json!({
                                "m": "resolve_symbol",
                                "p": [
                                    "chart_session_id",
                                    "symbol_id",
                                    r#"={"session":"regular", "symbol": "CRYPTO:BTCUSD"}"#
                                ]
                            }),
                            // add candles to chart
                            serde_json::json!({
                                "m": "create_series",
                                "p": [
                                    "chart_session_id",
                                    "series_id",
                                    "study_parent_id",
                                    "symbol_id",
                                    "1",
                                    300,
                                    ""
                                ]
                            }),
                            // add indicator to chart
                            serde_json::json!({
                                "m": "create_study",
                                "p": [
                                    "chart_session_id",
                                    "study_id",
                                    "study_parent_id",
                                    "series_id",
                                    "Script@tv-scripting-101!",
                                    serde_json::json!({
                                        "text": "OvVf/cLhRZ8QR5Vpxqne7w==_pKuthoDJLaA6sn40TmHddOk0SwJb9ct8cm5JeGz0a5O4YBeoFgtEgyKwwKcVk+KQMJV96wVs+ms71b8+nds3580VFsC3U3MQvGaF+Xidbsm/vP9HK+rGeR/2iTxMfDT+sRSuAcY4mm/u9CPgHlc/1U5QoLL0+qSxw6spC2g33HJDdjZkWojBpa50yH0oELcUqVKNbKFX/RFReEzTqpc0Moo10cw8IVnBIp5Fu1SPEM2AIASQaI58LmwDyNdo2d/Rqn3u7JyRqt+TYu+asL9NynYoLVtTem2BonOTknu7NoBkQI9GJgMdxE4+jU9efxZk8jOGgP9XQPWAhX5jmZJDefGl1s2c/09TM29lPzUTFJRyyfmtZShBdiP3BqRfYXzEr6vCNetnsebCenWWkQtDjQ80ZgBV+HB8rciWhB34jXZ/MA8sGtT1lbknJbX5koliQ/pDj4tYY3Mp6eon+jvVDO6EyxTNk/9tj5h8b1Jdqy1svNAfr5MF3TfksELRGkzKFLxPNQUZz+Cn60T7vP/Qi+HDM/mfwdiYkaLXSXDQ6VkDc+K8vxJkYWRWONghVnzbeqhCYn747OB0u0xWxs1O+D0KjRq9CEjgsRLmMDqg2KLrdGRGrEpNjwy6jb31SXDQLR+IdKgSD/O71iNXXcd3KGdDXQpi0c70NuaKdUEGWIpBRjp6tFOTGp8yJHkwFJPkic9yGVQRMbqTctqbGHbaxVNvbhZdnhkl2bkTh7wkDXsYjxt2jTtAYlwq6RoJmzlKBBj2VR894emRQyipvvAz6bjxnQZC8zqxR/BF7HnzLtVMIMr+0nE0Ol0TDDkpkMsAiM5zH4212LNyOU4obRzYhwCuOR8L+W3/+fDhOHg+tSseK+d4QrFkn+qFsVHqEpeVoyIQDm1wwHsFiqN6by4Du4LtxHMRuasSzajwmxQNOe+qbbALRtpiVMFL/BVdH0bk0r43mnMC3s9CHcDB2CMCk4TjZZwNfWmQVQGqprukCQJFtqNY+SnK26rYby9/a2WnbnRW6lLcazUfwQHf6wPHfLLlNYiAayuUsPZyNZGnwvBkFZK6GG2eYZYam2XurXk2uMZRusQVuw6nDPk1R6CKg+KILriNHp2b2TM2zb4jogmbrqug3nqGky8oM9n/1lIsht+Jm8GztD99g2j/7crHI6DgZ3Bu8LKdmm7t+cnsPBLLNncdnbQhow1WZTffmi0=",
                                        "pineId": "PUB;N16MOYK6AEJGGAoy40axs0S48GRFYcNn",
                                        "pineVersion": "1.0",
                                        "in_0": {
                                          "v": 1,
                                          "f": true,
                                          "t": "integer"
                                        },
                                        "in_1": {
                                          "v": "close",
                                          "f": true,
                                          "t": "source"
                                        },
                                        "in_2": {
                                          "v": 7,
                                          "f": true,
                                          "t": "integer"
                                        },
                                        "in_3": {
                                          "v": "close",
                                          "f": true,
                                          "t": "source"
                                        },
                                        "in_4": {
                                          "v": 25,
                                          "f": true,
                                          "t": "integer"
                                        },
                                        "in_5": {
                                          "v": 65,
                                          "f": true,
                                          "t": "integer"
                                        },
                                        "in_6": {
                                          "v": 51,
                                          "f": true,
                                          "t": "integer"
                                        },
                                        "in_7": {
                                          "v": 21,
                                          "f": true,
                                          "t": "integer"
                                        }
                                      })
                                ]
                            })
                        ])?;
                    },
                    MessageType::Ping(id) => {
                        println!("ping:{}", id);
                        let response = format!("~h~{id}");
                        let formatted_response = format!("~m~{}~m~{}", response.len(), response);
                        self.outgoing_tx.send(vec![formatted_response]).map_err(|_| TradingviewError::SendError)?;
                    },
                    MessageType::QsdBidAsk(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::QsdDescription(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::QsdLocalPopularity(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::QuoteCompleted(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::SeriesLoading(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::SymbolResolved(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::TimescaleUpdate(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::SeriesCompleted(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::StudyLoading(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::QsdLastPriceTime(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::QsdLastPrice(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::SeriesUpdate(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::StudyUpdate(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::StudyCompleted(message) => {
                        println!("{}", message);
                        self.outgoing_tx.send(vec![]).map_err(|_| TradingviewError::SendError)?;
                    }
                    MessageType::Empty => {},
                    MessageType::StudyError(message) => {
                        panic!("study_error: {}", message);
                    }
                    MessageType::CriticalError(message) => {
                        panic!("critical_error: {}", message);
                    }
                    MessageType::ProtocolError(message) => {
                        panic!("protocol_error: {}", message);
                    },
                }
            }
        }
    }
}
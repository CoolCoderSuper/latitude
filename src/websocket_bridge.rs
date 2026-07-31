use axum::extract::ws::Message as AxumMessage;
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

pub(crate) fn to_tungstenite(message: AxumMessage) -> TungsteniteMessage {
    match message {
        AxumMessage::Text(text) => TungsteniteMessage::Text(text.to_string().into()),
        AxumMessage::Binary(bytes) => TungsteniteMessage::Binary(bytes.to_vec().into()),
        AxumMessage::Ping(bytes) => TungsteniteMessage::Ping(bytes.to_vec().into()),
        AxumMessage::Pong(bytes) => TungsteniteMessage::Pong(bytes.to_vec().into()),
        AxumMessage::Close(_) => TungsteniteMessage::Close(None),
    }
}

pub(crate) fn to_axum(message: TungsteniteMessage) -> Option<AxumMessage> {
    match message {
        TungsteniteMessage::Text(text) => Some(AxumMessage::Text(text.to_string().into())),
        TungsteniteMessage::Binary(bytes) => Some(AxumMessage::Binary(bytes.to_vec().into())),
        TungsteniteMessage::Ping(bytes) => Some(AxumMessage::Ping(bytes.to_vec().into())),
        TungsteniteMessage::Pong(bytes) => Some(AxumMessage::Pong(bytes.to_vec().into())),
        TungsteniteMessage::Close(_) => Some(AxumMessage::Close(None)),
        TungsteniteMessage::Frame(_) => None,
    }
}

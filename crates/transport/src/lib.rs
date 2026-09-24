//! How frames move between the svp server and its clients: the codec that
//! turns [`svp_wire`] types into frames, and the transports that carry them.
//! What the frames say is up to the server's session and the client. No
//! Nautilus, so the app can use it too.

pub mod codec;
pub mod protocols;

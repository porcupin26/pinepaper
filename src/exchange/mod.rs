//! Live, client-owned collaboration through a content-blind WebSocket relay.
pub mod replica;
pub mod transport;
pub use replica::Replica;
pub use transport::{Command, Event, Link};

pub struct Session {
    pub link: Link,
    pub replica: Option<Replica>,
    pub local_document: Option<String>,
    pub source: Option<std::path::PathBuf>,
    pub code: String,
    pub peers: Vec<String>,
    pub status: String,
    pub online: bool,
    pub pending: Vec<(String, Vec<replica::Change>)>,
}

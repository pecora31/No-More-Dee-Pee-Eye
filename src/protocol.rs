use crate::config::Settings;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Start(Settings),
    Validate(Settings),
    Stop,
    Shutdown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Starting,
    Running(u32),
    Stopped,
    Error(String),
    Log(String),
    Ready,
    Validated,
}
pub fn send<T: Serialize>(stream: &mut impl Write, message: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(message)?;
    if bytes.len() > 65_536 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()
}
pub fn receive<T: for<'de> Deserialize<'de>>(stream: &mut impl Read) -> io::Result<T> {
    let mut prefix = [0; 4];
    stream.read_exact(&mut prefix)?;
    let size = u32::from_le_bytes(prefix) as usize;
    if size > 65_536 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_large_frames() {
        assert!(receive::<Event>(&mut &70_000u32.to_le_bytes()[..]).is_err());
    }
    #[test]
    fn frames_roundtrip() {
        let mut bytes = Vec::new();
        send(&mut bytes, &Event::Running(42)).unwrap();
        assert!(matches!(
            receive::<Event>(&mut bytes.as_slice()).unwrap(),
            Event::Running(42)
        ));
    }
    #[test]
    fn rejects_truncated_frames() {
        assert!(receive::<Event>(&mut &[4, 0, 0, 0, 1][..]).is_err());
    }
}

use serde::Serialize;
use std::io::{self, BufRead, Write};
pub(crate) fn read_frame(reader: &mut impl BufRead, limit: usize) -> io::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete process frame",
                ))
            };
        }
        let n = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |i| i + 1);
        if frame.len().checked_add(n).is_none_or(|size| size > limit) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "process frame budget exceeded",
            ));
        }
        frame.extend_from_slice(&available[..n]);
        reader.consume(n);
        if frame.last() == Some(&b'\n') {
            return Ok(Some(frame));
        }
    }
}
pub(crate) fn encode(value: &impl Serialize, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "process frame budget exceeded",
        ));
    }
    Ok(bytes)
}
pub(crate) fn write_frame(
    writer: &mut impl Write,
    value: &impl Serialize,
    limit: usize,
) -> io::Result<()> {
    writer.write_all(&encode(value, limit)?)?;
    writer.flush()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_bound_and_partial_eof_are_rejected() {
        assert!(read_frame(&mut &b"long\n"[..], 4).is_err());
        assert!(read_frame(&mut &b"partial"[..], 40).is_err());
        assert_eq!(read_frame(&mut &b"ok\n"[..], 3).unwrap().unwrap(), b"ok\n");
    }
}

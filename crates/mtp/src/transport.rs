//! How containers reach the device. The USB implementation is in `usb.rs`; tests use the
//! simulator. A transport moves whole containers and knows nothing of operations.

use std::io::Read;

pub trait Transport: Send {
    /// Sends one container: the bytes of `head`, then everything `body` yields, ending the
    /// transfer (with a zero-length packet where USB needs one).
    fn send(&mut self, head: &[u8], body: &mut dyn Read) -> std::io::Result<()>;

    /// Receives one container. One longer than `limit` bytes is an error, so a misbehaving
    /// device cannot make TuneBox allocate without bound.
    fn receive(&mut self, limit: usize) -> std::io::Result<Vec<u8>>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn send(&mut self, head: &[u8], body: &mut dyn Read) -> std::io::Result<()> {
        (**self).send(head, body)
    }

    fn receive(&mut self, limit: usize) -> std::io::Result<Vec<u8>> {
        (**self).receive(limit)
    }
}

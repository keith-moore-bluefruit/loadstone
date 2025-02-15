//! XMODEM file transfer implementation.
//!
//! Provides methods to receive arbitrary byte streams through serial
//! via the XMODEM protocol.

use blue_hal::{
    hal::serial::{TimeoutRead, Write},
    utilities::xmodem,
};

/// The size of a single byte block retrieved from an XMODEM stream.
pub const BLOCK_SIZE: usize = xmodem::PAYLOAD_SIZE;

/// Generic file transfer iterator trait, returning an iterator over byte blocks.
pub trait FileTransfer: TimeoutRead + Write {
    fn blocks(&mut self, max_retries: Option<u32>) -> BlockIterator<Self> {
        BlockIterator {
            serial: self,
            received_block: false,
            finished: false,
            block_number: 0,
            max_retries,
        }
    }
}

impl<T: TimeoutRead + Write> FileTransfer for T {}

/// Generic iterator over byte blocks.
pub struct BlockIterator<'a, S: TimeoutRead + Write + ?Sized> {
    serial: &'a mut S,
    received_block: bool,
    finished: bool,
    block_number: u8,
    max_retries: Option<u32>,
}

impl<'a, S: TimeoutRead + Write + ?Sized> Iterator for BlockIterator<'a, S> {
    type Item = [u8; BLOCK_SIZE];

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        let mut retries = 0;
        let mut buffer = [0u8; xmodem::MAX_PACKET_SIZE];

        'block_loop: while self.max_retries.is_none() || retries < self.max_retries.unwrap() {
            let mut buffer_index = 0usize;

            let message = if self.received_block { xmodem::ACK } else { xmodem::NAK };
            if self.serial.write_char(message as char).is_err() {
                retries += 1;
                continue 'block_loop;
            }
            self.received_block = false;

            loop {
                buffer[buffer_index] = match self.serial.read(xmodem::DEFAULT_TIMEOUT) {
                    Ok(byte) => byte,
                    Err(_) => {
                        retries += 1;
                        continue 'block_loop;
                    }
                };

                if buffer_index == 0 || buffer_index == (xmodem::MAX_PACKET_SIZE - 1) {
                    if let Some(block) = self.process_message(&buffer) {
                        self.received_block = true;
                        return Some(block);
                    }

                    if self.finished {
                        return None;
                    }
                }
                buffer_index += 1;
                if buffer_index == xmodem::MAX_PACKET_SIZE {
                    continue 'block_loop;
                }
            }
        }

        // Fully timed out
        self.finished = true;
        None
    }
}

impl<'a, S: TimeoutRead + Write + ?Sized> BlockIterator<'a, S> {
    fn process_message(&mut self, buffer: &[u8]) -> Option<[u8; BLOCK_SIZE]> {
        match xmodem::parse_message(&buffer) {
            Ok((_, xmodem::Message::EndOfTransmission)) => {
                self.end_transmission();
                None
            }
            Ok((_, xmodem::Message::Chunk(chunk))) => {
                if let Some(block) = self.process_chunk(chunk) {
                    self.block_number = self.block_number.wrapping_add(1);
                    Some(block)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn process_chunk(&self, chunk: xmodem::Chunk) -> Option<[u8; BLOCK_SIZE]> {
        let next_block = self.block_number.wrapping_add(1);
        (chunk.block_number == next_block).then_some(chunk.payload)
    }

    fn end_transmission(&mut self) {
        self.finished = true;
        if self.serial.write_char(xmodem::ACK as char).is_err() {
            return;
        }
        if let Ok(xmodem::ETB) = self.serial.read(xmodem::DEFAULT_TIMEOUT) {
            // We don't care about this being received, as there's no
            // recovering from a failure here.
            let _ = self.serial.write_char(xmodem::ACK as char);
        }
    }
}

impl<'a, S: TimeoutRead + Write + ?Sized> Drop for BlockIterator<'a, S> {
    // Must fully consume the iterator on drop
    // to close the xmodem communication cleanly
    fn drop(&mut self) { self.for_each(drop); }
}

//! Streaming decode by file path.

use craft_audio::{AudioError, DecodeOptions};
use std::fs::File;
use std::path::Path;
use std::time::Duration;

/// A run of decoded audio: interleaved samples, full scale ±1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    pub samples: Vec<f32>,
    pub channels: u16,
}

impl Block {
    /// Sample frames in the block (one sample per channel each).
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }
}

/// Decodes one audio file from disk without loading it whole (where the format allows).
pub struct Decoder {
    inner: craft_audio::Decoder<'static>,
    sample_rate: u32,
    channels: u16,
    frames: Option<u64>,
}

impl std::fmt::Debug for Decoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoder").field("sample_rate", &self.sample_rate).field("channels", &self.channels).finish()
    }
}

impl Decoder {
    /// Opens `path`; the extension helps pick the format but the content decides.
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let file = File::open(path)?;
        let hint = path.extension().and_then(|e| e.to_str());
        let inner = craft_audio::Decoder::open(file, hint, &DecodeOptions::default())?;
        let info = inner.info();
        let (sample_rate, channels) = (info.sample_rate, info.channels);
        if sample_rate == 0 || channels == 0 {
            return Err(AudioError::Malformed("the file reports no sample rate or no channels".into()));
        }
        let frames = inner.length_known().then_some(info.frames);
        Ok(Self { inner, sample_rate, channels, frames })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// The length, when the file states it.
    pub fn duration(&self) -> Option<Duration> {
        self.frames.map(|f| Duration::from_secs_f64(f as f64 / f64::from(self.sample_rate)))
    }

    /// The next block, or `None` at the end of the file.
    pub fn next_block(&mut self) -> Result<Option<Block>, AudioError> {
        let Some(buffer) = self.inner.next_block()? else { return Ok(None) };
        Ok(Some(Block { samples: buffer.interleaved(), channels: buffer.num_channels() }))
    }

    /// Moves to `position`; returns where decoding will actually resume.
    pub fn seek(&mut self, position: Duration) -> Result<Duration, AudioError> {
        let frame = (position.as_secs_f64() * f64::from(self.sample_rate)) as u64;
        let landed = self.inner.seek(frame)?;
        Ok(Duration::from_secs_f64(landed as f64 / f64::from(self.sample_rate)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One Ogg page (RFC 3533) carrying `packet` whole, CRC included.
    fn ogg_page(packet: &[u8], header_type: u8, granule: u64, sequence: u32) -> Vec<u8> {
        let mut segments = vec![255u8; packet.len() / 255];
        segments.push((packet.len() % 255) as u8);
        let mut page = b"OggS".to_vec();
        page.push(0); // stream structure version
        page.push(header_type);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&7u32.to_le_bytes()); // stream serial number
        page.extend_from_slice(&sequence.to_le_bytes());
        page.extend_from_slice(&[0; 4]); // CRC, filled in below
        page.push(segments.len() as u8);
        page.extend_from_slice(&segments);
        page.extend_from_slice(packet);
        // CRC-32 with polynomial 0x04C11DB7, no reflection, initial value and final XOR zero.
        let crc = page.iter().fold(0u32, |crc, &byte| {
            (0..8).fold(crc ^ (u32::from(byte) << 24), |c, _| {
                if c & 0x8000_0000 != 0 { (c << 1) ^ 0x04C1_1DB7 } else { c << 1 }
            })
        });
        page[22..26].copy_from_slice(&crc.to_le_bytes());
        page
    }

    /// An Ogg Opus file (RFC 7845) of `packets` silent 20 ms stereo CELT frames, with 312
    /// samples of pre-skip, ending `end_trim` samples before the last packet does.
    fn silent_opus(packets: u64, end_trim: u64) -> Vec<u8> {
        let mut head = b"OpusHead".to_vec();
        head.push(1); // version
        head.push(2); // channels
        head.extend_from_slice(&312u16.to_le_bytes()); // pre-skip
        head.extend_from_slice(&44_100u32.to_le_bytes()); // the source's sample rate
        head.extend_from_slice(&0i16.to_le_bytes()); // output gain
        head.push(0); // channel mapping family 0: mono or stereo
        let mut file = ogg_page(&head, 0x02, 0, 0);
        let mut tags = b"OpusTags".to_vec();
        tags.extend_from_slice(&[0; 8]); // empty vendor string, no comments
        file.extend(ogg_page(&tags, 0, 0, 1));
        for index in 1..=packets {
            let last = index == packets;
            let granule = index * 960 - if last { end_trim } else { 0 };
            // TOC 0xFC: CELT-only fullband 20 ms, stereo, one frame; 0xFF 0xFE codes silence.
            file.extend(ogg_page(&[0xFC, 0xFF, 0xFE], if last { 0x04 } else { 0 }, granule, index as u32 + 1));
        }
        file
    }

    #[test]
    fn opus_decodes_to_its_stated_length() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quiet.opus");
        std::fs::write(&path, silent_opus(50, 100)).unwrap();

        let mut decoder = Decoder::open(&path).unwrap();
        assert_eq!((decoder.sample_rate(), decoder.channels()), (48_000, 2));
        // 50 packets of 960 samples, less the pre-skip at the start and the trim at the end.
        let expected = 50 * 960 - 312 - 100;
        assert_eq!(decoder.duration(), Some(Duration::from_secs_f64(expected as f64 / 48_000.0)));
        let mut frames = 0;
        while let Some(block) = decoder.next_block().unwrap() {
            // The decoder may leave tiny values (around 1e-34) rather than exact zeros.
            assert!(block.samples.iter().all(|s| s.abs() < 1e-6), "silent frames decode to silence");
            frames += block.frames();
        }
        assert_eq!(frames, expected);
    }
}

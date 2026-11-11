//! Channel mixing.

/// Mixes planar channels into one, at the average of the channels, so a centred voice keeps its
/// level and nothing clips.
pub fn mono_mix(block: &[Vec<f32>]) -> Vec<f32> {
    let frames = block.iter().map(Vec::len).min().unwrap_or(0);
    let channels = block.len().max(1) as f32;
    (0..frames).map(|frame| block.iter().filter_map(|c| c.get(frame)).sum::<f32>() / channels).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_the_channels() {
        assert_eq!(mono_mix(&[vec![1.0, 0.5, -1.0], vec![1.0, -0.5, 0.0]]), vec![1.0, 0.0, -0.5]);
        assert_eq!(mono_mix(&[vec![0.25]]), vec![0.25]);
        assert!(mono_mix(&[]).is_empty());
    }
}

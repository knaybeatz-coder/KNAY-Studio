//! Streaming tape-rate conversion after the complete mix, including hosted effects.
#[derive(Default)]
pub(crate) struct Varispeed {
    history: Vec<f32>,
    phase: f64,
    ratio: f64,
    active: bool,
}
impl Varispeed {
    pub fn new(ratio: f64) -> Self { Self { ratio, ..Self::default() } }
    pub fn process(&mut self, audio: &mut Vec<f32>, target: f64) {
        if audio.is_empty() { return; }
        if target == 1.0 && self.ratio == 1.0 && !self.active {
            self.history = audio[audio.len().saturating_sub(64)..].to_vec();
            self.phase = 0.0;
            return;
        }
        self.active = true;
        let frames = audio.len() / 2;
        let mut input = std::mem::take(&mut self.history);
        if input.len() < 64 { let mut pad = vec![0.0; 64-input.len()]; pad.extend(input); input=pad; }
        input.extend_from_slice(audio);
        audio.clear();
        // Sixteen frames of lookahead are supplied by a fixed causal delay. History
        // and fractional phase survive block boundaries and live engine rebuilds.
        while self.phase < frames as f64 {
            let pos = 16.0 + self.phase;
            let (l,r) = os_dsp::resample::read_by(input.len()/2, pos, self.ratio, os_dsp::resample::Quality::Table, |i| (input[2*i],input[2*i+1]));
            audio.extend([l,r]);
            self.ratio += (target-self.ratio).clamp(-0.0001,0.0001);
            if (target-self.ratio).abs() < 0.000001 {self.ratio=target;}
            self.phase += self.ratio;
        }
        self.phase -= frames as f64;
        self.history = input[input.len()-64..].to_vec();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tape_rate_changes_pitch_duration_and_preserves_block_continuity() {
        let source:Vec<f32>=(0..48000).flat_map(|i|{let v=(i as f32*std::f32::consts::TAU*440.0/48000.0).sin()*0.5;[v,v]}).collect();
        for ratio in [0.5,1.0,2.0] {
            let mut whole=source.clone();Varispeed::new(ratio).process(&mut whole,ratio);
            assert_eq!(whole.len()/2,(48000.0/ratio) as usize);
            let mut converter=Varispeed::new(ratio);let mut blocks=Vec::new();
            for block in source.chunks(512) {let mut b=block.to_vec();converter.process(&mut b,ratio);blocks.extend(b);}
            assert_eq!(blocks.len(),whole.len());
            assert!(blocks.iter().zip(&whole).all(|(a,b)|(a-b).abs()<0.00001));
            let wave:Vec<_>=whole.chunks_exact(2).map(|s|s[0]).collect();
            let crossings=wave.windows(2).skip(100).filter(|w|w[0]<=0.0 && w[1]>0.0).count();
            let hz=crossings as f64*48000.0/(wave.len()-101) as f64;
            assert!((hz-440.0*ratio).abs()<3.0,"{ratio}: {hz}");
            if ratio==1.0 {assert_eq!(source,whole);}
        }
    }
}

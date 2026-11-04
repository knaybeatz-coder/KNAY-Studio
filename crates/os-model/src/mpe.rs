//! Per-note expression data. Curve records are immutable once attached to a note;
//! editors append a replacement so note copies and undo remain independent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub upper: bool,
    pub members: u8,
    pub bend: u8,
    pub sensitivity: f32,
    pub pressure: f32,
    pub slide: f32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            upper: false,
            members: 15,
            bend: 48,
            sensitivity: 1.,
            pressure: 1.,
            slide: 1.,
        }
    }
}
impl Config {
    pub fn valid(self) -> Self {
        Self {
            members: self.members.clamp(1, 15),
            bend: self.bend.clamp(1, 96),
            sensitivity: finite(self.sensitivity, 1.).clamp(0.25, 2.),
            pressure: finite(self.pressure, 1.).clamp(0.25, 4.),
            slide: finite(self.slide, 1.).clamp(0.25, 4.),
            ..self
        }
    }
    pub fn manager(self) -> u8 {
        if self.upper { 15 } else { 0 }
    }
    pub fn member(self, ch: u8) -> bool {
        if self.upper {
            ch < 15 && ch >= 15 - self.members.clamp(1, 15)
        } else {
            ch > 0 && ch <= self.members.clamp(1, 15)
        }
    }
    pub fn channels(self) -> Vec<u8> {
        (0..16).filter(|&c| self.member(c)).collect()
    }
    pub fn setup(self) -> Vec<[u8; 3]> {
        let c = self.valid();
        let mut out = vec![];
        let mut rpn = |ch, n, v| {
            out.extend([
                [0xb0 | ch, 101, 0],
                [0xb0 | ch, 100, n],
                [0xb0 | ch, 6, v],
                [0xb0 | ch, 101, 127],
                [0xb0 | ch, 100, 127],
            ]);
        };
        rpn(c.manager(), 6, c.members);
        rpn(c.manager(), 0, 2);
        for ch in c.channels() {
            rpn(ch, 0, c.bend);
        }
        out
    }
}
fn finite(v: f32, f: f32) -> f32 {
    if v.is_finite() { v } else { f }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub at: f32,
    pub value: f32,
    pub smooth: bool,
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Expression {
    pub lanes: [Vec<Point>; 3],
}
impl Expression {
    pub fn value(&self, lane: usize, at: f32) -> f32 {
        let Some(p) = self.lanes.get(lane) else {
            return 0.;
        };
        let default = if lane == 2 { 0.5 } else { 0. };
        let Some(first) = p.first() else {
            return default;
        };
        let i = p.partition_point(|p| p.at <= at);
        if i == 0 {
            return first.value;
        }
        let a = p[i - 1];
        let Some(b) = p.get(i) else {
            return a.value;
        };
        if !a.smooth || b.at <= a.at {
            return a.value;
        }
        a.value + (b.value - a.value) * ((at - a.at) / (b.at - a.at)).clamp(0., 1.)
    }
    pub fn set(&mut self, lane: usize, at: f32, value: f32, smooth: bool) {
        if lane >= 3 || !at.is_finite() || !value.is_finite() {
            return;
        }
        let p = &mut self.lanes[lane];
        let at = at.clamp(0., 1.);
        let value = if lane == 0 {
            value.clamp(-96., 96.)
        } else {
            value.clamp(0., 1.)
        };
        p.retain(|p| (p.at - at).abs() > 0.0001);
        p.push(Point { at, value, smooth });
        p.sort_by(|a, b| a.at.total_cmp(&b.at));
    }
    pub fn remove(&mut self, lane: usize, at: f32) {
        if let Some(p) = self.lanes.get_mut(lane) {
            if let Some((i, _)) = p
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| (a.at - at).abs().total_cmp(&(b.at - at).abs()))
            {
                p.remove(i);
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Data {
    pub channels: Vec<(u16, Config)>,
    pub expressions: Vec<Expression>,
}
impl Data {
    pub fn config(&self, id: u16) -> Config {
        self.channels
            .iter()
            .find(|c| c.0 == id)
            .map_or(Config::default(), |c| c.1.valid())
    }
    pub fn set_config(&mut self, id: u16, c: Config) {
        self.channels.retain(|x| x.0 != id);
        self.channels.push((id, c.valid()));
    }
    pub fn get(&self, id: u32) -> Option<&Expression> {
        id.checked_sub(1)
            .and_then(|i| self.expressions.get(i as usize))
    }
    pub fn add(&mut self, e: Expression) -> u32 {
        self.expressions.push(e);
        self.expressions.len() as u32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mpe_curves_and_zones() {
        let c = Config::default();
        assert!(c.member(1) && c.member(15) && !c.member(0));
        let upper = Config {
            upper: true,
            members: 7,
            ..c
        };
        assert!(upper.member(8) && upper.member(14) && !upper.member(7) && !upper.member(15));
        assert_eq!(c.setup()[2], [0xb0, 6, 15]);
        let mut e = Expression::default();
        e.set(0, 0., -12., true);
        e.set(0, 1., 12., false);
        assert_eq!(e.value(0, 0.5), 0.);
        e.lanes[0][0].smooth = false;
        assert_eq!(e.value(0, 0.5), -12.);
        let mut data = Data::default();
        let a = data.add(e.clone());
        e.set(0, 0., 0., false);
        data.add(e);
        assert_eq!(data.get(a).unwrap().value(0, 0.), -12.);
    }
}

impl Expression {
    pub fn window(&self, from: f32, to: f32) -> Self {
        let from = from.clamp(0., 1.);
        let to = to.clamp(from, 1.);
        let span = (to - from).max(f32::EPSILON);
        let mut e = Self::default();
        for lane in 0..3 {
            if self.lanes[lane].is_empty() {
                continue;
            }
            let smooth = self.lanes[lane]
                .iter()
                .rev()
                .find(|p| p.at <= from)
                .is_some_and(|p| p.smooth);
            e.set(lane, 0., self.value(lane, from), smooth);
            for p in self.lanes[lane].iter().filter(|p| p.at > from && p.at < to) {
                e.set(lane, (p.at - from) / span, p.value, p.smooth);
            }
            e.set(lane, 1., self.value(lane, to), false);
        }
        e
    }
}

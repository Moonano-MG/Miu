//! Keyframe animation for scene nodes.
//!
//! An [`Animation`] owns one [`Track`] per animated node property. Tracks are
//! sampled independently, so a scene can run several animations at once (an idle
//! loop plus a one-shot entrance) without interfering. [`Animation::apply`] writes
//! sampled values back into the scene.
//!
//! Looping uses `time % duration`; a non-looping animation holds its final value.

use miu_core::Transform2D;

use crate::node::NodeId;
use crate::scene::{Scene, SceneError};

/// Interpolation curve applied between two keyframes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Easing {
    /// Constant rate.
    #[default]
    Linear = 0,
    /// Slow start.
    EaseIn = 1,
    /// Slow end.
    EaseOut = 2,
    /// Slow start and end.
    EaseInOut = 3,
    /// Holds the previous value until the keyframe time.
    Step = 4,
}

impl Easing {
    /// Converts a C ABI/document discriminant into an easing.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Linear),
            1 => Some(Self::EaseIn),
            2 => Some(Self::EaseOut),
            3 => Some(Self::EaseInOut),
            4 => Some(Self::Step),
            _ => None,
        }
    }

    /// Applies the curve to a normalised parameter in `[0, 1]`.
    pub fn evaluate(self, t: f32) -> f32 {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Self::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                }
            }
            Self::Step => {
                if t >= 1.0 { 1.0 } else { 0.0 }
            }
        }
    }
}

/// Node property a track drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NodeProperty {
    /// Local X translation.
    X = 0,
    /// Local Y translation.
    Y = 1,
    /// Clockwise rotation in radians.
    Rotation = 2,
    /// X scale.
    ScaleX = 3,
    /// Y scale.
    ScaleY = 4,
    /// Node opacity in `[0, 1]`.
    Opacity = 5,
    /// Painter layer (interpolated then rounded).
    Layer = 6,
}

impl NodeProperty {
    /// Parses a property name as used by scene documents.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "rotation" | "angle" => Some(Self::Rotation),
            "scale_x" | "scaleX" => Some(Self::ScaleX),
            "scale_y" | "scaleY" => Some(Self::ScaleY),
            "opacity" | "alpha" => Some(Self::Opacity),
            "layer" => Some(Self::Layer),
            _ => None,
        }
    }

    /// Stable property name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Rotation => "rotation",
            Self::ScaleX => "scale_x",
            Self::ScaleY => "scale_y",
            Self::Opacity => "opacity",
            Self::Layer => "layer",
        }
    }
}

/// One keyframe of a track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    /// Time in seconds from the start of the animation.
    pub time: f32,
    /// Value reached at `time`.
    pub value: f32,
    /// Curve used from this keyframe to the next one.
    pub easing: Easing,
}

impl Keyframe {
    /// Creates a keyframe.
    pub const fn new(time: f32, value: f32) -> Self {
        Self { time, value, easing: Easing::Linear }
    }

    /// Creates a keyframe with an easing curve.
    pub const fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }
}

/// Animation channel for one node property.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// Node animated by this track.
    pub node: NodeId,
    /// Property written when the track is applied.
    pub property: NodeProperty,
    /// Keyframes sorted by ascending time.
    pub keys: Vec<Keyframe>,
}

impl Track {
    /// Creates a track, sorting the keys by time.
    pub fn new(node: NodeId, property: NodeProperty, mut keys: Vec<Keyframe>) -> Self {
        keys.sort_by(|left, right| left.time.total_cmp(&right.time));
        Self { node, property, keys }
    }

    /// Time of the last keyframe, or zero for an empty track.
    pub fn end_time(&self) -> f32 {
        self.keys.last().map(|key| key.time).unwrap_or(0.0)
    }

    /// Samples the track at `time`.
    pub fn sample(&self, time: f32) -> Option<f32> {
        match self.keys.len() {
            0 => None,
            1 => self.keys.first().map(|key| key.value),
            _ => {
                let time = if time.is_finite() { time.max(0.0) } else { 0.0 };
                let first = self.keys.first()?;
                if time <= first.time {
                    return Some(first.value);
                }
                let last = self.keys.last()?;
                if time >= last.time {
                    return Some(last.value);
                }
                for pair in self.keys.windows(2) {
                    let start = pair[0];
                    let end = pair[1];
                    if time >= start.time && time <= end.time {
                        let span = end.time - start.time;
                        let local = if span > 0.0 { (time - start.time) / span } else { 1.0 };
                        let curved = start.easing.evaluate(local);
                        return Some(start.value + (end.value - start.value) * curved);
                    }
                }
                Some(last.value)
            }
        }
    }
}

/// A named collection of tracks with a duration.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// Name used by documents and scripting.
    pub name: String,
    /// Length in seconds. Tracks longer than this are still sampled to their end.
    pub duration: f32,
    /// Whether the animation restarts from zero when it reaches `duration`.
    pub looping: bool,
    /// Channels in application order.
    pub tracks: Vec<Track>,
}

impl Animation {
    /// Creates an empty animation.
    pub fn new(name: impl Into<String>, duration: f32) -> Self {
        Self {
            name: name.into(),
            duration: if duration.is_finite() && duration > 0.0 { duration } else { 1.0 },
            looping: false,
            tracks: Vec::new(),
        }
    }

    /// Marks the animation as looping.
    #[must_use]
    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Adds a track.
    pub fn add_track(&mut self, track: Track) -> &mut Self {
        self.tracks.push(track);
        self
    }

    /// Maps `time` into the animation's range, honouring looping.
    pub fn local_time(&self, time: f32) -> f32 {
        if !time.is_finite() {
            return 0.0;
        }
        let time = time.max(0.0);
        if self.looping && self.duration > 0.0 {
            time % self.duration
        } else {
            time.min(self.duration)
        }
    }

    /// Writes sampled values into the scene.
    pub fn apply(&self, scene: &mut Scene, time: f32) -> Result<(), SceneError> {
        let local = self.local_time(time);
        for track in &self.tracks {
            let Some(value) = track.sample(local) else {
                continue;
            };
            let Some(node) = scene.node_mut(track.node) else {
                continue;
            };
            match track.property {
                NodeProperty::X => node.transform.translation.x = value,
                NodeProperty::Y => node.transform.translation.y = value,
                NodeProperty::Rotation => node.transform.rotation_radians = value,
                NodeProperty::ScaleX => node.transform.scale.x = value,
                NodeProperty::ScaleY => node.transform.scale.y = value,
                NodeProperty::Opacity => node.opacity = value.clamp(0.0, 1.0),
                NodeProperty::Layer => node.layer = value.round() as i32,
            }
        }
        scene.touch();
        Ok(())
    }

    /// Applies the animation without cloning: convenience wrapper for `&mut Scene`.
    pub fn apply_to(&self, scene: &mut Scene, time: f32) {
        let _ = self.apply(scene, time);
    }
}

/// Time-based fade helper used by scene transitions and visual-novel effects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fade {
    duration: f32,
    elapsed: f32,
    from: f32,
    to: f32,
}

impl Fade {
    /// Creates a fade from `from` to `to` opacity over `duration` seconds.
    pub fn new(from: f32, to: f32, duration: f32) -> Self {
        Self {
            duration: if duration.is_finite() && duration > 0.0 { duration } else { 0.0 },
            elapsed: 0.0,
            from: from.clamp(0.0, 1.0),
            to: to.clamp(0.0, 1.0),
        }
    }

    /// Advances the fade and returns the current opacity.
    pub fn advance(&mut self, delta_seconds: f32) -> f32 {
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.elapsed += delta_seconds;
        }
        self.value()
    }

    /// Current opacity without advancing.
    pub fn value(&self) -> f32 {
        if self.duration <= 0.0 {
            return self.to;
        }
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * t
    }

    /// Whether the fade reached its target.
    pub fn finished(&self) -> bool {
        self.duration <= 0.0 || self.elapsed >= self.duration
    }

    /// Restarts the fade with new endpoints.
    pub fn restart(&mut self, from: f32, to: f32) {
        self.from = from.clamp(0.0, 1.0);
        self.to = to.clamp(0.0, 1.0);
        self.elapsed = 0.0;
    }
}

/// Builds an animation that loops a node's `y` position, useful as a sample.
pub fn bob(node: NodeId, amplitude: f32, period: f32) -> Animation {
    let period = if period.is_finite() && period > 0.0 { period } else { 1.0 };
    let mut animation = Animation::new("bob", period).looping(true);
    animation.add_track(Track::new(
        node,
        NodeProperty::Y,
        vec![
            Keyframe::new(0.0, 0.0).with_easing(Easing::EaseInOut),
            Keyframe::new(period * 0.5, amplitude).with_easing(Easing::EaseInOut),
            Keyframe::new(period, 0.0),
        ],
    ));
    animation
}

/// Convenience: identity transform used as a starting point by tools.
pub const IDENTITY: Transform2D = Transform2D::IDENTITY;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;
    use miu_core::Vec2;

    fn scene_with_node() -> (Scene, NodeId) {
        let mut scene = Scene::new(Vec2::new(64.0, 64.0)).unwrap();
        let id = scene.add(Node::group("target")).unwrap();
        (scene, id)
    }

    #[test]
    fn keys_are_sorted_and_sampled() {
        let track = Track::new(
            1,
            NodeProperty::X,
            vec![Keyframe::new(1.0, 100.0), Keyframe::new(0.0, 0.0)],
        );
        assert_eq!(track.keys[0].time, 0.0);
        assert_eq!(track.sample(0.5), Some(50.0));
        assert_eq!(track.sample(-10.0), Some(0.0));
        assert_eq!(track.sample(10.0), Some(100.0));
    }

    #[test]
    fn easing_curves_are_monotonic() {
        for easing in [Easing::Linear, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
            let mut previous = easing.evaluate(0.0);
            for step in 1..=10 {
                let value = easing.evaluate(step as f32 / 10.0);
                assert!(value >= previous - 1.0e-6, "{:?} went backwards", easing);
                previous = value;
            }
            assert!((easing.evaluate(1.0) - 1.0).abs() < 1.0e-6);
        }
    }

    #[test]
    fn step_easing_holds_until_the_end() {
        assert_eq!(Easing::Step.evaluate(0.99), 0.0);
        assert_eq!(Easing::Step.evaluate(1.0), 1.0);
    }

    #[test]
    fn applying_animation_writes_properties() {
        let (mut scene, id) = scene_with_node();
        let mut animation = Animation::new("move", 1.0);
        animation.add_track(Track::new(
            id,
            NodeProperty::X,
            vec![Keyframe::new(0.0, 0.0), Keyframe::new(1.0, 64.0)],
        ));
        animation.add_track(Track::new(
            id,
            NodeProperty::Opacity,
            vec![Keyframe::new(0.0, 1.0), Keyframe::new(1.0, 0.0)],
        ));
        animation.apply(&mut scene, 0.5).unwrap();
        let node = scene.node(id).unwrap();
        assert!((node.transform.translation.x - 32.0).abs() < 0.01);
        assert!((node.opacity - 0.5).abs() < 0.01);
    }

    #[test]
    fn looping_wraps_time_and_holding_does_not() {
        let looping = Animation::new("loop", 2.0).looping(true);
        assert!((looping.local_time(5.0) - 1.0).abs() < 0.001);
        let once = Animation::new("once", 2.0);
        assert!((once.local_time(5.0) - 2.0).abs() < 0.001);
    }

    #[test]
    fn bob_builds_a_looping_track() {
        let animation = bob(7, 10.0, 2.0);
        assert!(animation.looping);
        assert_eq!(animation.tracks.len(), 1);
        assert_eq!(animation.tracks[0].sample(1.0), Some(10.0));
    }

    #[test]
    fn fades_interpolate_and_finish() {
        let mut fade = Fade::new(0.0, 1.0, 1.0);
        assert!((fade.advance(0.5) - 0.5).abs() < 0.001);
        assert!(!fade.finished());
        assert!((fade.advance(1.0) - 1.0).abs() < 0.001);
        assert!(fade.finished());
    }
}

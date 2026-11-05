use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use crate::color::Color;

pub const HISTOGRAM_BINS: usize = 4096;
pub const HISTOGRAM_MAX: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticObject {
    Solaris,
    Vesper,
    Selene,
    Nymara,
    RingInner,
    RingMiddle,
    RingOuter,
    Cryon,
    Veridis,
    Umbra,
    Solene,
}

impl DiagnosticObject {
    pub const ALL: [Self; 11] = [
        Self::Solaris,
        Self::Vesper,
        Self::Selene,
        Self::Nymara,
        Self::RingInner,
        Self::RingMiddle,
        Self::RingOuter,
        Self::Cryon,
        Self::Veridis,
        Self::Umbra,
        Self::Solene,
    ];

    fn index(self) -> usize {
        match self {
            Self::Solaris => 0,
            Self::Vesper => 1,
            Self::Selene => 2,
            Self::Nymara => 3,
            Self::RingInner => 4,
            Self::RingMiddle => 5,
            Self::RingOuter => 6,
            Self::Cryon => 7,
            Self::Veridis => 8,
            Self::Umbra => 9,
            Self::Solene => 10,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Solaris => "Solaris",
            Self::Vesper => "Vesper",
            Self::Selene => "Selene",
            Self::Nymara => "Nymara",
            Self::RingInner => "Nymara ring inner",
            Self::RingMiddle => "Nymara ring middle",
            Self::RingOuter => "Nymara ring outer",
            Self::Cryon => "Cryon",
            Self::Veridis => "Veridis",
            Self::Umbra => "Umbra",
            Self::Solene => "Solene",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DiagnosticSample {
    pub albedo: Color,
    pub emission: Color,
    pub ambient_component: Color,
    pub direct_component: Color,
    pub material_pre_emission: Color,
    pub combined_pre_clamp: Color,
    pub display_color: Color,
    pub shadow_visibility: Option<f32>,
    pub material_intensity: f32,
}

#[derive(Clone, Debug)]
pub struct DiagnosticsHandle(Rc<RefCell<DiagnosticAccumulator>>);

impl PartialEq for DiagnosticsHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl DiagnosticsHandle {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(DiagnosticAccumulator::default())))
    }

    pub fn record(&self, object: DiagnosticObject, sample: DiagnosticSample) {
        self.0.borrow_mut().record(object, sample);
    }

    pub fn report(&self) -> String {
        self.0.borrow().report()
    }
}

#[derive(Clone, Debug)]
pub struct DiagnosticAccumulator {
    objects: [ObjectStatistics; 11],
}

impl Default for DiagnosticAccumulator {
    fn default() -> Self {
        Self {
            objects: std::array::from_fn(|_| ObjectStatistics::default()),
        }
    }
}

#[derive(Clone, Debug)]
struct ObjectStatistics {
    effective_fragments: u64,
    illuminated_fragments: u64,
    shadowed_fragments: u64,
    material_intensity_min: f32,
    material_intensity_max: f32,
    components: [[ChannelStatistics; 3]; 6],
}

impl Default for ObjectStatistics {
    fn default() -> Self {
        Self {
            effective_fragments: 0,
            illuminated_fragments: 0,
            shadowed_fragments: 0,
            material_intensity_min: f32::INFINITY,
            material_intensity_max: f32::NEG_INFINITY,
            components: std::array::from_fn(|_| std::array::from_fn(|_| ChannelStatistics::default())),
        }
    }
}

#[derive(Clone, Debug)]
struct ChannelStatistics {
    observations: u64,
    non_finite: u64,
    negative: u64,
    saturated: u64,
    finite_nonnegative: u64,
    min: f32,
    max: f32,
    sum: f64,
    max_excess: f32,
    clamp_loss: f64,
    bins: Box<[u64; HISTOGRAM_BINS]>,
    overflow: u64,
}

impl Default for ChannelStatistics {
    fn default() -> Self {
        Self {
            observations: 0,
            non_finite: 0,
            negative: 0,
            saturated: 0,
            finite_nonnegative: 0,
            min: f32::INFINITY,
            max: f32::NEG_INFINITY,
            sum: 0.0,
            max_excess: 0.0,
            clamp_loss: 0.0,
            bins: Box::new([0; HISTOGRAM_BINS]),
            overflow: 0,
        }
    }
}

impl ChannelStatistics {
    fn record(&mut self, value: f32) {
        self.observations += 1;
        if !value.is_finite() {
            self.non_finite += 1;
            return;
        }
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        if value < 0.0 {
            self.negative += 1;
            return;
        }
        self.finite_nonnegative += 1;
        self.sum += value as f64;
        if value > 1.0 {
            self.saturated += 1;
            let excess = value - 1.0;
            self.max_excess = self.max_excess.max(excess);
            self.clamp_loss += excess as f64;
        }
        if value >= HISTOGRAM_MAX {
            self.overflow += 1;
        } else {
            let bin = ((value / HISTOGRAM_MAX) * HISTOGRAM_BINS as f32)
                .floor()
                .clamp(0.0, (HISTOGRAM_BINS - 1) as f32) as usize;
            self.bins[bin] += 1;
        }
    }

    fn percentile(&self, fraction: f64) -> Option<f32> {
        if self.finite_nonnegative == 0 {
            return None;
        }
        let rank = ((self.finite_nonnegative as f64) * fraction)
            .ceil()
            .max(1.0) as u64;
        let mut cumulative = 0u64;
        for (index, count) in self.bins.iter().copied().enumerate() {
            cumulative += count;
            if cumulative >= rank {
                return Some(((index as f32 + 0.5) / HISTOGRAM_BINS as f32) * HISTOGRAM_MAX);
            }
        }
        None
    }

    fn average(&self) -> Option<f32> {
        (self.finite_nonnegative > 0).then_some((self.sum / self.finite_nonnegative as f64) as f32)
    }

    fn format(&self, effective_fragments: u64) -> String {
        let percentage = if effective_fragments == 0 {
            0.0
        } else {
            self.saturated as f64 * 100.0 / effective_fragments as f64
        };
        let percentile = |p: f64| {
            self.percentile(p)
                .map(|value| format!("{value:.6}"))
                .unwrap_or_else(|| "N/E".to_string())
        };
        format!(
            "count={} sat={} ({percentage:.4}%) min={} max={} avg={} p50={} p90={} p95={} p99={} p99.9={} headroom_min={} excess_max={} clamp_loss={} negative={} nonfinite={} overflow={}",
            self.observations,
            self.saturated,
            format_f32(self.min),
            format_f32(self.max),
            self.average().map(format_f32).unwrap_or_else(|| "N/E".into()),
            percentile(0.50),
            percentile(0.90),
            percentile(0.95),
            percentile(0.99),
            percentile(0.999),
            self.max.is_finite().then(|| format_f32(1.0 - self.max)).unwrap_or_else(|| "N/E".into()),
            format_f32(self.max_excess),
            format_f32(self.clamp_loss as f32),
            self.negative,
            self.non_finite,
            self.overflow,
        )
    }
}

impl DiagnosticAccumulator {
    fn record(&mut self, object: DiagnosticObject, sample: DiagnosticSample) {
        let stats = &mut self.objects[object.index()];
        stats.effective_fragments += 1;
        if sample.material_intensity.is_finite() {
            stats.material_intensity_min = stats.material_intensity_min.min(sample.material_intensity);
            stats.material_intensity_max = stats.material_intensity_max.max(sample.material_intensity);
        }
        match sample.shadow_visibility {
            Some(visibility) if visibility == 0.0 => stats.shadowed_fragments += 1,
            Some(_) => stats.illuminated_fragments += 1,
            None => {}
        }
        let values = [
            sample.albedo,
            sample.emission,
            sample.ambient_component,
            sample.direct_component,
            sample.material_pre_emission,
            sample.combined_pre_clamp,
        ];
        for (component, color) in values.into_iter().enumerate() {
            stats.components[component][0].record(color.r);
            stats.components[component][1].record(color.g);
            stats.components[component][2].record(color.b);
        }
    }

    fn report(&self) -> String {
        const COMPONENT_NAMES: [&str; 6] = [
            "albedo",
            "emission",
            "ambient",
            "direct",
            "pre_emission",
            "combined_pre_clamp",
        ];
        let mut output = String::from("\n=== AETHERIA saturation diagnostics ===\n");
        output.push_str("Statistics count effective fragments that passed depth testing and reached shader output. Percentiles are histogram approximations; N/E means not estimable (usually overflow).\n");
        output.push_str("Object | fragments | saturated R/G/B | max R/G/B | illuminated | shadowed | material intensity min/max\n");
        for (index, object) in DiagnosticObject::ALL.into_iter().enumerate() {
            let stats = &self.objects[index];
            let combined = &stats.components[5];
            let _ = writeln!(
                output,
                "{} | {} | {}/{}/{} | {}/{}/{} | {} | {} | {}/{}",
                object.name(),
                stats.effective_fragments,
                combined[0].saturated,
                combined[1].saturated,
                combined[2].saturated,
                format_f32(combined[0].max),
                format_f32(combined[1].max),
                format_f32(combined[2].max),
                stats.illuminated_fragments,
                stats.shadowed_fragments,
                format_f32(stats.material_intensity_min),
                format_f32(stats.material_intensity_max),
            );
        }
        output.push_str("\nPer-object/channel/component statistics:\n");
        for (index, object) in DiagnosticObject::ALL.into_iter().enumerate() {
            let stats = &self.objects[index];
            let _ = writeln!(output, "\n[{}] fragments={}", object.name(), stats.effective_fragments);
            for (component, name) in COMPONENT_NAMES.into_iter().enumerate() {
                for (channel, label) in [(0, 'R'), (1, 'G'), (2, 'B')] {
                    let _ = writeln!(output, "  {name} {label}: {}", stats.components[component][channel].format(stats.effective_fragments));
                }
            }
        }
        output
    }
}

fn format_f32(value: f32) -> String {
    if value.is_finite() { format!("{value:.6}") } else { "N/E".to_string() }
}
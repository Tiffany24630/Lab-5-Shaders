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

    pub(crate) fn index(self) -> usize {
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
    pub lava: Option<LavaDiagnostic>,
}

#[derive(Clone, Copy, Debug)]
pub struct LavaDiagnostic {
    pub activity: f32,
    pub lava_intensity: f32,
    pub lava_strength: f32,
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
    vesper_correlation: VesperCorrelation,
}

impl Default for DiagnosticAccumulator {
    fn default() -> Self {
        Self {
            objects: std::array::from_fn(|_| ObjectStatistics::default()),
            vesper_correlation: VesperCorrelation::default(),
        }
    }
}

#[derive(Clone, Debug)]
struct VesperCorrelation {
    total: u64,
    emission_red_over_one: u64,
    combined_red_over_one: u64,
    combined_only_after_emission: u64,
    emission_and_combined_over_one: u64,
    combined_over_090: [u64; 3],
    combined_over_095: [u64; 3],
    emission_red_bins: [u64; 6],
    combined_red_saturated_by_emission_bin: [u64; 6],
    max_emission: [f32; 3],
    max_direct: [f32; 3],
    max_ambient: [f32; 3],
    max_pre_emission: [f32; 3],
    max_combined: [f32; 3],
    max_excess_red: f32,
    clamp_loss_red: f64,
    lava_bins: [LavaBinStatistics; 6],
    lava_emission_cross: [[u64; 6]; 6],
    saturated_lava: LavaDistribution,
    nonsaturated_lava: LavaDistribution,
    zero_lava: LavaRegionStatistics,
    positive_lava: LavaRegionStatistics,
    excess_classification: [u64; 4],
    activity_min: f32,
    activity_max: f32,
    lava_intensity_min: f32,
    lava_intensity_max: f32,
    lava_strength_min: f32,
    lava_strength_max: f32,
    curves: [CurveStatistics; CURVE_COUNT],
}

#[derive(Clone, Debug, Default)]
struct LavaRegionStatistics {
    fragments: u64,
    emission_sum: f64,
    emission_max: f32,
    combined_sum: f64,
    combined_max: f32,
    over_090: u64,
    over_095: u64,
    over_one: u64,
}

impl LavaRegionStatistics {
    fn record(&mut self, emission: f32, combined: f32) {
        self.fragments += 1;
        if emission.is_finite() { self.emission_sum += emission as f64; self.emission_max = self.emission_max.max(emission); }
        if combined.is_finite() { self.combined_sum += combined as f64; self.combined_max = self.combined_max.max(combined); }
        self.over_090 += (combined > 0.90) as u64;
        self.over_095 += (combined > 0.95) as u64;
        self.over_one += (combined > 1.0) as u64;
    }
}

#[derive(Clone, Debug)]
struct LavaBinStatistics {
    fragments: u64,
    emission_over_one: u64,
    combined_over_one: u64,
    combined_over_095: u64,
    combined_over_090: u64,
    emission_sum: f64,
    emission_max: f32,
    combined_sum: f64,
    combined_max: f32,
}

impl Default for LavaBinStatistics {
    fn default() -> Self {
        Self {
            fragments: 0,
            emission_over_one: 0,
            combined_over_one: 0,
            combined_over_095: 0,
            combined_over_090: 0,
            emission_sum: 0.0,
            emission_max: f32::NEG_INFINITY,
            combined_sum: 0.0,
            combined_max: f32::NEG_INFINITY,
        }
    }
}

const LAVA_HISTOGRAM_BINS: usize = 256;
const LAVA_HISTOGRAM_MAX: f32 = 2.0;
const CURVE_COUNT: usize = 16;
const CURVE_HISTOGRAM_BINS: usize = 2048;
const CURVE_HISTOGRAM_MAX: f32 = 2.0;

const CURVE_SPECS: [(f32, f32); CURVE_COUNT] = [
    (1.00, 1.00),
    (0.50, 0.90), (0.50, 0.80), (0.50, 0.70),
    (0.60, 0.90), (0.60, 0.80), (0.60, 0.70),
    (0.65, 0.90), (0.65, 0.80), (0.65, 0.70),
    (0.70, 0.90), (0.70, 0.80), (0.70, 0.70),
    (0.75, 0.90), (0.75, 0.80), (0.75, 0.70),
];

const CURVE_NAMES: [&str; CURVE_COUNT] = [
    "Original", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O",
];

#[derive(Clone, Debug)]
struct CurveBinSummary {
    count: u64,
    saturated: u64,
    over_095: u64,
    over_090: u64,
}

impl Default for CurveBinSummary {
    fn default() -> Self { Self { count: 0, saturated: 0, over_095: 0, over_090: 0 } }
}

#[derive(Clone, Debug)]
struct CurveStatistics {
    count: u64,
    saturated: u64,
    over_095: u64,
    over_090: u64,
    emission_over_one: u64,
    max_rgb: [f32; 3],
    red_sum: f64,
    red_max_excess: f32,
    red_clamp_loss: f64,
    red_bins: Box<[u64; CURVE_HISTOGRAM_BINS]>,
    red_overflow: u64,
    original_emission_sum: [f64; 3],
    simulated_emission_sum: [f64; 3],
    max_emission_reduction: f32,
    by_lava: [CurveBinSummary; 5],
}

impl Default for CurveStatistics {
    fn default() -> Self {
        Self {
            count: 0,
            saturated: 0,
            over_095: 0,
            over_090: 0,
            emission_over_one: 0,
            max_rgb: [f32::NEG_INFINITY; 3],
            red_sum: 0.0,
            red_max_excess: 0.0,
            red_clamp_loss: 0.0,
            red_bins: Box::new([0; CURVE_HISTOGRAM_BINS]),
            red_overflow: 0,
            original_emission_sum: [0.0; 3],
            simulated_emission_sum: [0.0; 3],
            max_emission_reduction: 0.0,
            by_lava: std::array::from_fn(|_| CurveBinSummary::default()),
        }
    }
}

impl CurveStatistics {
    fn record_red(&mut self, value: f32) {
        self.red_sum += value as f64;
        if value > 1.0 {
            self.saturated += 1;
            self.red_max_excess = self.red_max_excess.max(value - 1.0);
            self.red_clamp_loss += (value - 1.0) as f64;
        }
        if value > 0.95 { self.over_095 += 1; }
        if value > 0.90 { self.over_090 += 1; }
        if value >= CURVE_HISTOGRAM_MAX {
            self.red_overflow += 1;
        } else {
            let bin = ((value / CURVE_HISTOGRAM_MAX) * CURVE_HISTOGRAM_BINS as f32)
                .floor().clamp(0.0, (CURVE_HISTOGRAM_BINS - 1) as f32) as usize;
            self.red_bins[bin] += 1;
        }
        self.red_sum += 0.0;
    }

    fn percentile(&self, fraction: f64) -> Option<f32> {
        if self.count == 0 { return None; }
        let rank = ((self.count as f64) * fraction).ceil().max(1.0) as u64;
        let mut cumulative = 0;
        for (index, count) in self.red_bins.iter().copied().enumerate() {
            cumulative += count;
            if cumulative >= rank {
                return Some(((index as f32 + 0.5) / CURVE_HISTOGRAM_BINS as f32) * CURVE_HISTOGRAM_MAX);
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
struct LavaDistribution {
    count: u64,
    negative: u64,
    non_finite: u64,
    min: f32,
    max: f32,
    sum: f64,
    bins: Box<[u64; LAVA_HISTOGRAM_BINS]>,
    overflow: u64,
}

impl Default for LavaDistribution {
    fn default() -> Self {
        Self {
            count: 0,
            negative: 0,
            non_finite: 0,
            min: f32::INFINITY,
            max: f32::NEG_INFINITY,
            sum: 0.0,
            bins: Box::new([0; LAVA_HISTOGRAM_BINS]),
            overflow: 0,
        }
    }
}

impl LavaDistribution {
    fn record(&mut self, value: f32) {
        self.count += 1;
        if !value.is_finite() { self.non_finite += 1; return; }
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        if value < 0.0 { self.negative += 1; return; }
        self.sum += value as f64;
        if value >= LAVA_HISTOGRAM_MAX {
            self.overflow += 1;
        } else {
            let bin = ((value / LAVA_HISTOGRAM_MAX) * LAVA_HISTOGRAM_BINS as f32)
                .floor().clamp(0.0, (LAVA_HISTOGRAM_BINS - 1) as f32) as usize;
            self.bins[bin] += 1;
        }
    }

    fn percentile(&self, fraction: f64) -> Option<f32> {
        let valid = self.count.saturating_sub(self.negative + self.non_finite);
        if valid == 0 { return None; }
        let rank = ((valid as f64) * fraction).ceil().max(1.0) as u64;
        let mut cumulative = 0;
        for (index, count) in self.bins.iter().copied().enumerate() {
            cumulative += count;
            if cumulative >= rank {
                return Some(((index as f32 + 0.5) / LAVA_HISTOGRAM_BINS as f32) * LAVA_HISTOGRAM_MAX);
            }
        }
        None
    }

    fn mean(&self) -> Option<f32> {
        let valid = self.count.saturating_sub(self.negative + self.non_finite);
        (valid > 0).then_some((self.sum / valid as f64) as f32)
    }
}

impl Default for VesperCorrelation {
    fn default() -> Self {
        Self {
            total: 0,
            emission_red_over_one: 0,
            combined_red_over_one: 0,
            combined_only_after_emission: 0,
            emission_and_combined_over_one: 0,
            combined_over_090: [0; 3],
            combined_over_095: [0; 3],
            emission_red_bins: [0; 6],
            combined_red_saturated_by_emission_bin: [0; 6],
            max_emission: [f32::NEG_INFINITY; 3],
            max_direct: [f32::NEG_INFINITY; 3],
            max_ambient: [f32::NEG_INFINITY; 3],
            max_pre_emission: [f32::NEG_INFINITY; 3],
            max_combined: [f32::NEG_INFINITY; 3],
            max_excess_red: 0.0,
            clamp_loss_red: 0.0,
            lava_bins: std::array::from_fn(|_| LavaBinStatistics::default()),
            lava_emission_cross: [[0; 6]; 6],
            saturated_lava: LavaDistribution::default(),
            nonsaturated_lava: LavaDistribution::default(),
            zero_lava: LavaRegionStatistics { emission_max: f32::NEG_INFINITY, combined_max: f32::NEG_INFINITY, ..LavaRegionStatistics::default() },
            positive_lava: LavaRegionStatistics { emission_max: f32::NEG_INFINITY, combined_max: f32::NEG_INFINITY, ..LavaRegionStatistics::default() },
            excess_classification: [0; 4],
            activity_min: f32::INFINITY,
            activity_max: f32::NEG_INFINITY,
            lava_intensity_min: f32::INFINITY,
            lava_intensity_max: f32::NEG_INFINITY,
            lava_strength_min: f32::INFINITY,
            lava_strength_max: f32::NEG_INFINITY,
            curves: std::array::from_fn(|_| CurveStatistics::default()),
        }
    }
}

impl VesperCorrelation {
    fn record(&mut self, sample: DiagnosticSample) {
        self.total += 1;
        let emission_red = sample.emission.r;
        let combined_red = sample.combined_pre_clamp.r;
        let emission_over_one = emission_red > 1.0;
        let combined_over_one = combined_red > 1.0;
        if emission_over_one {
            self.emission_red_over_one += 1;
        }
        if combined_over_one {
            self.combined_red_over_one += 1;
            self.max_excess_red = self.max_excess_red.max(combined_red - 1.0);
            self.clamp_loss_red += (combined_red - 1.0) as f64;
        }
        if !emission_over_one && combined_over_one {
            self.combined_only_after_emission += 1;
        }
        if emission_over_one && combined_over_one {
            self.emission_and_combined_over_one += 1;
        }

        let combined = [sample.combined_pre_clamp.r, sample.combined_pre_clamp.g, sample.combined_pre_clamp.b];
        for channel in 0..3 {
            if combined[channel] > 0.90 { self.combined_over_090[channel] += 1; }
            if combined[channel] > 0.95 { self.combined_over_095[channel] += 1; }
        }

        let bin = if emission_red < 0.25 { 0 }
            else if emission_red < 0.50 { 1 }
            else if emission_red < 0.75 { 2 }
            else if emission_red <= 1.00 { 3 }
            else if emission_red <= 1.25 { 4 }
            else { 5 };
        self.emission_red_bins[bin] += 1;
        if combined_over_one {
            self.combined_red_saturated_by_emission_bin[bin] += 1;
        }

        let components = [
            sample.emission,
            sample.direct_component,
            sample.ambient_component,
            sample.material_pre_emission,
            sample.combined_pre_clamp,
        ];
        let maxima = [
            &mut self.max_emission,
            &mut self.max_direct,
            &mut self.max_ambient,
            &mut self.max_pre_emission,
            &mut self.max_combined,
        ];
        for (values, maximums) in components.into_iter().zip(maxima) {
            for channel in 0..3 {
                let value = color_channel(values, channel);
                if value.is_finite() {
                    maximums[channel] = maximums[channel].max(value);
                }
            }
        }
    }

    fn record_lava(&mut self, sample: DiagnosticSample, lava: LavaDiagnostic) {
        let strength = lava.lava_strength;
        self.activity_min = self.activity_min.min(lava.activity);
        self.activity_max = self.activity_max.max(lava.activity);
        self.lava_intensity_min = self.lava_intensity_min.min(lava.lava_intensity);
        self.lava_intensity_max = self.lava_intensity_max.max(lava.lava_intensity);
        self.lava_strength_min = self.lava_strength_min.min(strength);
        self.lava_strength_max = self.lava_strength_max.max(strength);

        let lava_bin = if strength < 0.10 { 0 }
            else if strength < 0.25 { 1 }
            else if strength < 0.50 { 2 }
            else if strength < 0.75 { 3 }
            else if strength <= 1.00 { 4 }
            else { 5 };
        let emission_bin = if sample.emission.r < 0.25 { 0 }
            else if sample.emission.r < 0.50 { 1 }
            else if sample.emission.r < 0.75 { 2 }
            else if sample.emission.r <= 1.00 { 3 }
            else if sample.emission.r <= 1.25 { 4 }
            else { 5 };
        self.lava_emission_cross[lava_bin][emission_bin] += 1;
        let combined = sample.combined_pre_clamp.r;
        let emission = sample.emission.r;

        if strength == 0.0 {
            self.zero_lava.record(emission, combined);
        } else if strength > 0.0 {
            self.positive_lava.record(emission, combined);
        }

        let bin = &mut self.lava_bins[lava_bin];
        bin.fragments += 1;
        bin.emission_over_one += (emission > 1.0) as u64;
        bin.combined_over_one += (combined > 1.0) as u64;
        bin.combined_over_095 += (combined > 0.95) as u64;
        bin.combined_over_090 += (combined > 0.90) as u64;
        if emission.is_finite() {
            bin.emission_sum += emission as f64;
            bin.emission_max = bin.emission_max.max(emission);
        }
        if combined.is_finite() {
            bin.combined_sum += combined as f64;
            bin.combined_max = bin.combined_max.max(combined);
        }

        if combined > 1.0 {
            self.saturated_lava.record(strength);
        } else {
            self.nonsaturated_lava.record(strength);
        }

        if emission > 1.0 {
            self.excess_classification[0] += 1;
        } else if emission + sample.ambient_component.r > 1.0 {
            self.excess_classification[1] += 1;
        } else if combined > 1.0 {
            self.excess_classification[2] += 1;
        } else {
            self.excess_classification[3] += 1;
        }
        self.record_curves(sample, strength);
    }

    fn record_curves(&mut self, sample: DiagnosticSample, strength: f32) {
        let lava_bin = if strength < 0.10 { 0 }
            else if strength < 0.25 { 1 }
            else if strength < 0.50 { 2 }
            else if strength < 0.75 { 3 }
            else { 4 };
        for (index, (threshold, minimum)) in CURVE_SPECS.into_iter().enumerate() {
            let factor = if index == 0 || strength <= threshold {
                1.0
            } else {
                1.0 - (1.0 - minimum) * ((strength - threshold) / (1.0 - threshold))
            };
            let simulated_emission = Color::new_const(
                sample.emission.r * factor,
                sample.emission.g * factor,
                sample.emission.b * factor,
            );
            let combined = Color::new_const(
                sample.material_pre_emission.r + simulated_emission.r,
                sample.material_pre_emission.g + simulated_emission.g,
                sample.material_pre_emission.b + simulated_emission.b,
            );
            let stats = &mut self.curves[index];
            stats.count += 1;
            stats.record_red(combined.r);
            stats.emission_over_one += (simulated_emission.r > 1.0) as u64;
            for channel in 0..3 {
                let value = color_channel(combined, channel);
                stats.max_rgb[channel] = stats.max_rgb[channel].max(value);
                stats.original_emission_sum[channel] += color_channel(sample.emission, channel) as f64;
                stats.simulated_emission_sum[channel] += color_channel(simulated_emission, channel) as f64;
            }
            stats.max_emission_reduction = stats.max_emission_reduction.max(1.0 - factor);
            let lava_stats = &mut stats.by_lava[lava_bin];
            lava_stats.count += 1;
            lava_stats.saturated += (combined.r > 1.0) as u64;
            lava_stats.over_095 += (combined.r > 0.95) as u64;
            lava_stats.over_090 += (combined.r > 0.90) as u64;
        }
    }

    fn report(&self) -> String {
        let mut output = String::from("\n=== Vesper emission correlation ===\n");
        let _ = writeln!(output, "effective fragments: {}", self.total);
        let _ = writeln!(output, "emission R > 1: {}", self.emission_red_over_one);
        let _ = writeln!(output, "combined R > 1: {}", self.combined_red_over_one);
        let _ = writeln!(output, "combined R > 1 with emission R <= 1: {}", self.combined_only_after_emission);
        let _ = writeln!(output, "emission R > 1 and combined R > 1: {}", self.emission_and_combined_over_one);
        let _ = writeln!(output, "combined > 0.90 RGB: {}/{}/{}", self.combined_over_090[0], self.combined_over_090[1], self.combined_over_090[2]);
        let _ = writeln!(output, "combined > 0.95 RGB: {}/{}/{}", self.combined_over_095[0], self.combined_over_095[1], self.combined_over_095[2]);
        let _ = writeln!(output, "emission R bins [0,.25), [.25,.50), [.50,.75), [.75,1], (1,1.25], (1.25,+inf): counts={:?} combined_R_saturated={:?} (sum={})", self.emission_red_bins, self.combined_red_saturated_by_emission_bin, self.emission_red_bins.iter().sum::<u64>());
        let _ = writeln!(output, "max emission RGB: {}", format_rgb(self.max_emission));
        let _ = writeln!(output, "max direct RGB: {}", format_rgb(self.max_direct));
        let _ = writeln!(output, "max ambient RGB: {}", format_rgb(self.max_ambient));
        let _ = writeln!(output, "max pre-emission RGB: {}", format_rgb(self.max_pre_emission));
        let _ = writeln!(output, "max combined RGB: {}", format_rgb(self.max_combined));
        let _ = writeln!(output, "combined R max excess: {:.6}", self.max_excess_red);
        let _ = writeln!(output, "combined R clamp loss: {:.6}", self.clamp_loss_red);
        let _ = writeln!(output, "activity range: {:.6}/{:.6}; lava_intensity range: {:.6}/{:.6}; lava_strength range: {:.6}/{:.6}", self.activity_min, self.activity_max, self.lava_intensity_min, self.lava_intensity_max, self.lava_strength_min, self.lava_strength_max);
        output.push_str("lava_strength bins [0,.10), [.10,.25), [.25,.50), [.50,.75), [.75,1], (1,+inf): fragments | emission_R>1 | combined_R>1 | combined_R>.95 | combined_R>.90 | emission_avg/max | combined_avg/max\n");
        for (index, bin) in self.lava_bins.iter().enumerate() {
            let _ = writeln!(output, "  {} | {} | {} | {} | {} | {} | {:.6}/{:.6} | {:.6}/{:.6}", index, bin.fragments, bin.emission_over_one, bin.combined_over_one, bin.combined_over_095, bin.combined_over_090, bin.emission_sum / bin.fragments.max(1) as f64, bin.emission_max, bin.combined_sum / bin.fragments.max(1) as f64, bin.combined_max);
        }
        let _ = writeln!(output, "lava/emission cross counts rows=lava bins cols=emission bins: {:?}", self.lava_emission_cross);
        let _ = writeln!(output, "red excess classification: emission>1={} | emission<=1 then +ambient>1={} | then +direct>1={} | combined<=1={}", self.excess_classification[0], self.excess_classification[1], self.excess_classification[2], self.excess_classification[3]);
        let format_distribution = |name: &str, distribution: &LavaDistribution| {
            format!("{}: count={} mean={} min={} max={} p50={} p90={} p95={} p99={} p99.9={} negative={} nonfinite={} overflow={}\n", name, distribution.count, distribution.mean().map(|v| format!("{v:.6}")).unwrap_or_else(|| "N/E".into()), format_f32(distribution.min), format_f32(distribution.max), distribution.percentile(0.50).map(|v| format!("{v:.6}")).unwrap_or_else(|| "N/E".into()), distribution.percentile(0.90).map(|v| format!("{v:.6}")).unwrap_or_else(|| "N/E".into()), distribution.percentile(0.95).map(|v| format!("{v:.6}")).unwrap_or_else(|| "N/E".into()), distribution.percentile(0.99).map(|v| format!("{v:.6}")).unwrap_or_else(|| "N/E".into()), distribution.percentile(0.999).map(|v| format!("{v:.6}")).unwrap_or_else(|| "N/E".into()), distribution.negative, distribution.non_finite, distribution.overflow)
        };
        output.push_str(&format_distribution("saturated lava_strength", &self.saturated_lava));
        output.push_str(&format_distribution("non-saturated lava_strength", &self.nonsaturated_lava));
        let format_region = |name: &str, region: &LavaRegionStatistics| {
            format!("{}: count={} emission_avg/max={:.6}/{:.6} combined_avg/max={:.6}/{:.6} combined>.90/.95/>1={}/{}/{}\n", name, region.fragments, region.emission_sum / region.fragments.max(1) as f64, region.emission_max, region.combined_sum / region.fragments.max(1) as f64, region.combined_max, region.over_090, region.over_095, region.over_one)
        };
        output.push_str(&format_region("lava_strength == 0", &self.zero_lava));
        output.push_str(&format_region("lava_strength > 0", &self.positive_lava));
        output.push_str("\n=== Counterfactual emission curves ===\n");
        output.push_str("curve | T/M | saturated | >.95 | >.90 | saturated % | max RGB | P50/P90/P95/P99/P99.9 red | max excess | clamp loss | simulated emission R>1 | emission conserved RGB | max reduction\n");
        for (index, stats) in self.curves.iter().enumerate() {
            let (threshold, minimum) = CURVE_SPECS[index];
            let conserved = [
                stats.simulated_emission_sum[0] / stats.original_emission_sum[0].max(f64::MIN_POSITIVE),
                stats.simulated_emission_sum[1] / stats.original_emission_sum[1].max(f64::MIN_POSITIVE),
                stats.simulated_emission_sum[2] / stats.original_emission_sum[2].max(f64::MIN_POSITIVE),
            ];
            let pct = stats.saturated as f64 * 100.0 / stats.count.max(1) as f64;
            let percentile = |p: f64| stats.percentile(p).map(|v| format!("{v:.6}")).unwrap_or_else(|| "N/E".into());
            let _ = writeln!(output, "{} | {:.2}/{:.2} | {} | {} | {} | {:.4}% | {} | {}/{}/{}/{}/{} | {:.6} | {:.6} | {} | {:.4}/{:.4}/{:.4} | {:.6}", CURVE_NAMES[index], threshold, minimum, stats.saturated, stats.over_095, stats.over_090, pct, format_rgb(stats.max_rgb), percentile(0.50), percentile(0.90), percentile(0.95), percentile(0.99), percentile(0.999), stats.red_max_excess, stats.red_clamp_loss, stats.emission_over_one, conserved[0], conserved[1], conserved[2], stats.max_emission_reduction);
        }
        output.push_str("curve | lava bins [0,.10), [.10,.25), [.25,.50), [.50,.75), [.75,1]: saturated / >.95 / >.90\n");
        for (index, stats) in self.curves.iter().enumerate() {
            let values: Vec<String> = stats.by_lava.iter().map(|bin| format!("{}/{}/{}", bin.saturated, bin.over_095, bin.over_090)).collect();
            let _ = writeln!(output, "{} | {}", CURVE_NAMES[index], values.join(" | "));
        }
        output
    }
}

fn format_rgb(values: [f32; 3]) -> String {
    format!("{:.6}/{:.6}/{:.6}", values[0], values[1], values[2])
}

fn color_channel(color: Color, channel: usize) -> f32 {
    match channel {
        0 => color.r,
        1 => color.g,
        _ => color.b,
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
        if object == DiagnosticObject::Vesper {
            self.vesper_correlation.record(sample);
            if let Some(lava) = sample.lava {
                self.vesper_correlation.record_lava(sample, lava);
            }
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
        output.push_str(&self.vesper_correlation.report());
        output
    }
}

fn format_f32(value: f32) -> String {
    if value.is_finite() { format!("{value:.6}") } else { "N/E".to_string() }
}
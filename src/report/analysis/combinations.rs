//! Signal combinations (combinations command): Geometry Free, Ionosphere Free, code multipath
use std::collections::{BTreeMap, BTreeSet, HashMap};

use gnss_qc::prelude::{
    Constellation, Epoch, QcCombination, QcCombinationKind, QcMultipathArc, QcOutput, QcOutputItem,
    SV,
};

use crate::{
    engine::SessionContext,
    report::{
        analysis::Analysis, elapsed_s, stats::fmt, Block, Chart, Section, Series, Stats, Table,
    },
    sink::RunSummary,
};

/// A data gap longer than this starts a new satellite pass (s)
const PASS_GAP_S: f64 = 600.0;

/// A jump of the phase GF larger than this between two epochs starts a new pass (m):
/// cycle slip (one L1 cycle = 0.19 m)
const GF_SLIP_M: f64 = 0.1;

/// Pairs with fewer samples are not charted
const MIN_SAMPLES: usize = 100;

/// Pairs with fewer samples than this fraction of the main pair of their constellation
/// are not charted (incomplete tracking: L2/L5 when L1 is missing..)
const MIN_SHARE: f64 = 0.1;

/// Best / worst satellites: at least this number of samples
const MIN_SATELLITE_SAMPLES: usize = 30;

/// Points per chart (all series), to keep the reports light
const CHART_POINTS: usize = 5000;

/// Uniform decimation of the series of a chart, within the [CHART_POINTS] budget
fn lighten(mut chart: Chart) -> Chart {
    let n = chart.series.len().max(1);
    let max = (CHART_POINTS / n).max(100);
    chart.series = chart.series.iter().map(|s| s.decimated(max)).collect();
    chart
}

/// Largest number of samples per (receiver, constellation)
fn main_counts<'a, K: 'a>(
    entries: impl Iterator<Item = (&'a K, (String, Constellation), usize)>,
) -> HashMap<(String, Constellation), usize> {
    let mut counts = HashMap::new();
    for (_, key, samples) in entries {
        let count = counts.entry(key).or_insert(0);
        *count = samples.max(*count);
    }
    counts
}

/// Explanations, presented once per signal source
const GF_TEXT: &str = "Geometry Free: GF phase = Lj − L1, GF code = C1 − Cj. Both measure the \
    ionospheric delay difference I1 − Ij (negative, proportional to the slant TEC), plus \
    inter-frequency biases. The phase is leveled on the code, pass after pass (median of \
    code − phase): the curves combine the precision of the phase and the absolute level of \
    the code. A pass ends on a data gap (10 min) or a phase jump (0.1 m: cycle slip). \
    The dispersion of GF code − GF phase is the code noise and multipath of the GF combination.";

const IF_TEXT: &str = "Ionosphere Free: IF = (f1² X1 − fj² Xj) / (f1² − fj²), the first order \
    ionospheric delay cancels out. This is what dual frequency navigation (CPP, PPP) observes. \
    IF code − IF phase, mean removed per pass (ambiguities), reveals the noise and multipath \
    of the IF code: about three times the noise of the individual codes (L1/L2).";

const MP_TEXT: &str = "Code multipath: MPi = Ci − Li − 2 (Li − Lj) / (αij − 1), \
    αij = (fi / fj)². Geometry, clocks, troposphere and ionosphere cancel out, the code \
    multipath and noise remain, plus a constant bias per continuous phase arc (removed: mean \
    of the arc). An arc ends on a loss of lock, a data gap or a jump (cycle slip). RMS around \
    the mean of the arcs, per code and per satellite.";

/// Band (frequency) number of an observable: `C1C` → '1'
fn band(observable: &str) -> char {
    observable.chars().nth(1).unwrap_or('?')
}

/// Code and phase value of one combination, at one epoch
#[derive(Debug, Default, Clone, Copy)]
struct Sample {
    code: Option<f64>,
    phase: Option<f64>,
}

/// GF or IF combination of a pair of frequencies (code and phase), for one constellation
#[derive(Debug, Default)]
struct Pair {
    /// name of the code combination (`C1C-C2W`)
    code_name: Option<String>,
    /// name of the phase combination (`L2W-L1C`)
    phase_name: Option<String>,
    samples: usize,
    satellites: BTreeMap<SV, BTreeMap<Epoch, Sample>>,
}

impl Pair {
    fn push(&mut self, t: Epoch, c: &QcCombination) {
        let name = format!("{}-{}", c.lhs, c.rhs);
        let sample = self
            .satellites
            .entry(c.sv)
            .or_default()
            .entry(t)
            .or_default();

        if c.is_code() {
            self.code_name.get_or_insert(name);
            sample.code = Some(c.value_m);
        } else {
            self.phase_name.get_or_insert(name);
            sample.phase = Some(c.value_m);
        }
        self.samples += 1;
    }

    fn label(&self) -> String {
        match (&self.code_name, &self.phase_name) {
            (Some(code), Some(phase)) => format!("{}, {}", phase, code),
            (Some(code), None) => code.clone(),
            (None, Some(phase)) => phase.clone(),
            (None, None) => String::new(),
        }
    }

    /// Continuous passes of one satellite: data gaps and phase jumps (GF)
    fn passes(samples: &BTreeMap<Epoch, Sample>, jumps: bool) -> Vec<Vec<(Epoch, Sample)>> {
        let mut passes: Vec<Vec<(Epoch, Sample)>> = Vec::new();
        let mut last: Option<(Epoch, Option<f64>)> = None;

        for (t, sample) in samples.iter() {
            let new_pass = match last {
                None => true,
                Some((t0, phase0)) => {
                    (*t - t0).to_seconds() > PASS_GAP_S
                        || (jumps
                            && matches!((phase0, sample.phase), (Some(a), Some(b)) if (b - a).abs() > GF_SLIP_M))
                },
            };

            if new_pass {
                passes.push(Vec::new());
            }

            passes.last_mut().unwrap().push((*t, *sample));

            last = Some((*t, sample.phase.or(last.and_then(|l| l.1))));
        }

        passes
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some(values[values.len() / 2])
}

/// Code multipath of one code / phase pair, for one constellation
#[derive(Debug, Default)]
struct Multipath {
    samples: usize,
    /// per satellite: (epoch, arc, raw value)
    satellites: BTreeMap<SV, Vec<(Epoch, u32, f64)>>,
}

/// GF / IF pairs: (receiver, kind, constellation, band 1, band j)
type PairKey = (String, QcCombinationKind, Constellation, char, char);

/// MP: (receiver, constellation, code, phase)
type MultipathKey = (String, Constellation, String, String);

#[derive(Debug, Default)]
pub struct CombinationAnalysis {
    /// Role of each signal source (navigation: rover, base)
    roles: HashMap<String, &'static str>,
    origin: Option<Epoch>,
    pairs: BTreeMap<PairKey, Pair>,
    multipath: BTreeMap<MultipathKey, Multipath>,
    /// (receiver, sv, code, phase, arc)
    arcs: HashMap<(String, SV, String, String, u32), QcMultipathArc>,
}

impl CombinationAnalysis {
    #[allow(unused_variables)]
    pub fn new(context: &SessionContext) -> Self {
        #[allow(unused_mut)]
        let mut roles = HashMap::new();

        #[cfg(feature = "navigation")]
        if let Some(navigation) = &context.navigation {
            // CGGTTS: a single (static) receiver
            let rover = if context.command == "cggtts" {
                "receiver"
            } else {
                "rover"
            };
            roles.insert(navigation.rover.clone(), rover);
            if let Some(base) = &navigation.base {
                roles.insert(base.clone(), "base");
            }
        }

        Self {
            roles,
            ..Default::default()
        }
    }

    fn consume_combination(&mut self, t: Epoch, c: &QcCombination) {
        let receiver = c.receiver.to_string();
        let (lhs, rhs) = (c.lhs.to_string(), c.rhs.to_string());

        match c.kind {
            QcCombinationKind::Multipath => {
                let mp = self
                    .multipath
                    .entry((receiver, c.sv.constellation, lhs, rhs))
                    .or_default();
                mp.samples += 1;
                mp.satellites.entry(c.sv).or_default().push((
                    t,
                    c.arc.unwrap_or_default(),
                    c.value_m,
                ));
            },
            kind => {
                // GF phase: Lj − L1, code: C1 − Cj
                let (b1, bj) = match (kind, c.is_code()) {
                    (QcCombinationKind::GeometryFree, false) => (band(&rhs), band(&lhs)),
                    _ => (band(&lhs), band(&rhs)),
                };
                self.pairs
                    .entry((receiver, kind, c.sv.constellation, b1, bj))
                    .or_default()
                    .push(t, c);
            },
        }
    }

    /// Geometry Free: phase leveled on the code, pass after pass
    fn geometry_free(
        &self,
        origin: Epoch,
        receiver: &str,
        only: Constellation,
        section: &mut Section,
    ) {
        let mut table = Table::new(&[
            "Frequencies",
            "Combinations",
            "Satellites",
            "Samples",
            "Passes",
            "GF code − GF phase σ (m)",
            "Leveled GF min (m)",
            "Leveled GF max (m)",
        ]);

        let mut charts = Vec::new();

        let main = main_counts(
            self.pairs
                .iter()
                .filter(|(k, _)| k.1 == QcCombinationKind::GeometryFree)
                .map(|(k, p)| (k, (k.0.clone(), k.2), p.samples)),
        );

        for ((rx, kind, constellation, b1, bj), pair) in self.pairs.iter() {
            if rx != receiver || *kind != QcCombinationKind::GeometryFree || *constellation != only
            {
                continue;
            }

            let charted = pair.samples >= MIN_SAMPLES
                && pair.samples as f64 >= MIN_SHARE * main[&(rx.clone(), *constellation)] as f64;

            let title = format!(
                "GF {} L{}/L{} ({}): I{} − I{}, phase leveled on the code",
                constellation,
                b1,
                bj,
                pair.label(),
                b1,
                bj
            );

            let mut chart = Chart::time(&title, "m", origin);
            let (mut noise, mut range) = (Stats::default(), Stats::default());
            let mut passes = 0;

            for (sv, samples) in pair.satellites.iter() {
                let mut series = Series::new(&sv.to_string());

                for pass in Pair::passes(samples, true) {
                    passes += 1;

                    // leveling: median of code − phase over the pass
                    let offset = median(
                        pass.iter()
                            .filter_map(|(_, s)| Some(s.code? - s.phase?))
                            .collect(),
                    );

                    for (t, s) in pass.iter() {
                        let value = match (s.phase, offset) {
                            (Some(phase), Some(offset)) => phase + offset,
                            (None, _) => match s.code {
                                Some(code) if pair.phase_name.is_none() => code,
                                _ => continue,
                            },
                            _ => continue,
                        };
                        range.push(value);
                        series.push(elapsed_s(origin, *t), value);

                        if let (Some(code), Some(phase), Some(offset)) = (s.code, s.phase, offset) {
                            noise.push(code - phase - offset);
                        }
                    }
                }

                if !series.x.is_empty() {
                    chart.push(series);
                }
            }

            table.push(vec![
                format!("L{} / L{}", b1, bj),
                pair.label(),
                pair.satellites.len().to_string(),
                pair.samples.to_string(),
                passes.to_string(),
                fmt(noise.rms(), 3),
                fmt(range.min, 2),
                fmt(range.max, 2),
            ]);

            if charted && !chart.is_empty() {
                charts.push(lighten(chart));
            }
        }

        if table.rows.is_empty() {
            return;
        }

        section.push(Block::Heading(format!("{}: Geometry Free", only)));
        section.push(Block::Table(table));
        for chart in charts {
            section.push(Block::Chart(chart));
        }
    }

    /// Ionosphere Free: code − phase (mean removed per pass)
    fn ionosphere_free(
        &self,
        origin: Epoch,
        receiver: &str,
        only: Constellation,
        section: &mut Section,
    ) {
        let mut table = Table::new(&[
            "Frequencies",
            "Combinations",
            "Satellites",
            "Samples",
            "IF code − IF phase σ (m)",
        ]);

        let mut charts = Vec::new();

        let main = main_counts(
            self.pairs
                .iter()
                .filter(|(k, _)| k.1 == QcCombinationKind::IonosphereFree)
                .map(|(k, p)| (k, (k.0.clone(), k.2), p.samples)),
        );

        for ((rx, kind, constellation, b1, bj), pair) in self.pairs.iter() {
            if rx != receiver
                || *kind != QcCombinationKind::IonosphereFree
                || *constellation != only
            {
                continue;
            }

            let charted = pair.samples >= MIN_SAMPLES
                && pair.samples as f64 >= MIN_SHARE * main[&(rx.clone(), *constellation)] as f64;

            let title = format!(
                "IF {} L{}/L{} ({}): code − phase, mean removed per pass",
                constellation,
                b1,
                bj,
                pair.label()
            );

            let mut chart = Chart::time(&title, "m", origin);
            let mut noise = Stats::default();

            for (sv, samples) in pair.satellites.iter() {
                let mut series = Series::new(&sv.to_string());

                for pass in Pair::passes(samples, false) {
                    let differences = pass
                        .iter()
                        .filter_map(|(t, s)| Some((*t, s.code? - s.phase?)))
                        .collect::<Vec<_>>();

                    let Some(offset) = median(differences.iter().map(|(_, d)| *d).collect()) else {
                        continue;
                    };

                    for (t, difference) in differences {
                        // phase jumps (cycle slips) are not part of the code noise
                        let value = difference - offset;
                        if value.abs() > 20.0 {
                            continue;
                        }
                        noise.push(value);
                        series.push(elapsed_s(origin, t), value);
                    }
                }

                if !series.x.is_empty() {
                    chart.push(series);
                }
            }

            table.push(vec![
                format!("L{} / L{}", b1, bj),
                pair.label(),
                pair.satellites.len().to_string(),
                pair.samples.to_string(),
                fmt(noise.rms(), 3),
            ]);

            if charted && !chart.is_empty() {
                charts.push(lighten(chart));
            }
        }

        if table.rows.is_empty() {
            return;
        }

        section.push(Block::Heading(format!("{}: Ionosphere Free", only)));
        section.push(Block::Table(table));
        for chart in charts {
            section.push(Block::Chart(chart));
        }
    }

    /// Code multipath, debiased by the mean of each arc
    fn multipath(&self, origin: Epoch, receiver: &str, only: Constellation, section: &mut Section) {
        let mut table = Table::new(&[
            "Code",
            "Other phase",
            "Satellites",
            "Samples",
            "Arcs",
            "RMS (m)",
            "Best satellite",
            "Worst satellite",
        ]);

        let mut charts = Vec::new();

        // per constellation: satellite → (code, phase) → RMS
        let mut per_satellite =
            BTreeMap::<Constellation, BTreeMap<SV, BTreeMap<String, Stats>>>::new();

        let main = main_counts(
            self.multipath
                .iter()
                .map(|(k, mp)| (k, (k.0.clone(), k.1), mp.samples)),
        );

        for ((rx, constellation, code, phase), mp) in self.multipath.iter() {
            if rx != receiver || *constellation != only {
                continue;
            }

            let charted = mp.samples >= MIN_SAMPLES
                && mp.samples as f64 >= MIN_SHARE * main[&(rx.clone(), *constellation)] as f64;

            let title = format!(
                "MP {} {} ({}): code multipath, arc mean removed",
                constellation, code, phase
            );
            let column = format!("MP {} ({})", code, phase);

            let mut chart = Chart::time(&title, "m", origin);
            let mut total = Stats::default();
            let mut arcs = 0;
            let mut satellites = Vec::new();

            for (sv, samples) in mp.satellites.iter() {
                let mut series = Series::new(&sv.to_string());
                let mut stats = Stats::default();
                let mut sv_arcs = BTreeSet::new();

                for (t, arc, value) in samples.iter() {
                    let key = (rx.clone(), *sv, code.clone(), phase.clone(), *arc);
                    let Some(arc_stats) = self.arcs.get(&key) else {
                        continue;
                    };
                    sv_arcs.insert(*arc);
                    let debiased = value - arc_stats.mean_m;
                    stats.push(debiased);
                    total.push(debiased);
                    series.push(elapsed_s(origin, *t), debiased);
                }

                arcs += sv_arcs.len();

                if stats.n >= MIN_SATELLITE_SAMPLES {
                    satellites.push((*sv, stats.rms()));
                }

                if stats.n > 0 {
                    per_satellite
                        .entry(*constellation)
                        .or_default()
                        .entry(*sv)
                        .or_default()
                        .insert(column.clone(), stats);
                }

                if !series.x.is_empty() {
                    chart.push(series);
                }
            }

            satellites.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

            let describe = |entry: Option<&(SV, f64)>| {
                entry
                    .map(|(sv, rms)| format!("{} ({} m)", sv, fmt(*rms, 3)))
                    .unwrap_or_default()
            };

            table.push(vec![
                code.clone(),
                phase.clone(),
                mp.satellites.len().to_string(),
                mp.samples.to_string(),
                arcs.to_string(),
                fmt(total.rms(), 3),
                describe(satellites.first()),
                describe(satellites.last()),
            ]);

            if charted && !chart.is_empty() {
                charts.push(lighten(chart));
            }
        }

        if table.rows.is_empty() {
            return;
        }

        section.push(Block::Heading(format!("{}: code multipath", only)));
        section.push(Block::Table(table));

        // RMS per satellite
        for satellites in per_satellite.values() {
            let columns = satellites
                .values()
                .flat_map(|codes| codes.keys().cloned())
                .collect::<BTreeSet<_>>();

            let mut headers = vec!["Satellite".to_string()];
            headers.extend(columns.iter().cloned());

            let mut table = Table::new(&headers.iter().map(|h| h.as_str()).collect::<Vec<_>>());

            for (sv, codes) in satellites.iter() {
                let mut row = vec![sv.to_string()];
                for column in columns.iter() {
                    row.push(
                        codes
                            .get(column)
                            .map(|stats| fmt(stats.rms(), 3))
                            .unwrap_or_default(),
                    );
                }
                table.push(row);
            }

            section.push(Block::Text(
                "Code multipath RMS (m), per satellite:".to_string(),
            ));
            section.push(Block::Table(table));
        }

        for chart in charts {
            section.push(Block::Chart(chart));
        }
    }
}

impl Analysis for CombinationAnalysis {
    fn consume(&mut self, item: &QcOutputItem) {
        match &item.payload {
            QcOutput::Combination(c) => {
                let origin = self.origin.get_or_insert(item.epoch);
                if item.epoch < *origin {
                    *origin = item.epoch;
                }
                self.consume_combination(item.epoch, c);
            },
            QcOutput::MultipathArc(arc) => {
                self.arcs.insert(
                    (
                        arc.receiver.to_string(),
                        arc.sv,
                        arc.code.to_string(),
                        arc.phase.to_string(),
                        arc.arc,
                    ),
                    arc.clone(),
                );
            },
            _ => {},
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut sections = Vec::new();

        let Some(origin) = self.origin else {
            let mut section = Section::new("combinations", "Signal combinations");
            section.push(Block::Notice(
                "No signal combination: dual frequency observations are required \
                 (Glonass is not supported)."
                    .to_string(),
            ));
            return vec![section];
        };

        let receivers = self
            .pairs
            .keys()
            .map(|k| k.0.clone())
            .chain(self.multipath.keys().map(|k| k.0.clone()))
            .collect::<BTreeSet<_>>();

        for (index, receiver) in receivers.iter().enumerate() {
            let mut section = Section::new(
                &format!("combinations-{}", index),
                &match self.roles.get(receiver) {
                    Some(role) => format!("Signal combinations: {} ({})", receiver, role),
                    None => format!("Signal combinations: {}", receiver),
                },
            );

            let constellations = self
                .pairs
                .keys()
                .filter(|k| &k.0 == receiver)
                .map(|k| k.2)
                .chain(
                    self.multipath
                        .keys()
                        .filter(|k| &k.0 == receiver)
                        .map(|k| k.1),
                )
                .collect::<BTreeSet<_>>();

            let mut items = vec![("Signal source".to_string(), receiver.clone())];
            if let Some(role) = self.roles.get(receiver) {
                items.push(("Role".to_string(), role.to_string()));
            }

            items.extend([
                (
                    "Constellations".to_string(),
                    constellations
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                (
                    "Multipath arcs".to_string(),
                    self.arcs
                        .keys()
                        .filter(|k| &k.0 == receiver)
                        .count()
                        .to_string(),
                ),
            ]);

            section.push(Block::KeyValue(items));

            let has = |kind: QcCombinationKind| {
                self.pairs.keys().any(|k| &k.0 == receiver && k.1 == kind)
            };

            if has(QcCombinationKind::GeometryFree) {
                section.push(Block::Text(GF_TEXT.to_string()));
            }
            if has(QcCombinationKind::IonosphereFree) {
                section.push(Block::Text(IF_TEXT.to_string()));
            }
            if self.multipath.keys().any(|k| &k.0 == receiver) {
                section.push(Block::Text(MP_TEXT.to_string()));
            }

            // one sub-section per constellation
            for constellation in constellations.iter() {
                self.geometry_free(origin, receiver, *constellation, &mut section);
                self.ionosphere_free(origin, receiver, *constellation, &mut section);
                self.multipath(origin, receiver, *constellation, &mut section);
            }

            sections.push(section);
        }

        sections
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn bands() {
        assert_eq!(band("C1C"), '1');
        assert_eq!(band("L5Q"), '5');
    }

    #[test]
    fn passes() {
        let t0 = Epoch::from_str("2020-06-25T00:00:00 GPST").unwrap();
        let mut samples = BTreeMap::new();

        for (i, (dt_s, phase)) in [
            (0.0, 1.00),
            (30.0, 1.01),
            (60.0, 1.02),
            // cycle slip
            (90.0, 1.25),
            (120.0, 1.26),
            // data gap
            (1200.0, 1.27),
        ]
        .iter()
        .enumerate()
        {
            samples.insert(
                t0 + gnss_qc::prelude::Duration::from_seconds(*dt_s),
                Sample {
                    code: Some(i as f64),
                    phase: Some(*phase),
                },
            );
        }

        let lengths =
            |passes: Vec<Vec<(Epoch, Sample)>>| passes.iter().map(|p| p.len()).collect::<Vec<_>>();

        // GF: gaps and jumps
        assert_eq!(lengths(Pair::passes(&samples, true)), vec![3, 2, 1]);
        // IF: gaps only
        assert_eq!(lengths(Pair::passes(&samples, false)), vec![5, 1]);
    }
}

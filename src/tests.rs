//! End to end tests: command lines, as documented
use clap::Parser;

use crate::{cli::Cli, engine, input::Inputs, workspace::Workspace};

/// Runs this command line, returns the session workspace
fn run(args: &[&str]) -> std::path::PathBuf {
    let cli = Cli::try_parse_from(args).unwrap();
    let inputs = Inputs::load(&cli.inputs, &cli.preprocessing).unwrap();
    let session = cli.outputs.session.clone().unwrap();
    let workspace = Workspace::new(&cli.outputs.output, &session).unwrap();
    let plan = engine::plan(&cli, inputs, &workspace).unwrap();
    engine::run(plan).unwrap();
    cli.outputs.output.join(session)
}

/// obs2tec on a network of two receivers (ESBC00DNK, MOJN00DNK, 65 km apart),
/// formatted as IONEX, then the IONEX injected back into rinex-cli (tec command),
/// as documented in documentation/Obs2Tec.md
#[test]
fn obs2tec_network_ionex() {
    use ionex::prelude::{Key, IONEX};
    use std::str::FromStr;

    let output = std::env::temp_dir().join("rinex-cli-tests");
    let output = output.to_str().unwrap();

    let session = run(&[
        "rinex-cli",
        "-f",
        "data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz",
        "-f",
        "data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz",
        "-f",
        "data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz",
        "-o",
        output,
        "--session",
        "DNK_OBS2TEC",
        "--data",
        "none",
        "--no-qc",
        "-q",
        "obs2tec",
        "-c",
        "GPS",
        "--ionex",
        "--point",
        "Esbjerg:55.5,8.45",
    ]);

    assert!(session.join("report.html").exists());

    let path = session.join("IONEX/DNKOBS2TE_01H.20I");
    let ionex = IONEX::from_file(&path).unwrap();

    // one map every hour, 2020-06-25 00:00 to 2020-06-26 00:00 UTC
    let header = &ionex.header;
    assert_eq!(header.number_of_maps, 25);
    assert_eq!(header.num_stations, 2);
    assert_eq!(header.sampling_period.to_seconds(), 3600.0);
    assert_eq!(
        header.epoch_of_first_map,
        gnss_qc::prelude::Epoch::from_str("2020-06-25T00:00:00 UTC").unwrap()
    );
    assert!(header.comments.iter().any(|c| c.contains("UNCALIBRATED")));

    // Esbjerg (55.0°N, 10.0°E): every map
    let mut values = Vec::new();
    for epoch in ionex.record.epochs_iter() {
        let key = Key::from_decimal_degrees_km(epoch, 55.0, 10.0, 450.0);
        let tec = ionex.record.get(&key).expect("node next to the network");
        values.push(tec.tecu());
    }
    assert_eq!(values.len(), 25);

    // uncalibrated (DCB): a few TECu off at most, at solar minimum, mid latitude
    for tecu in values.iter() {
        assert!(tecu.abs() < 30.0, "unrealistic TEC {}", tecu);
    }

    // the IONEX is injected back into rinex-cli: a standard TEC maps product
    let session = run(&[
        "rinex-cli",
        "-f",
        path.to_str().unwrap(),
        "-o",
        output,
        "--session",
        "DNK_OBS2TEC_maps",
        "-q",
        "tec",
        "--region",
        "Denmark:54.5,57.8,8,13",
    ]);

    let maps = std::fs::read_to_string(session.join("data/tec_maps.csv")).unwrap();
    assert_eq!(
        maps.lines().filter(|l| l.contains(",Denmark,")).count(),
        25,
        "statistics of every map over Denmark"
    );
}

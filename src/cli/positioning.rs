// Positioning OPMODE
use clap::{value_parser, Arg, ArgAction, Command};
use rinex::prelude::Duration;

fn shared_args(cmd: Command) -> Command {
    let cmd = cmd
        .arg(Arg::new("cfg")
            .short('c')
            .long("cfg")
            .value_name("FILE")
            .required(false)
            .action(ArgAction::Append)
            .help("Position Solver configuration file (JSON). See --help.")
            .long_help("
Refer to all our navigation demos (subfolder) and example scripts.
[https://docs.rs/gnss-rtk/latest/gnss_rtk/prelude/struct.Config.html] is the structure to be descrined.
"));

    let cmd = cmd.next_help_heading("User / Rover Profile")
        .arg(
            Arg::new("static")
                .long("static")
                .action(ArgAction::SetTrue)
                .help("Define the rover as static, meaning, its antenna was held static for the entire session.
The default profile is \"pedestrian\" (very low velocity), which is not suited for very fast moving rovers."))
            .arg(
                Arg::new("car")
                    .long("car")
                    .action(ArgAction::SetTrue)
                    .help("Define car profile (low velocity).
The default profile is \"pedestrian\" (very low velocity), which is not suited for very fast moving rovers."))
                .arg(
                    Arg::new("airplane")
                        .long("airplane")
                        .action(ArgAction::SetTrue)
                        .help("Define airplane profile (high velocity).
The default profile is \"pedestrian\" (very low velocity), which is not suited for very fast moving rovers."))
                        .arg(
                            Arg::new("rocket")
                                .long("rocket")
                                .action(ArgAction::SetTrue)
                                .help("Define rocket profile (very high velocity).
                                The default profile is \"pedestrian\" (very low velocity), which is not suited for very fast moving rovers."))
        .arg(
            Arg::new("quartz")
                .long("quartz")
                .action(ArgAction::SetTrue)
                .help("Define quartz (rover clock) profile (very poor quality).
The default profile is Oscillator/OCXO."))
.arg(
    Arg::new("atomic")
        .long("quartz")
        .action(ArgAction::SetTrue)
        .help("Define atomic (rover clock) profile (high quality, at the scale of a GNSS constellation).
The default profile is Oscillator/OCXO."))
.arg(
    Arg::new("h-maser")
        .long("h-maser")
        .action(ArgAction::SetTrue)
        .help("Define Hydrogen MASER (rover clock) profile (ultra high quality, better than GNSS constellation).
The default profile is Oscillator/OCXO.")
        );

    let cmd = cmd.next_help_heading("Solutions formating");

    let cmd = if cfg!(feature = "kml") {
        cmd.arg(
            Arg::new("kml")
                .long("kml")
                .action(ArgAction::SetTrue)
                .help("Format PVT solutions as KML track."),
        )
    } else {
        cmd.arg(
            Arg::new("kml")
                .long("kml")
                .action(ArgAction::SetTrue)
                .help("[NOT AVAILABLE] requires kml compilation option"),
        )
    };

    let cmd = if cfg!(feature = "gpx") {
        cmd.arg(
            Arg::new("gpx")
                .long("gpx")
                .action(ArgAction::SetTrue)
                .help("Format PVT solutions as GPX track."),
        )
    } else {
        cmd.arg(
            Arg::new("gpx")
                .long("gpx")
                .action(ArgAction::SetTrue)
                .help("[NOT AVAILABLE] requires gpx compilation option"),
        )
    };

    cmd
}

pub fn ppp_subcommand() -> Command {
    let cmd = Command::new("ppp")
        .arg_required_else_help(false)
        .about(
            "Post Processed Positioning. Use this mode to deploy the precise position solver.
The solutions are added to the final report as an extra chapter. See --help",
        )
        .long_about(
            "Post Processed Positioning (ppp) opmode resolves
PVT solutions from RINEX data sampled by a single receiver (! This is not RTK!).
The solutions are presented in the analysis report (post processed results chapter).
Use --cggtts to convert solutions to CGGTTS special format.",
        )
        .next_help_heading("CGGTTS Post FIT");

    let cmd = if cfg!(not(feature = "cggtts")) {
        cmd.arg(
            Arg::new("cggtts")
                .long("cggtts")
                .action(ArgAction::SetTrue)
                .help("[NOT AVAILABLE] requires cggtts compilation option"),
        )
    } else {
        cmd
                .arg(Arg::new("cggtts")
                    .long("cggtts")
                    .action(ArgAction::SetTrue)
                    .help("Activate CGGTTS special Post FIT"))
                .arg(Arg::new("tracking")
                    .long("trk")
                    .value_parser(value_parser!(Duration))
                    .action(ArgAction::Set)
                    .help("CGGTTS custom tracking duration.
        Otherwise, the default tracking duration is used. Refer to [https://docs.rs/cggtts/latest/cggtts/track/struct.Scheduler.html]."))
                .arg(Arg::new("lab")
                    .long("lab")
                    .action(ArgAction::Set)
                    .help("Define the name of your station or laboratory here."))
                .arg(Arg::new("utck")
                    .long("utck")
                    .action(ArgAction::Set)
                    .conflicts_with("clock")
                    .help("If the local clock tracks a local UTC replica, you can define the name
        of this replica here."))
                .arg(Arg::new("clock") 
                    .long("clk")
                    .action(ArgAction::Set)
                    .conflicts_with("utck")
                    .help("If the local clock is not a UTC replica and has a specific name, you
        can define it here."))
    };

    shared_args(cmd)
}

pub fn rtk_subcommand() -> Command {
    let cmd = Command::new("rtk")
        .arg_required_else_help(true)
        .about(
            "Post Processed RTK. Use this mode to deploy the precise differential positioning.
The initial context describes the Rover context. rtk accepts `--fp,f` once again, to describe the base station.
Other positioning flags still apply (like -c). See --help.",
        )
        .long_about(
            "RTK post opmode resolves PVT solutions by (post processed) differential navigation.
The initial context (-f, -d) describes the ROVER.
`rtk` also accepts -f and -d and you need to use those to describe the BASE (mandatory)."
        )
        .arg(
            Arg::new("fp")
                .long("fp")
                .value_name("FILE")
                .action(ArgAction::Append)
                .required_unless_present("dir")
                .help("Base station Observation RINEX file(s), one at a time, as many as needed.")
        )
        .arg(
            Arg::new("dir")
                .short('d')
                .value_name("DIR")
                .action(ArgAction::Append)
                .required_unless_present("fp")
                .help("Base station Observation RINEX directory, one at a time, as many as needed.")
        );
    shared_args(cmd)
}

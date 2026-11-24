//! PDF rendition: the report is written as a Typst document (code mode, every text
//! being a string literal) and compiled in memory, with the Typst embedded fonts.
//! Charts are static SVG figures.
use std::fmt::Write;

use typst_as_lib::{typst_kit_options::TypstKitFontOptions, TypstEngine};
use typst_layout::PagedDocument;

use crate::{
    error::Error,
    report::{svg, Block, ChartKind, Report},
};

/// Typst string literal
fn s(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {},
            '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

const PREAMBLE: &str = r##"
#set document(title: TITLE)
#set page(
  paper: "a4",
  margin: (x: 1.5cm, top: 1.8cm, bottom: 1.6cm),
  header: context {
    if counter(page).get().first() > 1 [
      #set text(8pt, fill: luma(120))
      #TITLE #h(1fr) #SUBTITLE
    ]
  },
  footer: context [
    #set text(8pt, fill: luma(120))
    #FOOTER #h(1fr) #counter(page).display("1 / 1", both: true)
  ],
)
#set text(font: "Libertinus Serif", size: 10pt)
#set par(justify: false)
#set table(stroke: 0.4pt + luma(200), inset: 4pt)
#show table.cell.where(y: 0): set text(weight: "bold")
#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  set text(15pt)
  block(below: 10pt, it)
}
#show heading.where(level: 2): set text(11.5pt, fill: luma(60))
#show figure.caption: set text(8.5pt, fill: luma(80))

#let kv(..pairs) = {
  show table.cell: set text(weight: "regular")
  table(
  columns: (auto, 1fr),
  stroke: none,
  inset: (x: 0pt, y: 2.5pt),
  column-gutter: 12pt,
  ..pairs.pos().map(((k, v)) => (text(fill: luma(80), k), v)).flatten()
  )
}

#let notice(body) = block(fill: rgb("#fff8c5"), inset: 8pt, radius: 4pt, width: 100%, text(fill: rgb("#7a5200"), body))

#block(below: 14pt)[
  #text(20pt, weight: "bold", TITLE)
  #linebreak()
  #text(fill: luma(100), SUBTITLE + " · generated " + GENERATED)
]
#text(13pt, weight: "bold")[Contents]
#outline(title: none, depth: 1)
"##;

pub fn render(report: &Report) -> Result<Vec<u8>, Error> {
    let mut doc = String::new();
    let mut figures = Vec::<(String, Vec<u8>)>::new();

    let _ = writeln!(doc, "#let TITLE = {}", s(&report.title));
    let _ = writeln!(doc, "#let SUBTITLE = {}", s(&report.subtitle));
    let _ = writeln!(doc, "#let GENERATED = {}", s(&report.generated));
    let _ = writeln!(
        doc,
        "#let FOOTER = {}",
        s(&format!(
            "rinex-cli v{}, powered by GNSS-Qc",
            env!("CARGO_PKG_VERSION")
        ))
    );
    doc.push_str(PREAMBLE);

    for section in report.sections.iter() {
        let _ = writeln!(doc, "\n#heading(level: 1, {})", s(&section.title));

        for block in section.blocks.iter() {
            match block {
                Block::Heading(text) => {
                    let _ = writeln!(doc, "#heading(level: 2, {})", s(text));
                },
                Block::Text(text) => {
                    let _ = writeln!(doc, "#par({})", s(text));
                },
                Block::Notice(text) => {
                    let _ = writeln!(doc, "#notice({})", s(text));
                },
                Block::KeyValue(pairs) => {
                    let _ = write!(doc, "#kv(");
                    for (key, value) in pairs {
                        let _ = write!(doc, "({}, {}), ", s(key), s(value));
                    }
                    let _ = writeln!(doc, ")");
                },
                Block::Table(table) => {
                    let columns = table.headers.len().max(1);
                    let size = match columns {
                        0..=4 => "9pt",
                        5..=6 => "8pt",
                        _ => "7pt",
                    };
                    let _ = write!(
                        doc,
                        "#text(size: {})[#table(columns: {}, table.header(",
                        size, columns
                    );
                    for header in table.headers.iter() {
                        let _ = write!(doc, "{}, ", s(header));
                    }
                    let _ = write!(doc, "), ");
                    for row in table.rows.iter() {
                        for index in 0..columns {
                            let cell = row.get(index).map(|c| c.as_str()).unwrap_or("");
                            let _ = write!(doc, "{}, ", s(cell));
                        }
                    }
                    let _ = writeln!(doc, ")]");
                },
                Block::Heatmap(heatmap) => {
                    // a selection of frames (at most 6, evenly spaced)
                    let n = heatmap.frames.len();
                    let step = n.div_ceil(6).max(1);

                    for frame in heatmap.frames.iter().step_by(step) {
                        let name = format!("figure-{}.svg", figures.len());
                        figures.push((
                            name.clone(),
                            svg::render_heatmap(heatmap, frame).into_bytes(),
                        ));

                        let _ = writeln!(
                            doc,
                            "#figure(image({}, width: 100%), caption: {})",
                            s(&name),
                            s(&format!("{}: {}", heatmap.title, frame.label))
                        );
                    }

                    if n > step {
                        let _ = writeln!(
                            doc,
                            "#par(text(size: 8.5pt, fill: luma(90), {}))",
                            s(&format!(
                                "{} maps in total, one map out of {} is shown (the HTML report shows every map).",
                                n, step
                            ))
                        );
                    }
                },
                Block::Skyplot(skyplot) => {
                    let name = format!("figure-{}.svg", figures.len());
                    figures.push((name.clone(), svg::render_skyplot(skyplot).into_bytes()));
                    let _ = writeln!(
                        doc,
                        "#figure(image({}, width: 78%), caption: {})",
                        s(&name),
                        s(&skyplot.title)
                    );
                },
                Block::Timeline(timeline) => {
                    let name = format!("figure-{}.svg", figures.len());
                    figures.push((name.clone(), svg::render_timeline(timeline).into_bytes()));
                    let _ = writeln!(
                        doc,
                        "#figure(image({}, width: 100%), caption: {})",
                        s(&name),
                        s(&timeline.title)
                    );
                },
                Block::Chart(chart) => {
                    if chart.is_empty() {
                        continue;
                    }
                    if chart.kind == ChartKind::Globe {
                        let name = format!("figure-{}.svg", figures.len());
                        figures.push((name.clone(), svg::render(chart).into_bytes()));
                        let _ = writeln!(
                            doc,
                            "#figure(image({}, width: 45%), caption: {})",
                            s(&name),
                            s(&chart.title)
                        );
                        continue;
                    }
                    let name = format!("figure-{}.svg", figures.len());
                    figures.push((name.clone(), svg::render(chart).into_bytes()));

                    let mut caption = chart.title.clone();
                    if chart.series.len() > 12
                        && !matches!(chart.kind, ChartKind::Map | ChartKind::Region)
                    {
                        caption.push_str(&format!(" ({} series)", chart.series.len()));
                    }

                    let _ = writeln!(
                        doc,
                        "#figure(image({}, width: 100%), caption: {})",
                        s(&name),
                        s(&caption)
                    );
                },
            }
        }
    }

    let engine = TypstEngine::builder()
        .search_fonts_with(
            TypstKitFontOptions::default()
                .include_system_fonts(false)
                .include_embedded_fonts(true),
        )
        .with_static_source_file_resolver([("report.typ", doc)])
        .with_static_file_resolver(
            figures
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.clone())),
        )
        .build();

    let document: PagedDocument = engine
        .compile("report.typ")
        .output
        .map_err(|e| Error::Pdf(format!("{:?}", e)))?;

    typst_pdf::pdf(&document, &Default::default()).map_err(|e| Error::Pdf(format!("{:?}", e)))
}

#[cfg(test)]
mod test {
    use super::s;

    #[test]
    fn typst_strings() {
        assert_eq!(s("a \"b\" \\ c"), "\"a \\\"b\\\" \\\\ c\"");
        assert_eq!(s("#[]*_$"), "\"#[]*_$\"");
    }
}
